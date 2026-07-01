//! App launch sequence tracker ported from GNOME Mutter startup-notification.c.
//!
//! Manages "app is launching" sequences with IDs, timestamps, and window associations
//! so the desktop shell can display launch feedback (spinner, busy cursor).
//!
//! Source: mutter-main/src/core/startup-notification.c (GNU GPL 2+)

use alloc::string::String;
use alloc::vec::Vec;
use core::cmp::Ordering;

/// Maximum time (ms) before an unmatched startup sequence is considered stale.
const STARTUP_TIMEOUT_MS: u64 = 15000;

/// A single app launch sequence.
#[derive(Debug, Clone)]
pub struct StartupSequence {
    /// Unique launch sequence identifier.
    pub id: String,
    /// Timestamp in milliseconds when sequence was created.
    pub timestamp: u64,
    /// Whether the app window has appeared.
    pub completed: bool,
    /// Application display name.
    pub name: String,
    /// Application ID (e.g., org.gnome.Shell).
    pub application_id: String,
    /// Icon name for launch feedback.
    pub icon_name: String,
    /// WM_CLASS hint from app.
    pub wmclass: String,
    /// Workspace ID (-1 = unset).
    pub workspace: i32,
}

impl StartupSequence {
    /// Create a new startup sequence.
    pub fn new(id: String, timestamp: u64) -> Self {
        Self {
            id,
            timestamp,
            completed: false,
            name: String::new(),
            application_id: String::new(),
            icon_name: String::new(),
            wmclass: String::new(),
            workspace: -1,
        }
    }

    /// Check if this sequence has exceeded the timeout.
    pub fn is_timed_out(&self, now_ms: u64) -> bool {
        if self.completed {
            return false;
        }
        now_ms.saturating_sub(self.timestamp) > STARTUP_TIMEOUT_MS
    }
}

/// Registry of in-flight app launch sequences.
pub struct StartupNotification {
    sequences: Vec<StartupSequence>,
}

impl StartupNotification {
    /// Create a new startup notification registry.
    pub fn new() -> Self {
        Self {
            sequences: Vec::new(),
        }
    }

    /// Add a new launch sequence.
    pub fn add_sequence(&mut self, seq: StartupSequence) {
        self.sequences.push(seq);
    }

    /// Remove a sequence by ID.
    pub fn remove_sequence(&mut self, id: &str) {
        self.sequences.retain(|seq| seq.id != id);
    }

    /// Look up a sequence by ID.
    pub fn lookup_sequence(&self, id: &str) -> Option<&StartupSequence> {
        self.sequences.iter().find(|seq| seq.id == id)
    }

    /// Look up a sequence by ID (mutable).
    pub fn lookup_sequence_mut(&mut self, id: &str) -> Option<&mut StartupSequence> {
        self.sequences.iter_mut().find(|seq| seq.id == id)
    }

    /// Check if there are any incomplete sequences.
    pub fn has_pending_sequences(&self) -> bool {
        self.sequences.iter().any(|seq| !seq.completed)
    }

    /// Sweep out timed-out sequences and return them.
    /// Sequences that have exceeded STARTUP_TIMEOUT_MS are marked completed but not removed.
    pub fn sweep_timed_out(&mut self, now_ms: u64) -> Vec<String> {
        let mut timed_out = Vec::new();
        for seq in self.sequences.iter_mut() {
            if seq.is_timed_out(now_ms) {
                seq.completed = true;
                timed_out.push(seq.id.clone());
            }
        }
        timed_out
    }

    /// Get all sequences (completed or not).
    pub fn sequences(&self) -> &[StartupSequence] {
        &self.sequences
    }

    /// Get all pending (incomplete) sequences.
    pub fn pending_sequences(&self) -> Vec<&StartupSequence> {
        self.sequences.iter().filter(|seq| !seq.completed).collect()
    }

    /// Clear all sequences.
    pub fn clear(&mut self) {
        self.sequences.clear();
    }

    /// Get the number of all sequences.
    pub fn len(&self) -> usize {
        self.sequences.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.sequences.is_empty()
    }
}

impl Default for StartupNotification {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_add_sequence() {
        let mut notif = StartupNotification::new();
        let seq = StartupSequence::new("app.launch.1".into(), 1000);
        notif.add_sequence(seq);
        assert_eq!(notif.len(), 1);
    }

    #[test]
    fn test_lookup_sequence() {
        let mut notif = StartupNotification::new();
        let seq = StartupSequence::new("app.launch.1".into(), 1000);
        notif.add_sequence(seq);

        let found = notif.lookup_sequence("app.launch.1");
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "app.launch.1");
        assert!(!found.unwrap().completed);
    }

    #[test]
    fn test_remove_sequence() {
        let mut notif = StartupNotification::new();
        notif.add_sequence(StartupSequence::new("app.launch.1".into(), 1000));
        notif.add_sequence(StartupSequence::new("app.launch.2".into(), 1000));
        assert_eq!(notif.len(), 2);

        notif.remove_sequence("app.launch.1");
        assert_eq!(notif.len(), 1);
        assert!(notif.lookup_sequence("app.launch.1").is_none());
    }

    #[test]
    fn test_has_pending_sequences() {
        let mut notif = StartupNotification::new();
        assert!(!notif.has_pending_sequences());

        let seq = StartupSequence::new("app.launch.1".into(), 1000);
        notif.add_sequence(seq);
        assert!(notif.has_pending_sequences());

        notif.sequences[0].completed = true;
        assert!(!notif.has_pending_sequences());
    }

    #[test]
    fn test_sweep_timed_out() {
        let mut notif = StartupNotification::new();
        notif.add_sequence(StartupSequence::new("app.launch.1".into(), 1000));
        notif.add_sequence(StartupSequence::new("app.launch.2".into(), 20000));

        let now = 17000; // 1000 + 16s (beyond timeout), 20000 + -3s (still pending)
        let timed_out = notif.sweep_timed_out(now);

        assert_eq!(timed_out.len(), 1);
        assert_eq!(timed_out[0], "app.launch.1");
        assert!(notif.lookup_sequence("app.launch.1").unwrap().completed);
        assert!(!notif.lookup_sequence("app.launch.2").unwrap().completed);
    }

    #[test]
    fn test_pending_sequences() {
        let mut notif = StartupNotification::new();
        let mut seq1 = StartupSequence::new("app.launch.1".into(), 1000);
        let mut seq2 = StartupSequence::new("app.launch.2".into(), 1000);
        seq1.completed = false;
        seq2.completed = true;
        notif.add_sequence(seq1);
        notif.add_sequence(seq2);

        let pending = notif.pending_sequences();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "app.launch.1");
    }

    #[test]
    fn test_sequence_properties() {
        let mut seq = StartupSequence::new("app.launch.1".into(), 5000);
        seq.name = "My App".into();
        seq.application_id = "org.example.MyApp".into();
        seq.icon_name = "application-x-executable".into();
        seq.wmclass = "MyApp".into();
        seq.workspace = 0;

        assert_eq!(seq.name, "My App");
        assert_eq!(seq.application_id, "org.example.MyApp");
        assert_eq!(seq.workspace, 0);
    }
}
