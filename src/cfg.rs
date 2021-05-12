// libcfg interface for Rust
// Copyright (c) 2021 Red Hat, Inc.
//
// All rights reserved.
//
// Author: Christine Caulfield (ccaulfi@redhat.com)
//

// For the code generated by bindgen
use crate::sys::cfg as ffi;

use std::os::raw::{c_void, c_int};
use std::collections::HashMap;
use std::sync::Mutex;
use std::ffi::CString;

use crate::{CsError, DispatchFlags, Result, NodeId};
use crate::string_from_bytes;

// Used to convert a CFG handle into one of ours
lazy_static! {
    static ref HANDLE_HASH: Mutex<HashMap<u64, Handle>> = Mutex::new(HashMap::new());
}

/// Callback from [track_start]. Will be called if another process
/// requests to shut down corosync. [reply_to_shutdown] should be called
/// with a [ShutdownReply] of either Yes or No.
#[derive(Copy, Clone)]
pub struct Callbacks {
    pub corosync_cfg_shutdown_callback_fn: Option<fn(handle: &Handle,
						     flags: u32)>
}

/// A handle into the cfg library. returned from [initialize] and needed for all other calls
#[derive(Copy, Clone)]
pub struct Handle {
    cfg_handle: u64,
    callbacks: Callbacks
}

/// Flags for [try_shutdown]
pub enum ShutdownFlags
{
    /// Request shutdown (other daemons will be consulted)
    Request,
    /// Tells other daemons but ignore their opinions
    Regardless,
    /// Go down straight away (but still tell other nodes)
    Immediate,
}

/// Responses for [reply_to_shutdown]
pub enum ShutdownReply
{
    Yes = 1,
    No = 0
}

/// Trackflags for [track_start]. None currently supported
pub enum TrackFlags
{
    None,
}

/// Version of the [NodeStatus] structure returned from [node_status_get]
pub enum NodeStatusVersion
{
    V1,
}

/// Status of a link inside [NodeStatus] struct
pub struct LinkStatus
{
    pub enabled: bool,
    pub connected: bool,
    pub dynconnected: bool,
    pub mtu: u32,
    pub src_ipaddr: String,
    pub dst_ipaddr: String,
}

/// Structure returned from [node_status_get], shows all the details of a node
/// that is known to corosync, including all configured links
pub struct NodeStatus
{
    pub version: NodeStatusVersion,
    pub nodeid: NodeId,
    pub reachable: bool,
    pub remote: bool,
    pub external: bool,
    pub onwire_min: u8,
    pub onwire_max: u8,
    pub onwire_ver: u8,
    pub link_status: Vec<LinkStatus>,
}

extern "C" fn rust_shutdown_notification_fn(handle: ffi::corosync_cfg_handle_t, flags: u32)
{
    if let Some(h) = HANDLE_HASH.lock().unwrap().get(&handle) {
	if let Some(cb) = h.callbacks.corosync_cfg_shutdown_callback_fn {
	    (cb)(h, flags);
	}
    }
}


/// Initialize a connection to the cfg library. You must call this before doing anything
/// else and use the passed back [Handle].
/// Remember to free the handle using [finalize] when finished.
pub fn initialize(callbacks: &Callbacks) -> Result<Handle>
{
    let mut handle: ffi::corosync_cfg_handle_t = 0;

    let c_callbacks = ffi::corosync_cfg_callbacks_t {
	corosync_cfg_shutdown_callback: Some(rust_shutdown_notification_fn),
    };

    unsafe {
	let res = ffi::corosync_cfg_initialize(&mut handle,
					       &c_callbacks);
	if res == ffi::CS_OK {
	    let rhandle = Handle{cfg_handle: handle, callbacks: *callbacks};
	    HANDLE_HASH.lock().unwrap().insert(handle, rhandle);
	    Ok(rhandle)
	} else {
	    Err(CsError::from_c(res))
	}
    }
}


/// Finish with a connection to corosync, after calling this the [Handle] is invalid
pub fn finalize(handle: Handle) -> Result<()>
{
    let res =
	unsafe {
	    ffi::corosync_cfg_finalize(handle.cfg_handle)
	};
    if res == ffi::CS_OK {
	HANDLE_HASH.lock().unwrap().remove(&handle.cfg_handle);
	Ok(())
    } else {
	Err(CsError::from_c(res))
    }
}

// not sure if an fd is the right thing to return here, but it will do for now.
/// Returns a file descriptor to use for poll/select on the CFG handle
pub fn fd_get(handle: Handle) -> Result<i32>
{
    let c_fd: *mut c_int = &mut 0 as *mut _ as *mut c_int;
    let res =
	unsafe {
	    ffi::corosync_cfg_fd_get(handle.cfg_handle, c_fd)
	};
    if res == ffi::CS_OK {
	Ok(c_fd as i32)
    } else {
	Err(CsError::from_c(res))
    }
}

/// Get the local [NodeId]
pub fn local_get(handle: Handle) -> Result<NodeId>
{
    let mut nodeid: u32 = 0;
    let res =
	unsafe {
	    ffi::corosync_cfg_local_get(handle.cfg_handle, &mut nodeid)
	};
    if res == ffi::CS_OK {
	Ok(NodeId::from(nodeid))
    } else {
	Err(CsError::from_c(res))
    }
}

/// Reload the cluster configuration on all nodes
pub fn reload_cnfig(handle: Handle) -> Result<()>
{
    let res =
	unsafe {
	    ffi::corosync_cfg_reload_config(handle.cfg_handle)
	};
    if res == ffi::CS_OK {
	Ok(())
    } else {
	Err(CsError::from_c(res))
    }
}

/// Re-open the cluster log files, on this node only
pub fn reopen_log_files(handle: Handle) -> Result<()>
{
    let res =
	unsafe {
	    ffi::corosync_cfg_reopen_log_files(handle.cfg_handle)
	};
    if res == ffi::CS_OK {
	Ok(())
    } else {
	Err(CsError::from_c(res))
    }
}


/// Tell another cluster node to shutdown. reason is a string that
/// will be written to the system log files.
pub fn kill_node(handle: Handle, nodeid: NodeId, reason: &str) -> Result<()>
{
    let c_string = {
	match CString::new(reason) {
	    Ok(cs) => cs,
	    Err(_) => return Err(CsError::CsErrInvalidParam),
	}
    };

    let res =
	unsafe {
	    ffi::corosync_cfg_kill_node(handle.cfg_handle, u32::from(nodeid), c_string.as_ptr())
	};
    if res == ffi::CS_OK {
	Ok(())
    } else {
	Err(CsError::from_c(res))
    }
}

/// Ask this cluster node to shutdown. If [ShutdownFlags] is set to Request then
///it may be refused by other applications
/// that have registered for shutdown callbacks.
pub fn try_shutdown(handle: Handle, flags: ShutdownFlags) -> Result<()>
{
    let c_flags = match flags {
	ShutdownFlags::Request => 0,
	ShutdownFlags::Regardless => 1,
	ShutdownFlags::Immediate => 2
    };
    let res =
	unsafe {
	    ffi::corosync_cfg_try_shutdown(handle.cfg_handle, c_flags)
	};
    if res == ffi::CS_OK {
	Ok(())
    } else {
	Err(CsError::from_c(res))
    }
}


/// Reply to a shutdown request with Yes or No [ShutdownReply]
pub fn reply_to_shutdown(handle: Handle, flags: ShutdownReply) -> Result<()>
{
    let c_flags = match flags {
	ShutdownReply::No => 0,
	ShutdownReply::Yes => 1,
    };
    let res =
	unsafe {
	    ffi::corosync_cfg_replyto_shutdown(handle.cfg_handle, c_flags)
	};
    if res == ffi::CS_OK {
	Ok(())
    } else {
	Err(CsError::from_c(res))
    }
}

/// Call any/all active CFG callbacks for this [Handle] see [DispatchFlags] for details
pub fn dispatch(handle: Handle, flags: DispatchFlags) -> Result<()>
{
    let res =
	unsafe {
	    ffi::corosync_cfg_dispatch(handle.cfg_handle, flags as u32)
	};
    if res == ffi::CS_OK {
	Ok(())
    } else {
	Err(CsError::from_c(res))
    }
}

// Quick & dirty u8 to boolean
fn u8_to_bool(val: u8) -> bool
{
    val != 0
}

const CFG_MAX_LINKS: usize = 8;
const CFG_MAX_HOST_LEN: usize = 256;
fn unpack_nodestatus(c_nodestatus: ffi::corosync_cfg_node_status_v1) -> Result<NodeStatus>
{
    let mut ns = NodeStatus {
	version: NodeStatusVersion::V1,
	nodeid: NodeId::from(c_nodestatus.nodeid),
	reachable: u8_to_bool(c_nodestatus.reachable),
	remote: u8_to_bool(c_nodestatus.remote),
	external: u8_to_bool(c_nodestatus.external),
	onwire_min: c_nodestatus.onwire_min,
	onwire_max: c_nodestatus.onwire_max,
	onwire_ver: c_nodestatus.onwire_min,
	link_status: Vec::<LinkStatus>::new()
    };
    for i in 0..CFG_MAX_LINKS {
	let ls = LinkStatus {
	    enabled: u8_to_bool(c_nodestatus.link_status[i].enabled),
	    connected: u8_to_bool(c_nodestatus.link_status[i].connected),
	    dynconnected: u8_to_bool(c_nodestatus.link_status[i].dynconnected),
	    mtu: c_nodestatus.link_status[i].mtu,
	    src_ipaddr: string_from_bytes(&c_nodestatus.link_status[i].src_ipaddr[0], CFG_MAX_HOST_LEN)?,
	    dst_ipaddr: string_from_bytes(&c_nodestatus.link_status[i].dst_ipaddr[0], CFG_MAX_HOST_LEN)?,
	};
	ns.link_status.push(ls);
    }

    Ok(ns)
}

// Constructor for link status to make c_ndostatus initialization tidier.
fn new_ls() -> ffi::corosync_knet_link_status_v1
{
    ffi::corosync_knet_link_status_v1 {
	enabled:0,
	connected:0,
	dynconnected:0,
	mtu:0,
	src_ipaddr: [0; 256],
	dst_ipaddr: [0; 256],
    }
}

/// Get the extended status of a node in the cluster (including active links) from its [NodeId].
/// Returns a filled in [NodeStatus] struct
pub fn node_status_get(handle: Handle, nodeid: NodeId, _version: NodeStatusVersion) -> Result<NodeStatus>
{
    // Currently only supports V1 struct
    unsafe {
	// We need to initialize this even though it's all going to be overwritten.
	let mut c_nodestatus = ffi::corosync_cfg_node_status_v1 {
	    version: 1,
	    nodeid:0,
	    reachable:0,
	    remote:0,
	    external:0,
	    onwire_min:0,
	    onwire_max:0,
	    onwire_ver:0,
	    link_status: [new_ls(); 8],
	};

	let res = ffi::corosync_cfg_node_status_get(handle.cfg_handle, u32::from(nodeid), 1, &mut c_nodestatus as *mut _ as *mut c_void);

	if res == ffi::CS_OK {
	    unpack_nodestatus(c_nodestatus)
	} else {
	    Err(CsError::from_c(res))
	}
    }
}

/// Start tracking for shutdown notifications
pub fn track_start(handle: Handle, _flags: TrackFlags) -> Result<()>
{
    let res =
	unsafe {
	    ffi::corosync_cfg_trackstart(handle.cfg_handle, 0)
	};
    if res == ffi::CS_OK {
	Ok(())
    } else {
	Err(CsError::from_c(res))
    }
}

/// Stop tracking for shutdown notifications
pub fn track_stop(handle: Handle) -> Result<()>
{
    let res =
	unsafe {
	    ffi::corosync_cfg_trackstop(handle.cfg_handle)
	};
    if res == ffi::CS_OK {
	Ok(())
    } else {
	Err(CsError::from_c(res))
    }
}
