//! Remote Desktop — ported from GNOME Mutter
//!
//! Provides remote desktop control (keyboard, pointer, touchscreen input injection),
//! complementing screen casting to allow bi-directional control of desktop sessions.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-remote-desktop.h

/// Device types that can be controlled via remote desktop.
pub const META_REMOTE_DESKTOP_DEVICE_TYPE_NONE: u32 = 0;
pub const META_REMOTE_DESKTOP_DEVICE_TYPE_KEYBOARD: u32 = 1 << 0;
pub const META_REMOTE_DESKTOP_DEVICE_TYPE_POINTER: u32 = 1 << 1;
pub const META_REMOTE_DESKTOP_DEVICE_TYPE_TOUCHSCREEN: u32 = 1 << 2;

/// Type alias for device type bitmask.
pub type MetaRemoteDesktopDeviceTypes = u32;

/// Session handle to a remote desktop control session.
pub struct MetaRemoteDesktopSession {
    device_types: MetaRemoteDesktopDeviceTypes,
    // TODO: Input state, event queue from C implementation
}

impl MetaRemoteDesktopSession {
    pub fn new(device_types: MetaRemoteDesktopDeviceTypes) -> Self {
        MetaRemoteDesktopSession { device_types }
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
    // TODO: Backend reference, enabled flag, session tracking from C implementation
}

impl MetaRemoteDesktop {
    pub fn new() -> Self {
        MetaRemoteDesktop {}
    }

    pub fn is_enabled(&self) -> bool {
        // TODO: Check runtime enable/disable flag
        false
    }
}

impl Default for MetaRemoteDesktop {
    fn default() -> Self {
        Self::new()
    }
}