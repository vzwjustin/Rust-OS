//! Input Mapper — ported from GNOME Mutter
//!
//! Device-to-output mapping for tablets and touch devices. Handles matching input
//! devices to logical monitors based on EDID, size, and configuration hints.
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

/// Input device mapper for associating devices with outputs.
pub struct InputMapper {
    // backend reference
    // monitor_manager reference
    // seat reference
    // input_devices: GHashTable
    // output_devices: GHashTable
}

impl InputMapper {
    pub fn new() -> Self {
        InputMapper {}
    }
}

impl Default for InputMapper {
    fn default() -> Self {
        Self::new()
    }
}
