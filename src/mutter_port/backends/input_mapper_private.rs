//! Input Mapper Private ported from GNOME Mutter's src/backends/
//!
//! Maps input devices to logical monitors using EDID-based matching and size heuristics.
//! Provides device-to-monitor assignment with 5%-tolerance size matching and configuration storage.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-input-mapper-private.c

use core::cmp::Ordering;

/// D-Bus Input Mapping skeleton base type (opaque, D-Bus I/O bound).
pub struct DBusInputMappingSkeleton;

/// Output match type for input device mapping (EDID vendor/model, size, builtin, or config).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaOutputMatchType {
    /// EDID vendor code match (e.g. "WAC" for Wacom).
    META_MATCH_EDID_VENDOR = 0,
    /// Partial EDID model match (e.g. "Cintiq").
    META_MATCH_EDID_PARTIAL = 1,
    /// Full EDID model match (e.g. "Cintiq 12WX").
    META_MATCH_EDID_FULL = 2,
    /// Size match from input device and output physical dimensions (5% tolerance).
    META_MATCH_SIZE = 3,
    /// Output is builtin (system-integrated device).
    META_MATCH_IS_BUILTIN = 4,
    /// Explicit configuration mapping.
    META_MATCH_CONFIG = 5,
}

impl MetaOutputMatchType {
    /// Scoring priority: higher match types score lower (better matches first).
    /// Returns a score suitable for sorting in ascending order.
    pub fn score(&self) -> u32 {
        match self {
            MetaOutputMatchType::META_MATCH_CONFIG => 0,          // Highest priority
            MetaOutputMatchType::META_MATCH_EDID_FULL => 1,
            MetaOutputMatchType::META_MATCH_EDID_PARTIAL => 2,
            MetaOutputMatchType::META_MATCH_IS_BUILTIN => 3,
            MetaOutputMatchType::META_MATCH_EDID_VENDOR => 4,
            MetaOutputMatchType::META_MATCH_SIZE => 5,            // Lowest priority
        }
    }
}

impl PartialOrd for MetaOutputMatchType {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for MetaOutputMatchType {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score().cmp(&other.score())
    }
}

/// Input device to output mapping score threshold (5% size tolerance).
pub const META_INPUT_MAPPING_SIZE_TOLERANCE: f32 = 0.05;

/// Input mapper for device-to-monitor assignment.
///
/// Maps input devices (keyboard, pointer, touch) to logical monitors using:
/// - EDID vendor/model matching
/// - Physical size matching (5% tolerance)
/// - Builtin device heuristics
/// - Explicit user configuration
pub struct MetaInputMapper {
    /// D-Bus skeleton (opaque).
    pub dbus: DBusInputMappingSkeleton,
}

impl MetaInputMapper {
    /// Create a new input mapper.
    pub fn new() -> Self {
        MetaInputMapper {
            dbus: DBusInputMappingSkeleton,
        }
    }

    /// Add an input device to the mapper.
    pub fn add_device(&mut self, _device_id: u32) {
        // TODO: Implement D-Bus device tracking and mapping
    }

    /// Remove an input device from the mapper.
    pub fn remove_device(&mut self, _device_id: u32) {
        // TODO: Implement D-Bus device removal
    }

    /// Get the logical monitor assigned to a device (D-Bus/hardware bound).
    pub fn get_device_logical_monitor(&self, _device_id: u32) -> Option<u32> {
        // TODO: Implement device-to-monitor query via D-Bus
        None
    }

    /// Get the device assigned to a logical monitor (D-Bus/hardware bound).
    pub fn get_logical_monitor_device(&self, _monitor_id: u32) -> Option<u32> {
        // TODO: Implement monitor-to-device query via D-Bus
        None
    }
}

impl Default for MetaInputMapper {
    fn default() -> Self {
        Self::new()
    }
}
