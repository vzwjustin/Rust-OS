//! GFilenameCompleter matching `gio/gfilenamecompleter.h`.
//!
//! Upstream `GFilenameCompleter` provides filename completion for
//! tab-completion-like functionality. We port it as a struct with
//! a list of known filenames and a `dirs_only` flag.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A filename completer (`GFilenameCompleter`).
pub struct FilenameCompleter {
    entries: Mutex<Vec<String>>,
    dirs_only: Mutex<bool>,
}

impl FilenameCompleter {
    /// Creates a new filename completer.
    ///
    /// Mirrors `g_filename_completer_new`.
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(Vec::new()),
            dirs_only: Mutex::new(false),
        }
    }

    /// Adds a filename entry to the completer's known set.
    pub fn add_entry(&self, name: &str) {
        self.entries.lock().push(name.to_string());
    }

    /// Gets the completion suffix for the given initial text.
    ///
    /// Mirrors `g_filename_completer_get_completion_suffix`.
    /// Returns the common suffix that all matching entries share
    /// beyond the initial text, or `None` if no unique completion.
    pub fn get_completion_suffix(&self, initial_text: &str) -> Option<String> {
        let entries = self.entries.lock();
        let dirs_only = *self.dirs_only.lock();

        let matches: Vec<&String> = entries
            .iter()
            .filter(|e| e.starts_with(initial_text))
            .filter(|e| !dirs_only || e.ends_with('/'))
            .collect();

        if matches.is_empty() {
            return None;
        }

        // Find the common prefix among all matches
        let first = matches[0];
        let mut common_len = first.len();
        for m in &matches[1..] {
            common_len = common_len.min(
                first
                    .chars()
                    .zip(m.chars())
                    .take_while(|(a, b)| a == b)
                    .count(),
            );
        }

        if common_len <= initial_text.len() {
            return None;
        }

        Some(first[initial_text.len()..common_len].to_string())
    }

    /// Gets all completions for the given initial text.
    ///
    /// Mirrors `g_filename_completer_get_completions`.
    pub fn get_completions(&self, initial_text: &str) -> Vec<String> {
        let entries = self.entries.lock();
        let dirs_only = *self.dirs_only.lock();

        entries
            .iter()
            .filter(|e| e.starts_with(initial_text))
            .filter(|e| !dirs_only || e.ends_with('/'))
            .cloned()
            .collect()
    }

    /// Sets whether to complete directories only.
    ///
    /// Mirrors `g_filename_completer_set_dirs_only`.
    pub fn set_dirs_only(&self, dirs_only: bool) {
        *self.dirs_only.lock() = dirs_only;
    }
}

impl Default for FilenameCompleter {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let completer = FilenameCompleter::new();
        assert!(completer.get_completions("test").is_empty());
    }

    #[test]
    fn test_get_completions() {
        let completer = FilenameCompleter::new();
        completer.add_entry("apple.txt");
        completer.add_entry("apple.json");
        completer.add_entry("banana.txt");
        let matches = completer.get_completions("apple");
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&"apple.txt".to_string()));
        assert!(matches.contains(&"apple.json".to_string()));
    }

    #[test]
    fn test_get_completion_suffix_unique() {
        let completer = FilenameCompleter::new();
        completer.add_entry("apple.txt");
        completer.add_entry("apple.json");
        let suffix = completer.get_completion_suffix("apple");
        // Common prefix is "apple." so suffix is "."
        assert_eq!(suffix.unwrap(), ".");
    }

    #[test]
    fn test_get_completion_suffix_single_match() {
        let completer = FilenameCompleter::new();
        completer.add_entry("hello.txt");
        let suffix = completer.get_completion_suffix("hel");
        assert_eq!(suffix.unwrap(), "lo.txt");
    }

    #[test]
    fn test_get_completion_suffix_no_match() {
        let completer = FilenameCompleter::new();
        completer.add_entry("hello.txt");
        assert!(completer.get_completion_suffix("xyz").is_none());
    }

    #[test]
    fn test_get_completion_suffix_no_common() {
        let completer = FilenameCompleter::new();
        completer.add_entry("apple.txt");
        completer.add_entry("banana.txt");
        // Common prefix beyond "a" is empty for "a" since "banana" doesn't start with "a"
        // Actually "apple" starts with "a", "banana" doesn't, so only 1 match
        let suffix = completer.get_completion_suffix("a");
        assert_eq!(suffix.unwrap(), "pple.txt");
    }

    #[test]
    fn test_dirs_only() {
        let completer = FilenameCompleter::new();
        completer.add_entry("docs/");
        completer.add_entry("docs.txt");
        completer.add_entry("images/");
        completer.set_dirs_only(true);
        let matches = completer.get_completions("d");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], "docs/");
    }

    #[test]
    fn test_dirs_only_suffix() {
        let completer = FilenameCompleter::new();
        completer.add_entry("docs/");
        completer.add_entry("docs.txt");
        completer.set_dirs_only(true);
        let suffix = completer.get_completion_suffix("do");
        assert_eq!(suffix.unwrap(), "cs/");
    }
}
