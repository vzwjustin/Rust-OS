//! Port of GNOME mutter's `clutter/clutter-box-layout.{c,h}`.
//!
//! `ClutterBoxLayout` arranges children on a single line (horizontal or
//! vertical per `orientation`): each child gets its natural size, or — if
//! it expands — a share of the extra space; if `homogeneous`, every child
//! gets an equal share. Spacing is inserted between children. Horizontal
//! layout honors the container's RTL text direction by mirroring positions.
//!
//! This is a faithful port of:
//! - The three layout virtuals (`get_preferred_width`/`get_preferred_height`/
//!   `allocate`), including the opposite-orientation size-for-size
//!   negotiation (`get_preferred_size_for_opposite_orientation`).
//! - The GTK-derived `distribute_natural_allocation` space-distribution
//!   algorithm (pulled from gtksizerequest.c) and its `compare_gap` sort.
//! - `count_expand_children`, `get_child_size`, `get_base_size_for_opposite_orientation`.
//! - The `set_orientation`/`set_spacing`/`set_homogeneous` property setters
//!   (without the GObject property-notify machinery — they're plain field
//!   setters that the caller can use to trigger a relayout).
//!
//! The container and children are addressed by `ActorId` and read from a
//! borrowed `&ActorTree` / `&mut ActorTree`, matching `fixed_layout.rs` /
//! `bin_layout.rs`. The `easing_mode`/`easing_duration` animation fields
//! from `ClutterBoxLayoutPrivate` are dropped (no `ClutterTimeline` port).

use alloc::vec::Vec;

use super::actor::{ActorId, ActorTree};
use super::actor_box::ActorBox;
use super::enums::{Orientation, TextDirection};
use super::layout_manager::{visible_children, LayoutManager};

/// Port of `RequestedSize` (clutter-box-layout.c).
#[derive(Debug, Clone, Copy)]
struct RequestedSize {
    actor: ActorId,
    minimum_size: f32,
    natural_size: f32,
}

/// Port of `ClutterBoxLayout` / `ClutterBoxLayoutPrivate`.
///
/// `orientation`, `spacing`, and `is_homogeneous` mirror the C private
/// fields; `easing_mode`/`easing_duration` (animation) are dropped.
#[derive(Debug, Clone)]
pub struct BoxLayout {
    pub orientation: Orientation,
    pub spacing: u32,
    pub homogeneous: bool,
}

impl Default for BoxLayout {
    fn default() -> Self {
        // `clutter_box_layout_init` leaves orientation at the default
        // (`CLUTTER_ORIENTATION_HORIZONTAL` = 0), spacing 0, homogeneous
        // false.
        BoxLayout {
            orientation: Orientation::Horizontal,
            spacing: 0,
            homogeneous: false,
        }
    }
}

impl BoxLayout {
    /// `clutter_box_layout_new`.
    pub fn new() -> Self {
        Self::default()
    }

    /// `clutter_box_layout_set_orientation` (minus the property notify +
    /// `layout_changed` emission — the caller triggers relayout).
    pub fn set_orientation(&mut self, orientation: Orientation) {
        self.orientation = orientation;
    }

    /// `clutter_box_layout_set_spacing`.
    pub fn set_spacing(&mut self, spacing: u32) {
        self.spacing = spacing;
    }

    /// `clutter_box_layout_set_homogeneous`.
    pub fn set_homogeneous(&mut self, homogeneous: bool) {
        self.homogeneous = homogeneous;
    }

    /// `get_child_size`: preferred size of `child` along `orientation`,
    /// passing `for_size` to the perpendicular preferred-size query.
    fn get_child_size(
        tree: &ActorTree,
        child: ActorId,
        orientation: Orientation,
        for_size: f32,
    ) -> (f32, f32) {
        let for_size_arg = if for_size < 0.0 { None } else { Some(for_size) };
        match orientation {
            Orientation::Horizontal => {
                let p = tree.preferred_width(child, for_size_arg);
                (p.min, p.natural)
            }
            Orientation::Vertical => {
                let p = tree.preferred_height(child, for_size_arg);
                (p.min, p.natural)
            }
        }
    }

    /// `count_expand_children`: returns `(visible, expanding)` counts
    /// along this box's orientation.
    fn count_expand(&self, tree: &ActorTree, container: ActorId) -> (i32, i32) {
        let mut vis = 0;
        let mut exp = 0;
        for &child in visible_children(tree, container).iter() {
            vis += 1;
            if tree.needs_expand(child, self.orientation) {
                exp += 1;
            }
        }
        (vis, exp)
    }

    /// `get_preferred_size_for_orientation`: sum (or, when homogeneous,
    /// max*n) of children's preferred sizes along the box orientation,
    /// plus spacing.
    fn preferred_for_orientation(
        &self,
        tree: &ActorTree,
        container: ActorId,
        for_size: f32,
    ) -> (f32, f32) {
        let mut n = 0_i32;
        let mut min = 0.0_f32;
        let mut nat = 0.0_f32;
        let mut largest_min = 0.0_f32;
        let mut largest_nat = 0.0_f32;
        for &child in visible_children(tree, container).iter() {
            n += 1;
            let (cmin, cnat) = Self::get_child_size(tree, child, self.orientation, for_size);
            if self.homogeneous {
                if cmin > largest_min {
                    largest_min = cmin;
                }
                if cnat > largest_nat {
                    largest_nat = cnat;
                }
            } else {
                min += cmin;
                nat += cnat;
            }
        }
        if self.homogeneous {
            min = largest_min * n as f32;
            nat = largest_nat * n as f32;
        }
        if n > 1 {
            let sp = self.spacing as f32 * (n - 1) as f32;
            min += sp;
            nat += sp;
        }
        (min, nat)
    }

    /// `get_base_size_for_opposite_orientation`: max of children's
    /// preferred sizes along the opposite orientation (unconstrained).
    fn preferred_base_opposite(&self, tree: &ActorTree, container: ActorId) -> (f32, f32) {
        let opposite = match self.orientation {
            Orientation::Horizontal => Orientation::Vertical,
            Orientation::Vertical => Orientation::Horizontal,
        };
        let mut min = 0.0_f32;
        let mut nat = 0.0_f32;
        for &child in visible_children(tree, container).iter() {
            let (cmin, cnat) = Self::get_child_size(tree, child, opposite, -1.0);
            if cmin > min {
                min = cmin;
            }
            if cnat > nat {
                nat = cnat;
            }
        }
        (min, nat)
    }

    /// `get_preferred_size_for_opposite_orientation`: the size-for-size
    /// negotiation — virtually allocate children along the box orientation
    /// for `for_size`, then take the max of each child's opposite-orientation
    /// preferred size at that allocation.
    fn preferred_opposite_for_size(
        &self,
        tree: &ActorTree,
        container: ActorId,
        for_size: f32,
    ) -> (f32, f32) {
        let opposite = match self.orientation {
            Orientation::Horizontal => Orientation::Vertical,
            Orientation::Vertical => Orientation::Horizontal,
        };
        let (nvis, nexpand) = self.count_expand(tree, container);
        if nvis < 1 {
            return (0.0, 0.0);
        }

        // Collect requested sizes along the box orientation (unconstrained).
        let mut sizes: Vec<RequestedSize> = Vec::new();
        let mut size = for_size;
        for &child in visible_children(tree, container).iter() {
            let (cmin, cnat) = Self::get_child_size(tree, child, self.orientation, -1.0);
            size -= cmin;
            sizes.push(RequestedSize {
                actor: child,
                minimum_size: cmin,
                natural_size: cnat,
            });
        }

        let mut extra = 0.0_f32;
        let mut n_extra_widgets = 0_i32;
        if self.homogeneous {
            size = for_size - (nvis - 1) as f32 * self.spacing as f32;
            extra = size / nvis as f32;
            n_extra_widgets = (size as i32) % nvis;
        } else {
            size -= (nvis - 1) as f32 * self.spacing as f32;
            if size.is_normal() || size == 0.0 {
                size = distribute_natural_allocation(size.max(0.0), &mut sizes);
            } else {
                size = 0.0;
            }
            if nexpand > 0 {
                extra = size / nexpand as f32;
                n_extra_widgets = (size as i32) % nexpand;
            }
        }

        // Distribute expand space, mirroring the C second pass.
        for s in sizes.iter_mut() {
            if self.homogeneous {
                s.minimum_size = extra;
                if n_extra_widgets > 0 {
                    s.minimum_size += 1.0;
                    n_extra_widgets -= 1;
                }
            } else if tree.needs_expand(s.actor, self.orientation) {
                s.minimum_size += extra;
                if n_extra_widgets > 0 {
                    s.minimum_size += 1.0;
                    n_extra_widgets -= 1;
                }
            }
        }

        // Now query opposite-orientation size-for-size and take the max.
        let mut min = 0.0_f32;
        let mut nat = 0.0_f32;
        for s in sizes.iter() {
            let (cmin, cnat) = Self::get_child_size(tree, s.actor, opposite, s.minimum_size);
            if cmin > min {
                min = cmin;
            }
            if cnat > nat {
                nat = cnat;
            }
        }
        (min, nat)
    }

    // ---- public preferred-size entry points ----

    /// `clutter_box_layout_get_preferred_width`.
    pub fn preferred_width(
        &self,
        tree: &ActorTree,
        container: ActorId,
        for_height: Option<f32>,
    ) -> (f32, f32) {
        let for_height = for_height.map_or(-1.0, |h| h);
        if self.orientation == Orientation::Vertical {
            if for_height < 0.0 {
                self.preferred_base_opposite(tree, container)
            } else {
                self.preferred_opposite_for_size(tree, container, for_height)
            }
        } else {
            self.preferred_for_orientation(tree, container, for_height)
        }
    }

    /// `clutter_box_layout_get_preferred_height`.
    pub fn preferred_height(
        &self,
        tree: &ActorTree,
        container: ActorId,
        for_width: Option<f32>,
    ) -> (f32, f32) {
        let for_width = for_width.map_or(-1.0, |w| w);
        if self.orientation == Orientation::Horizontal {
            if for_width < 0.0 {
                self.preferred_base_opposite(tree, container)
            } else {
                self.preferred_opposite_for_size(tree, container, for_width)
            }
        } else {
            self.preferred_for_orientation(tree, container, for_width)
        }
    }

    /// `clutter_box_layout_allocate`: distribute `box` among visible
    /// children along the box orientation, placing each via
    /// `allocate_box_child` (a plain `clutter_actor_allocate`).
    pub fn allocate(&mut self, tree: &mut ActorTree, container: ActorId, box_: &ActorBox) {
        let (nvis, nexpand) = self.count_expand(tree, container);
        if nvis <= 0 {
            return;
        }

        let (box_w, box_h) = (box_.width(), box_.height());
        let (box_x1, box_y1) = box_.origin();

        // Collect requested sizes along the box orientation, sized for the
        // perpendicular extent of the box.
        let mut sizes: Vec<RequestedSize> = Vec::new();
        let mut size = match self.orientation {
            Orientation::Vertical => box_h - (nvis - 1) as f32 * self.spacing as f32,
            Orientation::Horizontal => box_w - (nvis - 1) as f32 * self.spacing as f32,
        } as i32;

        for &child in visible_children(tree, container).iter() {
            let (cmin, cnat) = match self.orientation {
                Orientation::Vertical => {
                    let p = tree.preferred_height(child, Some(box_w));
                    (p.min, p.natural)
                }
                Orientation::Horizontal => {
                    let p = tree.preferred_width(child, Some(box_h));
                    (p.min, p.natural)
                }
            };
            size -= cmin as i32;
            sizes.push(RequestedSize {
                actor: child,
                minimum_size: cmin,
                natural_size: cnat,
            });
        }

        let extra: i32;
        let mut n_extra_widgets: i32;
        if self.homogeneous {
            size = match self.orientation {
                Orientation::Vertical => box_h - (nvis - 1) as f32 * self.spacing as f32,
                Orientation::Horizontal => box_w - (nvis - 1) as f32 * self.spacing as f32,
            } as i32;
            extra = size / nvis;
            n_extra_widgets = size % nvis;
        } else {
            size = distribute_natural_allocation((size.max(0) as f32), &mut sizes) as i32;
            if nexpand > 0 {
                extra = size / nexpand;
                n_extra_widgets = size % nexpand;
            } else {
                extra = 0;
                n_extra_widgets = 0;
            }
        }

        let is_rtl = self.orientation == Orientation::Horizontal
            && tree
                .common(container)
                .map_or(false, |c| c.text_direction == TextDirection::Rtl);

        // Position cursor and the fixed axis of the child allocation box.
        let mut child_alloc = ActorBox::default();
        let mut x: i32;
        let mut y: i32;
        if self.orientation == Orientation::Vertical {
            child_alloc.x1 = box_x1;
            child_alloc.x2 = box_w.max(1.0);
            y = box_y1 as i32;
            x = 0; // unused
        } else {
            child_alloc.y1 = box_y1;
            child_alloc.y2 = box_h.max(1.0);
            x = box_x1 as i32;
            y = 0; // unused
        }

        for s in sizes.iter() {
            let child = s.actor;
            // Assign child size.
            let child_size: f32 = if self.homogeneous {
                let mut cs = extra as f32;
                if n_extra_widgets > 0 {
                    cs += 1.0;
                    n_extra_widgets -= 1;
                }
                cs
            } else {
                let mut cs = s.minimum_size;
                if tree.needs_expand(child, self.orientation) {
                    cs += extra as f32;
                    if n_extra_widgets > 0 {
                        cs += 1.0;
                        n_extra_widgets -= 1;
                    }
                }
                cs
            };

            // Assign child position along the box orientation.
            if self.orientation == Orientation::Vertical {
                if tree.needs_expand(child, self.orientation) {
                    child_alloc.y1 = y as f32;
                    child_alloc.y2 = child_alloc.y1 + child_size.max(1.0);
                } else {
                    child_alloc.y1 = y as f32 + (child_size - s.minimum_size) / 2.0;
                    child_alloc.y2 = child_alloc.y1 + s.minimum_size;
                }
                y += child_size as i32 + self.spacing as i32;
            } else {
                if tree.needs_expand(child, self.orientation) {
                    child_alloc.x1 = x as f32;
                    child_alloc.x2 = child_alloc.x1 + child_size.max(1.0);
                } else {
                    child_alloc.x1 = x as f32 + (child_size - s.minimum_size) / 2.0;
                    child_alloc.x2 = child_alloc.x1 + s.minimum_size;
                }
                x += child_size as i32 + self.spacing as i32;

                if is_rtl {
                    let width = child_alloc.x2 - child_alloc.x1;
                    child_alloc.x2 = box_x1 + (box_x1 + box_w - child_alloc.x1);
                    child_alloc.x1 = child_alloc.x2 - width;
                }
            }

            // `allocate_box_child` is a plain `clutter_actor_allocate`.
            tree.allocate(child, child_alloc);
        }
    }
}

/// `compare_gap` (clutter-box-layout.c, from gtksizerequest.c): sort
/// indices descending by `(natural - minimum)` gap, breaking ties by
/// descending index. Returns ordering such that `spreading[0]` has the
/// largest gap.
fn compare_gap(sizes: &[RequestedSize], a: usize, b: usize) -> core::cmp::Ordering {
    let d1 = (sizes[a].natural_size - sizes[a].minimum_size).max(0.0) as i32;
    let d2 = (sizes[b].natural_size - sizes[b].minimum_size).max(0.0) as i32;
    let delta = d2 - d1;
    if delta == 0 {
        (b as i32).cmp(&(a as i32))
    } else {
        delta.cmp(&0)
    }
}

/// `distribute_natural_allocation` (clutter-box-layout.c, from
/// gtksizerequest.c): distribute `extra_space` to children by bringing
/// smaller-gap children up to natural size first. Returns the remainder.
fn distribute_natural_allocation(extra_space: f32, sizes: &mut [RequestedSize]) -> f32 {
    if !(extra_space.is_normal() || extra_space == 0.0) || extra_space < 0.0 {
        return 0.0;
    }
    let n = sizes.len();
    if n == 0 {
        return extra_space;
    }
    let mut spreading: Vec<usize> = (0..n).collect();
    // Sort descending by gap (compare_gap returns descending order via
    // `d2 - d1`); `sort_by` is stable, matching g_sort_array's behavior
    // for the tie-break (we encode the tie-break in compare_gap directly).
    spreading.sort_by(|&a, &b| compare_gap(sizes, a, b));

    let mut extra = extra_space;
    let mut i = n;
    while i > 0 && extra > 0.0 {
        i -= 1;
        let idx = spreading[i];
        let glue = (extra + i as f32) / (i as f32 + 1.0);
        let gap = sizes[idx].natural_size - sizes[idx].minimum_size;
        let add = glue.min(gap);
        sizes[idx].minimum_size += add;
        extra -= add;
    }
    extra
}

// Trait impl is a no-op fallback for the same reason as the other layouts:
// the `LayoutManager` trait surface can't borrow the `ActorTree`. Containers
// call the inherent `BoxLayout::preferred_width`/`allocate` methods directly.
impl LayoutManager for BoxLayout {
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
    fn horizontal_preferred_sums_with_spacing() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), leaf(0.0, 0.0));
        let a = tree.create(ActorCommon::default(), leaf(40.0, 10.0));
        let b = tree.create(ActorCommon::default(), leaf(60.0, 10.0));
        tree.add_child(parent, a);
        tree.add_child(parent, b);
        let mut lay = BoxLayout::new();
        lay.set_spacing(5);
        let (min_w, nat_w) = lay.preferred_width(&tree, parent, None);
        // natural: 40 + 60 + 5 spacing = 105. min: children report min=0.0
        // (NullBehavior), so min is just the 5px spacing.
        assert_eq!((min_w, nat_w), (5.0, 105.0));
    }

    #[test]
    fn homogeneous_preferred_is_max_times_n() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), leaf(0.0, 0.0));
        let a = tree.create(ActorCommon::default(), leaf(40.0, 10.0));
        let b = tree.create(ActorCommon::default(), leaf(60.0, 10.0));
        tree.add_child(parent, a);
        tree.add_child(parent, b);
        let mut lay = BoxLayout::new();
        lay.set_homogeneous(true);
        let (min_w, nat_w) = lay.preferred_width(&tree, parent, None);
        // natural: max(40,60) * 2 = 120. min: children report min=0.0
        // (NullBehavior), so homogeneous min collapses to max(0,0) * 2 = 0.
        assert_eq!((min_w, nat_w), (0.0, 120.0));
    }

    #[test]
    fn allocate_horizontal_places_children_left_to_right() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), leaf(0.0, 0.0));
        let a = tree.create(ActorCommon::default(), leaf(40.0, 50.0));
        let b = tree.create(ActorCommon::default(), leaf(60.0, 50.0));
        tree.add_child(parent, a);
        tree.add_child(parent, b);
        let mut lay = BoxLayout::new();
        lay.allocate(&mut tree, parent, &ActorBox::new(0.0, 0.0, 100.0, 50.0));
        let a_alloc = tree.common(a).unwrap().allocation;
        let b_alloc = tree.common(b).unwrap().allocation;
        // a gets 40 at x=0, b gets 60 at x=40
        assert_eq!(a_alloc, ActorBox::new(0.0, 0.0, 40.0, 50.0));
        assert_eq!(b_alloc, ActorBox::new(40.0, 0.0, 100.0, 50.0));
    }

    #[test]
    fn allocate_distributes_extra_to_expanding_child() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), leaf(0.0, 0.0));
        let a = tree.create(ActorCommon::default(), leaf(40.0, 50.0));
        let mut b_cm = ActorCommon::default();
        b_cm.x_expand = true;
        let b = tree.create(b_cm, leaf(40.0, 50.0));
        tree.add_child(parent, a);
        tree.add_child(parent, b);
        let mut lay = BoxLayout::new();
        lay.allocate(&mut tree, parent, &ActorBox::new(0.0, 0.0, 100.0, 50.0));
        // a natural 40, b natural 40, extra 20 -> b expands to 60
        let a_alloc = tree.common(a).unwrap().allocation;
        let b_alloc = tree.common(b).unwrap().allocation;
        assert_eq!(a_alloc, ActorBox::new(0.0, 0.0, 40.0, 50.0));
        assert_eq!(b_alloc, ActorBox::new(40.0, 0.0, 100.0, 50.0));
    }

    #[test]
    fn distribute_brings_smaller_gap_up_first() {
        // `distribute_natural_allocation` doesn't touch the `actor` field,
        // but `ActorId` has no public constructor, so pull two real ids
        // from a throwaway tree.
        let mut tree = ActorTree::new();
        let a0 = tree.create(ActorCommon::default(), leaf(0.0, 0.0));
        let a1 = tree.create(ActorCommon::default(), leaf(0.0, 0.0));
        let mut sizes = [
            RequestedSize {
                actor: a0,
                minimum_size: 0.0,
                natural_size: 10.0,
            },
            RequestedSize {
                actor: a1,
                minimum_size: 0.0,
                natural_size: 30.0,
            },
        ];
        let rem = distribute_natural_allocation(20.0, &mut sizes);
        // 20 extra: spreading sorted desc by gap -> [1,0]. i=1 -> idx=1,
        // glue=(20+1)/2=10, gap=30 -> add 10, extra=10. i=0 -> idx=0,
        // glue=(10+0)/1=10, gap=10 -> add 10, extra=0. Both get +10.
        assert_eq!(rem, 0.0);
        assert_eq!(sizes[0].minimum_size, 10.0);
        assert_eq!(sizes[1].minimum_size, 10.0);
    }
}
