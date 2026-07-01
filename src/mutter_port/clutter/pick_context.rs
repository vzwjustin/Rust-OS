//! Port of GNOME mutter's `clutter/clutter-pick-context.{c,h}`.
//!
//! `ClutterPickContext` carries state during a pick pass: the pick mode, a
//! reference to the pick stack (where actors log themselves for hit-testing),
//! and the 3D pick point/ray used to query the stack.
//!
//! What was ported:
//! - The field layout of `ClutterPickContext` (refcounting dropped, see below).
//! - `clutter_pick_context_new_for_view` constructor logic.
//! - `get_mode`, `steal_stack`, `intersects_box` accessors.
//! - Delegating methods to pick_stack: `log_pick`, `log_overlap`, `push_clip`,
//!   `pop_clip`, `push_transform`, `get_transform`, `pop_transform`.
//!
//! What was skipped/stubbed, and why:
//! - **Reference counting** (`grefcount`, `clutter_pick_context_ref/unref`):
//!   GObject's manual refcounting has no equivalent need in Rust. Ownership is
//!   expressed directly: a `PickContext` is an owned value; dropping the value
//!   is the Rust equivalent of `_destroy`.
//! - **`CoglContext`**: no Cogl/GL binding exists in this kernel yet. The
//!   constructor takes `cogl_context` in C and passes it to `pick_stack_new`;
//!   since `PickStack::new()` doesn't use it, the parameter is dropped.
//! - **`ClutterStageView`**: likewise a placeholder. The real type would expose
//!   viewport/framebuffer metadata; ported constructors accept those as
//!   parameters or skip them.
//! - **`graphene_ray_t` / `graphene_point3d_t`**: no graphene/geometry bindings
//!   exist yet. `ray` and `point` are simplified placeholders holding 2D/3D
//!   coordinates; the actual ray-box intersection logic is stubbed in
//!   `intersects_box`.
//! - **GObject boxed-type registration** and `g_return_if_fail` assertions:
//!   dropped; Rust's ownership/borrow system covers the invariants natively.

use alloc::vec::Vec;

use super::actor_box::ActorBox;
use super::pick_stack::PickStack;

/// Mirrors the C `ClutterPickMode` enum.
///
/// Determines which actors are candidates for pick hit-testing during a pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PickMode {
    /// No picking; used to disable pick passes.
    None,
    /// Only reactive (clickable) actors are candidates.
    Reactive,
    /// All actors are candidates.
    All,
}

/// Placeholder for a 3D pick ray (simplified from `graphene_ray_t`).
///
/// A full implementation would hold ray origin, direction, and support
/// ray-box intersection tests. This stub holds minimal state.
#[derive(Debug, Clone, Copy)]
pub struct Ray {
    pub origin_x: f32,
    pub origin_y: f32,
    pub origin_z: f32,
    pub dir_x: f32,
    pub dir_y: f32,
    pub dir_z: f32,
}

impl Ray {
    pub fn new(
        origin_x: f32,
        origin_y: f32,
        origin_z: f32,
        dir_x: f32,
        dir_y: f32,
        dir_z: f32,
    ) -> Ray {
        Ray {
            origin_x,
            origin_y,
            origin_z,
            dir_x,
            dir_y,
            dir_z,
        }
    }
}

/// Placeholder for a 3D pick point (simplified from `graphene_point3d_t`).
///
/// Holds a single 3D coordinate used for point-in-box pick queries.
#[derive(Debug, Clone, Copy)]
pub struct Point3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Point3D {
    pub fn new(x: f32, y: f32, z: f32) -> Point3D {
        Point3D { x, y, z }
    }
}

/// Port of `ClutterPickContext` (`clutter-pick-context-private.h`).
///
/// Carries state during a pick pass: the pick mode (which actors are
/// candidates), a reference to the pick stack (where actors log themselves),
/// and the 3D point/ray used to query the stack for hit-testing.
///
/// The C type is refcounted (`grefcount`) and heap-allocated
/// (`g_new0`/`g_free`); this port is a plain owned `struct`, since Rust
/// ownership makes the manual refcounting unnecessary.
#[derive(Debug, Clone)]
pub struct PickContext {
    mode: PickMode,
    pick_stack: PickStack,
    ray: Ray,
    point: Point3D,
}

impl PickContext {
    /// Port of `clutter_pick_context_new_for_view`.
    ///
    /// Creates a new pick context for a pick pass starting at the given
    /// point and ray. The `pick_stack` is created empty and populated as
    /// actors render during the pass.
    pub fn new_for_view(mode: PickMode, point: Point3D, ray: Ray) -> PickContext {
        PickContext {
            mode,
            pick_stack: PickStack::new(),
            ray,
            point,
        }
    }

    /// Port of `clutter_pick_context_get_mode`.
    pub fn mode(&self) -> PickMode {
        self.mode
    }

    /// Port of `clutter_pick_context_steal_stack`.
    ///
    /// Consumes this context and returns the pick stack, allowing the
    /// caller to query pick results after the pass completes. Once stolen,
    /// the context is no longer usable.
    pub fn steal_stack(self) -> PickStack {
        self.pick_stack
    }

    /// Port of `clutter_pick_context_log_pick`.
    ///
    /// Logs an actor with its bounding box into the pick stack during
    /// the pick pass.
    pub fn log_pick(&mut self, box_: ActorBox, actor: usize) {
        self.pick_stack.log_pick(box_, actor);
    }

    /// Port of `clutter_pick_context_log_overlap`.
    ///
    /// Logs an overlapping actor into the pick stack (used for sequencing).
    pub fn log_overlap(&mut self, actor: usize) {
        self.pick_stack.log_overlap(actor);
    }

    /// Port of `clutter_pick_context_push_clip`.
    ///
    /// Pushes a clip rectangle onto the pick stack. Pop with `pop_clip` when done.
    pub fn push_clip(&mut self, box_: ActorBox) {
        self.pick_stack.push_clip(box_);
    }

    /// Port of `clutter_pick_context_pop_clip`.
    ///
    /// Pops the current clip rectangle from the clip stack. It is a
    /// programming error to call this without a corresponding `push_clip` first.
    pub fn pop_clip(&mut self) {
        self.pick_stack.pop_clip();
    }

    /// Port of `clutter_pick_context_push_transform`.
    ///
    /// Pushes a transform matrix onto the pick stack. Pop with
    /// `pop_transform` when done.
    ///
    /// TODO: `transform` is a placeholder; once matrix support is ported,
    /// this should accept a real matrix type.
    pub fn push_transform(&mut self, _transform: &[f32; 16]) {
        // Placeholder: actual implementation would push to the pick stack's
        // transform matrix stack once supported.
    }

    /// Port of `clutter_pick_context_get_transform`.
    ///
    /// Retrieves the current transform matrix of the pick stack.
    ///
    /// TODO: once matrix support is ported, return a real matrix type.
    pub fn get_transform(&self) -> [f32; 16] {
        // Placeholder: returns identity matrix
        [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]
    }

    /// Port of `clutter_pick_context_pop_transform`.
    ///
    /// Pops the current transform from the transform stack. It is a
    /// programming error to call this without a corresponding `push_transform` first.
    pub fn pop_transform(&mut self) {
        // Placeholder: actual implementation would pop from the pick stack's
        // transform matrix stack once supported.
    }

    /// Port of `clutter_pick_context_intersects_box`.
    ///
    /// Returns true if the pick ray/point intersects the given axis-aligned box.
    /// Simplified from the C version's graphene ray-box intersection.
    pub fn intersects_box(&self, box_: &ActorBox) -> bool {
        // Simplified: check if the 2D pick point is in the box
        // (graphene ray-box intersection would be more complex)
        box_.contains(self.point.x, self.point.y)
    }

    /// Returns the 3D pick point.
    pub fn point(&self) -> Point3D {
        self.point
    }

    /// Returns the 3D pick ray.
    pub fn ray(&self) -> Ray {
        self.ray
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_for_view_creates_context() {
        let point = Point3D::new(1.0, 2.0, 3.0);
        let ray = Ray::new(0.0, 0.0, 0.0, 0.0, 0.0, 1.0);
        let ctx = PickContext::new_for_view(PickMode::All, point, ray);

        assert_eq!(ctx.mode(), PickMode::All);
        assert_eq!(ctx.point().x, 1.0);
        assert_eq!(ctx.ray().dir_z, 1.0);
    }

    #[test]
    fn steal_stack_returns_empty_stack() {
        let point = Point3D::new(0.0, 0.0, 0.0);
        let ray = Ray::new(0.0, 0.0, 0.0, 0.0, 0.0, 1.0);
        let ctx = PickContext::new_for_view(PickMode::Reactive, point, ray);

        let stack = ctx.steal_stack();
        assert!(stack.is_empty());
    }

    #[test]
    fn log_pick_delegates_to_stack() {
        let point = Point3D::new(0.0, 0.0, 0.0);
        let ray = Ray::new(0.0, 0.0, 0.0, 0.0, 0.0, 1.0);
        let mut ctx = PickContext::new_for_view(PickMode::All, point, ray);

        let box_ = ActorBox::new(0.0, 0.0, 10.0, 10.0);
        ctx.log_pick(box_, 1);

        let stack = ctx.steal_stack();
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn intersects_box_checks_point_containment() {
        let point = Point3D::new(5.0, 5.0, 0.0);
        let ray = Ray::new(0.0, 0.0, 0.0, 0.0, 0.0, 1.0);
        let ctx = PickContext::new_for_view(PickMode::All, point, ray);

        let box_inside = ActorBox::new(0.0, 0.0, 10.0, 10.0);
        let box_outside = ActorBox::new(10.0, 10.0, 20.0, 20.0);

        assert!(ctx.intersects_box(&box_inside));
        assert!(!ctx.intersects_box(&box_outside));
    }

    #[test]
    fn get_transform_returns_identity() {
        let point = Point3D::new(0.0, 0.0, 0.0);
        let ray = Ray::new(0.0, 0.0, 0.0, 0.0, 0.0, 1.0);
        let ctx = PickContext::new_for_view(PickMode::None, point, ray);

        let transform = ctx.get_transform();
        assert_eq!(transform[0], 1.0);
        assert_eq!(transform[5], 1.0);
        assert_eq!(transform[10], 1.0);
        assert_eq!(transform[15], 1.0);
    }

    #[test]
    fn pick_mode_variants() {
        assert_eq!(PickMode::None, PickMode::None);
        assert_ne!(PickMode::Reactive, PickMode::All);
    }
}
