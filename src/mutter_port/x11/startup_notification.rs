//! X11 startup notification support (EWMH).
//!
//! Ported from GNOME Mutter's src/x11/meta-startup-notification-x11.c/.h.
//! Handles startup notifications (_NET_STARTUP_ID, _NET_STARTUP_INFO).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/meta-startup-notification-x11.c

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Represents an application startup sequence.
#[derive(Debug, Clone)]
pub struct StartupSequence {
    pub startup_id: String,

    /// Application icon name.
    pub application_name: Option<String>,

    /// Icon name or path.
    pub icon_name: Option<String>,

    /// Binary name.
    pub binary_name: Option<String>,

    /// Desktop file path.
    pub desktop_name: Option<String>,

    /// Screen number.
    pub screen_number: i32,

    /// Workspace number (if known).
    pub workspace_number: Option<i32>,

    /// PID of the launcher process.
    pub launcher_pid: Option<u32>,

    /// Completion timestamp.
    pub completion_timestamp: Option<u32>,

    /// Whether this sequence is complete.
    pub complete: bool,
}

impl StartupSequence {
    /// Create a new startup sequence.
    /// # TODO: port logic from meta_startup_notification_x11_new_sequence()
    pub fn new(startup_id: String) -> Self {
        Self {
            startup_id,
            application_name: None,
            icon_name: None,
            binary_name: None,
            desktop_name: None,
            screen_number: 0,
            workspace_number: None,
            launcher_pid: None,
            completion_timestamp: None,
            complete: false,
        }
    }

    /// Mark this sequence as complete.
    pub fn set_complete(&mut self) {
        self.complete = true;
    }

    /// Set the application name.
    pub fn set_application_name(&mut self, name: String) {
        self.application_name = Some(name);
    }

    /// Set the icon name.
    pub fn set_icon_name(&mut self, name: String) {
        self.icon_name = Some(name);
    }

    /// Set the binary name.
    pub fn set_binary_name(&mut self, name: String) {
        self.binary_name = Some(name);
    }
}

/// Manages startup sequences for the display.
pub struct MetaX11StartupNotification {
    /// Active startup sequences by ID.
    pub sequences: BTreeMap<String, StartupSequence>,

    /// Timeout ID for sequence expiration (if using idle timeout).
    pub timeout_id: Option<u64>,
}

impl MetaX11StartupNotification {
    /// Create a new startup notification manager.
    /// # TODO: port logic from meta_startup_notification_x11_new()
    pub fn new() -> Self {
        Self {
            sequences: BTreeMap::new(),
            timeout_id: None,
        }
    }

    /// Create a new startup sequence.
    /// # TODO: port logic from _startup_sequence_new()
    pub fn create_sequence(&mut self, startup_id: String) -> &mut StartupSequence {
        self.sequences
            .entry(startup_id.clone())
            .or_insert_with(|| StartupSequence::new(startup_id))
    }

    /// Complete a startup sequence.
    /// # TODO: port logic from sequence completion handling
    pub fn complete_sequence(&mut self, startup_id: &str) {
        if let Some(seq) = self.sequences.get_mut(startup_id) {
            seq.set_complete();
        }
    }

    /// Get a startup sequence by ID.
    pub fn get_sequence(&self, startup_id: &str) -> Option<&StartupSequence> {
        self.sequences.get(startup_id)
    }

    /// Remove a completed startup sequence.
    pub fn remove_sequence(&mut self, startup_id: &str) -> bool {
        self.sequences.remove(startup_id).is_some()
    }

    /// Get all active sequences.
    pub fn get_sequences(&self) -> Vec<&StartupSequence> {
        self.sequences.values().collect()
    }

    /// Process _NET_STARTUP_INFO_BEGIN messages.
    /// # TODO: port logic from meta_startup_notification_x11_begin()
    pub fn handle_startup_info_begin(&mut self, _message: &str) {
        // TODO: parse startup info message
    }

    /// Process _NET_STARTUP_INFO messages.
    /// # TODO: port logic from meta_startup_notification_x11_message()
    pub fn handle_startup_info_message(&mut self, _message: &str) {
        // TODO: parse and update startup info
    }
}

impl Default for MetaX11StartupNotification {
    fn default() -> Self {
        Self::new()
    }
}
