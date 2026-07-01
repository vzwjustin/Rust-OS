//! Remote Desktop — ported from GNOME Mutter
//!
//! Provides remote desktop control (keyboard, pointer, touchscreen input injection),
//! complementing screen casting to allow bi-directional control of desktop sessions via D-Bus.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-remote-desktop.h

use alloc::vec::Vec;
use core::ffi::c_void;

/// Device types that can be controlled via remote desktop.
pub const META_REMOTE_DESKTOP_DEVICE_TYPE_NONE: u32 = 0;
pub const META_REMOTE_DESKTOP_DEVICE_TYPE_KEYBOARD: u32 = 1 << 0;
pub const META_REMOTE_DESKTOP_DEVICE_TYPE_POINTER: u32 = 1 << 1;
pub const META_REMOTE_DESKTOP_DEVICE_TYPE_TOUCHSCREEN: u32 = 1 << 2;

/// Type alias for device type bitmask.
pub type MetaRemoteDesktopDeviceTypes = u32;

/// Session handle to a remote desktop control session.
pub struct MetaRemoteDesktopSession {
    /// Supported device types for this session.
    pub device_types: MetaRemoteDesktopDeviceTypes,
    /// Session object path for D-Bus.
    pub object_path: alloc::string::String,
    /// Input event queue (opaque event list).
    pub event_queue: *mut c_void,
}

impl MetaRemoteDesktopSession {
    pub fn new(device_types: MetaRemoteDesktopDeviceTypes) -> Self {
        MetaRemoteDesktopSession {
            device_types,
            object_path: alloc::string::String::new(),
            event_queue: core::ptr::null_mut(),
        }
    }

    pub fn get_device_types(&self) -> MetaRemoteDesktopDeviceTypes {
        self.device_types
    }
}

impl Default for MetaRemoteDesktopSession {
    fn default() -> Self {
        Self::new(META_REMOTE_DESKTOP_DEVICE_TYPE_NONE)
    }
}

/// Main remote desktop object managing sessions and input injection.
pub struct MetaRemoteDesktop {
    /// Reference to the backend (opaque).
    pub backend: *mut c_void,
    /// Whether remote desktop is enabled.
    pub enabled: bool,
    /// Active remote desktop sessions.
    pub sessions: Vec<*mut MetaRemoteDesktopSession>,
}

impl MetaRemoteDesktop {
    pub fn new() -> Self {
        MetaRemoteDesktop {
            backend: core::ptr::null_mut(),
            enabled: false,
            sessions: Vec::new(),
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for MetaRemoteDesktop {
    fn default() -> Self {
        Self::new()
    }
}
