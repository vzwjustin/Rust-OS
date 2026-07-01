//! Port of GNOME mutter's `clutter/clutter-grab.{c,h}` and
//! `clutter-grab-private.h`, plus the grab-list management from
//! `clutter-stage.c` (`clutter_grab_activate`/`_dismiss`/
//! `clutter_stage_unlink_grab`).
//!
//! `ClutterGrab` is the opaque handle returned by `clutter_stage_grab`:
//! it represents an input grab redirecting events to a specific actor.
//! Grabs form a doubly-linked list (the "grab stack") on the stage; the
//! topmost grab receives events. Dismissing a grab removes it from the
//! stack and notifies the new topmost grab.
//!
//! # What's ported
//!
//! - The `ClutterGrab` struct fields (`stage`, `actor`, `owns_actor`,
//!   `prev`, `next`) as a `Grab` struct. `stage` is dropped (no
//!   `Stage` port yet); the grab-list management is on a `GrabStack`
//!   that stands in for the stage's `topmost_grab` pointer.
//! - `clutter_grab_new`: `Grab::new(actor, owns_actor)`.
//! - `clutter_grab_is_revoked`: a grab is revoked when it's been unlinked
//!   from the stack — expressed as `prev.is_none() && next.is_none() &&
//!   !is_linked` (matching the C `grab->prev != NULL` check, which is
//!   inverted: in C `prev != NULL` means "still linked"; here the
//!   equivalent is "linked into a stack").
//! - `clutter_grab_activate` / `clutter_stage_unlink_grab` /
//!   `clutter_grab_dismiss`: the grab-stack push/unlink, ported as
//!   `GrabStack::activate`/`dismiss`. The C code uses raw pointer
//!   linked-list surgery; the port uses `Vec<Grab>` indices plus a
//!   `linked: bool` flag per grab (simpler and safe).
//! - The `owns_actor` semantics: when a grab that owns its actor is
//!   dismissed, the actor should be destroyed. The C version calls
//!   `clutter_actor_destroy`; here `dismiss` returns the `ActorId` of
//!   the actor to destroy (if `owns_actor`), so the caller can destroy
//!   it via `ActorTree::destroy` — keeping the grab module free of
//!   `ActorTree` coupling.
//! - The "notify the new topmost grab" behavior: `dismiss` returns
//!   whether the topmost grab changed (so the caller can fire the
//!   equivalent of `clutter_grab_notify`).
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery (`G_DEFINE_FINAL_TYPE`, `GParamSpec` for the
//!   `revoked` property, `g_object_notify_by_pspec`): plain fields +
//!   return values.
//! - `ClutterStage *stage` back-pointer: no `Stage` port. The grab stack
//!   is a standalone `GrabStack` struct; a future `Stage` port can own
//!   one.
//! - `clutter_actor_attach_grab`/`_detach_grab`: these track per-actor
//!   grab counts on the actor; no actor-grab-count storage on
//!   `ActorTree` yet. Deferred to the actor-grab-storage wave.
//! - `clutter_stage_notify_grab`: emits crossing events on grab
//!   changes; needs the event dispatch + stage machinery. Deferred.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use alloc::vec::Vec;

/// The kind of grab, distinguishing different grab policies (move,
/// resize, menu, ...). The Mutter `ClutterGrab` doesn't have this —
/// it's just an actor grab — but downstream consumers (like the window
/// manager) need to know what kind of grab is active to dispatch mouse
/// motion correctly. This is a RustOS extension to the ported type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum GrabKind {
    /// A generic actor grab (the Mutter default — events go to the
    /// actor, no special motion handling).
    #[default]
    Actor,
    /// A move grab: mouse motion moves the actor.
    Move,
    /// A resize grab: mouse motion resizes the actor.
    Resize,
}

/// Port of `ClutterGrab` / `struct _ClutterGrab`.
///
/// Generic over the actor-id type so it can be used with both
/// `mutter_port::clutter::actor::ActorId` (the generation-based id) and
/// `desktop::window_manager::WindowId` (a plain `usize` wrapper) without
/// coupling. The `Id` type must be `Copy + Eq + PartialEq + Debug` (the
/// operations on a grab stack only compare and store ids).
///
/// A grab is owned by the `GrabStack` it's linked into; the `linked`
/// flag tracks whether it's currently in a stack (the C version uses
/// `prev != NULL` for this, but the topmost grab has `prev == NULL`
/// while still linked, so a separate flag is clearer).
#[derive(Debug, Clone, PartialEq)]
pub struct Grab<Id: Copy + Eq + PartialEq + core::fmt::Debug> {
    /// The actor receiving grabbed events (`ClutterGrab::actor`).
    pub actor: Id,
    /// `ClutterGrab::owns_actor`: if true, dismissing the grab should
    /// destroy the actor.
    pub owns_actor: bool,
    /// The kind of grab (move/resize/actor). RustOS extension.
    pub kind: GrabKind,
    /// Whether this grab is currently linked into a `GrabStack` (the
    /// C `prev`/`next` pointers' "is this in the list" invariant).
    pub linked: bool,
}

impl<Id: Copy + Eq + PartialEq + core::fmt::Debug> Grab<Id> {
    /// `clutter_grab_new` (minus the stage back-pointer): construct a
    /// grab for `actor`. The grab is not yet linked into a stack; call
    /// `GrabStack::activate` to link it.
    pub fn new(actor: Id, owns_actor: bool) -> Self {
        Grab {
            actor,
            owns_actor,
            kind: GrabKind::Actor,
            linked: false,
        }
    }

    /// Construct a grab with a specific kind (move/resize/actor).
    /// RustOS extension for the window manager's drag/resize grabs.
    pub fn with_kind(actor: Id, owns_actor: bool, kind: GrabKind) -> Self {
        Grab {
            actor,
            owns_actor,
            kind,
            linked: false,
        }
    }

    /// `clutter_grab_is_revoked`: a grab is revoked when it's been
    /// unlinked from the stack.
    pub fn is_revoked(&self) -> bool {
        !self.linked
    }
}

/// The grab stack — stands in for `ClutterStagePrivate::topmost_grab`
/// and the linked-list management in `clutter-stage.c`. The topmost
/// grab is the last element of `grabs` (matching the C "topmost =
/// head, push prepends" convention; here we push to the end and treat
/// the end as topmost for simpler indexing).
#[derive(Debug)]
pub struct GrabStack<Id: Copy + Eq + PartialEq + core::fmt::Debug> {
    grabs: Vec<Grab<Id>>,
}

impl<Id: Copy + Eq + PartialEq + core::fmt::Debug> Default for GrabStack<Id> {
    fn default() -> Self {
        GrabStack { grabs: Vec::new() }
    }
}

impl<Id: Copy + Eq + PartialEq + core::fmt::Debug> GrabStack<Id> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of active grabs.
    pub fn len(&self) -> usize {
        self.grabs.len()
    }

    /// Whether the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.grabs.is_empty()
    }

    /// The topmost (most recently activated) grab, or `None` if empty.
    /// Mirrors `clutter_stage_get_grab_actor`'s underlying
    /// `priv->topmost_grab` access.
    pub fn topmost(&self) -> Option<&Grab<Id>> {
        self.grabs.last()
    }

    /// The actor receiving grabbed events, or `None` if no grab is
    /// active. Mirrors `clutter_stage_get_grab_actor`.
    pub fn grab_actor(&self) -> Option<Id> {
        self.grabs.last().map(|g| g.actor)
    }

    /// `clutter_grab_activate`: push `grab` onto the stack as the new
    /// topmost. Returns whether the grab was newly linked (matching the
    /// C early-return "this grab is already active" guard: if the grab
    /// is already linked, this is a no-op).
    ///
    /// The grab is consumed and stored; the caller can track it by
    /// index (`len() - 1` after activation).
    pub fn activate(&mut self, mut grab: Grab<Id>) -> bool {
        if grab.linked {
            return false; // already active, no-op (C early-return)
        }
        grab.linked = true;
        self.grabs.push(grab);
        true
    }

    /// `clutter_grab_dismiss` / `clutter_stage_unlink_grab`: remove the
    /// grab at `index` from the stack. Returns a `DismissOutcome`
    /// describing what the caller should do:
    /// - `actor_to_destroy`: if the grab owned its actor, the actor id
    ///   to destroy via `ActorTree::destroy` (matching the C
    ///   `g_clear_pointer(&grab->actor, clutter_actor_destroy)`).
    /// - `topmost_changed`: whether the topmost grab changed (so the
    ///   caller can fire `clutter_grab_notify` on the new topmost).
    ///
    /// Returns `None` if `index` is out of bounds or the grab isn't
    /// linked (matching the C "already detached" early-return).
    pub fn dismiss(&mut self, index: usize) -> Option<DismissOutcome<Id>> {
        let len = self.grabs.len();
        if index >= len {
            return None;
        }
        // Extract the info we need before removing (avoiding overlapping
        // borrows of `self.grabs`).
        let (owns_actor, actor, linked) = {
            let grab = &self.grabs[index];
            (grab.owns_actor, grab.actor, grab.linked)
        };
        if !linked {
            return None;
        }
        let was_topmost = index == len - 1;
        let actor_to_destroy = if owns_actor { Some(actor) } else { None };
        // Unlink: remove from the vec.
        let removed = self.grabs.remove(index);
        debug_assert!(removed.linked);
        let topmost_changed = was_topmost && !self.grabs.is_empty();
        Some(DismissOutcome {
            actor_to_destroy,
            topmost_changed,
        })
    }

    /// Dismiss the topmost grab. Convenience for
    /// `dismiss(len() - 1)`.
    pub fn dismiss_topmost(&mut self) -> Option<DismissOutcome<Id>> {
        let index = self.grabs.len().checked_sub(1)?;
        self.dismiss(index)
    }

    /// Iterate over the grabs from bottom to top.
    pub fn iter(&self) -> impl Iterator<Item = &Grab<Id>> {
        self.grabs.iter()
    }
}

/// The result of `GrabStack::dismiss`: what the caller should do after
/// a grab is removed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DismissOutcome<Id: Copy + Eq + PartialEq + core::fmt::Debug> {
    /// If the dismissed grab owned its actor, this is the actor to
    /// destroy (via `ActorTree::destroy`). `None` otherwise.
    pub actor_to_destroy: Option<Id>,
    /// Whether the topmost grab changed (the caller should notify the
    /// new topmost grab, matching `clutter_grab_notify(priv->topmost_grab)`).
    pub topmost_changed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_grab_is_revoked_until_activated() {
        let g = Grab::new(1u32, false);
        assert!(g.is_revoked());
        assert!(!g.linked);
    }

    #[test]
    fn activate_pushes_and_links() {
        let mut stack = GrabStack::new();
        assert!(stack.is_empty());
        assert!(stack.activate(Grab::new(1u32, false)));
        assert_eq!(stack.len(), 1);
        assert_eq!(stack.grab_actor(), Some(1u32));
        // The stored grab is linked (not revoked).
        assert!(!stack.topmost().unwrap().is_revoked());
    }

    #[test]
    fn activate_on_already_linked_is_noop() {
        let mut stack = GrabStack::new();
        let g = Grab::new(1u32, false);
        assert!(stack.activate(g.clone()));
        // Activating the same (already-linked) grab is a no-op.
        assert!(!stack.activate(g));
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn dismiss_removes_and_reports_outcome() {
        let mut stack = GrabStack::new();
        stack.activate(Grab::new(1u32, false));
        stack.activate(Grab::new(2u32, false));
        // Dismiss the topmost (index 1).
        let outcome = stack.dismiss(1).unwrap();
        assert_eq!(outcome.actor_to_destroy, None);
        // was_topmost=true, stack non-empty after removal ->
        // topmost_changed=true (the new topmost is grab 1).
        assert!(outcome.topmost_changed);
        assert_eq!(stack.len(), 1);
        assert_eq!(stack.grab_actor(), Some(1u32));
    }

    #[test]
    fn dismiss_only_grab_leaves_empty() {
        let mut stack = GrabStack::new();
        stack.activate(Grab::new(1u32, false));
        let outcome = stack.dismiss(0).unwrap();
        assert_eq!(outcome.actor_to_destroy, None);
        assert!(!outcome.topmost_changed); // stack is empty after
        assert!(stack.is_empty());
    }

    #[test]
    fn dismiss_owned_actor_returns_actor_to_destroy() {
        let mut stack = GrabStack::new();
        stack.activate(Grab::new(5u32, true));
        let outcome = stack.dismiss(0).unwrap();
        assert_eq!(outcome.actor_to_destroy, Some(5u32));
    }

    #[test]
    fn dismiss_non_topmost_does_not_change_topmost() {
        let mut stack = GrabStack::new();
        stack.activate(Grab::new(1u32, false)); // index 0
        stack.activate(Grab::new(2u32, false)); // index 1 (topmost)
        let outcome = stack.dismiss(0).unwrap();
        assert!(!outcome.topmost_changed); // topmost (index 1, now 0) unchanged
        assert_eq!(stack.grab_actor(), Some(2u32));
    }

    #[test]
    fn dismiss_out_of_bounds_returns_none() {
        let mut stack = GrabStack::new();
        stack.activate(Grab::new(1u32, false));
        assert!(stack.dismiss(5).is_none());
        assert!(stack.dismiss(0).is_some());
        assert!(stack.dismiss(0).is_none()); // now empty
    }

    #[test]
    fn dismiss_topmost_convenience() {
        let mut stack = GrabStack::new();
        stack.activate(Grab::new(1u32, false));
        stack.activate(Grab::new(2u32, false));
        let outcome = stack.dismiss_topmost().unwrap();
        assert!(outcome.topmost_changed);
        assert_eq!(stack.grab_actor(), Some(1u32));
    }

    #[test]
    fn iter_goes_bottom_to_top() {
        let mut stack = GrabStack::new();
        stack.activate(Grab::new(1u32, false));
        stack.activate(Grab::new(2u32, false));
        stack.activate(Grab::new(3u32, false));
        let actors: Vec<u32> = stack.iter().map(|g| g.actor).collect();
        assert_eq!(actors, vec![1u32, 2u32, 3u32]);
    }
}
