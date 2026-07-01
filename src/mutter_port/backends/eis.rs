//! Eis — Input Event System integration from GNOME Mutter
//!
//! Manages the EventIS (EIS) protocol for synchronized input device access
//! across multiple clients via the EIS D-Bus interface and libeis.
//! Types here define device capabilities and viewports; libei I/O is left as TODO.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-eis.h

use alloc::vec::Vec;
use alloc::collections::BTreeMap;

pub struct MetaBackend {
    // Opaque backend type
}

pub struct MetaEisViewport {
    // Opaque viewport type
}

pub struct MetaEventSource {
    // Opaque event source type
}

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
    pub backend: *mut MetaBackend,
    pub eis: *mut core::ffi::c_void,
    pub event_source: *mut MetaEventSource,
    pub device_types: MetaEisDeviceTypes,
    pub viewports: Vec<*mut MetaEisViewport>,
    pub mapping_ids: BTreeMap<usize, usize>,
    pub monitors_changed_handler_id: usize,
    pub eis_clients: BTreeMap<usize, *mut core::ffi::c_void>,
    pub cancellable: *mut core::ffi::c_void,
}

impl MetaEis {
    pub fn new() -> Self {
        MetaEis {
            backend: core::ptr::null_mut(),
            eis: core::ptr::null_mut(),
            event_source: core::ptr::null_mut(),
            device_types: META_EIS_DEVICE_TYPE_NONE,
            viewports: Vec::new(),
            mapping_ids: BTreeMap::new(),
            monitors_changed_handler_id: 0,
            eis_clients: BTreeMap::new(),
            cancellable: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaEis {
    fn default() -> Self {
        Self::new()
    }
}
