//! Eis — Input Event System integration from GNOME Mutter
//!
//! Manages the EventIS (EIS) protocol for synchronized input device access
//! across multiple clients via the EIS D-Bus interface and libeis.
//! Types here define device capabilities and viewports; libeis I/O (D-Bus
//! connection setup, event dispatch) is documented in the methods but not
//! issued here since there is no D-Bus daemon in `no_std`.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-eis.h

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

pub struct MetaBackend {
    // Opaque backend type
}

/// EIS device capability flags (bitmask). Mirrors the upstream
/// `MetaEisDeviceTypes` but is used here as a capability set: each bit
/// represents a class of input device the EIS instance is willing to
/// expose to clients. Clients query these caps to know which device types
/// they may receive events for.
pub type MetaEisDeviceCaps = u32;

pub const META_EIS_DEVICE_CAP_NONE: MetaEisDeviceCaps = 0;
pub const META_EIS_DEVICE_CAP_KEYBOARD: MetaEisDeviceCaps = 1 << 0;
pub const META_EIS_DEVICE_CAP_POINTER: MetaEisDeviceCaps = 1 << 1;
pub const META_EIS_DEVICE_CAP_TOUCHSCREEN: MetaEisDeviceCaps = 1 << 2;

/// EIS device type flags (bitmask). A type alias + consts (rather than an
/// `enum`) so the values can be combined with bitwise OR, matching upstream.
pub type MetaEisDeviceTypes = u32;

pub const META_EIS_DEVICE_TYPE_NONE: MetaEisDeviceTypes = 0;
pub const META_EIS_DEVICE_TYPE_KEYBOARD: MetaEisDeviceTypes = 1 << 0;
pub const META_EIS_DEVICE_TYPE_POINTER: MetaEisDeviceTypes = 1 << 1;
pub const META_EIS_DEVICE_TYPE_TOUCHSCREEN: MetaEisDeviceTypes = 1 << 2;

/// A viewport describing a rectangular region of the stage that EIS clients
/// may interact with. Corresponds to `MetaEisViewport` in upstream. Each
/// viewport has an x/y origin, width/height in logical pixels, and a scale
/// factor mapping logical to physical coordinates.
pub struct MetaEisViewport {
    /// Logical x origin of the viewport on the stage.
    pub x: i32,
    /// Logical y origin of the viewport on the stage.
    pub y: i32,
    /// Logical width of the viewport.
    pub width: i32,
    /// Logical height of the viewport.
    pub height: i32,
    /// Scale factor (physical pixels per logical pixel).
    pub scale: f32,
}

impl MetaEisViewport {
    /// Create a new viewport with the given geometry and scale.
    pub fn new(x: i32, y: i32, width: i32, height: i32, scale: f32) -> Self {
        MetaEisViewport {
            x,
            y,
            width,
            height,
            scale,
        }
    }
}

pub struct MetaEventSource {
    // Opaque event source type
}

/// MetaEis — Root EIS object managing clients, viewports, and device state.
/// Wraps libeis lifecycle and coordinates with MetaBackend.
pub struct MetaEis {
    pub backend: *mut MetaBackend,
    pub eis: *mut core::ffi::c_void,
    pub event_source: *mut MetaEventSource,
    /// Bitmask of device capabilities this EIS instance exposes to clients.
    /// Set with `add_device_caps` / `remove_device_caps` and queried with
    /// `has_device_cap`.
    pub device_caps: MetaEisDeviceCaps,
    /// Device type filter (upstream `device_types`).
    pub device_types: MetaEisDeviceTypes,
    /// Active viewports that EIS clients may interact with.
    pub viewports: Vec<MetaEisViewport>,
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
            device_caps: META_EIS_DEVICE_CAP_NONE,
            device_types: META_EIS_DEVICE_TYPE_NONE,
            viewports: Vec::new(),
            mapping_ids: BTreeMap::new(),
            monitors_changed_handler_id: 0,
            eis_clients: BTreeMap::new(),
            cancellable: core::ptr::null_mut(),
        }
    }

    /// Returns the current device capability bitmask.
    pub fn get_device_caps(&self) -> MetaEisDeviceCaps {
        self.device_caps
    }

    /// Adds the given capability bits to the set of exposed device caps.
    pub fn add_device_caps(&mut self, caps: MetaEisDeviceCaps) {
        self.device_caps |= caps;
    }

    /// Removes the given capability bits from the exposed device caps.
    pub fn remove_device_caps(&mut self, caps: MetaEisDeviceCaps) {
        self.device_caps &= !caps;
    }

    /// Returns `true` if all bits in `cap` are present in the device caps.
    pub fn has_device_cap(&self, cap: MetaEisDeviceCaps) -> bool {
        (self.device_caps & cap) == cap
    }

    /// Adds a viewport to the set of active viewports. A full implementation
    /// would also notify connected EIS clients of the new viewport via the
    /// libeis seat/region interface.
    pub fn add_viewport(&mut self, viewport: MetaEisViewport) {
        self.viewports.push(viewport);
    }

    /// Removes the viewport at the given index. Returns the removed viewport
    /// if the index was valid, or `None` otherwise.
    pub fn remove_viewport(&mut self, index: usize) -> Option<MetaEisViewport> {
        if index >= self.viewports.len() {
            return None;
        }
        Some(self.viewports.remove(index))
    }

    /// Returns the number of active viewports.
    pub fn viewport_count(&self) -> usize {
        self.viewports.len()
    }

    /// Returns a reference to the viewport at the given index.
    pub fn get_viewport(&self, index: usize) -> Option<&MetaEisViewport> {
        self.viewports.get(index)
    }
}

impl Default for MetaEis {
    fn default() -> Self {
        Self::new()
    }
}
