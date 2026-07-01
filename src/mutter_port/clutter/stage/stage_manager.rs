#![allow(dead_code)]

//! Port of GNOME mutter's clutter/clutter-stage-manager.{c,h} and
//! clutter-stage-manager-private.h.
//!
//! ClutterStageManager is the singleton that tracks all ClutterStage
//! instances in the application. In C it is a GObject owned by
//! ClutterContext; stages are added/removed via private
//! _clutter_stage_manager_add_stage/_remove_stage calls from
//! clutter_stage_init/_finalize. The public API
//! (clutter_stage_manager_get_default, list_stages,
//! get_default_stage) lets external code enumerate stages and access
//! the default one.
//!
//! # What's ported
//!
//! - StageManager struct holding a Vec<StageId> of all stages and
//!   an Option<StageId> default stage, mirroring the C
//!   ClutterStageManagerPrivate::stages (GSList) and
//!   default_stage fields.
//! - add_stage / remove_stage: the private
//!   _clutter_stage_manager_add_stage/_remove_stage functions.
//!   Adding a stage appends to the list and sets it as default if no
//!   default exists yet. Removing a stage clears the default if it
//!   was the default, and promotes the next stage if any.
//! - list_stages: returns a copy of the stage list.
//! - get_default_stage / set_default_stage: accessors for the
//!   default stage.
//! - stages_len / is_empty: convenience accessors.
//!
//! # What's skipped, with rationale
//!
//! - GObject singleton machinery (G_DEFINE_TYPE_WITH_PRIVATE,
//!   clutter_stage_manager_get_default returning a global singleton):
//!   there is no GObject type system in this port. StageManager is a
//!   plain struct; a future Context port can own one and expose it.
//! - ClutterContext back-pointer: the C struct has a construct-only
//!   back-pointer to the owning context. Not needed here.
//! - Signal emission (stage-added / stage-removed signals): modeled
//!   as return values from add_stage/remove_stage.
//!
//! As with the rest of mutter_port::clutter, this module uses no
//! unsafe, no external crates, and only core/alloc.

use alloc::vec::Vec;

use super::stage_window::StageId;

/// Port of ClutterStageManager / ClutterStageManagerPrivate.
///
/// Tracks all stages in the application. The first stage added becomes
/// the default (matching the C behavior where default_stage is set
/// on the first add_stage call if it is NULL).
#[derive(Debug, Clone, Default)]
pub struct StageManager {
    stages: Vec<StageId>,
    default_stage: Option<StageId>,
}

impl StageManager {
    /// Create a new empty StageManager.
    pub fn new() -> Self {
        StageManager {
            stages: Vec::new(),
            default_stage: None,
        }
    }

    /// Number of stages tracked.
    pub fn stages_len(&self) -> usize {
        self.stages.len()
    }

    /// Whether no stages are tracked.
    pub fn is_empty(&self) -> bool {
        self.stages.is_empty()
    }

    /// _clutter_stage_manager_add_stage: add a stage to the list.
    /// Returns true if the stage was newly added. The first stage
    /// added becomes the default stage.
    pub fn add_stage(&mut self, stage: StageId) -> bool {
        if self.stages.contains(&stage) {
            return false;
        }
        self.stages.push(stage);
        if self.default_stage.is_none() {
            self.default_stage = Some(stage);
        }
        true
    }

    /// _clutter_stage_manager_remove_stage: remove a stage from the
    /// list. Returns true if the stage was found and removed. If the
    /// removed stage was the default, the next stage in the list (if
    /// any) is promoted to default.
    pub fn remove_stage(&mut self, stage: StageId) -> bool {
        let idx = self.stages.iter().position(|s| *s == stage);
        match idx {
            Some(i) => {
                self.stages.remove(i);
                if self.default_stage == Some(stage) {
                    self.default_stage = self.stages.first().copied();
                }
                true
            }
            None => false,
        }
    }

    /// clutter_stage_manager_list_stages: return a copy of the stage
    /// list.
    pub fn list_stages(&self) -> Vec<StageId> {
        self.stages.clone()
    }

    /// clutter_stage_manager_get_default_stage: return the default
    /// stage, or None if no stages exist.
    pub fn get_default_stage(&self) -> Option<StageId> {
        self.default_stage
    }

    /// clutter_stage_manager_set_default_stage: set the default
    /// stage. The stage must already be in the list.
    pub fn set_default_stage(&mut self, stage: StageId) -> bool {
        if !self.stages.contains(&stage) {
            return false;
        }
        self.default_stage = Some(stage);
        true
    }

    /// Whether a given stage is tracked.
    pub fn contains(&self, stage: StageId) -> bool {
        self.stages.contains(&stage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_manager_is_empty() {
        let mgr = StageManager::new();
        assert!(mgr.is_empty());
        assert_eq!(mgr.stages_len(), 0);
        assert!(mgr.get_default_stage().is_none());
        assert!(mgr.list_stages().is_empty());
    }

    #[test]
    fn add_stage_sets_first_as_default() {
        let mut mgr = StageManager::new();
        let s1 = StageId(1);
        let s2 = StageId(2);

        assert!(mgr.add_stage(s1));
        assert_eq!(mgr.stages_len(), 1);
        assert_eq!(mgr.get_default_stage(), Some(s1));

        assert!(mgr.add_stage(s2));
        assert_eq!(mgr.stages_len(), 2);
        assert_eq!(mgr.get_default_stage(), Some(s1));
    }

    #[test]
    fn add_duplicate_stage_is_noop() {
        let mut mgr = StageManager::new();
        let s1 = StageId(1);
        assert!(mgr.add_stage(s1));
        assert!(!mgr.add_stage(s1));
        assert_eq!(mgr.stages_len(), 1);
    }

    #[test]
    fn remove_stage_promotes_next_default() {
        let mut mgr = StageManager::new();
        let s1 = StageId(1);
        let s2 = StageId(2);
        mgr.add_stage(s1);
        mgr.add_stage(s2);
        assert_eq!(mgr.get_default_stage(), Some(s1));

        assert!(mgr.remove_stage(s1));
        assert_eq!(mgr.stages_len(), 1);
        assert_eq!(mgr.get_default_stage(), Some(s2));
    }

    #[test]
    fn remove_only_stage_clears_default() {
        let mut mgr = StageManager::new();
        let s1 = StageId(1);
        mgr.add_stage(s1);
        assert!(mgr.remove_stage(s1));
        assert!(mgr.is_empty());
        assert!(mgr.get_default_stage().is_none());
    }

    #[test]
    fn remove_nonexistent_stage_returns_false() {
        let mut mgr = StageManager::new();
        let s1 = StageId(1);
        mgr.add_stage(s1);
        assert!(!mgr.remove_stage(StageId(99)));
        assert_eq!(mgr.stages_len(), 1);
    }

    #[test]
    fn list_stages_returns_copy() {
        let mut mgr = StageManager::new();
        mgr.add_stage(StageId(1));
        mgr.add_stage(StageId(2));
        mgr.add_stage(StageId(3));

        let list = mgr.list_stages();
        assert_eq!(list, vec![StageId(1), StageId(2), StageId(3)]);
        drop(list);
        assert_eq!(mgr.stages_len(), 3);
    }

    #[test]
    fn set_default_stage_requires_membership() {
        let mut mgr = StageManager::new();
        let s1 = StageId(1);
        let s2 = StageId(2);
        mgr.add_stage(s1);

        assert!(!mgr.set_default_stage(s2));
        assert_eq!(mgr.get_default_stage(), Some(s1));

        assert!(mgr.set_default_stage(s1));
        assert_eq!(mgr.get_default_stage(), Some(s1));
    }

    #[test]
    fn remove_non_default_keeps_default() {
        let mut mgr = StageManager::new();
        let s1 = StageId(1);
        let s2 = StageId(2);
        mgr.add_stage(s1);
        mgr.add_stage(s2);
        assert_eq!(mgr.get_default_stage(), Some(s1));

        mgr.remove_stage(s2);
        assert_eq!(mgr.get_default_stage(), Some(s1));
        assert_eq!(mgr.stages_len(), 1);
    }

    #[test]
    fn contains_checks_membership() {
        let mut mgr = StageManager::new();
        let s1 = StageId(1);
        mgr.add_stage(s1);
        assert!(mgr.contains(s1));
        assert!(!mgr.contains(StageId(2)));
    }
}
