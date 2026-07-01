//! Deferred callback scheduler ported from GNOME Mutter meta-later.c.
//!
//! Manages callbacks to be run at specific points in the frame cycle:
//! - Resize, CalcShowing, CheckFullscreen, SyncStack, BeforeRedraw (run before redraw)
//! - Idle (very low priority)
//!
//! Source: mutter-main/src/compositor/meta-later.c (GNU GPL 2+)

use alloc::boxed::Box;
use alloc::vec::Vec;

/// Phases at which deferred callbacks run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LaterType {
    /// Resize processing phase before repainting
    Resize,
    /// Calculate which windows should be visible
    CalcShowing,
    /// Check for fullscreen windows
    CheckFullscreen,
    /// Sync stacking order to server
    SyncStack,
    /// Before the stage redraws
    BeforeRedraw,
    /// Very low priority (can be blocked by animations/redraws)
    Idle,
}

/// A deferred callback to be run later
struct Later {
    id: u32,
    later_type: LaterType,
    callback: Box<dyn FnMut()>,
}

/// Scheduler for deferred callbacks, ordered by priority type.
pub struct LaterScheduler {
    laters: Vec<Later>,
    next_id: u32,
}

impl LaterScheduler {
    /// Create a new deferred callback scheduler.
    pub fn new() -> Self {
        Self {
            laters: Vec::new(),
            next_id: 1,
        }
    }

    /// Add a callback to run at the specified phase.
    /// Returns a non-zero ID that can be used to remove the callback.
    pub fn add<F>(&mut self, when: LaterType, callback: F) -> u32
    where
        F: FnMut() + 'static,
    {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        if self.next_id == 0 {
            self.next_id = 1;
        }

        self.laters.push(Later {
            id,
            later_type: when,
            callback: Box::new(callback),
        });

        id
    }

    /// Remove a callback by its ID.
    pub fn remove(&mut self, id: u32) {
        self.laters.retain(|later| later.id != id);
    }

    /// Run and clear all callbacks of a specific type.
    pub fn run_and_clear_for_type(&mut self, when: LaterType) {
        let mut to_run = Vec::new();

        // Extract callbacks matching the type
        let remaining: Vec<Later> = self
            .laters
            .drain(..)
            .filter_map(|later| {
                if later.later_type == when {
                    to_run.push(later);
                    None
                } else {
                    Some(later)
                }
            })
            .collect();

        self.laters = remaining;

        // Run extracted callbacks
        for mut later in to_run {
            (later.callback)();
        }
    }

    /// Clear all callbacks of a specific type without running them.
    pub fn clear_for_type(&mut self, when: LaterType) {
        self.laters.retain(|later| later.later_type != when);
    }

    /// Get the number of pending callbacks.
    pub fn len(&self) -> usize {
        self.laters.len()
    }

    /// Check if there are any pending callbacks.
    pub fn is_empty(&self) -> bool {
        self.laters.is_empty()
    }

    /// Clear all callbacks.
    pub fn clear(&mut self) {
        self.laters.clear();
    }
}

impl Default for LaterScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::sync::Arc;
    use core::cell::RefCell;

    #[test]
    fn test_add_and_remove() {
        let mut scheduler = LaterScheduler::new();
        let id1 = scheduler.add(LaterType::Idle, || {});
        let id2 = scheduler.add(LaterType::BeforeRedraw, || {});

        assert_eq!(scheduler.len(), 2);
        scheduler.remove(id1);
        assert_eq!(scheduler.len(), 1);
        scheduler.remove(id2);
        assert!(scheduler.is_empty());
    }

    #[test]
    fn test_run_and_clear() {
        let mut scheduler = LaterScheduler::new();
        let counter = Arc::new(RefCell::new(0));

        let c = counter.clone();
        scheduler.add(LaterType::Idle, move || {
            *c.borrow_mut() += 1;
        });

        let c = counter.clone();
        scheduler.add(LaterType::BeforeRedraw, move || {
            *c.borrow_mut() += 10;
        });

        scheduler.run_and_clear_for_type(LaterType::Idle);
        assert_eq!(*counter.borrow(), 1);
        assert_eq!(scheduler.len(), 1);

        scheduler.run_and_clear_for_type(LaterType::BeforeRedraw);
        assert_eq!(*counter.borrow(), 11);
        assert!(scheduler.is_empty());
    }

    #[test]
    fn test_id_uniqueness() {
        let mut scheduler = LaterScheduler::new();
        let id1 = scheduler.add(LaterType::Idle, || {});
        let id2 = scheduler.add(LaterType::Idle, || {});
        let id3 = scheduler.add(LaterType::Idle, || {});

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
    }
}
