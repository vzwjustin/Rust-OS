//! GNOME src/wayland/meta-wayland-fractional-scale.c
//!
//! Implements wp_fractional_scale_manager_v1 / wp_fractional_scale_v1. Lets a
//! client learn the preferred non-integer scale of a surface. The wire value is
//! `round(scale * 120)` (120ths of the logical pixel), and the compositor only
//! resends when the value actually changes.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-fractional-scale.c

use alloc::collections::BTreeMap;

/// Convert a floating scale to the wire representation (120ths).
pub fn scale_to_wire(scale: f64) -> u32 {
    // round(scale * 120)
    let v = scale * 120.0;
    (v + 0.5) as u32
}

/// Per-surface fractional scale state (wp_fractional_scale_v1).
#[derive(Debug, Clone, Copy)]
pub struct FractionalScale {
    pub surface_id: u32,
    /// Last scale sent to the client, or 0.0 if none sent yet.
    pub scale: f64,
}

impl FractionalScale {
    pub fn new(surface_id: u32) -> Self {
        FractionalScale {
            surface_id,
            scale: 0.0,
        }
    }
}

/// Manages fractional scale objects, keyed by surface id.
pub struct FractionalScaleManager {
    scales: BTreeMap<u32, FractionalScale>,
}

impl FractionalScaleManager {
    pub fn new() -> Self {
        FractionalScaleManager {
            scales: BTreeMap::new(),
        }
    }

    /// wp_fractional_scale_manager_v1.get_fractional_scale. Returns false if a
    /// fractional_scale object already exists for the surface (protocol error).
    pub fn get_fractional_scale(&mut self, surface_id: u32) -> bool {
        if self.scales.contains_key(&surface_id) {
            return false;
        }
        self.scales
            .insert(surface_id, FractionalScale::new(surface_id));
        true
    }

    pub fn has_scale(&self, surface_id: u32) -> bool {
        self.scales.contains_key(&surface_id)
    }

    /// meta_wayland_fractional_scale_maybe_send_preferred_scale.
    ///
    /// Returns `Some(wire_scale)` if a `preferred_scale` event should be sent,
    /// or `None` if there is no object or the value is unchanged/zero.
    ///
    /// STUB: the caller is responsible for actually emitting the wire event.
    pub fn maybe_send_preferred_scale(&mut self, surface_id: u32, scale: f64) -> Option<u32> {
        let fs = self.scales.get_mut(&surface_id)?;
        const EPS: f64 = 1.0e-6;
        if scale.abs() < EPS || (scale - fs.scale).abs() < EPS {
            return None;
        }
        fs.scale = scale;
        Some(scale_to_wire(scale))
    }

    pub fn destroy(&mut self, surface_id: u32) -> bool {
        self.scales.remove(&surface_id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wire_rounding() {
        assert_eq!(scale_to_wire(1.0), 120);
        assert_eq!(scale_to_wire(1.5), 180);
        assert_eq!(scale_to_wire(2.25), 270);
    }

    #[test]
    fn test_get_is_unique() {
        let mut mgr = FractionalScaleManager::new();
        assert!(mgr.get_fractional_scale(1));
        assert!(!mgr.get_fractional_scale(1));
    }

    #[test]
    fn test_maybe_send_dedup() {
        let mut mgr = FractionalScaleManager::new();
        mgr.get_fractional_scale(1);
        assert_eq!(mgr.maybe_send_preferred_scale(1, 1.5), Some(180));
        // Unchanged -> no resend.
        assert_eq!(mgr.maybe_send_preferred_scale(1, 1.5), None);
        // Changed -> resend.
        assert_eq!(mgr.maybe_send_preferred_scale(1, 2.0), Some(240));
        // Zero -> ignored.
        assert_eq!(mgr.maybe_send_preferred_scale(1, 0.0), None);
    }

    #[test]
    fn test_no_object() {
        let mut mgr = FractionalScaleManager::new();
        assert_eq!(mgr.maybe_send_preferred_scale(9, 1.5), None);
    }
}
