//! Cursor Tracker Private ported from GNOME Mutter's src/backends/
//!
//! Private interfaces for cursor tracker internal management,
//! position invalidation, and backend access.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-cursor-tracker-private.c

use core::cell::Cell;

/// Internal cursor tracker operations (private API).
pub struct MetaCursorTrackerPrivate {
    /// Whether the position cache is valid.
    position_valid: Cell<bool>,
    /// Whether the cursor sprite has been set.
    has_cursor: Cell<bool>,
}

impl MetaCursorTrackerPrivate {
    /// Create a new private cursor tracker state.
    pub fn new() -> Self {
        Self {
            position_valid: Cell::new(false),
            has_cursor: Cell::new(false),
        }
    }

    /// Update the current cursor sprite. Marks the cursor as set.
    /// A full implementation would emit the "cursor-changed" signal.
    pub fn set_current_cursor(&self) {
        self.has_cursor.set(true);
    }

    /// Invalidate cursor position cache. Marks position as needing
    /// recalculation on the next query.
    pub fn invalidate_position(&self) {
        self.position_valid.set(false);
    }

    /// Whether the position cache is valid.
    pub fn is_position_valid(&self) -> bool {
        self.position_valid.get()
    }

    /// Mark the position cache as valid (after recalculation).
    pub fn validate_position(&self) {
        self.position_valid.set(true);
    }

    /// Get the backend owning this tracker. Without a backend reference,
    /// returns None.
    pub fn get_backend() -> Option<()> {
        None
    }

    /// Clean up tracker resources. Clears cursor and position state.
    pub fn destroy(&self) {
        self.has_cursor.set(false);
        self.position_valid.set(false);
    }
}

impl Default for MetaCursorTrackerPrivate {
    fn default() -> Self {
        Self::new()
    }
}
