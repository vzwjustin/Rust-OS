//! GNOME Mutter's src/backends/meta-pointer-constraint.c
//!
//! Pointer client constraints. A pointer constraint restricts pointer movement
//! in relation to a client region — used to implement pointer confinement and
//! pointer locking (Wayland pointer-constraints protocol).
//!
//! Stubbed: MtkRegion is not available in the kernel, so the constrained area
//! is represented by a list of rectangles. The MetaPointerConstraintImpl class
//! (whose `constrain`/`ensure_constrained` are backend-native virtual methods)
//! is modeled as a trait; the actual native implementation is out of scope.
//!
//! Reference:
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-pointer-constraint.c

use alloc::vec::Vec;

/// A point in the pointer coordinate space (graphene_point_t).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// A single rectangle of the constraint region (replaces one MtkRectangle
/// making up the MtkRegion).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rectangle {
    /// Whether the point (px, py), in region-local coordinates, is inside.
    pub fn contains(&self, px: f64, py: f64) -> bool {
        px >= self.x as f64
            && py >= self.y as f64
            && px < (self.x + self.width) as f64
            && py < (self.y + self.height) as f64
    }
}

/// Pointer constraint state. Mirrors struct _MetaPointerConstraint.
#[derive(Debug, Clone)]
pub struct PointerConstraint {
    /// Region the pointer is constrained to (MtkRegion), as a set of rects.
    region: Vec<Rectangle>,
    /// Origin offset of the region in the global coordinate space.
    origin: Point,
    /// Minimum distance the pointer must keep from the region edge.
    min_edge_distance: f64,
}

impl PointerConstraint {
    /// meta_pointer_constraint_new()
    pub fn new(region: Vec<Rectangle>, origin: Point, min_edge_distance: f64) -> Self {
        PointerConstraint {
            region,
            origin,
            min_edge_distance,
        }
    }

    /// meta_pointer_constraint_get_region()
    ///
    /// Returns the constraint region; the origin is available via `origin()`.
    pub fn region(&self) -> &[Rectangle] {
        &self.region
    }

    /// The origin out-parameter of meta_pointer_constraint_get_region().
    pub fn origin(&self) -> Point {
        self.origin
    }

    /// meta_pointer_constraint_get_min_edge_distance()
    pub fn min_edge_distance(&self) -> f64 {
        self.min_edge_distance
    }

    /// Convenience: whether a global point lies within the constraint region,
    /// accounting for the origin offset.
    pub fn region_contains(&self, x: f64, y: f64) -> bool {
        let lx = x - self.origin.x;
        let ly = y - self.origin.y;
        self.region.iter().any(|r| r.contains(lx, ly))
    }
}

/// The backend-native implementation of a pointer constraint.
///
/// In Mutter this is MetaPointerConstraintImpl, an abstract GObject whose
/// `constrain`/`ensure_constrained` virtual methods are provided by the native
/// backend (MetaPointerConstraintImplNative). The kernel port has no native
/// backend, so this is left as a trait to be implemented later.
pub trait PointerConstraintImpl {
    /// meta_pointer_constraint_impl_constrain()
    ///
    /// Constrain the pointer movement from `(prev_x, prev_y)` to `(x, y)`,
    /// modifying `x`/`y` in place if needed. `time` is the event timestamp (ms).
    fn constrain(&mut self, time: u32, prev_x: f32, prev_y: f32, x: &mut f32, y: &mut f32);

    /// meta_pointer_constraint_impl_ensure_constrained()
    fn ensure_constrained(&mut self);
}
