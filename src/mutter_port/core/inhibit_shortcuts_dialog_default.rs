//! Default inhibit shortcuts dialog implementation ported from GNOME Mutter
//! (src/core/meta-inhibit-shortcuts-dialog-default.c).
//!
//! Provides a basic in-process implementation of the inhibit shortcuts dialog
//! for systems without a display server or as a fallback.
//!
//! Ported from: /home/justin/Downloads/mutter-main/src/core/meta-inhibit-shortcuts-dialog-default.c
//! Omitted: GTK widget machinery, ClutterActor rendering, compositor integration,
//! GObject class machinery

use super::inhibit_shortcuts_dialog::{InhibitShortcutsDialog, InhibitShortcutsDialogResponse};
use crate::desktop::window_manager::WindowId;

/// Default implementation of inhibit shortcuts dialog.
///
/// Provides minimal UI functionality suitable for headless or simple environments.
/// In full Mutter, this would be replaced with GTK/Clutter-based UI.
pub struct DefaultInhibitShortcutsDialog {
    /// Associated window ID.
    window_id: WindowId,
    /// Whether the dialog is visible.
    visible: bool,
    /// Pending response (if any).
    response: Option<InhibitShortcutsDialogResponse>,
    /// Number of times the inhibit was requested (for tracking stickiness).
    inhibit_count: u32,
}

impl DefaultInhibitShortcutsDialog {
    /// Create a new default inhibit shortcuts dialog.
    pub fn new(window_id: WindowId) -> Self {
        DefaultInhibitShortcutsDialog {
            window_id,
            visible: false,
            response: None,
            inhibit_count: 0,
        }
    }

    /// Record an inhibit request from the application.
    pub fn record_inhibit(&mut self) {
        self.inhibit_count += 1;
    }

    /// Record a shortcut release (application no longer inhibiting).
    pub fn record_uninhibit(&mut self) {
        if self.inhibit_count > 0 {
            self.inhibit_count -= 1;
        }
    }

    /// Check how many times shortcuts are currently inhibited.
    pub fn inhibit_count(&self) -> u32 {
        self.inhibit_count
    }

    /// Get the pending response (and clear it).
    pub fn take_response(&mut self) -> Option<InhibitShortcutsDialogResponse> {
        self.response.take()
    }

    /// Set the response to the dialog.
    pub fn set_response(&mut self, response: InhibitShortcutsDialogResponse) {
        self.response = Some(response);
    }

    /// Auto-hide the dialog after a timeout (for unresponsive apps).
    /// Returns true if dialog was visible and should be considered "timed out".
    pub fn check_timeout(&mut self, elapsed_ms: u32) -> bool {
        // Typical timeout: 10 seconds
        const DEFAULT_TIMEOUT_MS: u32 = 10_000;
        elapsed_ms > DEFAULT_TIMEOUT_MS && self.visible
    }

    /// Request confirmation via the dialog.
    /// In a real UI environment, this would show an on-screen prompt.
    pub fn request_confirmation(&mut self) {
        self.visible = true;
    }
}

impl InhibitShortcutsDialog for DefaultInhibitShortcutsDialog {
    fn show(&mut self) {
        self.visible = true;
    }

    fn hide(&mut self) {
        self.visible = false;
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn window_id(&self) -> WindowId {
        self.window_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creation() {
        let dialog = DefaultInhibitShortcutsDialog::new(WindowId(1));
        assert_eq!(dialog.window_id(), WindowId(1));
        assert!(!dialog.is_visible());
        assert_eq!(dialog.inhibit_count(), 0);
    }

    #[test]
    fn test_show_hide() {
        let mut dialog = DefaultInhibitShortcutsDialog::new(WindowId(1));
        assert!(!dialog.is_visible());

        dialog.show();
        assert!(dialog.is_visible());

        dialog.hide();
        assert!(!dialog.is_visible());
    }

    #[test]
    fn test_inhibit_count() {
        let mut dialog = DefaultInhibitShortcutsDialog::new(WindowId(1));
        assert_eq!(dialog.inhibit_count(), 0);

        dialog.record_inhibit();
        assert_eq!(dialog.inhibit_count(), 1);

        dialog.record_inhibit();
        assert_eq!(dialog.inhibit_count(), 2);

        dialog.record_uninhibit();
        assert_eq!(dialog.inhibit_count(), 1);

        dialog.record_uninhibit();
        assert_eq!(dialog.inhibit_count(), 0);
    }

    #[test]
    fn test_inhibit_count_wont_go_negative() {
        let mut dialog = DefaultInhibitShortcutsDialog::new(WindowId(1));
        dialog.record_uninhibit();
        dialog.record_uninhibit();
        assert_eq!(dialog.inhibit_count(), 0); // Should not be negative
    }

    #[test]
    fn test_response_handling() {
        let mut dialog = DefaultInhibitShortcutsDialog::new(WindowId(1));
        assert!(dialog.take_response().is_none());

        dialog.set_response(InhibitShortcutsDialogResponse::Allow);
        assert_eq!(
            dialog.take_response(),
            Some(InhibitShortcutsDialogResponse::Allow)
        );

        // Response should be consumed after take
        assert!(dialog.take_response().is_none());
    }

    #[test]
    fn test_request_confirmation() {
        let mut dialog = DefaultInhibitShortcutsDialog::new(WindowId(1));
        assert!(!dialog.is_visible());

        dialog.request_confirmation();
        assert!(dialog.is_visible());
    }

    #[test]
    fn test_timeout_detection() {
        let mut dialog = DefaultInhibitShortcutsDialog::new(WindowId(1));
        dialog.show();

        // Short timeout should not trigger
        assert!(!dialog.check_timeout(5000));

        // Long timeout should trigger
        assert!(dialog.check_timeout(11000));

        // After hiding, timeout should not trigger
        dialog.hide();
        assert!(!dialog.check_timeout(15000));
    }
}
