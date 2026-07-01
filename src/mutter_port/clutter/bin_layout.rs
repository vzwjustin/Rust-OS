//! Port of GNOME mutter's `clutter/clutter-bin-layout.{c,h}`.
//!
//! `ClutterBinLayout` stacks children in "layers" on top of each other:
//! the preferred size is the maximum preferred size across all children,
//! and each child is allocated the full container box (subject to its
//! `x_align`/`y_align`/`needs_expand` settings) via
//! `clutter_actor_allocate_align_fill`.
//!
//! This is a faithful port of the three virtuals
//! (`get_preferred_width`, `get_preferred_height`, `allocate`) plus the
//! `clutter_bin_layout_new` constructor. The C source has no
//! GObject-property logic of its own (it relies on the base
//! `ClutterLayoutManager`), so the whole file ports cleanly.
//!
//! The container and children are addressed by `ActorId` and read from a
//! borrowed `&ActorTree` / `&mut ActorTree`, matching the convention in
//! `fixed_layout.rs`. The `get_actor_align_factor` helper is
//! `ActorAlign::factor()` in `actor.rs`.

use super::actor::{ActorAlign, ActorId, ActorTree};
use super::actor_box::ActorBox;
use super::enums::Orientation;
use super::layout_manager::{visible_children, LayoutManager};

/// Port of `ClutterBinLayout`. Carries no state (matching
/// `clutter_bin_layout_init` being empty).
#[derive(Debug, Default)]
pub struct BinLayout;

impl BinLayout {
    /// `clutter_bin_layout_new`.
    pub fn new() -> Self {
        Self
    }

    /// Preferred width: the max over visible children of the child's
    /// preferred width, mirroring `clutter_bin_layout_get_preferred_width`.
    pub fn preferred_width(
        tree: &ActorTree,
        container: ActorId,
        for_height: Option<f32>,
    ) -> (f32, f32) {
        let mut min_w = 0.0_f32;
        let mut nat_w = 0.0_f32;
        for &child in visible_children(tree, container).iter() {
            let p = tree.preferred_width(child, for_height);
            if p.min > min_w {
                min_w = p.min;
            }
            if p.natural > nat_w {
                nat_w = p.natural;
            }
        }
        (min_w, nat_w)
    }

    /// Preferred height: the max over visible children of the child's
    /// preferred height, mirroring `clutter_bin_layout_get_preferred_height`.
    pub fn preferred_height(
        tree: &ActorTree,
        container: ActorId,
        for_width: Option<f32>,
    ) -> (f32, f32) {
        let mut min_h = 0.0_f32;
        let mut nat_h = 0.0_f32;
        for &child in visible_children(tree, container).iter() {
            let p = tree.preferred_height(child, for_width);
            if p.min > min_h {
                min_h = p.min;
            }
            if p.natural > nat_h {
                nat_h = p.natural;
            }
        }
        (min_h, nat_h)
    }

    /// Allocate each visible child inside `allocation`, mirroring
    /// `clutter_bin_layout_allocate`. Each child gets a box equal to the
    /// full allocation (offset to its fixed position if set), then
    /// `allocate_align_fill` applies the child's alignment and expand
    /// flags.
    pub fn allocate(tree: &mut ActorTree, container: ActorId, allocation: &ActorBox) {
        let (alloc_x, alloc_y) = allocation.origin();
        let (avail_w, avail_h) = (allocation.width(), allocation.height());
        let children = visible_children(tree, container);
        for &child in children.iter() {
            let fixed = tree.get_fixed_position(child);
            let (x1, y1) = match fixed {
                Some((fx, fy)) => (fx, fy),
                None => (alloc_x, alloc_y),
            };
            let child_box = ActorBox::new(x1, y1, alloc_x + avail_w, alloc_y + avail_h);

            let (x_fill, x_align) = if tree.needs_expand(child, Orientation::Horizontal) {
                let align = tree.get_x_align(child);
                (align == ActorAlign::Fill, align.factor())
            } else {
                (false, if fixed.is_none() { 0.5 } else { 0.0 })
            };
            let (y_fill, y_align) = if tree.needs_expand(child, Orientation::Vertical) {
                let align = tree.get_y_align(child);
                (align == ActorAlign::Fill, align.factor())
            } else {
                (false, if fixed.is_none() { 0.5 } else { 0.0 })
            };

            tree.allocate_align_fill(child, &child_box, x_align, y_align, x_fill, y_fill, None);
        }
    }
}

// Trait impl is a no-op fallback for the same reason as `FixedLayout`: the
// `LayoutManager` trait surface can't borrow the `ActorTree`. Containers
// call the inherent `BinLayout::preferred_width`/`allocate` methods
// directly. See `fixed_layout.rs` for the full rationale.
impl LayoutManager for BinLayout {
    fn get_preferred_width(&self, _c: ActorId, _h: Option<f32>) -> (f32, f32) {
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
    fn preferred_size_is_max_of_children() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), leaf(0.0, 0.0));
        let a = tree.create(ActorCommon::default(), leaf(40.0, 60.0));
        let b = tree.create(ActorCommon::default(), leaf(100.0, 10.0));
        tree.add_child(parent, a);
        tree.add_child(parent, b);

        let (min_w, nat_w) = BinLayout::preferred_width(&tree, parent, None);
        let (min_h, nat_h) = BinLayout::preferred_height(&tree, parent, None);
        assert_eq!((min_w, nat_w), (0.0, 100.0));
        assert_eq!((min_h, nat_h), (0.0, 60.0));
    }

    #[test]
    fn allocate_centers_non_expanding_child() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), leaf(0.0, 0.0));
        let a = tree.create(ActorCommon::default(), leaf(40.0, 60.0));
        tree.add_child(parent, a);

        let alloc = ActorBox::new(0.0, 0.0, 100.0, 100.0);
        BinLayout::allocate(&mut tree, parent, &alloc);
        // Non-expanding, no fixed pos -> x_align/y_align = 0.5 (center).
        // child 40x60 centered in 100x100 -> (30, 20, 70, 80).
        let child_alloc = tree.common(a).unwrap().allocation;
        assert_eq!(child_alloc, ActorBox::new(30.0, 20.0, 70.0, 80.0));
    }

    #[test]
    fn allocate_fills_expanding_child() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), leaf(0.0, 0.0));
        let mut a_cm = ActorCommon::default();
        a_cm.x_expand = true;
        a_cm.y_expand = true;
        a_cm.x_align = ActorAlign::Fill;
        a_cm.y_align = ActorAlign::Fill;
        let a = tree.create(a_cm, leaf(40.0, 60.0));
        tree.add_child(parent, a);

        let alloc = ActorBox::new(0.0, 0.0, 100.0, 100.0);
        BinLayout::allocate(&mut tree, parent, &alloc);
        // Expanding + Fill -> child gets the full box.
        let child_alloc = tree.common(a).unwrap().allocation;
        assert_eq!(child_alloc, ActorBox::new(0.0, 0.0, 100.0, 100.0));
    }
}
