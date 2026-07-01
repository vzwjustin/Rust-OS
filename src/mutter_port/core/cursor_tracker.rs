//! MetaCursorTracker ported from GNOME Mutter's src/core/meta-cursor-tracker.c
//!
//! MetaCursorTracker tracks the cursor position, visibility, and sprite for
//! the compositor. It bridges between the backend's hardware cursor support
//! and the Clutter stage's software cursor fallback.
//!
//! In Mutter this is a GObject that emits signals when the cursor changes
//! (position, visibility, sprite). The backend calls into it to update the
//! cursor position from input events, and the compositor reads it to decide
//! whether to render a software cursor.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-cursor-tracker.c

use alloc::string::String;

/// Cursor sprite identifier (opaque handle to backend cursor data).
/// In Mutter this is a MetaCursorSprite (GObject); here it is an opaque id.
pub type CursorSpriteId = u64;

/// The cursor role, mirroring MetaCursorRole.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorRole {
    /// The pointer cursor.
    Pointer,
    /// A tablet tool cursor.
    TabletTool,
}

impl Default for CursorRole {
    fn default() -> Self {
        CursorRole::Pointer
    }
}

/// Cursor tracker state. Mirrors the fields in MetaCursorTrackerPrivate.
#[derive(Debug)]
pub struct MetaCursorTracker {
    /// Whether the pointer cursor is visible.
    pointer_visible: bool,
    /// Whether the pointer cursor is shown (not hidden by the shell).
    pointer_shown: bool,
    /// Current pointer cursor sprite, if any.
    pointer_cursor: Option<CursorSpriteId>,
    /// Current tablet tool cursor sprite, if any.
    tablet_cursor: Option<CursorSpriteId>,
    /// Cursor position in stage coordinates.
    position: (i32, i32),
    /// Whether the position has changed since last query.
    position_changed: bool,
    /// Whether the hardware cursor is being used (vs software fallback).
    hardware_cursor: bool,
    /// The cursor role currently being tracked.
    role: CursorRole,
    /// Backend cursor scale (for HiDPI).
    scale: f32,
    /// Whether the cursor is on a fractional-scale monitor.
    is_fractional: bool,
}

impl MetaCursorTracker {
    /// Create a new cursor tracker. Mirrors meta_cursor_tracker_new().
    pub fn new() -> Self {
        MetaCursorTracker {
            pointer_visible: true,
            pointer_shown: true,
            pointer_cursor: None,
            tablet_cursor: None,
            position: (0, 0),
            position_changed: false,
            hardware_cursor: true,
            role: CursorRole::Pointer,
            scale: 1.0,
            is_fractional: false,
        }
    }

    // ── Position ──────────────────────────────────────────────────────

    /// Get the current cursor position. Mirrors meta_cursor_tracker_get_position().
    pub fn get_position(&self) -> (i32, i32) {
        self.position
    }

    /// Set the cursor position. Called by the backend on input events.
    /// Mirrors the position update path in meta_cursor_tracker_update_position().
    pub fn set_position(&mut self, x: i32, y: i32) {
        if self.position != (x, y) {
            self.position = (x, y);
            self.position_changed = true;
        }
    }

    /// Whether the position has changed since the last call.
    pub fn take_position_changed(&mut self) -> bool {
        let changed = self.position_changed;
        self.position_changed = false;
        changed
    }

    // ── Visibility ────────────────────────────────────────────────────

    /// Whether the pointer cursor is visible. Mirrors
    /// meta_cursor_tracker_get_pointer_visible().
    pub fn get_pointer_visible(&self) -> bool {
        self.pointer_visible && self.pointer_shown
    }

    /// Set pointer visibility (from hardware/backend). Mirrors
    /// meta_cursor_tracker_set_pointer_visible().
    pub fn set_pointer_visible(&mut self, visible: bool) {
        self.pointer_visible = visible;
    }

    /// Whether the pointer is shown (not hidden by the shell).
    /// Mirrors meta_cursor_tracker_set_pointer_shown().
    pub fn set_pointer_shown(&mut self, shown: bool) {
        self.pointer_shown = shown;
    }

    pub fn get_pointer_shown(&self) -> bool {
        self.pointer_shown
    }

    // ── Cursor sprite ─────────────────────────────────────────────────

    /// Get the current pointer cursor sprite. Mirrors
    /// meta_cursor_tracker_get_current_cursor().
    pub fn get_current_cursor(&self) -> Option<CursorSpriteId> {
        match self.role {
            CursorRole::Pointer => self.pointer_cursor,
            CursorRole::TabletTool => self.tablet_cursor,
        }
    }

    /// Set the current pointer cursor sprite. Mirrors
    /// meta_cursor_tracker_set_current_cursor().
    pub fn set_current_cursor(&mut self, cursor: Option<CursorSpriteId>) {
        match self.role {
            CursorRole::Pointer => self.pointer_cursor = cursor,
            CursorRole::TabletTool => self.tablet_cursor = cursor,
        }
    }

    /// Set the cursor for a specific role.
    pub fn set_cursor_for_role(&mut self, role: CursorRole, cursor: Option<CursorSpriteId>) {
        match role {
            CursorRole::Pointer => self.pointer_cursor = cursor,
            CursorRole::TabletTool => self.tablet_cursor = cursor,
        }
    }

    /// Get the cursor for a specific role.
    pub fn get_cursor_for_role(&self, role: CursorRole) -> Option<CursorSpriteId> {
        match role {
            CursorRole::Pointer => self.pointer_cursor,
            CursorRole::TabletTool => self.tablet_cursor,
        }
    }

    // ── Role ──────────────────────────────────────────────────────────

    pub fn get_role(&self) -> CursorRole {
        self.role
    }

    pub fn set_role(&mut self, role: CursorRole) {
        self.role = role;
    }

    // ── Hardware vs software cursor ───────────────────────────────────

    /// Whether the hardware cursor is being used. Mirrors
    /// meta_cursor_tracker_get_hardware_cursor().
    pub fn get_hardware_cursor(&self) -> bool {
        self.hardware_cursor
    }

    /// Set whether the hardware cursor is in use. Called by the backend's
    /// cursor renderer when it determines whether hardware cursor is possible.
    pub fn set_hardware_cursor(&mut self, enabled: bool) {
        self.hardware_cursor = enabled;
    }

    // ── Scale ─────────────────────────────────────────────────────────

    /// Get the cursor scale (for HiDPI). Mirrors
    /// meta_cursor_tracker_get_cursor_scale().
    pub fn get_scale(&self) -> f32 {
        self.scale
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    /// Whether the cursor is on a fractional-scale monitor.
    pub fn is_fractional(&self) -> bool {
        self.is_fractional
    }

    pub fn set_fractional(&mut self, fractional: bool) {
        self.is_fractional = fractional;
    }

    // ── Reset ─────────────────────────────────────────────────────────

    /// Reset the cursor tracker to its initial state. Called on
    /// monitors-changed to force cursor re-evaluation.
    pub fn reset(&mut self) {
        self.pointer_cursor = None;
        self.tablet_cursor = None;
        self.scale = 1.0;
        self.is_fractional = false;
    }
}

impl Default for MetaCursorTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let tracker = MetaCursorTracker::new();
        assert!(tracker.get_pointer_visible());
        assert_eq!(tracker.get_position(), (0, 0));
        assert!(tracker.get_current_cursor().is_none());
        assert!(tracker.get_hardware_cursor());
    }

    #[test]
    fn test_position() {
        let mut tracker = MetaCursorTracker::new();
        tracker.set_position(100, 200);
        assert_eq!(tracker.get_position(), (100, 200));
        assert!(tracker.take_position_changed());
        assert!(!tracker.take_position_changed());
    }

    #[test]
    fn test_visibility() {
        let mut tracker = MetaCursorTracker::new();
        tracker.set_pointer_visible(false);
        assert!(!tracker.get_pointer_visible());

        tracker.set_pointer_visible(true);
        tracker.set_pointer_shown(false);
        assert!(!tracker.get_pointer_visible());
    }

    #[test]
    fn test_cursor_sprite() {
        let mut tracker = MetaCursorTracker::new();
        tracker.set_current_cursor(Some(42));
        assert_eq!(tracker.get_current_cursor(), Some(42));

        tracker.set_current_cursor(None);
        assert_eq!(tracker.get_current_cursor(), None);
    }

    #[test]
    fn test_role_switch() {
        let mut tracker = MetaCursorTracker::new();
        tracker.set_current_cursor(Some(1));
        assert_eq!(tracker.get_cursor_for_role(CursorRole::Pointer), Some(1));

        tracker.set_role(CursorRole::TabletTool);
        tracker.set_current_cursor(Some(2));
        assert_eq!(tracker.get_cursor_for_role(CursorRole::TabletTool), Some(2));
        assert_eq!(tracker.get_cursor_for_role(CursorRole::Pointer), Some(1));
    }

    #[test]
    fn test_hardware_cursor() {
        let mut tracker = MetaCursorTracker::new();
        assert!(tracker.get_hardware_cursor());
        tracker.set_hardware_cursor(false);
        assert!(!tracker.get_hardware_cursor());
    }

    #[test]
    fn test_scale() {
        let mut tracker = MetaCursorTracker::new();
        assert_eq!(tracker.get_scale(), 1.0);
        tracker.set_scale(2.0);
        assert_eq!(tracker.get_scale(), 2.0);
    }

    #[test]
    fn test_reset() {
        let mut tracker = MetaCursorTracker::new();
        tracker.set_current_cursor(Some(42));
        tracker.set_scale(2.0);
        tracker.reset();
        assert!(tracker.get_current_cursor().is_none());
        assert_eq!(tracker.get_scale(), 1.0);
    }
}
