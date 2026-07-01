//! Close dialog interface ported from GNOME Mutter (src/core/meta-close-dialog.c).
//!
//! Defines the interface for unresponsive window close dialogs.
//! Implementations display UI when a window stops responding to close requests.
//!
//! Ported from: /home/justin/Downloads/mutter-main/src/core/meta-close-dialog.c
//! Omitted: GObject interface machinery (G_DEFINE_INTERFACE, g_signal_new, etc.),
//! signal emission (requires GObject introspection)

use crate::desktop::window_manager::WindowId;
use alloc::vec::Vec;

/// Possible responses from a close dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseDialogResponse {
    /// User chose to wait for the window to respond.
    Wait = 0,
    /// User chose to force-close the window.
    ForceClose = 1,
}

/// Close dialog interface trait.
///
/// Implementations provide UI for handling unresponsive windows.
pub trait CloseDialog: Sync + Send {
    /// Show the close dialog.
    fn show(&mut self);

    /// Hide the close dialog.
    fn hide(&mut self);

    /// Check if the dialog is currently visible.
    fn is_visible(&self) -> bool;

    /// Focus the dialog (bring it to foreground).
    fn focus(&mut self);

    /// Get the associated window ID.
    fn window_id(&self) -> WindowId;
}

/// Simple in-memory close dialog implementation for testing/fallback.
pub struct SimpleCloseDialog {
    /// Associated window ID.
    window_id: WindowId,
    /// Whether the dialog is visible.
    visible: bool,
    /// Last response received (if any).
    last_response: Option<CloseDialogResponse>,
}

impl SimpleCloseDialog {
    /// Create a new close dialog for a window.
    pub fn new(window_id: WindowId) -> Self {
        SimpleCloseDialog {
            window_id,
            visible: false,
            last_response: None,
        }
    }

    /// Get the last response from this dialog.
    pub fn last_response(&self) -> Option<CloseDialogResponse> {
        self.last_response
    }

    /// Set the response (for testing).
    pub fn set_response(&mut self, response: CloseDialogResponse) {
        self.last_response = Some(response);
        self.visible = false;
    }
}

impl CloseDialog for SimpleCloseDialog {
    fn show(&mut self) {
        self.visible = true;
    }

    fn hide(&mut self) {
        self.visible = false;
    }

    fn is_visible(&self) -> bool {
        self.visible
    }

    fn focus(&mut self) {
        self.visible = true;
    }

    fn window_id(&self) -> WindowId {
        self.window_id
    }
}

/// Manager for close dialogs.
pub struct CloseDialogManager {
    dialogs: alloc::collections::BTreeMap<WindowId, alloc::boxed::Box<dyn CloseDialog>>,
}

impl CloseDialogManager {
    /// Create a new close dialog manager.
    pub fn new() -> Self {
        CloseDialogManager {
            dialogs: alloc::collections::BTreeMap::new(),
        }
    }

    /// Register a new close dialog for a window.
    pub fn register(&mut self, dialog: alloc::boxed::Box<dyn CloseDialog>) {
        let window_id = dialog.window_id();
        self.dialogs.insert(window_id, dialog);
    }

    /// Unregister and remove a close dialog.
    pub fn unregister(&mut self, window_id: WindowId) {
        self.dialogs.remove(&window_id);
    }

    /// Show the dialog for a window.
    pub fn show(&mut self, window_id: WindowId) {
        if let Some(dialog) = self.dialogs.get_mut(&window_id) {
            dialog.show();
        }
    }

    /// Hide the dialog for a window.
    pub fn hide(&mut self, window_id: WindowId) {
        if let Some(dialog) = self.dialogs.get_mut(&window_id) {
            dialog.hide();
        }
    }

    /// Focus the dialog for a window.
    pub fn focus(&mut self, window_id: WindowId) {
        if let Some(dialog) = self.dialogs.get_mut(&window_id) {
            dialog.focus();
        }
    }

    /// Check if a dialog is visible for a window.
    pub fn is_visible(&self, window_id: WindowId) -> bool {
        self.dialogs
            .get(&window_id)
            .map(|d| d.is_visible())
            .unwrap_or(false)
    }

    /// Get all visible dialogs.
    pub fn visible_dialogs(&self) -> Vec<WindowId> {
        self.dialogs
            .iter()
            .filter(|(_, d)| d.is_visible())
            .map(|(id, _)| *id)
            .collect()
    }
}

impl Default for CloseDialogManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_close_dialog_creation() {
        let dialog = SimpleCloseDialog::new(WindowId(1));
        assert_eq!(dialog.window_id(), WindowId(1));
        assert!(!dialog.is_visible());
    }

    #[test]
    fn test_close_dialog_show_hide() {
        let mut dialog = SimpleCloseDialog::new(WindowId(1));
        assert!(!dialog.is_visible());

        dialog.show();
        assert!(dialog.is_visible());

        dialog.hide();
        assert!(!dialog.is_visible());
    }

    #[test]
    fn test_close_dialog_response() {
        let mut dialog = SimpleCloseDialog::new(WindowId(1));
        assert_eq!(dialog.last_response(), None);

        dialog.set_response(CloseDialogResponse::ForceClose);
        assert_eq!(
            dialog.last_response(),
            Some(CloseDialogResponse::ForceClose)
        );
        assert!(!dialog.is_visible());
    }

    #[test]
    fn test_close_dialog_manager() {
        let mut manager = CloseDialogManager::new();
        let dialog1 = SimpleCloseDialog::new(WindowId(1));
        let dialog2 = SimpleCloseDialog::new(WindowId(2));

        manager.register(alloc::boxed::Box::new(dialog1));
        manager.register(alloc::boxed::Box::new(dialog2));

        manager.show(WindowId(1));
        assert!(manager.is_visible(WindowId(1)));
        assert!(!manager.is_visible(WindowId(2)));

        manager.show(WindowId(2));
        assert!(manager.is_visible(WindowId(1)));
        assert!(manager.is_visible(WindowId(2)));

        manager.hide(WindowId(1));
        assert!(!manager.is_visible(WindowId(1)));
        assert!(manager.is_visible(WindowId(2)));
    }

    #[test]
    fn test_visible_dialogs() {
        let mut manager = CloseDialogManager::new();
        manager.register(alloc::boxed::Box::new(SimpleCloseDialog::new(WindowId(1))));
        manager.register(alloc::boxed::Box::new(SimpleCloseDialog::new(WindowId(2))));

        manager.show(WindowId(1));
        manager.show(WindowId(2));

        let visible = manager.visible_dialogs();
        assert_eq!(visible.len(), 2);
    }
}
