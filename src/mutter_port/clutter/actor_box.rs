//! Port of GNOME mutter's `clutter/clutter-actor-box.{c,h}` and
//! `clutter-actor-box-private.h`.
//!
//! `ClutterActorBox` is a float-based box defined by two corner points,
//! `(x1, y1)` (top-left) and `(x2, y2)` (bottom-right) -- unlike
//! `MtkRectangle` (ported in `mutter_port::mtk::rectangle`), which uses
//! origin + size. This port follows the same conventions established
//! there: no `unsafe`, no external crates, `core`/`alloc` only.
//!
//! Skipped, with rationale:
//! - GLib boxed-type registration (`G_DEFINE_BOXED_TYPE_WITH_CODE`,
//!   `clutter_actor_box_copy`/`_free`/`_alloc`): this is GObject reference
//!   counted heap-allocation glue. `ActorBox` is a plain `Copy` struct in
//!   Rust, so allocation/copy/free have no equivalent need.
//! - `clutter_actor_box_progress` and the `CLUTTER_REGISTER_INTERVAL_PROGRESS`
//!   hookup: this is `ClutterInterval`-specific animation glue that calls
//!   straight through to `interpolate`; callers can just call
//!   `interpolate` directly.
//! - `GValue` transform functions: GLib type-system glue with no
//!   equivalent in this Rust port.
//! - `clutter_actor_box_from_string`/`to_string`: the C source does not
//!   actually implement these for `ClutterActorBox` (unlike some other
//!   Clutter types), so there is nothing to port.
//! - `_clutter_actor_box_enlarge_for_effects`: ported below as
//!   `enlarge_for_effects` since it's a small, self-contained pixel-padding
//!   calculation with no GObject/effects-pipeline dependency baked into the
//!   math itself.
//! - `clutter_actor_box_is_initialized`: depends on IEEE 754 `isinf`/
//!   `signbit` sentinel semantics used by Clutter to mark "uninitialized"
//!   boxes (`{ -inf, -inf, +inf, +inf }`-style sentinels from
//!   `graphene`/Clutter's allocation code). `core::f32` exposes
//!   `is_infinite()`/`is_sign_negative()` so this still ports cleanly and
//!   is included below.

/// A 2D box defined by its top-left corner `(x1, y1)` and bottom-right
/// corner `(x2, y2)`.
///
/// Mirrors `ClutterActorBox` from mutter's `clutter` library.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ActorBox {
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

impl ActorBox {
    /// Creates a new `ActorBox` from the given coordinates.
    ///
    /// Mirrors `clutter_actor_box_new`.
    pub fn new(x1: f32, y1: f32, x2: f32, y2: f32) -> Self {
        ActorBox { x1, y1, x2, y2 }
    }

    /// Initializes (overwrites) `self` with the given coordinates and
    /// returns it, mirroring `clutter_actor_box_init`'s "fluent" return.
    pub fn init(&mut self, x1: f32, y1: f32, x2: f32, y2: f32) -> &mut Self {
        self.x1 = x1;
        self.y1 = y1;
        self.x2 = x2;
        self.y2 = y2;
        self
    }

    /// Initializes `self` from an origin and a size.
    ///
    /// Mirrors `clutter_actor_box_init_rect`.
    pub fn init_rect(&mut self, x: f32, y: f32, width: f32, height: f32) -> &mut Self {
        self.x1 = x;
        self.y1 = y;
        self.x2 = self.x1 + width;
        self.y2 = self.y1 + height;
        self
    }

    /// Creates an `ActorBox` from an origin and a size.
    pub fn from_rect(x: f32, y: f32, width: f32, height: f32) -> Self {
        ActorBox {
            x1: x,
            y1: y,
            x2: x + width,
            y2: y + height,
        }
    }

    /// Returns whether two boxes have the exact same coordinates.
    ///
    /// Mirrors `clutter_actor_box_equal`.
    pub fn equal(&self, other: &ActorBox) -> bool {
        self.x1 == other.x1 && self.y1 == other.y1 && self.x2 == other.x2 && self.y2 == other.y2
    }

    /// Returns the X coordinate of the origin.
    ///
    /// Mirrors `clutter_actor_box_get_x`.
    pub fn x(&self) -> f32 {
        self.x1
    }

    /// Returns the Y coordinate of the origin.
    ///
    /// Mirrors `clutter_actor_box_get_y`.
    pub fn y(&self) -> f32 {
        self.y1
    }

    /// Returns the width of the box.
    ///
    /// Mirrors `clutter_actor_box_get_width`.
    pub fn width(&self) -> f32 {
        if self.x2 > self.x1 {
            self.x2 - self.x1
        } else {
            0.0
        }
    }

    /// Returns the height of the box.
    ///
    /// Mirrors `clutter_actor_box_get_height`.
    pub fn height(&self) -> f32 {
        if self.y2 > self.y1 {
            self.y2 - self.y1
        } else {
            0.0
        }
    }

    /// Returns the `(x, y)` origin of the box.
    ///
    /// Mirrors `clutter_actor_box_get_origin`.
    pub fn origin(&self) -> (f32, f32) {
        (self.x1, self.y1)
    }

    /// Returns the `(width, height)` size of the box.
    ///
    /// Mirrors `clutter_actor_box_get_size`.
    pub fn size(&self) -> (f32, f32) {
        (self.width(), self.height())
    }

    /// Changes the origin of the box, keeping its size fixed.
    ///
    /// Mirrors `clutter_actor_box_set_origin`.
    pub fn set_origin(&mut self, x: f32, y: f32) {
        let width = self.x2 - self.x1;
        let height = self.y2 - self.y1;
        self.init_rect(x, y, width, height);
    }

    /// Changes the size of the box, keeping its origin fixed.
    ///
    /// Mirrors `clutter_actor_box_set_size`.
    pub fn set_size(&mut self, width: f32, height: f32) {
        self.x2 = self.x1 + width;
        self.y2 = self.y1 + height;
    }

    /// Returns the area of the box.
    ///
    /// Mirrors `clutter_actor_box_get_area`.
    pub fn area(&self) -> f32 {
        self.width() * self.height()
    }

    /// Returns whether the point `(x, y)` is strictly inside the box.
    ///
    /// Mirrors `clutter_actor_box_contains`, which uses strict (`>`/`<`)
    /// comparisons, so points exactly on the boundary are not contained.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        (x > self.x1 && x < self.x2) && (y > self.y1 && y < self.y2)
    }

    /// Computes the bounding box of four `(x, y)` vertices.
    ///
    /// Mirrors `clutter_actor_box_from_vertices`, which in the C source
    /// takes an array of `graphene_point3d_t` but only ever reads the
    /// `x`/`y` fields; this port takes plain `(f32, f32)` pairs to avoid
    /// pulling in a graphene-equivalent type.
    pub fn from_vertices(verts: [(f32, f32); 4]) -> Self {
        let mut x1 = verts[0].0;
        let mut y1 = verts[0].1;
        for &(x, _y) in &verts[1..] {
            if x < x1 {
                x1 = x;
            }
        }
        for &(_x, y) in &verts[1..] {
            if y < y1 {
                y1 = y;
            }
        }

        let mut x2 = verts[0].0;
        let mut y2 = verts[0].1;
        for &(x, _y) in &verts[1..] {
            if x > x2 {
                x2 = x;
            }
        }
        for &(_x, y) in &verts[1..] {
            if y > y2 {
                y2 = y;
            }
        }

        ActorBox { x1, y1, x2, y2 }
    }

    /// Linearly interpolates between `initial` and `final_box` by
    /// `progress` (0.0 = `initial`, 1.0 = `final_box`).
    ///
    /// Mirrors `clutter_actor_box_interpolate`.
    pub fn interpolate(initial: &ActorBox, final_box: &ActorBox, progress: f64) -> ActorBox {
        let progress = progress as f32;
        ActorBox {
            x1: initial.x1 + (final_box.x1 - initial.x1) * progress,
            y1: initial.y1 + (final_box.y1 - initial.y1) * progress,
            x2: initial.x2 + (final_box.x2 - initial.x2) * progress,
            y2: initial.y2 + (final_box.y2 - initial.y2) * progress,
        }
    }

    /// Clamps the box's coordinates to integer pixel boundaries, expanding
    /// outward (top-left rounds down, bottom-right rounds up).
    ///
    /// Mirrors `clutter_actor_box_clamp_to_pixel`.
    pub fn clamp_to_pixel(&mut self) {
        self.x1 = floorf(self.x1);
        self.y1 = floorf(self.y1);
        self.x2 = ceilf(self.x2);
        self.y2 = ceilf(self.y2);
    }

    /// Returns the bounding box (union) of `a` and `b`.
    ///
    /// Mirrors `clutter_actor_box_union`.
    pub fn union(a: &ActorBox, b: &ActorBox) -> ActorBox {
        ActorBox {
            x1: f32_min(a.x1, b.x1),
            y1: f32_min(a.y1, b.y1),
            x2: f32_max(a.x2, b.x2),
            y2: f32_max(a.y2, b.y2),
        }
    }

    /// Rescales the box by `scale`, applied to all four coordinates.
    ///
    /// Mirrors `clutter_actor_box_scale`.
    pub fn scale(&mut self, scale: f32) {
        self.x1 *= scale;
        self.x2 *= scale;
        self.y1 *= scale;
        self.y2 *= scale;
    }

    /// Pads the box out to a stable, pixel-quantized size, used by effects
    /// that need a paint-box size independent of sub-pixel position.
    ///
    /// Mirrors `_clutter_actor_box_enlarge_for_effects`. A no-op when the
    /// box has zero area, matching the C source's early return.
    pub fn enlarge_for_effects(&mut self) {
        if self.area() == 0.0 {
            return;
        }

        let width = nearbyintf(self.x2 - self.x1);
        let height = nearbyintf(self.y2 - self.y1);

        self.x2 = ceilf(self.x2 + 0.75);
        self.y2 = ceilf(self.y2 + 0.75);

        self.x1 = self.x2 - width - 3.0;
        self.y1 = self.y2 - height - 3.0;
    }

    /// Returns whether the box has been initialized.
    ///
    /// Mirrors `clutter_actor_box_is_initialized`: a box is considered
    /// *uninitialized* only when all four sentinel conditions hold
    /// (`x1 == -inf`, `x2 == +inf`, `y1 == -inf`, `y2 == +inf`); this
    /// matches the (slightly unusual) "OR-of-negations" logic in the C
    /// source, which returns `TRUE` (initialized) unless *every* field
    /// matches its sentinel.
    pub fn is_initialized(&self) -> bool {
        let x1_uninitialized = self.x1.is_infinite() && self.x1.is_sign_negative();
        let x2_uninitialized = self.x2.is_infinite() && !self.x2.is_sign_negative();
        let y1_uninitialized = self.y1.is_infinite() && self.y1.is_sign_negative();
        let y2_uninitialized = self.y2.is_infinite() && !self.y2.is_sign_negative();

        !x1_uninitialized || !x2_uninitialized || !y1_uninitialized || !y2_uninitialized
    }
}

// Minimal no_std-friendly float helpers, following the same approach as
// `mutter_port::mtk::rectangle`'s private `floorf`/`ceilf`/`roundf` (those
// are not `pub`, so this module hand-rolls its own rather than depending
// on them).

fn floorf(v: f32) -> f32 {
    let truncated = v as i64 as f32;
    if v < truncated {
        truncated - 1.0
    } else {
        truncated
    }
}

fn ceilf(v: f32) -> f32 {
    let truncated = v as i64 as f32;
    if v > truncated {
        truncated + 1.0
    } else {
        truncated
    }
}

/// Rounds to the nearest integer, ties away from zero (matches the
/// behaviour of `nearbyintf`/`CLUTTER_NEARBYINT` closely enough for the
/// padding calculation in `enlarge_for_effects`, which only cares about
/// stable quantization, not banker's rounding).
fn nearbyintf(v: f32) -> f32 {
    if v >= 0.0 {
        floorf(v + 0.5)
    } else {
        ceilf(v - 0.5)
    }
}

fn f32_min(a: f32, b: f32) -> f32 {
    if a < b {
        a
    } else {
        b
    }
}

fn f32_max(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;

    #[test]
    fn test_new_and_init() {
        let b = ActorBox::new(1.0, 2.0, 3.0, 4.0);
        assert_eq!(b.x1, 1.0);
        assert_eq!(b.y1, 2.0);
        assert_eq!(b.x2, 3.0);
        assert_eq!(b.y2, 4.0);

        let mut c = ActorBox::new(0.0, 0.0, 0.0, 0.0);
        c.init(5.0, 6.0, 7.0, 8.0);
        assert_eq!(c, ActorBox::new(5.0, 6.0, 7.0, 8.0));
    }

    #[test]
    fn test_init_rect_and_from_rect() {
        let mut b = ActorBox::new(0.0, 0.0, 0.0, 0.0);
        b.init_rect(10.0, 20.0, 30.0, 40.0);
        assert_eq!(b, ActorBox::new(10.0, 20.0, 40.0, 60.0));

        let c = ActorBox::from_rect(10.0, 20.0, 30.0, 40.0);
        assert_eq!(b, c);
    }

    #[test]
    fn test_getters() {
        let b = ActorBox::new(10.0, 20.0, 40.0, 60.0);
        assert_eq!(b.x(), 10.0);
        assert_eq!(b.y(), 20.0);
        assert_eq!(b.width(), 30.0);
        assert_eq!(b.height(), 40.0);
        assert_eq!(b.origin(), (10.0, 20.0));
        assert_eq!(b.size(), (30.0, 40.0));
        assert_eq!(b.area(), 1200.0);
    }

    #[test]
    fn test_width_height_clamped_at_zero() {
        // x2 < x1 / y2 < y1 should report zero, not negative, per the C
        // source's `if (box->x2 > box->x1) ... else 0`.
        let b = ActorBox::new(10.0, 10.0, 5.0, 5.0);
        assert_eq!(b.width(), 0.0);
        assert_eq!(b.height(), 0.0);
    }

    #[test]
    fn test_set_origin_keeps_size() {
        let mut b = ActorBox::new(0.0, 0.0, 10.0, 20.0);
        b.set_origin(5.0, 5.0);
        assert_eq!(b, ActorBox::new(5.0, 5.0, 15.0, 25.0));
    }

    #[test]
    fn test_set_size_keeps_origin() {
        let mut b = ActorBox::new(5.0, 5.0, 15.0, 25.0);
        b.set_size(100.0, 200.0);
        assert_eq!(b, ActorBox::new(5.0, 5.0, 105.0, 205.0));
    }

    #[test]
    fn test_equal() {
        let a = ActorBox::new(1.0, 2.0, 3.0, 4.0);
        let b = ActorBox::new(1.0, 2.0, 3.0, 4.0);
        let c = ActorBox::new(1.0, 2.0, 3.0, 5.0);
        assert!(a.equal(&b));
        assert!(!a.equal(&c));
    }

    #[test]
    fn test_contains_is_strict() {
        let b = ActorBox::new(0.0, 0.0, 10.0, 10.0);
        assert!(b.contains(5.0, 5.0));
        // Boundary points are not contained (strict inequality in C source).
        assert!(!b.contains(0.0, 5.0));
        assert!(!b.contains(10.0, 5.0));
        assert!(!b.contains(5.0, 0.0));
        assert!(!b.contains(5.0, 10.0));
        assert!(!b.contains(-1.0, 5.0));
    }

    #[test]
    fn test_from_vertices() {
        let verts = [(3.0, 8.0), (-1.0, 4.0), (5.0, -2.0), (0.0, 0.0)];
        let b = ActorBox::from_vertices(verts);
        assert_eq!(b, ActorBox::new(-1.0, -2.0, 5.0, 8.0));
    }

    #[test]
    fn test_interpolate() {
        let initial = ActorBox::new(0.0, 0.0, 10.0, 10.0);
        let final_box = ActorBox::new(10.0, 20.0, 30.0, 40.0);

        let start = ActorBox::interpolate(&initial, &final_box, 0.0);
        assert_eq!(start, initial);

        let end = ActorBox::interpolate(&initial, &final_box, 1.0);
        assert_eq!(end, final_box);

        let mid = ActorBox::interpolate(&initial, &final_box, 0.5);
        assert_eq!(mid, ActorBox::new(5.0, 10.0, 20.0, 25.0));
    }

    #[test]
    fn test_clamp_to_pixel() {
        let mut b = ActorBox::new(1.2, 1.8, 9.1, 9.9);
        b.clamp_to_pixel();
        assert_eq!(b, ActorBox::new(1.0, 1.0, 10.0, 10.0));

        let mut neg = ActorBox::new(-1.2, -1.8, -0.1, -0.9);
        neg.clamp_to_pixel();
        assert_eq!(neg, ActorBox::new(-2.0, -2.0, 0.0, 0.0));
    }

    #[test]
    fn test_union() {
        let a = ActorBox::new(0.0, 0.0, 10.0, 10.0);
        let b = ActorBox::new(5.0, -5.0, 20.0, 8.0);
        let u = ActorBox::union(&a, &b);
        assert_eq!(u, ActorBox::new(0.0, -5.0, 20.0, 10.0));
    }

    #[test]
    fn test_scale() {
        let mut b = ActorBox::new(1.0, 2.0, 3.0, 4.0);
        b.scale(2.0);
        assert_eq!(b, ActorBox::new(2.0, 4.0, 6.0, 8.0));
    }

    #[test]
    fn test_enlarge_for_effects_zero_area_is_noop() {
        let mut b = ActorBox::new(5.0, 5.0, 5.0, 5.0);
        b.enlarge_for_effects();
        assert_eq!(b, ActorBox::new(5.0, 5.0, 5.0, 5.0));
    }

    #[test]
    fn test_enlarge_for_effects_pads_box() {
        let mut b = ActorBox::new(0.0, 0.0, 10.0, 10.0);
        b.enlarge_for_effects();
        // width/height = nearbyintf(10) = 10
        // x2' = ceilf(10 + 0.75) = 11, x1' = 11 - 10 - 3 = -2
        assert_eq!(b, ActorBox::new(-2.0, -2.0, 11.0, 11.0));
    }

    #[test]
    fn test_is_initialized() {
        let normal = ActorBox::new(0.0, 0.0, 10.0, 10.0);
        assert!(normal.is_initialized());

        let uninitialized = ActorBox::new(
            f32::NEG_INFINITY,
            f32::NEG_INFINITY,
            f32::INFINITY,
            f32::INFINITY,
        );
        assert!(!uninitialized.is_initialized());

        // Partially-sentinel boxes are still considered initialized.
        let partial = ActorBox::new(f32::NEG_INFINITY, 0.0, 10.0, 10.0);
        assert!(partial.is_initialized());
    }
}
