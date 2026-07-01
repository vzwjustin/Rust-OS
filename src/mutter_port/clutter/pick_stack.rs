//! Port of GNOME mutter's `clutter/clutter-pick-stack.{c,h}`.
//!
//! `ClutterPickStack` maintains a stack of "pick records" (actor + clip region)
//! during a pick pass for hit-testing. Records are pushed as actors are painted,
//! and then queried top-down to find the topmost actor at a given point.
//!
//! This port:
//! - Uses `Vec<PickRecord>` and `Vec<PickClipRecord>` instead of GArray
//! - Drops GObject lifecycle (ref/unref/weak-refs) and matrix-stack management
//! - Simplifies geometry to 2D axis-aligned rectangles (ActorBox), dropping
//!   graphene projection and triangle tests
//! - Preserves the stack semantics: clip regions form a linked chain (via
//!   `prev` indices), and `search_actor` iterates top-down returning the
//!   topmost non-overlap record whose box and clip chain contain the point

use super::actor_box::ActorBox;
use alloc::vec::Vec;

/// Opaque handle to an actor. The port does not model ClutterActor itself;
/// callers manage actor identity externally.
pub type ActorHandle = usize;

/// A single record in the pick stack, representing an actor with its bounding
/// box and clip index. Mirrors `PickRecord` from upstream.
#[derive(Debug, Clone)]
pub struct PickRecord {
    /// The bounding box of the actor during this paint pass.
    pub box_: ActorBox,
    /// The actor being picked.
    pub actor: ActorHandle,
    /// Index into the clip stack for this record's active clip region (-1 = no clip).
    pub clip_index: i32,
    /// True if this is an overlap record (no bounding box, only for sequencing).
    pub is_overlap: bool,
}

/// A clipping region in the clip stack. Mirrors `PickClipRecord` from upstream.
/// Forms a linked chain via `prev` indices.
#[derive(Debug, Clone)]
pub struct PickClipRecord {
    /// The bounding box of the clip region.
    pub box_: ActorBox,
    /// Index of the previous clip record in the chain (-1 = top of chain).
    pub prev: i32,
}

/// The pick stack for a single pick pass. Records actors and clips as they are
/// painted, then queries topmost actor at a point. Mirrors `ClutterPickStack`
/// from upstream.
#[derive(Debug, Clone)]
pub struct PickStack {
    records: Vec<PickRecord>,
    clip_stack: Vec<PickClipRecord>,
    current_clip_index: i32,
}

impl PickStack {
    /// Creates a new empty pick stack.
    pub fn new() -> Self {
        PickStack {
            records: Vec::new(),
            clip_stack: Vec::new(),
            current_clip_index: -1,
        }
    }

    /// Logs a pick record for an actor with its bounding box.
    /// Mirrors `clutter_pick_stack_log_pick`.
    pub fn log_pick(&mut self, box_: ActorBox, actor: ActorHandle) {
        self.records.push(PickRecord {
            box_,
            actor,
            clip_index: self.current_clip_index,
            is_overlap: false,
        });
    }

    /// Logs an overlap record for an actor (no bounding box, used for sequencing).
    /// Mirrors `clutter_pick_stack_log_overlap`.
    pub fn log_overlap(&mut self, actor: ActorHandle) {
        self.records.push(PickRecord {
            box_: ActorBox::default(),
            actor,
            clip_index: self.current_clip_index,
            is_overlap: true,
        });
    }

    /// Pushes a clip region onto the clip stack.
    /// Mirrors `clutter_pick_stack_push_clip`.
    pub fn push_clip(&mut self, box_: ActorBox) {
        self.clip_stack.push(PickClipRecord {
            box_,
            prev: self.current_clip_index,
        });
        self.current_clip_index = (self.clip_stack.len() - 1) as i32;
    }

    /// Pops the top clip region from the clip stack.
    /// Mirrors `clutter_pick_stack_pop_clip`.
    pub fn pop_clip(&mut self) {
        if self.current_clip_index >= 0 {
            let top = &self.clip_stack[self.current_clip_index as usize];
            self.current_clip_index = top.prev;
        }
    }

    /// Returns the topmost non-overlap record whose box and clip chain contain
    /// the given point, or None if no record matches. Iterates from the top of
    /// the stack downward.
    ///
    /// Mirrors `clutter_pick_stack_search_actor`, simplified to 2D axis-aligned
    /// boxes (no projection/rays/triangles).
    pub fn search_actor(&self, point: (f32, f32)) -> Option<ActorHandle> {
        for i in (0..self.records.len()).rev() {
            let rec = &self.records[i];

            // Skip overlap records and records without actors
            if rec.is_overlap || rec.actor == 0 {
                continue;
            }

            // Check if the point is in the record's box
            if !rec.box_.contains(point.0, point.1) {
                continue;
            }

            // Walk the clip chain to verify the point is in all clips
            if self.point_in_clip_chain(rec.clip_index, point) {
                return Some(rec.actor);
            }
        }

        None
    }

    /// Helper: checks if a point is contained in all clip regions in the chain
    /// starting at `clip_index` (-1 = no clips).
    fn point_in_clip_chain(&self, mut clip_index: i32, point: (f32, f32)) -> bool {
        while clip_index >= 0 {
            let clip = &self.clip_stack[clip_index as usize];
            if !clip.box_.contains(point.0, point.1) {
                return false;
            }
            clip_index = clip.prev;
        }
        true
    }

    /// Returns the number of records currently in the stack.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Returns true if the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Clears all records and clips.
    pub fn clear(&mut self) {
        self.records.clear();
        self.clip_stack.clear();
        self.current_clip_index = -1;
    }
}

impl Default for PickStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_stack_is_empty() {
        let stack = PickStack::new();
        assert!(stack.is_empty());
        assert_eq!(stack.len(), 0);
    }

    #[test]
    fn log_pick_adds_record() {
        let mut stack = PickStack::new();
        let box_ = ActorBox::new(0.0, 0.0, 10.0, 10.0);
        stack.log_pick(box_, 1);
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn log_overlap_adds_record() {
        let mut stack = PickStack::new();
        stack.log_overlap(1);
        assert_eq!(stack.len(), 1);
    }

    #[test]
    fn search_actor_finds_topmost_hit() {
        let mut stack = PickStack::new();
        // Add two non-overlapping records
        stack.log_pick(ActorBox::new(0.0, 0.0, 5.0, 5.0), 1);
        stack.log_pick(ActorBox::new(5.0, 5.0, 10.0, 10.0), 2);

        // Point in second record should find actor 2
        assert_eq!(stack.search_actor((7.0, 7.0)), Some(2));
        // Point in first record should find actor 1
        assert_eq!(stack.search_actor((2.0, 2.0)), Some(1));
        // Point outside both should find nothing
        assert_eq!(stack.search_actor((15.0, 15.0)), None);
    }

    #[test]
    fn search_actor_returns_topmost_in_stack_order() {
        let mut stack = PickStack::new();
        // Overlapping records
        stack.log_pick(ActorBox::new(0.0, 0.0, 10.0, 10.0), 1);
        stack.log_pick(ActorBox::new(2.0, 2.0, 8.0, 8.0), 2);

        // Point in overlap should return the topmost (actor 2)
        assert_eq!(stack.search_actor((5.0, 5.0)), Some(2));
    }

    #[test]
    fn search_actor_skips_overlap_records() {
        let mut stack = PickStack::new();
        stack.log_pick(ActorBox::new(0.0, 0.0, 10.0, 10.0), 1);
        stack.log_overlap(2); // No bounding box
        stack.log_pick(ActorBox::new(0.0, 0.0, 10.0, 10.0), 3);

        // Should skip the overlap record and find actor 3
        assert_eq!(stack.search_actor((5.0, 5.0)), Some(3));
    }

    #[test]
    fn clip_stack_push_pop() {
        let mut stack = PickStack::new();
        assert_eq!(stack.current_clip_index, -1);

        stack.push_clip(ActorBox::new(0.0, 0.0, 10.0, 10.0));
        assert_eq!(stack.current_clip_index, 0);

        stack.push_clip(ActorBox::new(2.0, 2.0, 8.0, 8.0));
        assert_eq!(stack.current_clip_index, 1);

        stack.pop_clip();
        assert_eq!(stack.current_clip_index, 0);

        stack.pop_clip();
        assert_eq!(stack.current_clip_index, -1);
    }

    #[test]
    fn pick_record_respects_clip_chain() {
        let mut stack = PickStack::new();

        // Push a clip
        stack.push_clip(ActorBox::new(0.0, 0.0, 5.0, 5.0));

        // Add actor inside the clip
        stack.log_pick(ActorBox::new(1.0, 1.0, 4.0, 4.0), 1);

        // Point inside both box and clip should find the actor
        assert_eq!(stack.search_actor((2.0, 2.0)), Some(1));

        // Point inside actor's box but outside the clip should not find it
        assert_eq!(stack.search_actor((4.5, 4.5)), None);
    }

    #[test]
    fn clear_empties_stack() {
        let mut stack = PickStack::new();
        stack.log_pick(ActorBox::new(0.0, 0.0, 10.0, 10.0), 1);
        stack.push_clip(ActorBox::new(0.0, 0.0, 5.0, 5.0));

        assert!(!stack.is_empty());
        stack.clear();
        assert!(stack.is_empty());
        assert_eq!(stack.current_clip_index, -1);
    }
}
