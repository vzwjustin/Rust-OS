//! Port of GNOME mutter's `clutter/clutter-layout-manager.{c,h}` and
//! `clutter-layout-meta.{c,h}`.
//!
//! `ClutterLayoutManager` is the abstract base class for layout policies
//! applied to a container `ClutterActor`: it controls the container's
//! preferred size and the allocation of its children. Subclasses
//! (`ClutterBinLayout`, `ClutterBoxLayout`, `ClutterFixedLayout`, ...)
//! override a small vtable (`get_preferred_width`/`get_preferred_height`/
//! `allocate`).
//!
//! # What's ported
//!
//! - The `ClutterLayoutManagerClass` vtable as a `LayoutManager` trait with
//!   `get_preferred_width`/`get_preferred_height`/`allocate`/`set_container`/
//!   `layout_changed`. Default implementations match the C `real_*`
//!   functions: the size virtuals return `(0.0, 0.0)`, `allocate` and
//!   `set_container` are no-ops, `layout_changed` is the signal class
//!   handler (a no-op in C beyond emitting).
//! - `layout_changed` + the freeze/thaw counter (`freeze_layout_change` /
//!   `thaw_layout_change` in C, stored as qdata "freeze-change"). The C
//!   code uses `g_object_get_data`/`set_data` with a `guint` level; here
//!   it's an owned `u32` on `LayoutManagerState`. `layout_changed` is a
//!   no-op while frozen, matching the C guard.
//! - `ClutterLayoutMeta` (the per-(manager,container,child) property bag):
//!   ported as a thin `LayoutMeta` struct holding the `ActorId` triple plus
//!   an opaque `Box<[u8]>`-style payload slot — see its docs for why the
//!   GObject property machinery isn't reproduced.
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery (`G_DEFINE_ABSTRACT_TYPE`, `GInitiallyUnowned`
//!   floating ref, `GParamSpec` child-property install,
//!   `clutter_layout_manager_find_child_property`/
//!   `list_child_properties`/`child_set`/`child_get`/`child_set_property`/
//!   `child_get_property`): child properties are a GObject feature
//!   (per-child `GValue` storage keyed by `GParamSpec`) with no equivalent
//!   in this port. Layout managers that need per-child data (e.g.
//!   `ClutterBinLayout`'s alignment) store it on the child's `ActorCommon`
//!   or in their own map keyed by `ActorId` instead.
//! - `clutter_layout_manager_get_child_meta` / `create_child_meta` /
//!   `get_child_meta_type`: these build a `ClutterLayoutMeta` GObject
//!   instance from the manager's registered child-meta `GType`. Without
//!   GObject there's no `GType` to register; `LayoutMeta` is a plain struct
//!   the manager can construct directly if it wants per-child metadata.
//! - `set_container` storing a back-pointer via `g_object_set_data` on the
//!   container ("clutter-layout-manager"): the container owns the manager
//!   in Rust, so the back-pointer is implicit.
//! - The `layout-changed` signal emission (`g_signal_emit`): replaced by
//!   the trait method being callable directly; callers that need
//!   notification can wrap the manager in their own observer.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no `unsafe`,
//! no external crates, and only `core`/`alloc`.

use alloc::boxed::Box;
use alloc::vec::Vec;

use super::actor::ActorId;
use super::actor_box::ActorBox;

/// Port of `ClutterLayoutManagerClass` vtable. Implement this per layout
/// policy instead of subclassing the GObject.
///
/// The container and its children are addressed by `ActorId`; the layout
/// manager is expected to be used in conjunction with an `ActorTree` (see
/// `actor.rs`) which the caller passes in via the method arguments. This
/// differs from C where the manager pulls children from the `ClutterActor`
/// container pointer directly.
pub trait LayoutManager {
    /// `get_preferred_width`: natural/min width for the container, given an
    /// optional `for_height` (`None` = unconstrained, matching `-1` in C).
    /// Default returns `(0.0, 0.0)` (matching `layout_manager_real_get_preferred_width`).
    fn get_preferred_width(&self, _container: ActorId, _for_height: Option<f32>) -> (f32, f32) {
        (0.0, 0.0)
    }

    /// `get_preferred_height`, symmetric to `get_preferred_width`.
    fn get_preferred_height(&self, _container: ActorId, _for_width: Option<f32>) -> (f32, f32) {
        (0.0, 0.0)
    }

    /// `allocate`: assign final boxes to the container's children. Default
    /// is a no-op (matching `layout_manager_real_allocate`). The container's
    /// own `allocation` is already written by the caller; this method
    /// allocates children within `allocation`.
    fn allocate(&mut self, _container: ActorId, _allocation: &ActorBox) {}

    /// `set_container`: called when the manager is attached to / detached
    /// from a container. `container` is `None` on detach. Default no-op
    /// (matching `layout_manager_real_set_container`, minus the qdata
    /// back-pointer which has no equivalent here).
    fn set_container(&mut self, _container: Option<ActorId>) {}

    /// `layout_changed` class handler. The default implementation is a
    /// no-op (the C class handler just exists to be overridden); the
    /// freeze/thaw guard is handled by `LayoutManagerState::layout_changed`,
    /// which callers should invoke instead of calling this directly.
    fn layout_changed(&mut self) {}
}

/// Freeze/thaw state for a layout manager, equivalent to the
/// "freeze-change" qdata counter in C
/// (`layout_manager_freeze_layout_change` / `_thaw_layout_change`).
///
/// `layout_changed` is suppressed while the freeze level is non-zero,
/// matching the C guard. The C code stores this as qdata on the
/// `GObject`; here it's an owned struct the caller keeps alongside its
/// `LayoutManager` impl.
#[derive(Debug, Default)]
pub struct LayoutManagerState {
    freeze_level: u32,
}

impl LayoutManagerState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Port of `clutter_layout_manager_freeze_layout_change`. Increments the
    /// freeze level; while > 0, `layout_changed` calls are suppressed.
    pub fn freeze_layout_change(&mut self) {
        self.freeze_level = self.freeze_level.saturating_add(1);
    }

    /// Port of `clutter_layout_manager_thaw_layout_change`. Decrements the
    /// freeze level. The C version `g_critical`s on a mismatched thaw
    /// (level already 0); here that's expressed by returning `false` from
    /// this method so the caller can log/handle it.
    ///
    /// Returns `true` if the thaw matched a prior freeze, `false` if there
    /// was no outstanding freeze (mismatched thaw).
    pub fn thaw_layout_change(&mut self) -> bool {
        if self.freeze_level == 0 {
            return false;
        }
        self.freeze_level -= 1;
        true
    }

    /// Current freeze level (0 = not frozen).
    pub fn freeze_level(&self) -> u32 {
        self.freeze_level
    }

    /// Port of `clutter_layout_manager_layout_changed`: invoke the manager's
    /// `layout_changed` virtual unless frozen. Returns `true` if the
    /// virtual was actually called, `false` if it was suppressed by a
    /// non-zero freeze level.
    pub fn layout_changed<M: LayoutManager + ?Sized>(&mut self, manager: &mut M) -> bool {
        if self.freeze_level > 0 {
            return false;
        }
        manager.layout_changed();
        true
    }
}

/// Port of `ClutterLayoutMeta` — the per-(manager, container, child)
/// property bag. In C this is a `GInitiallyUnowned` subclass storing the
/// three object pointers and exposing child properties via `GParamSpec`.
///
/// Without GObject there's no `GParamSpec`/`GValue` child-property system,
/// so this is reduced to the identity triple (`manager` is implicit — it's
/// the owner of the meta — so only `container` and `actor` are stored) plus
/// an opaque `payload: Box<[u8]>` slot a concrete manager can use to stash
/// its per-child data (e.g. `BinLayout`'s alignment). A real port would
/// more idiomatically define a manager-specific struct and store *that*
/// directly; the payload slot is provided for parity with the C "one meta
/// type per manager" pattern.
#[derive(Debug)]
pub struct LayoutMeta {
    /// The container actor the meta is attached to.
    pub container: ActorId,
    /// The child actor the meta describes.
    pub actor: ActorId,
    /// Opaque per-manager payload. `None` by default; a concrete manager
    /// that needs per-child data sets this to a boxed representation of its
    /// own struct.
    pub payload: Option<Box<[u8]>>,
}

impl LayoutMeta {
    /// Equivalent to `g_object_new (meta_type, "manager", manager,
    /// "container", container, "actor", actor, NULL)` minus the manager
    /// pointer (which is implicit as the owner).
    pub fn new(container: ActorId, actor: ActorId) -> Self {
        LayoutMeta {
            container,
            actor,
            payload: None,
        }
    }
}

/// A layout manager that owns no per-child state and delegates to an inner
/// `Box<dyn LayoutManager>`. Useful as the concrete type a container stores
/// when it just needs to hold "some layout manager" by ownership.
#[derive(Default)]
pub struct BoxedLayoutManager {
    inner: Option<Box<dyn LayoutManager>>,
}

impl BoxedLayoutManager {
    pub fn new(inner: Box<dyn LayoutManager>) -> Self {
        BoxedLayoutManager { inner: Some(inner) }
    }
}

impl LayoutManager for BoxedLayoutManager {
    fn get_preferred_width(&self, c: ActorId, h: Option<f32>) -> (f32, f32) {
        self.inner
            .as_deref()
            .map(|m| m.get_preferred_width(c, h))
            .unwrap_or((0.0, 0.0))
    }
    fn get_preferred_height(&self, c: ActorId, w: Option<f32>) -> (f32, f32) {
        self.inner
            .as_deref()
            .map(|m| m.get_preferred_height(c, w))
            .unwrap_or((0.0, 0.0))
    }
    fn allocate(&mut self, c: ActorId, a: &ActorBox) {
        if let Some(m) = self.inner.as_deref_mut() {
            m.allocate(c, a);
        }
    }
    fn set_container(&mut self, c: Option<ActorId>) {
        if let Some(m) = self.inner.as_deref_mut() {
            m.set_container(c);
        }
    }
    fn layout_changed(&mut self) {
        if let Some(m) = self.inner.as_deref_mut() {
            m.layout_changed();
        }
    }
}

/// Helper: count visible children of `container` in `tree`. Mirrors the
/// `clutter_actor_iter_init`/`iter_next` + `clutter_actor_is_visible` loop
/// used by every concrete layout manager's preferred-size/allocate. Returns
/// the child ids in tree order.
pub fn visible_children(tree: &super::actor::ActorTree, container: ActorId) -> Vec<ActorId> {
    tree.children(container)
        .iter()
        .copied()
        .filter(|&c| tree.common(c).map_or(false, |cm| cm.flags.visible))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::super::actor::{ActorCommon, ActorTree, NullBehavior};
    use super::*;
    use alloc::vec;

    #[derive(Default)]
    struct Counting {
        allocate_calls: u32,
        layout_changed_calls: u32,
        set_container_calls: u32,
        last_container: Option<ActorId>,
    }
    impl LayoutManager for Counting {
        fn allocate(&mut self, _c: ActorId, _a: &ActorBox) {
            self.allocate_calls += 1;
        }
        fn set_container(&mut self, c: Option<ActorId>) {
            self.set_container_calls += 1;
            self.last_container = c;
        }
        fn layout_changed(&mut self) {
            self.layout_changed_calls += 1;
        }
    }

    #[test]
    fn defaults_return_zero() {
        struct Empty;
        impl LayoutManager for Empty {}
        let m = Empty;
        let mut tree = ActorTree::new();
        let id = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        assert_eq!(m.get_preferred_width(id, None), (0.0, 0.0));
        assert_eq!(m.get_preferred_height(id, None), (0.0, 0.0));
    }

    #[test]
    fn freeze_suppresses_layout_changed() {
        let mut state = LayoutManagerState::new();
        let mut m = Counting::default();
        assert!(state.layout_changed(&mut m));
        assert_eq!(m.layout_changed_calls, 1);
        state.freeze_layout_change();
        assert!(!state.layout_changed(&mut m));
        assert_eq!(m.layout_changed_calls, 1); // still 1
        assert!(state.thaw_layout_change());
        assert!(state.layout_changed(&mut m));
        assert_eq!(m.layout_changed_calls, 2);
    }

    #[test]
    fn mismatched_thaw_returns_false() {
        let mut state = LayoutManagerState::new();
        assert!(!state.thaw_layout_change());
    }

    #[test]
    fn set_container_tracks_attach_detach() {
        let mut m = Counting::default();
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        m.set_container(Some(parent));
        m.set_container(None);
        assert_eq!(m.set_container_calls, 2);
        assert_eq!(m.last_container, None);
    }

    #[test]
    fn visible_children_filters_invisible() {
        let mut tree = ActorTree::new();
        let parent = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        let vis = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        let mut hidden_cm = ActorCommon::default();
        hidden_cm.flags.visible = false;
        let hid = tree.create(hidden_cm, Box::new(NullBehavior::default()));
        tree.add_child(parent, vis);
        tree.add_child(parent, hid);
        assert_eq!(visible_children(&tree, parent), vec![vis]);
    }
}
