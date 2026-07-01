//! Port of GNOME mutter's `clutter/clutter-layout-meta.{c,h}`.
//!
//! `LayoutMeta` is a per-(manager, container, child) metadata wrapper.
//! See `layout_manager.rs` for the struct definition and docs.

use super::actor::ActorId;
use super::layout_manager::LayoutMeta;

impl LayoutMeta {
    /// Retrieves the container actor.
    pub fn get_container(&self) -> ActorId {
        self.container
    }

    /// Retrieves the child actor.
    pub fn get_actor(&self) -> ActorId {
        self.actor
    }

    /// Checks if this meta is for the given container and child actors.
    /// The manager is implicit (the owner); no manager check is performed.
    pub fn is_for(&self, container: ActorId, actor: ActorId) -> bool {
        self.container == container && self.actor == actor
    }
}

#[cfg(test)]
mod tests {
    use super::super::actor::{ActorCommon, ActorTree, NullBehavior};
    use super::*;

    #[test]
    fn getters_return_stored_ids() {
        let mut tree = ActorTree::new();
        let c = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        let a = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        let meta = LayoutMeta::new(c, a);
        assert_eq!(meta.get_container(), c);
        assert_eq!(meta.get_actor(), a);
    }

    #[test]
    fn is_for_matches_both_ids() {
        let mut tree = ActorTree::new();
        let c = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        let a = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        let other_c = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        let other_a = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        let meta = LayoutMeta::new(c, a);
        assert!(meta.is_for(c, a));
        assert!(!meta.is_for(other_c, a));
        assert!(!meta.is_for(c, other_a));
        assert!(!meta.is_for(other_c, other_a));
    }
}
