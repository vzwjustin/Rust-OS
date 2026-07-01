//! Port of GNOME mutter's `clutter/clutter-content.{c,h}` and
//! `clutter-content-private.h`.
//!
//! `ClutterContent` is a GObject *interface* (not a class) implemented by
//! types responsible for painting an actor's content — e.g. `ClutterImage`,
//! `ClutterCanvas`. Multiple actors can share one `ClutterContent` instance,
//! and the content tracks the set of attached actors so it can
//! `queue_redraw`/`queue_relayout` on all of them when its data changes.
//!
//! # What's ported
//!
//! - The `ClutterContentInterface` vtable as a `Content` trait with the
//!   same virtuals: `get_preferred_size`, `paint_content`, `attached`,
//!   `detached`, `invalidate`, `invalidate_size`. Default implementations
//!   match the C `clutter_content_real_*` functions
//!   (`get_preferred_size` returns `(0.0, 0.0, false)`, the rest are no-ops).
//! - The attached-actor set (`quark_content_actors` qdata in C) as an
//!   owned `Vec<ActorId>` on `ContentAttachment`, plus `attach`/`detach`/
//!   `is_attached`/`attached_actors` accessors mirroring
//!   `_clutter_content_attached`/`_detached` and the iteration in
//!   `clutter_content_invalidate`/`_invalidate_size`.
//! - `invalidate` / `invalidate_size`: call the trait virtual, then iterate
//!   the attached actors. In C each iteration calls
//!   `clutter_actor_queue_redraw` / `_clutter_actor_queue_only_relayout`;
//!   those queue APIs aren't ported on `ActorTree` yet, so the iteration
//!   returns the list of actors that *would* be queued, leaving the actual
//!   queueing to the caller (see method docs).
//!
//! # What's skipped, with rationale
//!
//! - GObject interface machinery (`G_DEFINE_INTERFACE`, `GTypeInterface`,
//!   `g_signal_new` for `attached`/`detached`, `GQuark` qdata storage):
//!   no GObject in this port. The "signals" become the trait virtuals
//!   (`attached`/`detached`) called directly from `attach`/`detach`; the
//!   qdata actor-set becomes an owned `Vec`.
//! - `clutter_content_invalidate` / `_invalidate_size` calling
//!   `clutter_actor_queue_redraw` / `_clutter_actor_queue_only_relayout`:
//!   those actor queue APIs don't exist on `ActorTree` yet. The ported
//!   methods return the affected actor ids so the caller can queue on them
//!   once that API lands; the trait virtual is still invoked for the
//!   implementation's own bookkeeping.
//! - `clutter_content_get_preferred_size`'s `gboolean` return indicating
//!   whether the content has a meaningful size: preserved as the `bool` in
//!   `get_preferred_size`'s return tuple.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no `unsafe`,
//! no external crates, and only `core`/`alloc`.

use alloc::vec::Vec;

use super::actor::ActorId;
use super::paint_context::PaintContext;
use super::paint_node::PaintNode;

/// Port of the `ClutterContentInterface` vtable. Implement this per content
/// type (image, canvas, ...) instead of subclassing the GObject interface.
///
/// Default implementations match `clutter_content_real_*`:
/// `get_preferred_size` returns `(0.0, 0.0, false)`, the rest are no-ops.
pub trait Content {
    /// `ClutterContent::get_preferred_size`. Returns
    /// `(width, height, has_size)`; `has_size == false` means the content
    /// has no intrinsic size (matching the C default returning `FALSE`).
    fn get_preferred_size(&self) -> (f32, f32, bool) {
        (0.0, 0.0, false)
    }

    /// `ClutterContent::paint_content` — paint into the given paint-node
    /// tree. Default is a no-op (matching `clutter_content_real_paint_content`).
    fn paint_content(&self, _actor: ActorId, _node: &mut PaintNode, _ctx: &PaintContext) {}

    /// `ClutterContent::attached` — called when an actor starts using this
    /// content. Default no-op.
    fn attached(&mut self, _actor: ActorId) {}

    /// `ClutterContent::detached` — called when an actor stops using this
    /// content. Default no-op.
    fn detached(&mut self, _actor: ActorId) {}

    /// `ClutterContent::invalidate` — the content's appearance changed
    /// regardless of actor state. Default no-op. (The base
    /// `ContentAttachment::invalidate` wrapper still iterates attached
    /// actors after calling this.)
    fn invalidate(&mut self) {}

    /// `ClutterContent::invalidate_size` — the content's size changed.
    /// Default no-op.
    fn invalidate_size(&mut self) {}
}

/// The per-content attached-actor set, equivalent to the `quark_content_actors`
/// `GHashTable` hung off the `GObject` in C. Owns the list of actors currently
/// using a content instance so `invalidate`/`invalidate_size` can fan out.
///
/// In C this is qdata on the content `GObject`; here it's a separate owned
/// struct the caller keeps alongside its `Content` implementation (typically
/// inside the implementing struct, or in a wrapper).
#[derive(Debug, Default)]
pub struct ContentAttachment {
    actors: Vec<ActorId>,
}

impl ContentAttachment {
    pub fn new() -> Self {
        Self::default()
    }

    /// Port of `_clutter_content_attached`: record `actor` as using the
    /// content and return whether it was newly added. The C version always
    /// inserts into the hash table (idempotent on the set) and emits the
    /// `attached` signal; here the caller is responsible for invoking the
    /// `Content::attached` virtual on the trait object.
    pub fn attach(&mut self, actor: ActorId) -> bool {
        if self.actors.iter().any(|&a| a == actor) {
            return false;
        }
        self.actors.push(actor);
        true
    }

    /// Port of `_clutter_content_detached`: remove `actor` and return whether
    /// it was present. The C version removes from the hash table and emits
    /// `detached`; the caller invokes `Content::detached`.
    pub fn detach(&mut self, actor: ActorId) -> bool {
        let len = self.actors.len();
        self.actors.retain(|&a| a != actor);
        self.actors.len() != len
    }

    /// Whether `actor` is currently attached.
    pub fn is_attached(&self, actor: ActorId) -> bool {
        self.actors.iter().any(|&a| a == actor)
    }

    /// The currently attached actors, in insertion order.
    pub fn attached_actors(&self) -> &[ActorId] {
        &self.actors
    }

    /// Number of attached actors.
    pub fn n_attached(&self) -> usize {
        self.actors.len()
    }

    /// Port of `clutter_content_invalidate`: call `content.invalidate()`,
    /// then return the list of attached actors that would need a redraw.
    ///
    /// In C this calls `clutter_actor_queue_redraw(actor)` for each attached
    /// actor; that queue API isn't on `ActorTree` yet, so the affected actors
    /// are returned for the caller to queue on. The trait virtual is still
    /// invoked for the implementation's own invalidation bookkeeping.
    pub fn invalidate<C: Content + ?Sized>(&mut self, content: &mut C) -> Vec<ActorId> {
        content.invalidate();
        self.actors.clone()
    }

    /// Port of `clutter_content_invalidate_size`: call
    /// `content.invalidate_size()`, then return the attached actors whose
    /// request mode is `ContentSize` (which would be re-laid-out in C via
    /// `_clutter_actor_queue_only_relayout`).
    ///
    /// Filtering by `CLUTTER_REQUEST_CONTENT_SIZE` requires reading each
    /// actor's `request_mode` from the `ActorTree`; since this struct
    /// doesn't hold the tree, the filter is left to the caller and *all*
    /// attached actors are returned here. The C behavior (only
    /// `REQUEST_CONTENT_SIZE` actors get a relayout) can be recovered by
    /// the caller filtering on `ActorCommon::request_mode`.
    pub fn invalidate_size<C: Content + ?Sized>(&mut self, content: &mut C) -> Vec<ActorId> {
        content.invalidate_size();
        self.actors.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::super::actor::{ActorCommon, ActorTree, NullBehavior};
    use super::*;
    use alloc::boxed::Box;
    use alloc::vec;

    /// A content impl that records the virtuals it gets called with.
    #[derive(Default)]
    struct Recorder {
        invalidate_calls: u32,
        invalidate_size_calls: u32,
        attached: Vec<ActorId>,
        detached: Vec<ActorId>,
    }
    impl Content for Recorder {
        fn invalidate(&mut self) {
            self.invalidate_calls += 1;
        }
        fn invalidate_size(&mut self) {
            self.invalidate_size_calls += 1;
        }
        fn attached(&mut self, actor: ActorId) {
            self.attached.push(actor);
        }
        fn detached(&mut self, actor: ActorId) {
            self.detached.push(actor);
        }
    }

    fn make_id(tree: &mut ActorTree) -> ActorId {
        tree.create(ActorCommon::default(), Box::new(NullBehavior::default()))
    }

    #[test]
    fn default_preferred_size_is_zero_false() {
        let r = Recorder::default();
        let (w, h, has) = r.get_preferred_size();
        assert_eq!((w, h, has), (0.0, 0.0, false));
    }

    #[test]
    fn attach_detach_track_set() {
        let mut att = ContentAttachment::new();
        let mut tree = ActorTree::new();
        let a = make_id(&mut tree);
        let b = make_id(&mut tree);
        assert!(att.attach(a));
        assert!(!att.attach(a)); // idempotent
        assert!(att.attach(b));
        assert_eq!(att.n_attached(), 2);
        assert!(att.is_attached(a));
        assert!(att.detach(a));
        assert!(!att.is_attached(a));
        assert!(!att.detach(a)); // already gone
        assert_eq!(att.n_attached(), 1);
    }

    #[test]
    fn invalidate_fans_out_and_calls_virtual() {
        let mut att = ContentAttachment::new();
        let mut r = Recorder::default();
        let mut tree = ActorTree::new();
        let a = make_id(&mut tree);
        let b = make_id(&mut tree);
        att.attach(a);
        att.attach(b);
        let affected = att.invalidate(&mut r);
        assert_eq!(affected, vec![a, b]);
        assert_eq!(r.invalidate_calls, 1);
    }

    #[test]
    fn invalidate_size_fans_out_and_calls_virtual() {
        let mut att = ContentAttachment::new();
        let mut r = Recorder::default();
        let mut tree = ActorTree::new();
        let a = make_id(&mut tree);
        att.attach(a);
        let affected = att.invalidate_size(&mut r);
        assert_eq!(affected, vec![a]);
        assert_eq!(r.invalidate_size_calls, 1);
    }
}
