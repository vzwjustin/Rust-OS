//! GNOME src/wayland/meta-window-xwayland.c
//!
//! MetaWindowXwayland is a MetaWindowX11 subclass representing an X11 client
//! window managed through Xwayland. It couples the X11 window to its backing
//! `wl_surface`, manages commit freeze/thaw (to avoid tearing on resize),
//! keyboard-grab policy, and the stage<->protocol coordinate scaling that
//! bridges Xwayland's integer-scaled world to the compositor stage.
//! The surface is referenced by id (`u32`) for loose coupling.
//!
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-window-xwayland.c

/// Rounding strategy for stage<->protocol coordinate conversions
/// (`MtkRoundingStrategy`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundingStrategy {
    /// Round to nearest.
    Round,
    /// Round toward zero (shrink).
    Shrink,
    /// Round away from zero (grow) — used for sizes.
    Grow,
}

/// MetaWindowXwayland — an X11 window backed by a Wayland surface.
pub struct MetaWindowXwayland {
    /// This window's id (X11 window / MetaWindow id).
    pub id: u32,
    /// Backing `wl_surface` id, set once Xwayland associates the window.
    pub surface_id: Option<u32>,
    /// Whether this window may grab the keyboard (`_XWAYLAND_MAY_GRAB_KEYBOARD`).
    pub xwayland_may_grab_keyboard: bool,
    /// Nested freeze counter; commits are blocked while > 0.
    pub freeze_count: i32,
    /// Effective integer scale applied to Xwayland geometry.
    pub effective_scale: i32,
    /// Cached last configured protocol size (width, height).
    pub protocol_size: (i32, i32),
}

impl MetaWindowXwayland {
    pub fn new(id: u32) -> Self {
        MetaWindowXwayland {
            id,
            surface_id: None,
            xwayland_may_grab_keyboard: false,
            freeze_count: 0,
            effective_scale: 1,
            protocol_size: (0, 0),
        }
    }

    /// Associate the backing Wayland surface (`meta_window_xwayland_set_surface`).
    pub fn set_surface(&mut self, surface_id: Option<u32>) {
        self.surface_id = surface_id;
    }

    pub fn get_wayland_surface(&self) -> Option<u32> {
        self.surface_id
    }

    /// True while commits are frozen (window actor should not update buffer).
    pub fn commits_frozen(&self) -> bool {
        self.freeze_count > 0
    }

    /// Freeze commits; the first freeze clears the X11 allow-commits property.
    /// Returns `true` if this transition actually blocked commits.
    pub fn freeze_commits(&mut self) -> bool {
        let first = self.freeze_count == 0;
        self.freeze_count += 1;
        // STUB: on `first`, set the `_XWAYLAND_ALLOW_COMMITS` X11 property to
        // FALSE so Xwayland stops committing buffers during the operation.
        first
    }

    /// Thaw commits; the last thaw restores the X11 allow-commits property.
    /// Returns `true` if commits became allowed again. Panics on underflow in
    /// debug builds, mirroring mutter's `g_return_if_fail (freeze_count > 0)`.
    pub fn thaw_commits(&mut self) -> bool {
        debug_assert!(self.freeze_count > 0, "thaw without matching freeze");
        if self.freeze_count == 0 {
            return false;
        }
        self.freeze_count -= 1;
        let thawed = self.freeze_count == 0;
        // STUB: on `thawed`, set `_XWAYLAND_ALLOW_COMMITS` back to TRUE.
        thawed
    }

    /// Xwayland always updates window shape to avoid black shadows on resize.
    pub fn always_update_shape(&self) -> bool {
        true
    }

    /// Convert a stage coordinate to an Xwayland protocol coordinate.
    pub fn stage_to_protocol(&self, stage: i32, strategy: RoundingStrategy) -> i32 {
        scale_and_round(stage, self.effective_scale as f32, strategy)
    }

    /// Convert an Xwayland protocol coordinate back to a stage coordinate.
    pub fn protocol_to_stage(&self, protocol: i32, strategy: RoundingStrategy) -> i32 {
        let inv = 1.0 / self.effective_scale.max(1) as f32;
        scale_and_round(protocol, inv, strategy)
    }

    /// Record the last configured protocol size (used on `configure`).
    pub fn set_protocol_size(&mut self, width: i32, height: i32) {
        self.protocol_size = (width.max(0), height.max(0));
    }
}

/// Scale `value` and round according to `strategy`, saturating on overflow
/// (`scale_and_handle_overflow`).
fn scale_and_round(value: i32, scale: f32, strategy: RoundingStrategy) -> i32 {
    let scaled = value as f32 * scale;
    let rounded = match strategy {
        RoundingStrategy::Round => {
            if scaled >= 0.0 {
                (scaled + 0.5) as i64 as f32
            } else {
                (scaled - 0.5) as i64 as f32
            }
        }
        RoundingStrategy::Shrink => scaled as i64 as f32,
        RoundingStrategy::Grow => {
            let t = scaled as i64 as f32;
            if scaled > t {
                t + 1.0
            } else if scaled < t {
                t - 1.0
            } else {
                t
            }
        }
    };
    if rounded > i32::MAX as f32 {
        i32::MAX
    } else if rounded < i32::MIN as f32 {
        i32::MIN
    } else {
        rounded as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surface_association() {
        let mut w = MetaWindowXwayland::new(1);
        assert_eq!(w.get_wayland_surface(), None);
        w.set_surface(Some(42));
        assert_eq!(w.get_wayland_surface(), Some(42));
    }

    #[test]
    fn test_freeze_thaw_nesting() {
        let mut w = MetaWindowXwayland::new(1);
        assert!(w.freeze_commits()); // first freeze blocks
        assert!(!w.freeze_commits()); // nested freeze no-op
        assert!(w.commits_frozen());
        assert!(!w.thaw_commits()); // still frozen
        assert!(w.thaw_commits()); // last thaw unblocks
        assert!(!w.commits_frozen());
    }

    #[test]
    fn test_stage_to_protocol_scale() {
        let mut w = MetaWindowXwayland::new(1);
        w.effective_scale = 2;
        assert_eq!(w.stage_to_protocol(100, RoundingStrategy::Round), 200);
        assert_eq!(w.protocol_to_stage(200, RoundingStrategy::Round), 100);
    }

    #[test]
    fn test_grow_rounding() {
        let mut w = MetaWindowXwayland::new(1);
        w.effective_scale = 1;
        // 3 * 1.0 stays 3; check grow rounds up fractional inputs.
        assert_eq!(scale_and_round(3, 1.5, RoundingStrategy::Grow), 5);
        assert_eq!(scale_and_round(3, 1.5, RoundingStrategy::Shrink), 4);
    }

    #[test]
    fn test_always_update_shape() {
        let w = MetaWindowXwayland::new(1);
        assert!(w.always_update_shape());
    }
}
