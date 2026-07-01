//! Input Mapper — ported from GNOME Mutter
//!
//! Device-to-output mapping for tablets and touch devices. Handles matching input
//! devices to logical monitors based on EDID, size, and configuration hints.
//! Maintains hash tables of input and output device mappings with D-Bus name tracking.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-input-mapper.c

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaOutputMatchType {
    META_MATCH_EDID_VENDOR = 0,
    META_MATCH_EDID_PARTIAL = 1,
    META_MATCH_EDID_FULL = 2,
    META_MATCH_SIZE = 3,
    META_MATCH_IS_BUILTIN = 4,
    META_MATCH_CONFIG = 5,
}

pub const MAX_SIZE_MATCH_DIFF: f64 = 0.05;

pub struct MetaBackend {
    // Opaque backend type
}

pub struct MetaMonitorManager {
    // Opaque monitor manager type
}

pub struct ClutterSeat {
    // Opaque Clutter seat type
}

/// Input device mapper for associating input devices (tablets, touch) with outputs.
/// Maintains mappings between ClutterInputDevice and MetaLogicalMonitor via D-Bus.
pub struct InputMapper {
    pub backend: *mut MetaBackend,
    pub monitor_manager: *mut MetaMonitorManager,
    pub seat: *mut ClutterSeat,
    pub input_devices: *mut core::ffi::c_void,  // GHashTable<ClutterInputDevice, MetaMapperInputInfo>
    pub output_devices: *mut core::ffi::c_void, // GHashTable<MetaLogicalMonitor, MetaMapperOutputInfo>
    pub dbus_name_id: u32,
}

impl InputMapper {
    pub fn new() -> Self {
        InputMapper {
            backend: core::ptr::null_mut(),
            monitor_manager: core::ptr::null_mut(),
            seat: core::ptr::null_mut(),
            input_devices: core::ptr::null_mut(),
            output_devices: core::ptr::null_mut(),
            dbus_name_id: 0,
        }
    }
}

impl Default for InputMapper {
    fn default() -> Self {
        Self::new()
    }
}
