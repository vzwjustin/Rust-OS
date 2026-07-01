//! Wayland Client module
//!
//! Represents a trusted Wayland client launched by the compositor.
//! Allows detection and management of client capabilities and lifecycle.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-client.c

use alloc::string::String;

/// Client kind enumeration (launch origin).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaWaylandClientKind {
    /// Public client (not launched by compositor).
    PUBLIC = 0,
    /// Created/launched by compositor.
    CREATED = 1,
    /// Subprocess of compositor.
    SUBPROCESS = 2,
}

/// Client capability flags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaWaylandClientCaps {
    /// X11 interop capability.
    X11INTEROP = 1,
}

pub const META_WAYLAND_CLIENT_CAPS_X11_INTEROP: u32 = 1;

/// A trusted Wayland client instance managed by the compositor.
pub struct MetaWaylandClient {
    /// Pointer to parent MetaContext.
    pub context: *mut core::ffi::c_void,
    /// Pointer to wl_client.
    pub wayland_client: *mut core::ffi::c_void,
    /// Client capabilities bitset.
    pub caps: u32,
    /// Kind/origin of this client.
    pub kind: u32,
    /// Client FD for CREATED kind.
    pub created_client_fd: i32,
    /// GSubprocess pointer for SUBPROCESS kind.
    pub subprocess: *mut core::ffi::c_void,
    /// Optional window tag (window grouping hint).
    pub window_tag: Option<String>,
    /// Process ID of client.
    pub pid: i32,
}

impl MetaWaylandClient {
    /// Create a new client instance.
    pub fn new() -> Self {
        MetaWaylandClient {
            context: core::ptr::null_mut(),
            wayland_client: core::ptr::null_mut(),
            caps: 0,
            kind: 0,
            created_client_fd: -1,
            subprocess: core::ptr::null_mut(),
            window_tag: None,
            pid: 0,
        }
    }

    /// Get the MetaContext for this client
    pub fn get_context(&self) -> *mut core::ffi::c_void {
        self.context
    }

    /// Get the underlying wayland client
    pub fn get_wl_client(&self) -> *mut core::ffi::c_void {
        self.wayland_client
    }

    /// Check if this client matches the given wl_client
    pub fn matches(&self, _wl_client: *const core::ffi::c_void) -> bool {
        // TODO: implement cross-reference logic
        false
    }

    /// Set capabilities for this client
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
        if self.subprocess.is_null() {
            None
        } else {
            Some(self.subprocess)
        }
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
