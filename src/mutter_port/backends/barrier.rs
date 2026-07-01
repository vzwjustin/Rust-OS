//! GNOME Mutter's src/backends/meta-barrier.c
//!
//! Pointer barriers: invisible line segments that block pointer movement in a
//! given direction, emitting "hit"/"left" events when crossed. This port also
//! folds in the geometry helpers from src/core/meta-border.c (line/border math
//! and blocking-direction logic) that the barrier relies on.
//!
//! Stubbed: GObject signals ("hit"/"left") and the backend-native
//! MetaBarrierImpl (which does the real event delivery via evdev) are not
//! available. The barrier keeps its border and flags; hit-testing geometry is
//! ported faithfully, and impls are modeled as a trait.
//!
//! Reference:
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-barrier.c

use alloc::boxed::Box;

/// MetaBarrierDirection — kept in sync with the border motion directions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarrierDirection(pub u32);

impl BarrierDirection {
    pub const POSITIVE_X: u32 = 1 << 0;
    pub const POSITIVE_Y: u32 = 1 << 1;
    pub const NEGATIVE_X: u32 = 1 << 2;
    pub const NEGATIVE_Y: u32 = 1 << 3;
}

/// MetaBarrierFlags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BarrierFlags(pub u32);

impl BarrierFlags {
    pub const NONE: u32 = 1 << 0;
    pub const STICKY: u32 = 1 << 1;
}

/// Border motion direction bitmask (MetaBorderMotionDirection). Values match
/// BarrierDirection by design (G_STATIC_ASSERT in the C).
pub mod motion_direction {
    pub const POSITIVE_X: u32 = 1 << 0;
    pub const POSITIVE_Y: u32 = 1 << 1;
    pub const NEGATIVE_X: u32 = 1 << 2;
    pub const NEGATIVE_Y: u32 = 1 << 3;
    pub const ALL: u32 = POSITIVE_X | POSITIVE_Y | NEGATIVE_X | NEGATIVE_Y;
}

/// MetaVector2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vector2 {
    pub x: f32,
    pub y: f32,
}

impl Vector2 {
    pub fn new(x: f32, y: f32) -> Self {
        Vector2 { x, y }
    }

    /// meta_vector2_subtract()
    pub fn subtract(a: Vector2, b: Vector2) -> Vector2 {
        Vector2 {
            x: a.x - b.x,
            y: a.y - b.y,
        }
    }

    /// meta_vector2_add()
    pub fn add(a: Vector2, b: Vector2) -> Vector2 {
        Vector2 {
            x: a.x + b.x,
            y: a.y + b.y,
        }
    }

    /// meta_vector2_cross_product()
    pub fn cross(a: Vector2, b: Vector2) -> f32 {
        a.x * b.y - a.y * b.x
    }

    /// meta_vector2_multiply_constant()
    pub fn multiply_constant(c: f32, a: Vector2) -> Vector2 {
        Vector2 {
            x: c * a.x,
            y: c * a.y,
        }
    }
}

/// MetaLine2.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Line2 {
    pub a: Vector2,
    pub b: Vector2,
}

impl Line2 {
    /// meta_line2_intersects_with(): returns the intersection point if the two
    /// segments cross, else None. Faithful port of the cross-product method.
    pub fn intersects_with(line1: &Line2, line2: &Line2) -> Option<Vector2> {
        let p = line1.a;
        let r = Vector2::subtract(line1.b, line1.a);
        let q = line2.a;
        let s = Vector2::subtract(line2.b, line2.a);

        let rxs = Vector2::cross(r, s);
        let sxr = Vector2::cross(s, r);

        // If r × s = 0 the lines are parallel or collinear.
        if libm_fabsf(rxs) < f32::MIN_POSITIVE {
            return None;
        }

        let t = Vector2::cross(Vector2::subtract(q, p), s) / rxs;
        let u = Vector2::cross(Vector2::subtract(p, q), r) / sxr;

        // Only intersect if 0 ≤ t ≤ 1 and 0 ≤ u ≤ 1.
        if !(0.0..=1.0).contains(&t) || !(0.0..=1.0).contains(&u) {
            return None;
        }

        Some(Vector2::add(p, Vector2::multiply_constant(t, r)))
    }
}

/// meta_border: a line segment plus the set of blocked motion directions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Border {
    pub line: Line2,
    pub blocking_directions: u32,
}

impl Border {
    /// meta_border_is_horizontal()
    pub fn is_horizontal(&self) -> bool {
        self.line.a.y == self.line.b.y
    }

    /// meta_border_is_blocking_directions()
    pub fn is_blocking_directions(&self, directions: u32) -> bool {
        if self.is_horizontal() {
            if directions & (motion_direction::POSITIVE_Y | motion_direction::NEGATIVE_Y) == 0 {
                return false;
            }
        } else if directions & (motion_direction::POSITIVE_X | motion_direction::NEGATIVE_X) == 0 {
            return false;
        }

        (!self.blocking_directions & directions) != directions
    }

    /// meta_border_get_allows_directions()
    pub fn get_allows_directions(&self) -> u32 {
        !self.blocking_directions & motion_direction::ALL
    }

    /// meta_border_set_allows_directions()
    pub fn set_allows_directions(&mut self, directions: u32) {
        self.blocking_directions = !directions & motion_direction::ALL;
    }
}

/// MetaBarrierEvent. Ported without the atomic ref-count (Rust owns the value).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BarrierEvent {
    pub event_id: i32,
    pub dt: i32,
    pub time: u32,
    pub x: f64,
    pub y: f64,
    pub dx: f64,
    pub dy: f64,
    pub released: bool,
    pub grabbed: bool,
}

/// Backend-native barrier implementation (MetaBarrierImpl). The real impl
/// delivers events off evdev motion; here it is a trait to be provided later.
pub trait BarrierImpl {
    /// is_active vfunc
    fn is_active(&self) -> bool;
    /// release vfunc
    fn release(&mut self, event: &BarrierEvent);
    /// destroy vfunc
    fn destroy(&mut self);
}

/// MetaBarrier. Mirrors MetaBarrierPrivate (minus the GObject backend pointer,
/// which is not represented in the kernel).
pub struct Barrier {
    border: Border,
    flags: u32,
    impl_: Option<Box<dyn BarrierImpl>>,
}

impl Barrier {
    /// meta_barrier_new()
    ///
    /// Requires the barrier be axis-aligned (horizontal or vertical) with
    /// non-negative coordinates, matching init_barrier_impl()'s assertions.
    /// Returns Err on invalid geometry (the GError path).
    pub fn new(
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        directions: u32,
        flags: u32,
    ) -> Result<Self, &'static str> {
        if !(x1 == x2 || y1 == y2) {
            return Err("barrier must be horizontal or vertical");
        }
        if x1 < 0 || y1 < 0 || x2 < 0 || y2 < 0 {
            return Err("barrier coordinates must be non-negative");
        }

        let mut border = Border {
            line: Line2 {
                a: Vector2::new(x1 as f32, y1 as f32),
                b: Vector2::new(x2 as f32, y2 as f32),
            },
            blocking_directions: 0,
        };
        border.set_allows_directions(directions);

        Ok(Barrier {
            border,
            flags,
            impl_: None,
        })
    }

    /// Attach the backend-native implementation (init_barrier_impl()).
    pub fn set_impl(&mut self, impl_: Box<dyn BarrierImpl>) {
        self.impl_ = Some(impl_);
    }

    /// meta_barrier_is_active()
    pub fn is_active(&self) -> bool {
        self.impl_.as_ref().map_or(false, |i| i.is_active())
    }

    /// meta_barrier_release()
    pub fn release(&mut self, event: &BarrierEvent) {
        if let Some(i) = self.impl_.as_mut() {
            i.release(event);
        }
    }

    /// meta_barrier_destroy()
    pub fn destroy(&mut self) {
        if let Some(mut i) = self.impl_.take() {
            i.destroy();
        }
    }

    /// meta_barrier_get_border()
    pub fn border(&self) -> &Border {
        &self.border
    }

    /// meta_barrier_get_flags()
    pub fn flags(&self) -> u32 {
        self.flags
    }

    /// meta_barrier_emit_hit_signal() — signal delivery is stubbed; this just
    /// returns the event a listener would receive.
    pub fn emit_hit_signal(&self, event: BarrierEvent) -> BarrierEvent {
        event
    }

    /// meta_barrier_emit_left_signal() — see emit_hit_signal().
    pub fn emit_left_signal(&self, event: BarrierEvent) -> BarrierEvent {
        event
    }
}

/// Minimal no_std fabsf (avoids pulling in an external libm dependency here).
fn libm_fabsf(v: f32) -> f32 {
    f32::from_bits(v.to_bits() & 0x7fff_ffff)
}
