//! KMS hardware cursor management.
//!
//! Manages hardware cursor plane updates and synchronization.
//! Ported from `meta-kms-cursor-manager.c`.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;

/// Cursor visibility state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CursorVisibility {
    Visible,
    Hidden,
}

/// Cursor update for a single CRTC
#[derive(Debug, Clone)]
pub struct CursorUpdate {
    /// CRTC ID this cursor belongs to
    pub crtc_id: u32,
    /// Framebuffer ID for cursor image
    pub fb_id: Option<u32>,
    /// X coordinate
    pub x: i32,
    /// Y coordinate
    pub y: i32,
    /// Visibility
    pub visibility: CursorVisibility,
}

impl CursorUpdate {
    /// Create a cursor update
    pub fn new(crtc_id: u32, x: i32, y: i32) -> Self {
        CursorUpdate {
            crtc_id,
            fb_id: None,
            x,
            y,
            visibility: CursorVisibility::Visible,
        }
    }

    /// Set cursor framebuffer
    pub fn set_framebuffer(&mut self, fb_id: u32) {
        self.fb_id = Some(fb_id);
    }

    /// Hide cursor
    pub fn hide(&mut self) {
        self.fb_id = None;
        self.visibility = CursorVisibility::Hidden;
    }

    /// Show cursor
    pub fn show(&mut self) {
        self.visibility = CursorVisibility::Visible;
    }

    /// Move cursor
    pub fn move_to(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    /// Check if cursor is visible
    pub fn is_visible(&self) -> bool {
        self.visibility == CursorVisibility::Visible && self.fb_id.is_some()
    }
}

/// Manages cursor updates across all CRTCs
#[derive(Debug)]
pub struct KmsCursorManager {
    /// Pending cursor updates
    pub pending_updates: Vec<CursorUpdate>,
    /// Whether hardware cursor is available
    pub has_hw_cursor: bool,
}

impl KmsCursorManager {
    /// Create a new cursor manager
    pub fn new() -> Self {
        KmsCursorManager {
            pending_updates: Vec::new(),
            has_hw_cursor: false,
        }
    }

    /// Set whether hardware cursor is available
    pub fn set_hw_cursor_available(&mut self, available: bool) {
        self.has_hw_cursor = available;
    }

    /// Check if hardware cursor is available
    pub fn has_hardware_cursor(&self) -> bool {
        self.has_hw_cursor
    }

    /// Queue a cursor update
    pub fn queue_update(&mut self, update: CursorUpdate) {
        // Remove any existing update for this CRTC
        self.pending_updates.retain(|u| u.crtc_id != update.crtc_id);
        self.pending_updates.push(update);
    }

    /// Get pending updates
    pub fn get_pending_updates(&self) -> &[CursorUpdate] {
        &self.pending_updates
    }

    /// Apply all pending updates (clear the queue)
    pub fn apply_updates(&mut self) -> Vec<CursorUpdate> {
        core::mem::take(&mut self.pending_updates)
    }

    /// Clear pending updates
    pub fn clear_updates(&mut self) {
        self.pending_updates.clear();
    }

    /// Get update for specific CRTC
    pub fn get_update_for_crtc(&self, crtc_id: u32) -> Option<&CursorUpdate> {
        self.pending_updates.iter().find(|u| u.crtc_id == crtc_id)
    }
}

impl Default for KmsCursorManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cursor_update_creation() {
        let update = CursorUpdate::new(0, 100, 200);
        assert_eq!(update.crtc_id, 0);
        assert_eq!(update.x, 100);
        assert_eq!(update.y, 200);
        assert!(!update.is_visible());
    }

    #[test]
    fn test_cursor_visibility() {
        let mut update = CursorUpdate::new(0, 100, 200);
        update.set_framebuffer(42);
        assert!(update.is_visible());
        update.hide();
        assert!(!update.is_visible());
        update.show();
        assert!(update.is_visible());
    }

    #[test]
    fn test_cursor_manager() {
        let mut manager = KmsCursorManager::new();
        manager.set_hw_cursor_available(true);
        assert!(manager.has_hardware_cursor());

        let update = CursorUpdate::new(0, 100, 200);
        manager.queue_update(update);
        assert_eq!(manager.get_pending_updates().len(), 1);
    }

    #[test]
    fn test_apply_updates() {
        let mut manager = KmsCursorManager::new();
        let update1 = CursorUpdate::new(0, 100, 200);
        let update2 = CursorUpdate::new(1, 150, 250);
        manager.queue_update(update1);
        manager.queue_update(update2);

        let applied = manager.apply_updates();
        assert_eq!(applied.len(), 2);
        assert!(manager.pending_updates.is_empty());
    }
}
