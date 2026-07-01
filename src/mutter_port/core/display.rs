//! Display/Compositor core ported from GNOME Mutter's src/core/display.c / display-private.h
//!
//! Implements the central MetaDisplay object that manages the compositor, window stack,
//! keyboard bindings, focus, and display-wide operations. A single display manages all
//! connected monitors and windows.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/display.c

use crate::desktop::window_manager::{WindowId, WindowManager};
use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

/// Unique identifier for a display (typically only one per compositor).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DisplayId(pub u32);

/// Result of window focus operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusResult {
    /// Window successfully received focus.
    Focused,
    /// Window could not be focused (e.g., unmap pending).
    Denied,
    /// Already focused.
    AlreadyFocused,
}

/// Focus mode for window selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusMode {
    /// Focus follows the mouse pointer.
    FollowsMouse,
    /// Focus follows keyboard navigation (sloppy focus).
    Sloppy,
    /// Strict focus mode.
    Strict,
}

/// The central display/compositor object managing all windows and operations.
#[derive(Debug)]
pub struct MetaDisplay {
    /// Unique identifier for this display.
    id: DisplayId,

    /// The currently focused window (if any).
    focus_window: Option<WindowId>,

    /// Last focus timestamp (for focus steal prevention).
    last_focus_time: u32,

    /// Last user interaction timestamp.
    last_user_time: u32,

    /// Current server time estimate.
    current_time: u32,

    /// Mouse mode tracking (vs keyboard navigation mode).
    mouse_mode: bool,

    /// Focus mode (follows mouse, sloppy, strict).
    focus_mode: FocusMode,

    /// Window sequence counter for stable ordering.
    window_sequence_counter: u32,

    /// Map of window IDs to their metadata.
    windows: BTreeMap<WindowId, WindowMetadata>,

    /// Reference to the underlying window manager.
    window_manager: Option<*mut WindowManager>,

    /// Pending focus change timeout ID (if any).
    focus_timeout_id: Option<u32>,

    /// Pending autoraise timeout ID.
    autoraise_timeout_id: Option<u32>,
    autoraise_window: Option<WindowId>,

    /// Whether display is currently opening.
    display_opening: bool,

    /// Display closing state (0 = open, > 0 = closing).
    closing: u32,

    /// Last visual bell time (microseconds).
    last_visual_bell_time_us: u64,
}

/// Per-window metadata stored in the display.
#[derive(Debug, Clone)]
struct WindowMetadata {
    /// Stable sequence number for this window.
    stable_sequence: u32,
    /// Last user interaction time for this window.
    net_wm_user_time: u32,
    /// Unmaps pending from window manager.
    unmaps_pending: u32,
}

impl MetaDisplay {
    /// Create a new display with the given ID.
    pub fn new(id: DisplayId) -> Self {
        static SEQUENCE_COUNTER: AtomicU32 = AtomicU32::new(0);

        MetaDisplay {
            id,
            focus_window: None,
            last_focus_time: 0,
            last_user_time: 0,
            current_time: 0,
            mouse_mode: false,
            focus_mode: FocusMode::FollowsMouse,
            window_sequence_counter: SEQUENCE_COUNTER.fetch_add(1, Ordering::Relaxed),
            windows: BTreeMap::new(),
            window_manager: None,
            focus_timeout_id: None,
            autoraise_timeout_id: None,
            autoraise_window: None,
            display_opening: true,
            closing: 0,
            last_visual_bell_time_us: 0,
        }
    }

    /// Get this display's unique identifier.
    pub fn id(&self) -> DisplayId {
        self.id
    }

    /// Get the currently focused window, if any.
    pub fn focus_window(&self) -> Option<WindowId> {
        self.focus_window
    }

    /// Set the focused window. Returns result of focus operation.
    pub fn set_focus_window(&mut self, window_id: Option<WindowId>, timestamp: u32) -> FocusResult {
        if self.focus_window == window_id {
            return FocusResult::AlreadyFocused;
        }

        self.focus_window = window_id;
        self.last_focus_time = timestamp;

        if let Some(wid) = window_id {
            if let Some(meta) = self.windows.get_mut(&wid) {
                meta.net_wm_user_time = timestamp;
            }
        }

        FocusResult::Focused
    }

    /// Register a new window with the display.
    pub fn register_window(&mut self, window_id: WindowId, timestamp: u32) {
        let stable_seq = self.allocate_window_sequence();
        self.windows.insert(
            window_id,
            WindowMetadata {
                stable_sequence: stable_seq,
                net_wm_user_time: timestamp,
                unmaps_pending: 0,
            },
        );
    }

    /// Unregister a window from the display.
    pub fn unregister_window(&mut self, window_id: WindowId) {
        self.windows.remove(&window_id);
        if self.focus_window == Some(window_id) {
            self.focus_window = None;
        }
    }

    /// Check if a window is registered on this display.
    pub fn has_window(&self, window_id: WindowId) -> bool {
        self.windows.contains_key(&window_id)
    }

    /// Get all registered window IDs.
    pub fn list_windows(&self) -> Vec<WindowId> {
        self.windows.keys().copied().collect()
    }

    /// Get the stable sequence number for a window (used for consistent ordering).
    pub fn window_stable_sequence(&self, window_id: WindowId) -> Option<u32> {
        self.windows.get(&window_id).map(|m| m.stable_sequence)
    }

    /// Update the last user interaction timestamp.
    pub fn update_user_time(&mut self, timestamp: u32) {
        if self.is_timestamp_after(timestamp, self.last_user_time) {
            self.last_user_time = timestamp;
        }
    }

    /// Get the last user interaction timestamp.
    pub fn last_user_time(&self) -> u32 {
        self.last_user_time
    }

    /// Update the current server time.
    pub fn update_current_time(&mut self, timestamp: u32) {
        self.current_time = timestamp;
    }

    /// Get the current server time.
    pub fn current_time(&self) -> u32 {
        self.current_time
    }

    /// Check if time1 is before time2, accounting for timestamp wraparound.
    /// This matches Mutter's XSERVER_TIME_IS_BEFORE semantics.
    pub fn is_timestamp_after(&self, time1: u32, time2: u32) -> bool {
        if time2 == 0 {
            return false;
        }
        if time1 == 0 {
            return true;
        }

        let time_diff = time1.wrapping_sub(time2);
        time_diff < (u32::MAX / 2)
    }

    /// Set focus mode (follows mouse vs keyboard navigation).
    pub fn set_focus_mode(&mut self, mode: FocusMode) {
        self.focus_mode = mode;
    }

    /// Get current focus mode.
    pub fn focus_mode(&self) -> FocusMode {
        self.focus_mode
    }

    /// Set mouse mode tracking (vs keyboard navigation).
    pub fn set_mouse_mode(&mut self, mouse_mode: bool) {
        self.mouse_mode = mouse_mode;
    }

    /// Get mouse mode state.
    pub fn mouse_mode(&self) -> bool {
        self.mouse_mode
    }

    /// Update visual bell timestamp.
    pub fn update_visual_bell_time(&mut self, time_us: u64) {
        self.last_visual_bell_time_us = time_us;
    }

    /// Get last visual bell time.
    pub fn last_visual_bell_time(&self) -> u64 {
        self.last_visual_bell_time_us
    }

    /// Allocate a new window sequence number for stable ordering.
    fn allocate_window_sequence(&mut self) -> u32 {
        let seq = self.window_sequence_counter;
        self.window_sequence_counter = self.window_sequence_counter.wrapping_add(1);
        seq
    }

    /// Check if display is in opening phase.
    pub fn is_display_opening(&self) -> bool {
        self.display_opening
    }

    /// Mark display opening phase as complete.
    pub fn set_display_opening(&mut self, opening: bool) {
        self.display_opening = opening;
    }

    /// Check if display is closing.
    pub fn is_closing(&self) -> bool {
        self.closing > 0
    }

    /// Begin display shutdown.
    pub fn begin_shutdown(&mut self) {
        self.closing = self.closing.saturating_add(1);
    }

    /// End display shutdown.
    pub fn end_shutdown(&mut self) {
        if self.closing > 0 {
            self.closing -= 1;
        }
    }

    /// Queue an autoraise operation for a window.
    pub fn queue_autoraise(&mut self, window_id: WindowId, timeout_id: u32) {
        self.autoraise_window = Some(window_id);
        self.autoraise_timeout_id = Some(timeout_id);
    }

    /// Remove pending autoraise operation.
    pub fn remove_autoraise(&mut self) {
        self.autoraise_window = None;
        self.autoraise_timeout_id = None;
    }

    /// Get the pending autoraise window (if any).
    pub fn pending_autoraise(&self) -> Option<WindowId> {
        self.autoraise_window
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_creation() {
        let display = MetaDisplay::new(DisplayId(0));
        assert_eq!(display.id(), DisplayId(0));
        assert_eq!(display.focus_window(), None);
        assert_eq!(display.is_display_opening(), true);
        assert_eq!(display.is_closing(), false);
    }

    #[test]
    fn test_window_registration() {
        let mut display = MetaDisplay::new(DisplayId(0));
        let window_id = WindowId(1);

        display.register_window(window_id, 1000);
        assert!(display.has_window(window_id));
        assert!(display.window_stable_sequence(window_id).is_some());

        display.unregister_window(window_id);
        assert!(!display.has_window(window_id));
    }

    #[test]
    fn test_focus_management() {
        let mut display = MetaDisplay::new(DisplayId(0));
        let window_id = WindowId(42);

        display.register_window(window_id, 1000);
        let result = display.set_focus_window(Some(window_id), 1000);

        assert_eq!(result, FocusResult::Focused);
        assert_eq!(display.focus_window(), Some(window_id));
        assert_eq!(display.last_focus_time, 1000);
    }

    #[test]
    fn test_timestamp_ordering() {
        let mut display = MetaDisplay::new(DisplayId(0));

        // Time 0 is special (no timestamp)
        assert!(!display.is_timestamp_after(100, 0));
        assert!(display.is_timestamp_after(0, 100));

        // Normal ordering
        assert!(display.is_timestamp_after(200, 100));
        assert!(!display.is_timestamp_after(100, 200));

        // Wraparound cases
        let t1 = u32::MAX - 100;
        let t2 = 100;
        assert!(display.is_timestamp_after(t2, t1)); // Wrapped around
    }

    #[test]
    fn test_focus_mode() {
        let mut display = MetaDisplay::new(DisplayId(0));
        assert_eq!(display.focus_mode(), FocusMode::FollowsMouse);

        display.set_focus_mode(FocusMode::Strict);
        assert_eq!(display.focus_mode(), FocusMode::Strict);
    }

    #[test]
    fn test_autoraise() {
        let mut display = MetaDisplay::new(DisplayId(0));
        let window_id = WindowId(1);

        display.queue_autoraise(window_id, 42);
        assert_eq!(display.pending_autoraise(), Some(window_id));

        display.remove_autoraise();
        assert_eq!(display.pending_autoraise(), None);
    }

    #[test]
    fn test_list_windows() {
        let mut display = MetaDisplay::new(DisplayId(0));

        for i in 0..5 {
            display.register_window(WindowId(i as u64), 1000);
        }

        let windows = display.list_windows();
        assert_eq!(windows.len(), 5);
    }
}
