//! Eis — Input Event System integration from GNOME Mutter
//!
//! Manages the EventIS (EIS) protocol for synchronized input device access
//! across multiple clients via the EIS D-Bus interface and libeis.
//! Types here define device capabilities and viewports; libei I/O is left as TODO.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-eis.h

/// EIS device type flags (bitmask).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaEisDeviceTypes {
    META_EIS_DEVICE_TYPE_NONE = 0,
    META_EIS_DEVICE_TYPE_KEYBOARD = 1 << 0,
    META_EIS_DEVICE_TYPE_POINTER = 1 << 1,
    META_EIS_DEVICE_TYPE_TOUCHSCREEN = 1 << 2,
}

/// MetaEis — Root EIS object managing clients, viewports, and device state.
/// Wraps libeis lifecycle and coordinates with MetaBackend.
pub struct MetaEis {
    // TODO: port fields from meta-eis.c
}

impl MetaEis {
    pub fn new() -> Self {
        MetaEis {}
    }
}

impl Default for MetaEis {
    fn default() -> Self {
        Self::new()
    }
}
