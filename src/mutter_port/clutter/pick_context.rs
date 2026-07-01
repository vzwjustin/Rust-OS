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

/// A 4x4 row-major transform matrix stored as 16 `f32` values.
///
/// Stand-in for `graphene_matrix_t`, which is not available in this port.
/// The layout is row-major: `m[row * 4 + col]`. Supports `identity()` and
/// `multiply()` (this * other, matching `graphene_matrix_multiply`'s
/// "result = self * b" convention where vectors are column vectors
/// post-multiplied).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Matrix4x4(pub [f32; 16]);

impl Matrix4x4 {
    /// Returns the identity matrix.
    pub const fn identity() -> Self {
        Matrix4x4([
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ])
    }

    /// Returns `self * other` (row-major, column-vector convention).
    ///
    /// `result[row * 4 + col] = sum_k self[row*4+k] * other[k*4+col]`.
    pub fn multiply(&self, other: &Matrix4x4) -> Matrix4x4 {
        let mut out = [0.0f32; 16];
        for row in 0..4 {
            for col in 0..4 {
                let mut sum = 0.0;
                for k in 0..4 {
                    sum += self.0[row * 4 + k] * other.0[k * 4 + col];
                }
                out[row * 4 + col] = sum;
            }
        }
        Matrix4x4(out)
    }

    /// Returns the matrix elements as a flat row-major `[f32; 16]` array.
    pub fn as_array(&self) -> [f32; 16] {
        self.0
    }
}

impl Default for Matrix4x4 {
    fn default() -> Self {
        Matrix4x4::identity()
    }
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
    /// Stack of accumulated transform matrices. `transforms[0]` is the
    /// current/topmost (innermost) transform, mirroring the C
    /// `graphene_matrix_t` stack managed inside `ClutterPickStack`. The
    /// base (bottom of stack) is the identity; `get_transform` returns the
    /// product of all pushed matrices, i.e. the composite transform from
    /// the root to the current node.
    transforms: Vec<Matrix4x4>,
}

impl PickContext {
    /// Port of `clutter_pick_context_new_for_view`.
    ///
    /// Creates a new pick context for a pick pass starting at the given
    /// point and ray. The `pick_stack` is created empty and populated as
    /// actors render during the pass. The transform stack is initialized
    /// with a single identity matrix as the base.
    pub fn new_for_view(mode: PickMode, point: Point3D, ray: Ray) -> PickContext {
        PickContext {
            mode,
            pick_stack: PickStack::new(),
            ray,
            point,
            transforms: Vec::new(),
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
    /// Pushes a transform matrix onto the transform stack. The pushed
    /// matrix is composed with the current (topmost) transform so that
    /// `get_transform` returns the cumulative root-to-current transform.
    /// Pop with `pop_transform` when done.
    pub fn push_transform(&mut self, transform: Matrix4x4) {
        let current = self.current_transform();
        let composed = current.multiply(&transform);
        self.transforms.insert(0, composed);
    }

    /// Port of `clutter_pick_context_get_transform`.
    ///
    /// Retrieves the current (composite) transform matrix of the pick
    /// stack: the product of all matrices pushed since the base. Returns
    /// the identity if nothing has been pushed.
    pub fn get_transform(&self) -> Matrix4x4 {
        self.current_transform()
    }

    /// Port of `clutter_pick_context_pop_transform`.
    ///
    /// Pops the current transform from the transform stack. It is a
    /// programming error to call this without a corresponding
    /// `push_transform` first.
    pub fn pop_transform(&mut self) {
        debug_assert!(
            !self.transforms.is_empty(),
            "pop_transform without a matching push_transform"
        );
        if !self.transforms.is_empty() {
            self.transforms.remove(0);
        }
    }

    /// Returns the current composite transform: the topmost pushed matrix
    /// if any, otherwise the identity (the implicit base of the stack).
    fn current_transform(&self) -> Matrix4x4 {
        self.transforms
            .first()
            .copied()
            .unwrap_or(Matrix4x4::identity())
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
        assert_eq!(transform, Matrix4x4::identity());
    }

    #[test]
    fn push_get_pop_transform_composes_matrices() {
        let point = Point3D::new(0.0, 0.0, 0.0);
        let ray = Ray::new(0.0, 0.0, 0.0, 0.0, 0.0, 1.0);
        let mut ctx = PickContext::new_for_view(PickMode::All, point, ray);

        // Base transform is identity.
        assert_eq!(ctx.get_transform(), Matrix4x4::identity());

        // Push a translation matrix (row-major: x offset in [3], y in [7]).
        let translate = Matrix4x4([
            1.0, 0.0, 0.0, 10.0, 0.0, 1.0, 0.0, 20.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        ctx.push_transform(translate);

        // After one push, the composite equals the pushed matrix.
        assert_eq!(ctx.get_transform(), translate);

        // Push a second translation; composite should be translate * translate2.
        let translate2 = Matrix4x4([
            1.0, 0.0, 0.0, 5.0, 0.0, 1.0, 0.0, 7.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ]);
        ctx.push_transform(translate2);
        assert_eq!(ctx.get_transform(), translate.multiply(&translate2));

        // Pop returns to the first pushed transform.
        ctx.pop_transform();
        assert_eq!(ctx.get_transform(), translate);

        // Pop returns to the identity base.
        ctx.pop_transform();
        assert_eq!(ctx.get_transform(), Matrix4x4::identity());
    }

    #[test]
    fn matrix4x4_identity_multiply_is_identity() {
        let m = Matrix4x4::identity();
        assert_eq!(m.multiply(&Matrix4x4::identity()), Matrix4x4::identity());
    }

    #[test]
    fn pick_mode_variants() {
        assert_eq!(PickMode::None, PickMode::None);
        assert_ne!(PickMode::Reactive, PickMode::All);
    }
}
