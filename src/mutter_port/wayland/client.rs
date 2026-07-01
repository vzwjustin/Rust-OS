//! Wayland Client module
//!
//! Ported from: meta-wayland-client.c/h

use alloc::{string::String, vec::Vec, format};

pub enum MetaWaylandClientKind {
    Public = 0,
    Created = 1,
    Subprocess = 2,
}

pub enum MetaWaylandClientCaps {
    X11Interop = 1,
}

pub struct MetaWaylandClient {
    pub context: Option<*mut core::ffi::c_void>, // MetaContext pointer
    pub wayland_client: Option<*mut core::ffi::c_void>, // wl_client pointer
    pub caps: u32,
    pub kind: u32,
    pub created_client_fd: i32,
    pub subprocess: Option<*mut core::ffi::c_void>, // GSubprocess pointer
    pub window_tag: Option<String>,
    pub pid: i32,
}

impl MetaWaylandClient {
    /// Get the MetaContext for this client
    /// TODO: port logic from meta_wayland_client_get_context
    pub fn get_context(&self) -> Option<*mut core::ffi::c_void> {
        self.context
    }

    /// Get the underlying wayland client
    /// TODO: port logic from meta_wayland_client_get_wl_client
    pub fn get_wl_client(&self) -> Option<*mut core::ffi::c_void> {
        self.wayland_client
    }

    /// Check if this client matches the given wl_client
    /// TODO: port logic from meta_wayland_client_matches
    pub fn matches(&self, _wl_client: *const core::ffi::c_void) -> bool {
        // TODO: implement
        false
    }

    /// Set capabilities for this client
    /// TODO: port logic from meta_wayland_client_set_caps
    pub fn set_caps(&mut self, caps: u32) {
        self.caps = caps;
    }

    /// Check if this client has specific capabilities
    /// TODO: port logic from meta_wayland_client_has_caps
    pub fn has_caps(&self, caps: u32) -> bool {
        (self.caps & caps) == caps
    }

    /// Take the client fd (only for CREATED clients)
    /// TODO: port logic from meta_wayland_client_take_client_fd
    pub fn take_client_fd(&mut self) -> i32 {
        let fd = self.created_client_fd;
        self.created_client_fd = -1;
        fd
    }

    /// Get the subprocess (only for SUBPROCESS clients)
    /// TODO: port logic from meta_wayland_client_get_subprocess
    pub fn get_subprocess(&self) -> Option<*mut core::ffi::c_void> {
        self.subprocess
    }

    /// Set the window tag for this client
    /// TODO: port logic from meta_wayland_client_set_window_tag
    pub fn set_window_tag(&mut self, tag: Option<String>) {
        self.window_tag = tag;
    }

    /// Get the window tag for this client
    /// TODO: port logic from meta_wayland_client_get_window_tag
    pub fn get_window_tag(&self) -> Option<&str> {
        self.window_tag.as_deref()
    }

    /// Get the PID of the process that created this client
    /// TODO: port logic from meta_wayland_client_get_pid
    pub fn get_pid(&self) -> i32 {
        self.pid
    }
}
