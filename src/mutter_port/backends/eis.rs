//! Eis — Input Event System integration from GNOME Mutter
//!
//! Manages the EventIS (EIS) protocol for synchronized input device access
//! across multiple clients via the EIS D-Bus interface and libeis.
//! Types here define device capabilities and viewports; libei I/O is left as TODO.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-eis.h

/// EIS device type flags (bitmask). A type alias + consts (rather than an
/// `enum`) so the values can be combined with bitwise OR, matching upstream.
pub type MetaEisDeviceTypes = u32;

pub const META_EIS_DEVICE_TYPE_NONE: MetaEisDeviceTypes = 0;
pub const META_EIS_DEVICE_TYPE_KEYBOARD: MetaEisDeviceTypes = 1 << 0;
pub const META_EIS_DEVICE_TYPE_POINTER: MetaEisDeviceTypes = 1 << 1;
pub const META_EIS_DEVICE_TYPE_TOUCHSCREEN: MetaEisDeviceTypes = 1 << 2;

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
