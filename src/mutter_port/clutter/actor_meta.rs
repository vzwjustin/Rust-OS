//! Port of GNOME mutter's `clutter/clutter-actor-meta.{c,h}` and
//! `clutter-actor-meta-private.h`.
//!
//! `ClutterActorMeta` is the abstract base class for "modifiers" attached to
//! a `ClutterActor` — the parent of `ClutterAction`, `ClutterConstraint`,
//! and `ClutterEffect`. It tracks the owning actor, an enabled flag, an
//! optional name, and a priority used to order modifiers of the same kind
//! on one actor.
//!
//! # What's ported
//!
//! - The field layout of `ClutterActorMetaPrivate` (`actor`, `name`,
//!   `is_enabled`, `priority`) as a plain `ActorMeta` struct.
//! - The `set_name`/`get_name`, `set_enabled`/`get_enabled`, and
//!   `get_actor` accessors.
//! - The `set_actor` virtual: in C this also wires a `destroy` signal
//!   handler on the actor so the meta's back-pointer is cleared when the
//!   actor is destroyed. Here the actor is stored as an `Option<ActorId>`
//!   (matching `actor.rs`'s arena convention); "destroy" is expressed by the
//!   actor slot being removed from the `ActorTree`, which the caller detects
//!   via `ActorTree::get`. So `set_actor` just stores the id — the
//!   signal-handler bookkeeping has no equivalent and is omitted.
//! - The `CLUTTER_ACTOR_META_PRIORITY_*` constants.
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery (`G_DEFINE_ABSTRACT_TYPE_WITH_PRIVATE`,
//!   `GInitiallyUnowned` floating-ref parent, `GParamSpec` property
//!   install/notify, `g_object_notify_by_pspec`): no GObject type system in
//!   this port. Properties become plain fields; "notify" becomes the caller
//!   observing the field directly.
//! - `g_set_str` / `g_free` on `name`: ownership is just `Option<String>`.
//! - The `g_warn_if_fail (!priv->actor || !CLUTTER_ACTOR_IN_PAINT (...))`
//!   paint-time assertions: there is no `CLUTTER_ACTOR_IN_PAINT` flag in the
//!   ported `ActorCommon` (paint is not yet wired), so the check has nothing
//!   to test. If/when paint state lands, the assertion can be re-added.
//! - `_clutter_actor_meta_get_debug_name`: a debug-only helper that returns
//!   the meta's name or a fallback `<unknown>` string; not needed without
//!   the debug logging subsystem.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no `unsafe`,
//! no external crates, and only `core`/`alloc`.

use alloc::string::String;

use super::actor::ActorId;

/// `CLUTTER_ACTOR_META_PRIORITY_DEFAULT` — default priority for actor metas.
pub const PRIORITY_DEFAULT: i32 = 0;
/// `CLUTTER_ACTOR_META_PRIORITY_HIGH` — metas with this priority run before
/// default-priority ones of the same kind.
pub const PRIORITY_HIGH: i32 = 1;
/// `CLUTTER_ACTOR_META_PRIORITY_LOW` — metas with this priority run after
/// default-priority ones of the same kind.
pub const PRIORITY_LOW: i32 = -1;

/// Port of `ClutterActorMeta` / `ClutterActorMetaPrivate`.
///
/// Stored as a plain struct rather than a GObject instance. The `actor`
/// field is an `Option<ActorId>` referencing the owning actor in an
/// `ActorTree` (see `actor.rs`), instead of a raw `ClutterActor *` plus a
/// `destroy` signal handler that clears it.
#[derive(Debug, Clone)]
pub struct ActorMeta {
    /// The actor this meta is attached to, or `None` if detached. Mirrors
    /// `ClutterActorMetaPrivate::actor`; the `destroy`-signal clearing is
    /// handled by the actor slot being removed from the `ActorTree`.
    pub actor: Option<ActorId>,
    /// Optional human-readable name. Mirrors
    /// `ClutterActorMetaPrivate::name`; owned `String` instead of `gchar *`.
    pub name: Option<String>,
    /// Whether the meta is active. Mirrors the `is_enabled` bitfield;
    /// defaults to `true` (matching `clutter_actor_meta_init`).
    pub enabled: bool,
    /// Ordering priority among metas of the same kind on one actor.
    /// Mirrors `ClutterActorMetaPrivate::priority`; defaults to
    /// `PRIORITY_DEFAULT` (matching `clutter_actor_meta_init`).
    pub priority: i32,
}

impl Default for ActorMeta {
    fn default() -> Self {
        // Mirrors `clutter_actor_meta_init`: `is_enabled = TRUE`,
        // `priority = CLUTTER_ACTOR_META_PRIORITY_DEFAULT`.
        ActorMeta {
            actor: None,
            name: None,
            enabled: true,
            priority: PRIORITY_DEFAULT,
        }
    }
}

impl ActorMeta {
    /// `clutter_actor_meta_new` equivalent — there is no `new` in C (it's
    /// abstract), but this gives a sensible default for embedded use.
    pub fn new() -> Self {
        Self::default()
    }

    /// Port of `clutter_actor_meta_set_name`. Ownership of `name` is taken
    /// (the C version `g_set_str` copies the incoming string).
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = Some(name.into());
    }

    /// Port of `clutter_actor_meta_get_name` (returns a borrowed view; the
    /// C version returns a `const gchar *` owned by the meta).
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Port of `clutter_actor_meta_set_enabled`.
    pub fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
    }

    /// Port of `clutter_actor_meta_get_enabled`.
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    /// Port of `clutter_actor_meta_get_actor`. Returns the `ActorId` of the
    /// attached actor, or `None` if detached.
    pub fn actor(&self) -> Option<ActorId> {
        self.actor
    }

    /// Port of the `ClutterActorMetaClass::set_actor` virtual's default
    /// implementation (`clutter_actor_meta_real_set_actor`), minus the
    /// `destroy`-signal wiring (see module docs). The caller is responsible
    /// for only attaching a meta to one actor at a time; passing `None`
    /// detaches.
    pub fn set_actor(&mut self, actor: Option<ActorId>) {
        // `clutter_actor_meta_real_set_actor` early-returns if unchanged.
        if self.actor == actor {
            return;
        }
        self.actor = actor;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;

    #[test]
    fn defaults_match_c_init() {
        let m = ActorMeta::new();
        assert!(m.enabled());
        assert_eq!(m.priority, PRIORITY_DEFAULT);
        assert_eq!(m.actor(), None);
        assert_eq!(m.name(), None);
    }

    #[test]
    fn set_name_round_trips() {
        let mut m = ActorMeta::new();
        m.set_name("green-x");
        assert_eq!(m.name(), Some("green-x"));
    }

    #[test]
    fn set_actor_noop_on_same() {
        use super::super::actor::{ActorCommon, ActorTree, NullBehavior};
        let mut tree = ActorTree::new();
        let id = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        let mut m = ActorMeta::new();
        m.set_actor(Some(id));
        m.set_actor(Some(id)); // unchanged -> no-op path
        assert_eq!(m.actor(), Some(id));
        m.set_actor(None);
        assert_eq!(m.actor(), None);
    }
}
