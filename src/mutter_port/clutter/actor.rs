//! Rust shape for `ClutterActor`'s portable core: the scene-graph tree,
//! layout (request-mode / preferred-size / allocate), and per-actor
//! transform state.
//!
//! # Design decision (recorded here, not in prose elsewhere)
//!
//! `ClutterActor` is a GObject base class: an intrusive doubly-linked
//! child list plus a single C struct of "private" fields, extended by
//! subclassing (`StWidget`, `MetaWindowActor`, ...) which override a
//! fixed set of virtual functions (`get_preferred_width/height`,
//! `allocate`, `paint`, `pick`, ...).
//!
//! That doesn't map to a closed Rust enum of "actor kinds" (rejected:
//! gnome-shell-style subclassing needs to keep extending behavior, and a
//! closed enum walls that off). Instead this mirrors GObject's actual
//! split:
//!
//! - **Storage**: an arena (`ActorTree`) keyed by generational `ActorId`,
//!   matching the existing convention in `desktop::window_manager`
//!   (`WindowId(usize)` + flat arena) rather than introducing a second
//!   ownership style (`Rc<RefCell<_>>` trees, intrusive pointers, etc.).
//!   Parent/children are stored as ids on the node, not as the "kind".
//! - **Common state** (`ActorCommon`): the shared instance-data struct —
//!   allocation, requested size cache, transform, flags — equivalent to
//!   `ClutterActorPrivate`.
//! - **Per-actor behavior** (`ActorBehavior` trait): the overridable
//!   vfuncs, stored as a `Box<dyn ActorBehavior>` per node — equivalent
//!   to the GObject class vtable. New actor types implement the trait
//!   instead of adding enum variants.
//!
//! # Integration seam with `desktop::window_manager` (deferred)
//!
//! `WindowManager::render` currently draws everything immediate-mode via
//! `graphics::framebuffer::{fill_rect, draw_rect, set_pixel}` — there is
//! no retained scene graph under it today. This module does **not**
//! replace that; the actor tree runs standalone until a later wave
//! decides whether `Window` becomes backed by an `ActorId` (the
//! `MetaWindowActor` pattern), the tree fully replaces `WindowManager`'s
//! render loop, or the two stay permanently parallel. That decision
//! gates the *root*/`Stage` API (how an external window enters the
//! tree) — it does not block the leaf types defined here.
//!
//! # Scope
//!
//! Only the three portable subsystems are ported: tree structure,
//! layout/allocation, and transform bookkeeping. `paint` stubs out to
//! producing a `PaintNode` (already ported, see `super::paint_node`)
//! rather than touching Cogl/GL, since no GPU backend exists yet.
//! GObject property/signal/GValue machinery, actions/constraints/effects
//! (separate subclasses in upstream), and animation/easing are not
//! ported in this wave.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

// `f32::floor`/`f32::ceil` aren't available without `std`/`libm`; hand
// -rolled the same way `mtk::rectangle` does for the same constraint.
fn floorf(x: f32) -> f32 {
    let i = x as i32 as f32;
    if x < 0.0 && i != x {
        i - 1.0
    } else {
        i
    }
}

fn ceilf(x: f32) -> f32 {
    let i = x as i32 as f32;
    if x > 0.0 && i != x {
        i + 1.0
    } else {
        i
    }
}

use super::actor_box::ActorBox;
use super::paint_context::PaintContext;
use super::paint_node::PaintNode;

/// Generational arena handle. Mirrors `WindowId`'s role in
/// `desktop::window_manager` but adds a generation counter so a stale
/// `ActorId` (actor removed, slot reused) is detectable instead of
/// silently aliasing a different actor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActorId {
    index: u32,
    generation: u32,
}

/// Mirrors `ClutterRequestMode`: which dimension an actor prefers to
/// compute first (width-for-height vs height-for-width).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RequestMode {
    #[default]
    WidthForHeight,
    HeightForWidth,
}

/// A `(min, natural)` size pair, as returned by `get_preferred_width`/
/// `get_preferred_height`.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Preferred {
    pub min: f32,
    pub natural: f32,
}

/// Mirrors the handful of `ClutterActorFlags` that affect layout/paint
/// traversal (`MAPPED`, `REALIZED`, `VISIBLE`, ...), collapsed to bools
/// since there's no GObject flags-as-bitfield-property machinery here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActorFlags {
    pub visible: bool,
    pub mapped: bool,
    pub reactive: bool,
}

impl Default for ActorFlags {
    fn default() -> Self {
        ActorFlags {
            visible: true,
            mapped: false,
            reactive: false,
        }
    }
}

/// Mirrors `ClutterActorAlign`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ActorAlign {
    Fill,
    #[default]
    Start,
    Center,
    End,
}

impl ActorAlign {
    /// The `0.0..=1.0` factor `clutter-bin-layout.c` multiplies spare
    /// space by (`FILL` and `START` both anchor to the start edge; the
    /// difference is whether the child stretches to fill, handled by
    /// the caller, not by this factor).
    pub fn factor(self) -> f32 {
        match self {
            ActorAlign::Fill | ActorAlign::Start => 0.0,
            ActorAlign::Center => 0.5,
            ActorAlign::End => 1.0,
        }
    }
}

/// Shared per-actor state, equivalent to `ClutterActorPrivate`'s
/// layout/transform fields (property storage, signals, and actions are
/// out of scope for this wave).
#[derive(Debug, Clone)]
pub struct ActorCommon {
    pub name: Option<String>,
    pub request_mode: RequestMode,
    pub allocation: ActorBox,
    pub min_width: Preferred,
    pub min_height: Preferred,
    pub flags: ActorFlags,
    /// Position/scale/rotation are tracked separately from `allocation`
    /// in upstream via a transform matrix; stubbed as a translation-only
    /// stand-in until a real 4x4 matrix type exists (see
    /// `monitor_transform.rs`'s note on the same gap).
    pub translation: (f32, f32),
    /// `clutter_actor_set_x_align`/`set_y_align`, consulted by layout
    /// managers (e.g. `BinLayout`) when placing a child inside a box
    /// larger than its preferred size.
    pub x_align: ActorAlign,
    pub y_align: ActorAlign,
    /// `clutter_actor_set_x_expand`/`set_y_expand`.
    pub x_expand: bool,
    pub y_expand: bool,
    /// `clutter_actor_set_fixed_position_set` + `fixed_x`/`fixed_y`:
    /// when set, a layout manager should place the child at this exact
    /// position instead of computing one.
    pub fixed_position: Option<(f32, f32)>,
    /// `ClutterActorPrivate::needs_allocation`: `true` until the actor has
    /// been allocated at least once. `get_x`/`get_y` consult this to decide
    /// whether to report the allocation origin or the fixed position.
    /// Defaults to `true` (matching `clutter_actor_init`).
    pub needs_allocation: bool,
    /// `ClutterActorPrivate::text_direction`, consulted by
    /// `allocate_align_fill` for the RTL `1.0 - x_align` flip. Defaults to
    /// `Ltr` (matching `CLUTTER_TEXT_DIRECTION_DEFAULT` behavior).
    pub text_direction: super::enums::TextDirection,
}

impl Default for ActorCommon {
    fn default() -> Self {
        ActorCommon {
            name: None,
            request_mode: RequestMode::default(),
            allocation: ActorBox::default(),
            min_width: Preferred::default(),
            min_height: Preferred::default(),
            flags: ActorFlags::default(),
            translation: (0.0, 0.0),
            x_align: ActorAlign::default(),
            y_align: ActorAlign::default(),
            x_expand: false,
            y_expand: false,
            fixed_position: None,
            // `#[derive(Default)]` would give `false` here, which is wrong:
            // upstream `clutter_actor_init` starts every actor unallocated.
            needs_allocation: true,
            text_direction: super::enums::TextDirection::default(),
        }
    }
}

/// The overridable behavior of an actor — equivalent to the subset of
/// `ClutterActorClass`'s vtable that matters once GObject machinery is
/// stripped out. Implement this per actor type instead of adding a
/// variant to a closed enum.
pub trait ActorBehavior {
    /// `get_preferred_width`: natural/min width, optionally for a given
    /// available height (`None` = unconstrained), matching
    /// `request_mode == WidthForHeight` callers that pass `-1`.
    fn preferred_width(&self, common: &ActorCommon, for_height: Option<f32>) -> Preferred;

    /// `get_preferred_height`, symmetric to `preferred_width`.
    fn preferred_height(&self, common: &ActorCommon, for_width: Option<f32>) -> Preferred;

    /// `allocate`: assign this actor's final box. Default does nothing
    /// beyond what `ActorTree::allocate` already wrote into
    /// `common.allocation` — override to additionally re-allocate
    /// children with custom layout policy (box-layout, grid, etc.).
    fn allocate(
        &mut self,
        _common: &mut ActorCommon,
        _children: &[ActorId],
        _tree: &mut ActorTree,
    ) {
    }

    /// `paint`: produce this actor's contribution to the retained-mode
    /// render tree. Default emits an empty `Root` node (no visible
    /// output) — concrete actors (color rect, texture, ...) override.
    fn paint(&self, _common: &ActorCommon, _ctx: &PaintContext) -> PaintNode {
        PaintNode::new(super::paint_node::PaintNodeKind::Root)
    }
}

/// Default leaf behavior: fixed natural size, no children re-layout, no
/// paint output. Useful for placeholder/container actors in tests.
#[derive(Debug, Default)]
pub struct NullBehavior {
    pub natural_width: f32,
    pub natural_height: f32,
}

impl ActorBehavior for NullBehavior {
    fn preferred_width(&self, _common: &ActorCommon, _for_height: Option<f32>) -> Preferred {
        Preferred {
            min: 0.0,
            natural: self.natural_width,
        }
    }

    fn preferred_height(&self, _common: &ActorCommon, _for_width: Option<f32>) -> Preferred {
        Preferred {
            min: 0.0,
            natural: self.natural_height,
        }
    }
}

struct ActorSlot {
    generation: u32,
    node: Option<ActorNode>,
}

struct ActorNode {
    common: ActorCommon,
    behavior: Box<dyn ActorBehavior>,
    parent: Option<ActorId>,
    children: Vec<ActorId>,
}

/// Arena owning all actors in one scene graph, addressed by
/// generational `ActorId`. Mirrors `window_manager::WindowManager`'s
/// flat-storage convention rather than an `Rc<RefCell<_>>` tree.
#[derive(Default)]
pub struct ActorTree {
    slots: Vec<ActorSlot>,
    free_list: Vec<u32>,
}

impl ActorTree {
    pub fn new() -> Self {
        ActorTree {
            slots: Vec::new(),
            free_list: Vec::new(),
        }
    }

    /// `clutter_actor_new` + setting up an empty child list.
    pub fn create(&mut self, common: ActorCommon, behavior: Box<dyn ActorBehavior>) -> ActorId {
        let node = ActorNode {
            common,
            behavior,
            parent: None,
            children: Vec::new(),
        };
        if let Some(index) = self.free_list.pop() {
            let slot = &mut self.slots[index as usize];
            slot.generation = slot.generation.wrapping_add(1);
            slot.node = Some(node);
            ActorId {
                index,
                generation: slot.generation,
            }
        } else {
            let index = self.slots.len() as u32;
            self.slots.push(ActorSlot {
                generation: 0,
                node: Some(node),
            });
            ActorId {
                index,
                generation: 0,
            }
        }
    }

    fn slot(&self, id: ActorId) -> Option<&ActorNode> {
        self.slots
            .get(id.index as usize)
            .filter(|s| s.generation == id.generation)
            .and_then(|s| s.node.as_ref())
    }

    fn slot_mut(&mut self, id: ActorId) -> Option<&mut ActorNode> {
        self.slots
            .get_mut(id.index as usize)
            .filter(|s| s.generation == id.generation)
            .and_then(|s| s.node.as_mut())
    }

    pub fn common(&self, id: ActorId) -> Option<&ActorCommon> {
        self.slot(id).map(|n| &n.common)
    }

    pub fn common_mut(&mut self, id: ActorId) -> Option<&mut ActorCommon> {
        self.slot_mut(id).map(|n| &mut n.common)
    }

    pub fn children(&self, id: ActorId) -> &[ActorId] {
        self.slot(id).map(|n| n.children.as_slice()).unwrap_or(&[])
    }

    pub fn parent(&self, id: ActorId) -> Option<ActorId> {
        self.slot(id).and_then(|n| n.parent)
    }

    /// `clutter_actor_add_child`. Detaches the child from any prior
    /// parent first (upstream asserts on this; here we just re-parent).
    pub fn add_child(&mut self, parent: ActorId, child: ActorId) {
        self.remove_from_parent(child);
        if let Some(node) = self.slot_mut(parent) {
            node.children.push(child);
        }
        if let Some(node) = self.slot_mut(child) {
            node.parent = Some(parent);
        }
    }

    /// `clutter_actor_remove_child`.
    pub fn remove_from_parent(&mut self, child: ActorId) {
        if let Some(parent) = self.parent(child) {
            if let Some(node) = self.slot_mut(parent) {
                node.children.retain(|c| *c != child);
            }
        }
        if let Some(node) = self.slot_mut(child) {
            node.parent = None;
        }
    }

    /// `clutter_actor_destroy`: drops the node and recursively destroys
    /// children (upstream unparents children rather than destroying
    /// them in some paths; this wave always destroys recursively, the
    /// simpler of the two upstream behaviors — revisit if a caller
    /// needs orphan-not-destroy semantics).
    pub fn destroy(&mut self, id: ActorId) {
        let children: Vec<ActorId> = self.children(id).to_vec();
        for child in children {
            self.destroy(child);
        }
        self.remove_from_parent(id);
        if let Some(slot) = self.slots.get_mut(id.index as usize) {
            if slot.generation == id.generation {
                slot.node = None;
                self.free_list.push(id.index);
            }
        }
    }

    /// `clutter_actor_get_preferred_width`, dispatched through the
    /// actor's behavior.
    pub fn preferred_width(&self, id: ActorId, for_height: Option<f32>) -> Preferred {
        match self.slot(id) {
            Some(node) => node.behavior.preferred_width(&node.common, for_height),
            None => Preferred::default(),
        }
    }

    pub fn preferred_height(&self, id: ActorId, for_width: Option<f32>) -> Preferred {
        match self.slot(id) {
            Some(node) => node.behavior.preferred_height(&node.common, for_width),
            None => Preferred::default(),
        }
    }

    /// `clutter_actor_get_x`: if the actor still needs allocation, return
    /// the fixed-position X (or 0 if none); otherwise return the
    /// allocation origin X.
    pub fn get_x(&self, id: ActorId) -> f32 {
        match self.slot(id) {
            Some(node) => {
                if node.common.needs_allocation {
                    node.common.fixed_position.map_or(0.0, |(x, _)| x)
                } else {
                    node.common.allocation.x1
                }
            }
            None => 0.0,
        }
    }

    /// `clutter_actor_get_y`, symmetric to `get_x`.
    pub fn get_y(&self, id: ActorId) -> f32 {
        match self.slot(id) {
            Some(node) => {
                if node.common.needs_allocation {
                    node.common.fixed_position.map_or(0.0, |(_, y)| y)
                } else {
                    node.common.allocation.y1
                }
            }
            None => 0.0,
        }
    }

    /// `clutter_actor_get_size`: returns the allocation size if allocated,
    /// otherwise the natural preferred size. Returns `(0.0, 0.0)` for a
    /// missing actor.
    pub fn get_size(&self, id: ActorId) -> (f32, f32) {
        match self.slot(id) {
            Some(node) => {
                if node.common.needs_allocation {
                    let w = node.behavior.preferred_width(&node.common, None).natural;
                    let h = node.behavior.preferred_height(&node.common, None).natural;
                    (w.max(0.0), h.max(0.0))
                } else {
                    let a = &node.common.allocation;
                    (a.width(), a.height())
                }
            }
            None => (0.0, 0.0),
        }
    }

    /// `clutter_actor_get_fixed_position`: returns the fixed `(x, y)` if
    /// `fixed_position` is set, `None` otherwise.
    /// `clutter_actor_get_allocation_box`: the actor's last-allocated
    /// box, or a zero-sized box at the origin if it hasn't been
    /// allocated yet.
    pub fn get_allocation(&self, id: ActorId) -> ActorBox {
        match self.slot(id) {
            Some(node) if !node.common.needs_allocation => node.common.allocation,
            _ => ActorBox::default(),
        }
    }

    pub fn get_fixed_position(&self, id: ActorId) -> Option<(f32, f32)> {
        self.common(id).and_then(|c| c.fixed_position)
    }

    /// `clutter_actor_get_x_align`.
    pub fn get_x_align(&self, id: ActorId) -> ActorAlign {
        self.common(id).map_or(ActorAlign::default(), |c| c.x_align)
    }

    /// `clutter_actor_get_y_align`.
    pub fn get_y_align(&self, id: ActorId) -> ActorAlign {
        self.common(id).map_or(ActorAlign::default(), |c| c.y_align)
    }

    /// `clutter_actor_needs_expand`: visible + the expand flag for the
    /// given orientation. (Upstream also recurses into children via
    /// `clutter_actor_compute_expand`; that propagation is not ported —
    /// this reports the actor's own `x_expand`/`y_expand` only, which is
    /// sufficient for the ported layout managers since they query each
    /// child directly.)
    pub fn needs_expand(&self, id: ActorId, orientation: super::enums::Orientation) -> bool {
        match self.slot(id) {
            Some(node) => {
                if !node.common.flags.visible {
                    return false;
                }
                match orientation {
                    super::enums::Orientation::Horizontal => node.common.x_expand,
                    super::enums::Orientation::Vertical => node.common.y_expand,
                }
            }
            None => false,
        }
    }

    /// `clutter_actor_allocate_preferred_size`: allocate the actor at
    /// `(x, y)` with its natural preferred size.
    pub fn allocate_preferred_size(&mut self, id: ActorId, x: f32, y: f32) {
        let (nat_w, nat_h) = match self.slot(id) {
            Some(node) => (
                node.behavior.preferred_width(&node.common, None).natural,
                node.behavior.preferred_height(&node.common, None).natural,
            ),
            None => return,
        };
        let box_ = ActorBox::new(x, y, x + nat_w, y + nat_h);
        self.allocate(id, box_);
    }

    /// `clutter_actor_allocate_align_fill`: allocate `id` inside `box`,
    /// applying `x_align`/`y_align` factors and `x_fill`/`y_fill` flags.
    /// Honors `request_mode` (width-for-height vs height-for-width) and
    /// the RTL text-direction flip, matching the C implementation. The
    /// `CLUTTER_REQUEST_CONTENT_SIZE` branch (which consults a
    /// `ClutterContent`'s preferred size) is folded in as the
    /// `content_size` fallback: callers without a content pass `None` and
    /// get the width-for-height/height-for-width path only.
    pub fn allocate_align_fill(
        &mut self,
        id: ActorId,
        box_: &ActorBox,
        x_align: f32,
        y_align: f32,
        x_fill: bool,
        y_fill: bool,
        content_size: Option<(f32, f32)>,
    ) {
        let (x_offset, y_offset) = box_.origin();
        let mut available_w = box_.width();
        let mut available_h = box_.height();
        if available_w <= 0.0 {
            available_w = 0.0;
        }
        if available_h <= 0.0 {
            available_h = 0.0;
        }

        let mut child_w = 0.0_f32;
        let mut child_h = 0.0_f32;
        let mut x1 = x_offset;
        let mut y1 = y_offset;

        if available_w == 0.0 && available_h == 0.0 {
            let alloc = ActorBox::new(floorf(x1), floorf(y1), ceilf(x1), ceilf(y1));
            self.allocate(id, alloc);
            return;
        }

        if x_fill {
            child_w = available_w;
        }
        if y_fill {
            child_h = available_h;
        }
        if x_fill && y_fill {
            let alloc = ActorBox::new(
                floorf(x1),
                floorf(y1),
                ceilf(x1 + child_w.max(0.0)),
                ceilf(y1 + child_h.max(0.0)),
            );
            self.allocate(id, alloc);
            return;
        }

        let request_mode = self
            .common(id)
            .map_or(RequestMode::default(), |c| c.request_mode);
        match request_mode {
            RequestMode::HeightForWidth => {
                if !x_fill {
                    let p = self.preferred_width(id, Some(available_h));
                    child_w = p.natural.clamp(p.min, available_w);
                }
                if !y_fill {
                    let p = self.preferred_height(id, Some(child_w));
                    child_h = p.natural.clamp(p.min, available_h);
                }
            }
            RequestMode::WidthForHeight => {
                if !y_fill {
                    let p = self.preferred_height(id, Some(available_w));
                    child_h = p.natural.clamp(p.min, available_h);
                }
                if !x_fill {
                    let p = self.preferred_width(id, Some(child_h));
                    child_w = p.natural.clamp(p.min, available_w);
                }
            }
        }

        // `CLUTTER_REQUEST_CONTENT_SIZE` + content branch: if a content
        // preferred size was supplied, clamp to it.
        if let Some((nat_w, nat_h)) = content_size {
            if !x_fill {
                child_w = nat_w.clamp(0.0, available_w);
            }
            if !y_fill {
                child_h = nat_h.clamp(0.0, available_h);
            }
        }

        // RTL flip.
        let rtl = self.common(id).map_or(false, |c| {
            c.text_direction == super::enums::TextDirection::Rtl
        });
        let x_align = if rtl { 1.0 - x_align } else { x_align };

        if !x_fill {
            x1 += (available_w - child_w) * x_align;
        }
        if !y_fill {
            y1 += (available_h - child_h) * y_align;
        }

        let alloc = ActorBox::new(
            floorf(x1),
            floorf(y1),
            ceilf(x1 + child_w.max(0.0)),
            ceilf(y1 + child_h.max(0.0)),
        );
        self.allocate(id, alloc);
    }

    /// `clutter_actor_allocate`: write the final box into
    /// `common.allocation`, then let the behavior re-allocate children
    /// (box-layout etc.) before recursing. The default `ActorBehavior`
    /// impl does no child layout, matching `ClutterActor`'s own base
    /// implementation (a plain actor with manually-positioned children
    /// must allocate them itself, same as upstream).
    pub fn allocate(&mut self, id: ActorId, allocation: ActorBox) {
        let children = {
            let node = match self.slot_mut(id) {
                Some(n) => n,
                None => return,
            };
            node.common.allocation = allocation;
            // `clutter_actor_allocate` clears `needs_allocation` once the
            // actor has been given a real box, so subsequent `get_x`/`get_y`
            // report the allocation origin instead of the fixed position.
            node.common.needs_allocation = false;
            node.children.clone()
        };
        self.run_behavior_allocate(id, &children);
    }

    fn run_behavior_allocate(&mut self, id: ActorId, children: &[ActorId]) {
        // `ActorBehavior::allocate` takes `&mut ActorTree` to let
        // box-layout-style behaviors recurse into `self.allocate(...)`
        // for children, which conflicts with also holding `&mut
        // ActorNode` for `id`. Temporarily move the `Box<dyn
        // ActorBehavior>` out (cheap: one pointer), call it, then put
        // it back — the same trick `Option::take` enables for `&mut
        // self` recursive calls.
        let mut behavior = match self.slot_mut(id) {
            Some(node) => match Self::take_behavior(node) {
                Some(b) => b,
                None => return,
            },
            None => return,
        };
        if let Some(node) = self.slot_mut(id) {
            let mut common = core::mem::take(&mut node.common);
            behavior.allocate(&mut common, children, self);
            if let Some(node) = self.slot_mut(id) {
                node.common = common;
                node.behavior = behavior;
            }
        }
    }

    fn take_behavior(node: &mut ActorNode) -> Option<Box<dyn ActorBehavior>> {
        // Placeholder swap target so `node.behavior` can be moved out;
        // replaced with the real value once `run_behavior_allocate`
        // finishes. `NullBehavior` is a harmless default since it's
        // immediately overwritten.
        Some(core::mem::replace(
            &mut node.behavior,
            Box::new(NullBehavior::default()),
        ))
    }

    /// `clutter_actor_paint`: depth-first traversal building a
    /// `PaintNode` tree, matching `paint_node::PaintNode::paint`'s own
    /// child-recursion order. Visibility (`common.flags.visible`) is
    /// honored the same way `CLUTTER_ACTOR_IS_VISIBLE` gates upstream's
    /// `clutter_actor_paint`.
    pub fn paint(&self, id: ActorId, ctx: &PaintContext) -> Option<PaintNode> {
        let node = self.slot(id)?;
        if !node.common.flags.visible {
            return None;
        }
        let mut root = node.behavior.paint(&node.common, ctx);
        for &child in &node.children {
            if let Some(child_node) = self.paint(child, ctx) {
                root.add_child(child_node);
            }
        }
        Some(root)
    }

    pub fn len(&self) -> usize {
        self.slots.iter().filter(|s| s.node.is_some()).count()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(w: f32, h: f32) -> Box<dyn ActorBehavior> {
        Box::new(NullBehavior {
            natural_width: w,
            natural_height: h,
        })
    }

    #[test]
    fn create_and_destroy_tracks_len() {
        let mut tree = ActorTree::new();
        let a = tree.create(ActorCommon::default(), leaf(10.0, 10.0));
        assert_eq!(tree.len(), 1);
        tree.destroy(a);
        assert_eq!(tree.len(), 0);
    }

    #[test]
    fn generation_guards_against_stale_id() {
        let mut tree = ActorTree::new();
        let a = tree.create(ActorCommon::default(), leaf(1.0, 1.0));
        tree.destroy(a);
        let b = tree.create(ActorCommon::default(), leaf(2.0, 2.0));
        // `b` reuses `a`'s slot index but bumps the generation, so the
        // stale `a` handle must not resolve to `b`'s data.
        assert_eq!(a.index, b.index);
        assert_ne!(a.generation, b.generation);
        assert!(tree.common(a).is_none());
        assert!(tree.common(b).is_some());
    }

    #[test]
    fn add_child_reparents_and_tracks_children() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), leaf(100.0, 100.0));
        let child = tree.create(ActorCommon::default(), leaf(10.0, 10.0));
        tree.add_child(parent, child);
        assert_eq!(tree.children(parent), &[child]);
        assert_eq!(tree.parent(child), Some(parent));

        let other_parent = tree.create(ActorCommon::default(), leaf(50.0, 50.0));
        tree.add_child(other_parent, child);
        assert!(tree.children(parent).is_empty());
        assert_eq!(tree.children(other_parent), &[child]);
        assert_eq!(tree.parent(child), Some(other_parent));
    }

    #[test]
    fn destroy_recurses_into_children() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), leaf(1.0, 1.0));
        let child = tree.create(ActorCommon::default(), leaf(1.0, 1.0));
        tree.add_child(parent, child);
        tree.destroy(parent);
        assert_eq!(tree.len(), 0);
        assert!(tree.common(child).is_none());
    }

    #[test]
    fn preferred_size_dispatches_through_behavior() {
        let mut tree = ActorTree::new();
        let a = tree.create(ActorCommon::default(), leaf(42.0, 24.0));
        assert_eq!(tree.preferred_width(a, None).natural, 42.0);
        assert_eq!(tree.preferred_height(a, None).natural, 24.0);
    }

    #[test]
    fn allocate_writes_allocation_box() {
        let mut tree = ActorTree::new();
        let a = tree.create(ActorCommon::default(), leaf(10.0, 10.0));
        let box_ = ActorBox::new(0.0, 0.0, 100.0, 50.0);
        tree.allocate(a, box_);
        assert_eq!(tree.common(a).unwrap().allocation, box_);
    }

    #[test]
    fn paint_skips_invisible_actors() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), leaf(1.0, 1.0));
        let mut hidden_common = ActorCommon::default();
        hidden_common.flags.visible = false;
        let hidden = tree.create(hidden_common, leaf(1.0, 1.0));
        tree.add_child(parent, hidden);

        let ctx = PaintContext::new_for_framebuffer(
            super::super::paint_context::Framebuffer,
            None,
            super::super::paint_context::PaintFlag::NONE,
            super::super::paint_context::ColorState::srgb(),
        );
        let root = tree.paint(parent, &ctx).unwrap();
        assert_eq!(root.n_children(), 0);
    }

    #[test]
    fn paint_includes_visible_children() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), leaf(1.0, 1.0));
        let child = tree.create(ActorCommon::default(), leaf(1.0, 1.0));
        tree.add_child(parent, child);

        let ctx = PaintContext::new_for_framebuffer(
            super::super::paint_context::Framebuffer,
            None,
            super::super::paint_context::PaintFlag::NONE,
            super::super::paint_context::ColorState::srgb(),
        );
        let root = tree.paint(parent, &ctx).unwrap();
        assert_eq!(root.n_children(), 1);
    }
}
