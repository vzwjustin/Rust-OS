//! X11 startup notification support (EWMH).
//!
//! Ported from GNOME Mutter's src/x11/meta-startup-notification-x11.c/.h.
//! Handles startup notifications (_NET_STARTUP_ID, _NET_STARTUP_INFO).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/meta-startup-notification-x11.c

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
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

    /// Apply a single key=value attribute parsed from a startup info message.
    fn apply_attribute(&mut self, key: &str, value: &str) {
        match key {
            "NAME" => self.application_name = Some(value.to_string()),
            "ICON" => self.icon_name = Some(value.to_string()),
            "BIN" => self.binary_name = Some(value.to_string()),
            "DESKTOP" => self.desktop_name = Some(value.to_string()),
            "SCREEN" => {
                if let Ok(n) = value.parse::<i32>() {
                    self.screen_number = n;
                }
            }
            "WORKSPACE" => {
                if let Ok(n) = value.parse::<i32>() {
                    self.workspace_number = Some(n);
                }
            }
            "PID" => {
                if let Ok(n) = value.parse::<u32>() {
                    self.launcher_pid = Some(n);
                }
            }
            _ => {}
        }
    }
}

/// Parses a startup-notification message into its command name and attributes.
///
/// Messages use the form `command:KEY=VALUE KEY=VALUE ...` (the
/// _NET_STARTUP_INFO encoding). The command is the leading token before the
/// first colon; the remainder is a space-separated list of KEY=VALUE pairs.
/// Values may be wrapped in single quotes to contain spaces. Returns the
/// command and a list of (key, value) pairs.
fn parse_startup_message(message: &str) -> (Option<String>, Vec<(String, String)>) {
    let mut command = None;
    let mut attrs = Vec::new();

    let body = match message.find(':') {
        Some(idx) => {
            command = Some(message[..idx].to_string());
            &message[idx + 1..]
        }
        None => message,
    };
    let mut chars = body.chars().peekable();
    let mut current = String::new();
    let mut tokens: Vec<String> = Vec::new();
    let mut in_quotes = false;

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '\'' {
                in_quotes = false;
            } else {
                current.push(c);
            }
        } else if c == '\'' {
            in_quotes = true;
        } else if c == ' ' || c == '\t' || c == '\n' {
            if !current.is_empty() {
                tokens.push(core::mem::take(&mut current));
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }

    for token in tokens {
        if let Some(eq) = token.find('=') {
            let key = token[..eq].to_string();
            let value = token[eq + 1..].to_string();
            attrs.push((key, value));
        }
    }

    (command, attrs)
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
    pub fn new() -> Self {
        Self {
            sequences: BTreeMap::new(),
            timeout_id: None,
        }
    }

    /// Create a new startup sequence.
    pub fn create_sequence(&mut self, startup_id: String) -> &mut StartupSequence {
        self.sequences
            .entry(startup_id.clone())
            .or_insert_with(|| StartupSequence::new(startup_id))
    }

    /// Complete a startup sequence.
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
    ///
    /// These begin a new startup sequence. The message body carries the
    /// startup id (ID=...) plus optional attributes. We parse the message and
    /// create or update the corresponding `StartupSequence`.
    pub fn handle_startup_info_begin(&mut self, message: &str) {
        let (command, attrs) = parse_startup_message(message);

        // Extract the startup id from the ID= attribute.
        let mut startup_id: Option<String> = None;
        for (key, value) in &attrs {
            if key == "ID" {
                startup_id = Some(value.clone());
                break;
            }
        }

        let id = match startup_id {
            Some(id) => id,
            None => return,
        };

        let seq = self.create_sequence(id);
        for (key, value) in attrs {
            if key != "ID" {
                seq.apply_attribute(&key, &value);
            }
        }
        let _ = command;
    }

    /// Process _NET_STARTUP_INFO messages.
    ///
    /// These update or complete an existing sequence. The command token
    /// determines the action: `new` creates/updates, `change` updates
    /// attributes, and `remove` marks the sequence complete. The target
    /// sequence is identified by its ID= attribute.
    pub fn handle_startup_info_message(&mut self, message: &str) {
        let (command, attrs) = parse_startup_message(message);

        let mut startup_id: Option<String> = None;
        for (key, value) in &attrs {
            if key == "ID" {
                startup_id = Some(value.clone());
                break;
            }
        }

        let id = match startup_id {
            Some(id) => id,
            None => return,
        };

        match command.as_deref() {
            Some("remove") => {
                if let Some(seq) = self.sequences.get_mut(&id) {
                    seq.set_complete();
                }
            }
            Some("change") | Some("new") | None => {
                let seq = self.create_sequence(id);
                for (key, value) in attrs {
                    if key != "ID" {
                        seq.apply_attribute(&key, &value);
                    }
                }
            }
            Some(_) => {
                // Unknown command: ignore but leave sequence intact.
            }
        }
    }
}

impl Default for MetaX11StartupNotification {
    fn default() -> Self {
        Self::new()
    }
}
