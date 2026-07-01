//! Window deletion and close handling ported from GNOME Mutter (src/core/delete.c).
//!
//! Implements window close/kill behavior: graceful close via WM_DELETE_WINDOW,
//! force-kill via signal, and close dialog management.
//!
//! Ported from: /home/justin/Downloads/mutter-main/src/core/delete.c
//! Omitted: MetaCloseDialog GObject machinery, compositor integration, X11/Wayland specific window ops,
//! Ping/pong window liveness checking (requires full compositor and window manager integration)

use crate::desktop::window_manager::WindowId;

/// Maximum number of events queued during window responsiveness check.
const MAX_QUEUED_EVENTS: u32 = 400;

/// Represents the response from a close dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseDialogResponse {
    /// User chose to wait for window to respond.
    Wait,
    /// User chose to force-close the window.
    ForceClose,
}

/// State of window close/responsiveness monitoring.
#[derive(Debug, Clone, Copy)]
pub struct WindowCloseState {
    /// Whether a close dialog is currently shown.
    pub has_dialog: bool,
    /// Number of events received during ping.
    pub events_during_ping: u32,
    /// Whether the window is considered alive.
    pub is_alive: bool,
}

impl Default for WindowCloseState {
    fn default() -> Self {
        WindowCloseState {
            has_dialog: false,
            events_during_ping: 0,
            is_alive: true,
        }
    }
}

/// Window deletion/close management.
pub struct WindowDeleteManager {
    window_states: alloc::collections::BTreeMap<WindowId, WindowCloseState>,
}

impl WindowDeleteManager {
    /// Create a new window deletion manager.
    pub fn new() -> Self {
        WindowDeleteManager {
            window_states: alloc::collections::BTreeMap::new(),
        }
    }

    /// Initialize close state for a window.
    pub fn register_window(&mut self, window_id: WindowId) {
        self.window_states
            .insert(window_id, WindowCloseState::default());
    }

    /// Remove a window from tracking (e.g., when destroyed).
    pub fn unregister_window(&mut self, window_id: WindowId) {
        self.window_states.remove(&window_id);
    }

    /// Send a close request to a window via WM_DELETE_WINDOW.
    ///
    /// This gracefully asks the window to close itself.
    /// If the window doesn't respond within a timeout, a close dialog appears.
    ///
    /// # Arguments
    /// * `window_id` - The window to close
    /// * `timestamp` - X11 timestamp for the close request
    pub fn delete_window(&mut self, window_id: WindowId, _timestamp: u32) {
        if let Some(state) = self.window_states.get_mut(&window_id) {
            // In a full integration, this would:
            // 1. Send WM_DELETE_WINDOW ClientMessage
            // 2. Start a ping timer
            // 3. Eventually show close dialog if no response
            // Omitted: X11/Wayland message sending - requires window manager integration
            state.events_during_ping = 0;
        }
    }

    /// Force-kill a window process.
    ///
    /// Sends SIGKILL (signal 9) to force the window's process to terminate.
    /// This should only be used after graceful close has failed.
    ///
    /// # Arguments
    /// * `window_id` - The window to kill
    /// * `pid` - Process ID of the window (0 = unknown/not available)
    pub fn kill_window(&self, window_id: WindowId, pid: i32) {
        if pid > 0 {
            // In a full implementation with process access:
            // kill(pid, SIGKILL) -> terminates the process
            // Omitted: signal sending - requires process/syscall integration
            _ = (window_id, pid);
        }
    }

    /// Record that an event occurred during window responsiveness check.
    ///
    /// Tracks the number of events received while waiting for a ping response.
    /// If too many events accumulate, the window is marked as unresponsive.
    pub fn record_event_during_ping(&mut self, window_id: WindowId) {
        if let Some(state) = self.window_states.get_mut(&window_id) {
            state.events_during_ping += 1;
            if state.events_during_ping > MAX_QUEUED_EVENTS {
                state.is_alive = false;
            }
        }
    }

    /// Check if a window is alive based on event count.
    pub fn is_window_alive(&self, window_id: WindowId) -> bool {
        self.window_states
            .get(&window_id)
            .map(|s| s.is_alive)
            .unwrap_or(true)
    }

    /// Set window alive status after ping response or timeout.
    pub fn set_window_alive(&mut self, window_id: WindowId, alive: bool) {
        if let Some(state) = self.window_states.get_mut(&window_id) {
            state.is_alive = alive;
            state.events_during_ping = 0;
        }
    }

    /// Show the close dialog for a window.
    pub fn show_close_dialog(&mut self, window_id: WindowId) {
        if let Some(state) = self.window_states.get_mut(&window_id) {
            state.has_dialog = true;
        }
    }

    /// Hide the close dialog for a window.
    pub fn hide_close_dialog(&mut self, window_id: WindowId) {
        if let Some(state) = self.window_states.get_mut(&window_id) {
            state.has_dialog = false;
        }
    }

    /// Check if a close dialog is shown for this window.
    pub fn has_close_dialog(&self, window_id: WindowId) -> bool {
        self.window_states
            .get(&window_id)
            .map(|s| s.has_dialog)
            .unwrap_or(false)
    }

    /// Handle a close dialog response.
    pub fn handle_close_dialog_response(
        &mut self,
        window_id: WindowId,
        response: CloseDialogResponse,
    ) {
        match response {
            CloseDialogResponse::ForceClose => {
                self.kill_window(window_id, 0);
                self.hide_close_dialog(window_id);
            }
            CloseDialogResponse::Wait => {
                // Reset event counter and wait longer
                if let Some(state) = self.window_states.get_mut(&window_id) {
                    state.events_during_ping = 0;
                }
            }
        }
    }
}

impl Default for WindowDeleteManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_window_close_state_default() {
        let state = WindowCloseState::default();
        assert!(!state.has_dialog);
        assert_eq!(state.events_during_ping, 0);
        assert!(state.is_alive);
    }

    #[test]
    fn test_register_window() {
        let mut mgr = WindowDeleteManager::new();
        let win_id = WindowId(1);

        mgr.register_window(win_id);
        assert!(mgr.is_window_alive(win_id));
    }

    #[test]
    fn test_show_hide_close_dialog() {
        let mut mgr = WindowDeleteManager::new();
        let win_id = WindowId(1);

        mgr.register_window(win_id);
        assert!(!mgr.has_close_dialog(win_id));

        mgr.show_close_dialog(win_id);
        assert!(mgr.has_close_dialog(win_id));

        mgr.hide_close_dialog(win_id);
        assert!(!mgr.has_close_dialog(win_id));
    }

    #[test]
    fn test_event_tracking() {
        let mut mgr = WindowDeleteManager::new();
        let win_id = WindowId(1);

        mgr.register_window(win_id);

        for _ in 0..100 {
            mgr.record_event_during_ping(win_id);
        }

        assert!(mgr.is_window_alive(win_id));
    }

    #[test]
    fn test_too_many_events_marks_unresponsive() {
        let mut mgr = WindowDeleteManager::new();
        let win_id = WindowId(1);

        mgr.register_window(win_id);

        for _ in 0..(MAX_QUEUED_EVENTS + 1) {
            mgr.record_event_during_ping(win_id);
        }

        assert!(!mgr.is_window_alive(win_id));
    }

    #[test]
    fn test_set_window_alive() {
        let mut mgr = WindowDeleteManager::new();
        let win_id = WindowId(1);

        mgr.register_window(win_id);
        mgr.set_window_alive(win_id, false);
        assert!(!mgr.is_window_alive(win_id));

        mgr.set_window_alive(win_id, true);
        assert!(mgr.is_window_alive(win_id));
    }

    #[test]
    fn test_handle_close_dialog_response() {
        let mut mgr = WindowDeleteManager::new();
        let win_id = WindowId(1);

        mgr.register_window(win_id);
        mgr.show_close_dialog(win_id);

        mgr.handle_close_dialog_response(win_id, CloseDialogResponse::Wait);
        assert!(mgr.has_close_dialog(win_id));

        mgr.handle_close_dialog_response(win_id, CloseDialogResponse::ForceClose);
        assert!(!mgr.has_close_dialog(win_id));
    }
}
