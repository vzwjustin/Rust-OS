//! Stage View Private — ported from GNOME Mutter
//!
//! Private implementation details for Clutter stage views,
//! including damage history and frame swapping operations.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stage-view-private.h

use alloc::vec::Vec;

/// A damaged rectangle in stage coordinates (x, y, width, height).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DamageRect {
    /// X coordinate of the damaged region (stage space).
    pub x: i32,
    /// Y coordinate of the damaged region (stage space).
    pub y: i32,
    /// Width of the damaged region in pixels.
    pub width: i32,
    /// Height of the damaged region in pixels.
    pub height: i32,
}

impl DamageRect {
    /// Create a new damage rectangle.
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        DamageRect {
            x,
            y,
            width,
            height,
        }
    }

    /// Check whether this damage rectangle is empty (zero area).
    pub fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    /// Compute the bounding box of this rectangle with another.
    pub fn union(&self, other: &DamageRect) -> DamageRect {
        if self.is_empty() {
            return *other;
        }
        if other.is_empty() {
            return *self;
        }
        let x = core::cmp::min(self.x, other.x);
        let y = core::cmp::min(self.y, other.y);
        let right = core::cmp::max(self.x + self.width, other.x + other.width);
        let bottom = core::cmp::max(self.y + self.height, other.y + other.height);
        DamageRect::new(x, y, right - x, bottom - y)
    }
}

/// Maximum number of damage rectangles retained in history.
///
/// Upstream Mutter keeps a small ring of recent damage regions so the
/// compositor can redraw the union of the last few frames when a swap
/// fails or a frame is dropped. The exact upstream constant is 4.
pub const DAMAGE_HISTORY_MAX_LEN: usize = 4;

/// Private state for a Clutter stage view.
///
/// Tracks the recent damage regions and the number of successful frame
/// swaps so the backend can decide when to perform partial redraws and
/// detect missed vblanks.
pub struct StageViewPrivate {
    /// Ring of recently damaged rectangles (most recent last).
    damage_history: Vec<DamageRect>,
    /// Number of frame swaps successfully completed for this view.
    frame_swap_count: u64,
}

impl StageViewPrivate {
    /// Create a new, empty stage view private state.
    pub fn new() -> Self {
        StageViewPrivate {
            damage_history: Vec::new(),
            frame_swap_count: 0,
        }
    }

    /// Record a newly damaged rectangle.
    ///
    /// If the history has reached `DAMAGE_HISTORY_MAX_LEN`, the oldest
    /// entry is dropped before the new one is appended, mirroring the
    /// fixed-size ring used upstream.
    pub fn add_damage_rect(&mut self, rect: DamageRect) {
        if rect.is_empty() {
            return;
        }
        if self.damage_history.len() >= DAMAGE_HISTORY_MAX_LEN {
            self.damage_history.remove(0);
        }
        self.damage_history.push(rect);
    }

    /// Return a snapshot of the recent damage rectangles.
    ///
    /// The slice is ordered oldest-first. Callers typically compute the
    /// union of all returned rectangles to determine the redraw region.
    pub fn get_damage_history(&self) -> &[DamageRect] {
        &self.damage_history
    }

    /// Compute the bounding box of all damage currently in history.
    ///
    /// Returns `None` when there is no recent damage.
    pub fn get_damage_bounding_box(&self) -> Option<DamageRect> {
        self.damage_history.iter().fold(None, |acc, r| {
            Some(match acc {
                None => *r,
                Some(b) => b.union(r),
            })
        })
    }

    /// Clear the damage history (e.g. after a full redraw).
    pub fn clear_damage_history(&mut self) {
        self.damage_history.clear();
    }

    /// Record that a frame swap completed successfully.
    ///
    /// Upstream bumps an internal counter each time the swap buffers
    /// request returns; the count is used to detect dropped frames by
    /// comparing expected vs. actual swap counts across vblanks.
    pub fn record_frame_swap(&mut self) {
        self.frame_swap_count = self.frame_swap_count.saturating_add(1);
    }

    /// Get the total number of successful frame swaps recorded.
    pub fn get_frame_swap_count(&self) -> u64 {
        self.frame_swap_count
    }
}

impl Default for StageViewPrivate {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_damage_history_grows_and_caps() {
        let mut view = StageViewPrivate::new();
        for i in 0..(DAMAGE_HISTORY_MAX_LEN + 3) as i32 {
            view.add_damage_rect(DamageRect::new(i, 0, 10, 10));
        }
        assert_eq!(view.get_damage_history().len(), DAMAGE_HISTORY_MAX_LEN);
        // Oldest entries should have been evicted; the first kept rect
        // should correspond to the (DAMAGE_HISTORY_MAX_LEN-1)th insert.
        let first = view.get_damage_history()[0];
        assert_eq!(first.x, 3);
    }

    #[test]
    fn test_empty_rect_ignored() {
        let mut view = StageViewPrivate::new();
        view.add_damage_rect(DamageRect::new(0, 0, 0, 10));
        assert!(view.get_damage_history().is_empty());
    }

    #[test]
    fn test_bounding_box() {
        let mut view = StageViewPrivate::new();
        view.add_damage_rect(DamageRect::new(10, 10, 20, 20));
        view.add_damage_rect(DamageRect::new(50, 5, 10, 40));
        let bb = view.get_damage_bounding_box().unwrap();
        assert_eq!(bb, DamageRect::new(10, 5, 50, 45));
    }

    #[test]
    fn test_frame_swap_count() {
        let mut view = StageViewPrivate::new();
        assert_eq!(view.get_frame_swap_count(), 0);
        view.record_frame_swap();
        view.record_frame_swap();
        assert_eq!(view.get_frame_swap_count(), 2);
    }
}
