//! Inhibit shortcuts dialog interface ported from GNOME Mutter (src/core/meta-inhibit-shortcuts-dialog.c).
//!
//! Interface for dialogs that notify users when applications are inhibiting system shortcuts.
//! Used when fullscreen games or apps disable compositor/WM keyboard shortcuts.
//!
//! Ported from: /home/justin/Downloads/mutter-main/src/core/meta-inhibit-shortcuts-dialog.c
//! Omitted: GObject interface machinery (G_DEFINE_INTERFACE, g_signal_new, etc.),
//! signal emission, D-Bus property installation

use crate::desktop::window_manager::WindowId;
use alloc::vec::Vec;

/// Possible responses from an inhibit shortcuts dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InhibitShortcutsDialogResponse {
    /// User chose to continue allowing shortcuts to be inhibited.
    Allow = 0,
    /// User chose to restore system shortcuts.
    Deny = 1,
}

/// Inhibit shortcuts dialog interface trait.
///
/// Implementations display UI when an application requests
/// inhibition of system shortcuts (e.g., fullscreen games).
pub trait InhibitShortcutsDialog: Sync + Send {
    /// Show the inhibit shortcuts dialog.
    fn show(&mut self);

    /// Hide the inhibit shortcuts dialog.
    fn hide(&mut self);

    /// Check if the dialog is currently visible.
    fn is_visible(&self) -> bool;

    /// Get the associated window ID.
    fn window_id(&self) -> WindowId;
}

/// Simple in-memory inhibit shortcuts dialog implementation.
pub struct SimpleInhibitShortcutsDialog {
    /// Associated window ID.
    window_id: WindowId,
    /// Whether the dialog is visible.
    visible: bool,
    /// Last response received (if any).
    last_response: Option<InhibitShortcutsDialogResponse>,
}

impl SimpleInhibitShortcutsDialog {
    /// Create a new inhibit shortcuts dialog for a window.
    pub fn new(window_id: WindowId) -> Self {
        SimpleInhibitShortcutsDialog {
            window_id,
            visible: false,
            last_response: None,
        }
    }

    /// Get the last response from this dialog.
    pub fn last_response(&self) -> Option<InhibitShortcutsDialogResponse> {
        self.last_response
    }

    /// Set the response (for testing).
    pub fn set_response(&mut self, response: InhibitShortcutsDialogResponse) {
        self.last_response = Some(response);
    }
}

impl InhibitShortcutsDialog for SimpleInhibitShortcutsDialog {
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

/// Manager for inhibit shortcuts dialogs.
pub struct InhibitShortcutsDialogManager {
    dialogs: alloc::collections::BTreeMap<WindowId, alloc::boxed::Box<dyn InhibitShortcutsDialog>>,
}

impl InhibitShortcutsDialogManager {
    /// Create a new inhibit shortcuts dialog manager.
    pub fn new() -> Self {
        InhibitShortcutsDialogManager {
            dialogs: alloc::collections::BTreeMap::new(),
        }
    }

    /// Register a new inhibit shortcuts dialog for a window.
    pub fn register(&mut self, dialog: alloc::boxed::Box<dyn InhibitShortcutsDialog>) {
        let window_id = dialog.window_id();
        self.dialogs.insert(window_id, dialog);
    }

    /// Unregister and remove an inhibit shortcuts dialog.
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

impl Default for InhibitShortcutsDialogManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_inhibit_shortcuts_dialog_creation() {
        let dialog = SimpleInhibitShortcutsDialog::new(WindowId(1));
        assert_eq!(dialog.window_id(), WindowId(1));
        assert!(!dialog.is_visible());
    }

    #[test]
    fn test_inhibit_shortcuts_dialog_show_hide() {
        let mut dialog = SimpleInhibitShortcutsDialog::new(WindowId(1));
        assert!(!dialog.is_visible());

        dialog.show();
        assert!(dialog.is_visible());

        dialog.hide();
        assert!(!dialog.is_visible());
    }

    #[test]
    fn test_inhibit_shortcuts_dialog_response() {
        let mut dialog = SimpleInhibitShortcutsDialog::new(WindowId(1));
        assert_eq!(dialog.last_response(), None);

        dialog.set_response(InhibitShortcutsDialogResponse::Deny);
        assert_eq!(
            dialog.last_response(),
            Some(InhibitShortcutsDialogResponse::Deny)
        );
    }

    #[test]
    fn test_inhibit_shortcuts_dialog_manager() {
        let mut manager = InhibitShortcutsDialogManager::new();
        manager.register(alloc::boxed::Box::new(SimpleInhibitShortcutsDialog::new(
            WindowId(1),
        )));
        manager.register(alloc::boxed::Box::new(SimpleInhibitShortcutsDialog::new(
            WindowId(2),
        )));

        manager.show(WindowId(1));
        assert!(manager.is_visible(WindowId(1)));
        assert!(!manager.is_visible(WindowId(2)));

        manager.show(WindowId(2));
        assert!(manager.is_visible(WindowId(1)));
        assert!(manager.is_visible(WindowId(2)));
    }
}
