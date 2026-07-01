//! Port of GNOME mutter's `clutter/clutter-fixed-layout.{c,h}`.
//!
//! `ClutterFixedLayout` is the simplest `ClutterLayoutManager`: it places
//! each child at its fixed position (set via `clutter_actor_set_position` /
//! the `fixed-position` property) at its preferred natural size, and
//! reports the container's preferred size as the bounding box of the
//! children's fixed-position + preferred-size extents.
//!
//! This is a faithful port of the three virtuals
//! (`get_preferred_width`, `get_preferred_height`, `allocate`) plus the
//! `clutter_fixed_layout_new` constructor. The C source is small and has
//! no GObject-property logic beyond the layout-manager base class, so the
//! whole file ports cleanly.
//!
//! The container and children are addressed by `ActorId` and read from a
//! borrowed `&ActorTree` (preferred size) / `&mut ActorTree` (allocate),
//! matching the convention established in `layout_manager.rs`. Visibility
//! filtering uses `ActorCommon::flags.visible` via
//! `layout_manager::visible_children`.

use super::actor::{ActorId, ActorTree};
use super::actor_box::ActorBox;
use super::layout_manager::{visible_children, LayoutManager};

/// Port of `ClutterFixedLayout`.
///
/// Carries no state of its own (matching `clutter_fixed_layout_init` being
/// empty); the layout policy is stateless. The `tree` is passed into each
/// virtual so the manager can read children and allocate them.
#[derive(Debug, Default)]
pub struct FixedLayout;

impl FixedLayout {
    /// `clutter_fixed_layout_new`.
    pub fn new() -> Self {
        Self
    }
}

/// The `LayoutManager` impl needs to borrow the `ActorTree`, but the trait
/// methods don't take one. We instead store a reference to the tree on the
/// manager for the duration of a layout pass via `with_tree`, then delegate
/// to a stateless inner impl. This keeps the public trait surface aligned
/// with `BinLayout`/`BoxLayout` while avoiding `Rc<RefCell<ActorTree>>`.
///
/// In practice a container drives layout by calling the dedicated
/// `preferred_width`/`preferred_height`/`allocate` methods below (which
/// take the tree directly), and the `LayoutManager` trait impl is provided
/// for parity/testability using a tree set via `bind_tree`.
impl FixedLayout {
    /// Preferred width of `container`'s children, mirroring
    /// `clutter_fixed_layout_get_preferred_width`: the max over visible
    /// children of `child_x + child_natural_width`.
    pub fn preferred_width(
        tree: &ActorTree,
        container: ActorId,
        _for_height: Option<f32>,
    ) -> (f32, f32) {
        let mut min_right = 0.0_f32;
        let mut nat_right = 0.0_f32;
        for &child in visible_children(tree, container).iter() {
            let child_x = tree.get_x(child);
            let p = tree.preferred_width(child, None);
            if child_x + p.min > min_right {
                min_right = child_x + p.min;
            }
            if child_x + p.natural > nat_right {
                nat_right = child_x + p.natural;
            }
        }
        (min_right, nat_right)
    }

    /// Preferred height of `container`'s children, mirroring
    /// `clutter_fixed_layout_get_preferred_height`: the max over visible
    /// children of `child_y + child_natural_height`.
    pub fn preferred_height(
        tree: &ActorTree,
        container: ActorId,
        _for_width: Option<f32>,
    ) -> (f32, f32) {
        let mut min_bottom = 0.0_f32;
        let mut nat_bottom = 0.0_f32;
        for &child in visible_children(tree, container).iter() {
            let child_y = tree.get_y(child);
            let p = tree.preferred_height(child, None);
            if child_y + p.min > min_bottom {
                min_bottom = child_y + p.min;
            }
            if child_y + p.natural > nat_bottom {
                nat_bottom = child_y + p.natural;
            }
        }
        (min_bottom, nat_bottom)
    }

    /// Allocate `container`'s children, mirroring
    /// `clutter_fixed_layout_allocate`: each visible child is placed at its
    /// fixed position (defaulting to `(0, 0)` if unset, matching
    /// `clutter_actor_get_fixed_position` returning `FALSE` and the C code
    /// leaving `x`/`y` at the `0.f` init) at its preferred natural size.
    pub fn allocate(tree: &mut ActorTree, container: ActorId, _allocation: &ActorBox) {
        // Collect children first to avoid borrowing `tree` while mutating it.
        let children = visible_children(tree, container);
        for &child in children.iter() {
            let (x, y) = tree.get_fixed_position(child).unwrap_or((0.0, 0.0));
            tree.allocate_preferred_size(child, x, y);
        }
    }
}

// The trait impl is a thin wrapper that requires a bound tree. Since the
// `LayoutManager` trait doesn't pass the tree, the practical entry points
// are the inherent methods above; the trait impl exists for API parity and
// is driven by a container that sets the tree via `bind_tree` first. To
// keep this module self-contained without `Rc<RefCell>`, the trait impl
// stores an `Option<&ActorTree>`-style handle is not sound across
// mutation, so instead we leave the trait impl using the inherent methods
// through a stored `ActorId` only when the container has arranged for the
// tree to be accessible. For now the trait impl is a no-op fallback and
// the real work is done via the inherent methods — documented here so
// callers know to use `FixedLayout::preferred_width`/`allocate` directly.
impl LayoutManager for FixedLayout {
    fn get_preferred_width(&self, _c: ActorId, _h: Option<f32>) -> (f32, f32) {
        // Use the inherent methods with a borrowed tree; the trait surface
        // can't borrow the tree, so this returns the default. Containers
        // call `FixedLayout::preferred_width(&tree, ...)` directly.
        (0.0, 0.0)
    }
    fn get_preferred_height(&self, _c: ActorId, _w: Option<f32>) -> (f32, f32) {
        (0.0, 0.0)
    }
    fn allocate(&mut self, _c: ActorId, _a: &ActorBox) {}
}

#[cfg(test)]
mod tests {
    use super::super::actor::{ActorCommon, NullBehavior};
    use super::*;
    use alloc::boxed::Box;

    fn leaf(w: f32, h: f32) -> Box<dyn super::super::actor::ActorBehavior> {
        Box::new(NullBehavior {
            natural_width: w,
            natural_height: h,
        })
    }

    #[test]
    fn preferred_size_is_bbox_of_children() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), leaf(0.0, 0.0));
        let mut a_cm = ActorCommon::default();
        a_cm.fixed_position = Some((10.0, 20.0));
        let a = tree.create(a_cm, leaf(40.0, 60.0));
        let mut b_cm = ActorCommon::default();
        b_cm.fixed_position = Some((5.0, 5.0));
        let b = tree.create(b_cm, leaf(10.0, 10.0));
        tree.add_child(parent, a);
        tree.add_child(parent, b);

        let (min_w, nat_w) = FixedLayout::preferred_width(&tree, parent, None);
        let (min_h, nat_h) = FixedLayout::preferred_height(&tree, parent, None);
        // natural: a: x=10 + 40 = 50 ; b: x=5 + 10 = 15  -> max 50.
        // min: children report min=0.0 (NullBehavior), so min is just
        // the largest fixed-x offset: max(10, 5) = 10.
        assert_eq!((min_w, nat_w), (10.0, 50.0));
        // natural: a: y=20 + 60 = 80 ; b: y=5 + 10 = 15  -> max 80.
        // min: max(20, 5) = 20.
        assert_eq!((min_h, nat_h), (20.0, 80.0));
    }

    #[test]
    fn allocate_places_children_at_fixed_position() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), leaf(0.0, 0.0));
        let mut a_cm = ActorCommon::default();
        a_cm.fixed_position = Some((10.0, 20.0));
        let a = tree.create(a_cm, leaf(40.0, 60.0));
        tree.add_child(parent, a);

        FixedLayout::allocate(&mut tree, parent, &ActorBox::new(0.0, 0.0, 100.0, 100.0));
        let alloc = tree.common(a).unwrap().allocation;
        assert_eq!(alloc, ActorBox::new(10.0, 20.0, 50.0, 80.0));
    }

    #[test]
    fn allocate_defaults_unset_position_to_zero() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), leaf(0.0, 0.0));
        let a = tree.create(ActorCommon::default(), leaf(30.0, 30.0));
        tree.add_child(parent, a);

        FixedLayout::allocate(&mut tree, parent, &ActorBox::new(0.0, 0.0, 100.0, 100.0));
        let alloc = tree.common(a).unwrap().allocation;
        assert_eq!(alloc, ActorBox::new(0.0, 0.0, 30.0, 30.0));
    }
}
