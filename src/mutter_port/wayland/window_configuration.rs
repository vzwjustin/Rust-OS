//! GNOME src/wayland/meta-wayland-window-configuration.c
//!
//! MetaWaylandWindowConfiguration captures a single pending configure sent to a
//! Wayland client. Each configuration carries a monotonic `serial`; the client
//! must echo it back via ack_configure before the compositor applies the new
//! size/position. The window keeps a queue of un-acked configurations and drops
//! the stale ones once a serial is acknowledged.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-window-configuration.c

use core::sync::atomic::{AtomicU32, Ordering};

/// Global monotonic serial counter (C: `global_serial_counter`).
static SERIAL_COUNTER: AtomicU32 = AtomicU32::new(0);

fn next_serial() -> u32 {
    SERIAL_COUNTER.fetch_add(1, Ordering::Relaxed) + 1
}

/// Window gravity used when resizing (which edge/corner stays fixed).
/// Mirrors MetaGravity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gravity {
    None,
    NorthWest,
    North,
    NorthEast,
    West,
    Center,
    East,
    SouthWest,
    South,
    SouthEast,
    Static,
}

/// Move/resize action flags (subset of MetaMoveResizeFlags).
pub mod move_resize_flags {
    pub const NONE: u32 = 0;
    pub const MOVE_ACTION: u32 = 1 << 0;
    pub const RESIZE_ACTION: u32 = 1 << 1;
    pub const STATE_CHANGED: u32 = 1 << 2;
    pub const CONSTRAIN: u32 = 1 << 3;
}

/// A single pending configure event for a Wayland window.
#[derive(Debug, Clone)]
pub struct MetaWaylandWindowConfiguration {
    pub serial: u32,

    pub has_position: bool,
    pub x: i32,
    pub y: i32,

    /// STUB: window_drag reference (compositor drag op) modeled as an id.
    pub window_drag_id: Option<u32>,

    pub has_relative_position: bool,
    pub rel_x: i32,
    pub rel_y: i32,

    pub has_size: bool,
    pub is_resizing: bool,
    pub width: i32,
    pub height: i32,

    pub scale: i32,
    pub gravity: Gravity,
    pub flags: u32,

    pub bounds_width: i32,
    pub bounds_height: i32,

    pub is_suspended: bool,
}

impl MetaWaylandWindowConfiguration {
    fn base(serial: u32) -> Self {
        MetaWaylandWindowConfiguration {
            serial,
            has_position: false,
            x: 0,
            y: 0,
            window_drag_id: None,
            has_relative_position: false,
            rel_x: 0,
            rel_y: 0,
            has_size: false,
            is_resizing: false,
            width: 0,
            height: 0,
            scale: 1,
            gravity: Gravity::None,
            flags: move_resize_flags::NONE,
            bounds_width: 0,
            bounds_height: 0,
            is_suspended: false,
        }
    }

    /// C: meta_wayland_window_configuration_new().
    ///
    /// `rect` is the requested (x, y, width, height). A position is only
    /// recorded for a move action or when the window is not floating (here
    /// approximated by `force_position`). A size is recorded when the rect is
    /// non-empty.
    ///
    /// STUB: floating/suspended state comes from MetaWindowConfig in mutter.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        rect: (i32, i32, i32, i32),
        scale: i32,
        flags: u32,
        gravity: Gravity,
        is_suspended: bool,
        force_position: bool,
    ) -> Self {
        let (x, y, width, height) = rect;
        let mut c = Self::base(next_serial());
        c.scale = scale;
        c.flags = flags;
        c.gravity = gravity;
        c.is_suspended = is_suspended;

        if (flags & move_resize_flags::MOVE_ACTION) != 0 || force_position {
            c.has_position = true;
            c.x = x;
            c.y = y;
        }

        c.has_size = width > 0 && height > 0;
        if (flags & move_resize_flags::RESIZE_ACTION) != 0 {
            c.is_resizing = true;
        }
        c.width = width;
        c.height = height;
        c
    }

    /// C: meta_wayland_window_configuration_new_relative().
    pub fn new_relative(rel_x: i32, rel_y: i32, width: i32, height: i32, scale: i32) -> Self {
        let mut c = Self::base(next_serial());
        c.has_relative_position = true;
        c.rel_x = rel_x;
        c.rel_y = rel_y;
        c.has_size = width > 0 && height > 0;
        c.width = width;
        c.height = height;
        c.scale = scale;
        c
    }

    /// C: meta_wayland_window_configuration_new_empty().
    pub fn new_empty(bounds_width: i32, bounds_height: i32, scale: i32) -> Self {
        let mut c = Self::base(next_serial());
        c.bounds_width = bounds_width;
        c.bounds_height = bounds_height;
        c.scale = scale;
        c
    }

    /// C: meta_wayland_window_configuration_new_from_other().
    /// Copies geometry but allocates a fresh serial.
    pub fn new_from_other(other: &MetaWaylandWindowConfiguration) -> Self {
        let mut c = other.clone();
        c.serial = next_serial();
        c.window_drag_id = None;
        c
    }

    /// C: meta_wayland_window_configuration_is_equivalent().
    /// Compares everything except the serial.
    pub fn is_equivalent(&self, other: &MetaWaylandWindowConfiguration) -> bool {
        self.has_position == other.has_position
            && self.x == other.x
            && self.y == other.y
            && self.has_relative_position == other.has_relative_position
            && self.rel_x == other.rel_x
            && self.rel_y == other.rel_y
            && self.has_size == other.has_size
            && self.width == other.width
            && self.height == other.height
            && self.scale == other.scale
            && self.flags == other.flags
            && self.bounds_width == other.bounds_width
            && self.bounds_height == other.bounds_height
            && self.is_suspended == other.is_suspended
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serial_is_monotonic() {
        let a = MetaWaylandWindowConfiguration::new_empty(0, 0, 1);
        let b = MetaWaylandWindowConfiguration::new_empty(0, 0, 1);
        assert!(b.serial > a.serial);
    }

    #[test]
    fn test_new_records_size_and_move_position() {
        let c = MetaWaylandWindowConfiguration::new(
            (10, 20, 640, 480),
            1,
            move_resize_flags::MOVE_ACTION,
            Gravity::NorthWest,
            false,
            false,
        );
        assert!(c.has_position);
        assert_eq!((c.x, c.y), (10, 20));
        assert!(c.has_size);
        assert_eq!((c.width, c.height), (640, 480));
        assert!(!c.is_resizing);
    }

    #[test]
    fn test_resize_flag_sets_is_resizing() {
        let c = MetaWaylandWindowConfiguration::new(
            (0, 0, 100, 100),
            1,
            move_resize_flags::RESIZE_ACTION,
            Gravity::SouthEast,
            false,
            false,
        );
        assert!(c.is_resizing);
        assert!(!c.has_position);
    }

    #[test]
    fn test_relative() {
        let c = MetaWaylandWindowConfiguration::new_relative(5, 7, 200, 150, 2);
        assert!(c.has_relative_position);
        assert_eq!((c.rel_x, c.rel_y), (5, 7));
        assert!(c.has_size);
        assert_eq!(c.scale, 2);
    }

    #[test]
    fn test_from_other_reserials_but_equivalent() {
        let a = MetaWaylandWindowConfiguration::new_relative(1, 2, 300, 300, 1);
        let b = MetaWaylandWindowConfiguration::new_from_other(&a);
        assert_ne!(a.serial, b.serial);
        assert!(a.is_equivalent(&b));
    }

    #[test]
    fn test_not_equivalent_on_size_change() {
        let a = MetaWaylandWindowConfiguration::new_empty(0, 0, 1);
        let mut b = MetaWaylandWindowConfiguration::new_from_other(&a);
        b.width = 42;
        b.has_size = true;
        assert!(!a.is_equivalent(&b));
    }
}
