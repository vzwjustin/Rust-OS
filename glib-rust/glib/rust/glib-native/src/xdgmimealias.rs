//! `xdgmimealias` matching `gio/xdgmime/xdgmimealias.h`.
//!
//! XDG MIME alias list: maps alias MIME types to canonical names.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

/// XDG alias entry.
#[derive(Debug, Clone)]
struct AliasEntry {
    alias: String,
    canonical: String,
}

/// XDG alias list (mirrors `XdgAliasList`).
#[derive(Debug, Default)]
pub struct XdgAliasList {
    entries: Vec<AliasEntry>,
}

impl XdgAliasList {
    /// Creates a new empty alias list (mirrors `_xdg_mime_alias_list_new`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Looks up the canonical MIME type for an alias
    /// (mirrors `_xdg_mime_alias_list_lookup`).
    pub fn lookup(&self, alias: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.alias == alias)
            .map(|e| e.canonical.as_str())
    }

    /// Adds an alias mapping.
    pub fn add(&mut self, alias: &str, canonical: &str) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.alias == alias) {
            entry.canonical = canonical.to_string();
        } else {
            self.entries.push(AliasEntry {
                alias: alias.to_string(),
                canonical: canonical.to_string(),
            });
        }
    }

    /// Reads alias mappings from a text file content
    /// (mirrors `_xdg_mime_alias_read_from_file`).
    /// Each line: `alias=canonical`
    pub fn read_from_file_content(&mut self, content: &str) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(eq) = line.find('=') {
                let alias = line[..eq].trim();
                let canonical = line[eq + 1..].trim();
                if !alias.is_empty() && !canonical.is_empty() {
                    self.add(alias, canonical);
                }
            }
        }
    }

    /// Returns the number of aliases.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the list is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let list = XdgAliasList::new();
        assert!(list.is_empty());
    }

    #[test]
    fn test_add_and_lookup() {
        let mut list = XdgAliasList::new();
        list.add("application/x-text", "text/plain");
        assert_eq!(list.lookup("application/x-text"), Some("text/plain"));
        assert_eq!(list.lookup("nonexistent"), None);
    }

    #[test]
    fn test_add_overwrites() {
        let mut list = XdgAliasList::new();
        list.add("alias1", "type1");
        list.add("alias1", "type2");
        assert_eq!(list.lookup("alias1"), Some("type2"));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_read_from_file_content() {
        let mut list = XdgAliasList::new();
        list.read_from_file_content("application/x-text=text/plain\napplication/x-shellscript=text/x-shellscript\n# comment\n");
        assert_eq!(list.lookup("application/x-text"), Some("text/plain"));
        assert_eq!(
            list.lookup("application/x-shellscript"),
            Some("text/x-shellscript")
        );
    }

    #[test]
    fn test_read_from_file_content_ignores_empty() {
        let mut list = XdgAliasList::new();
        list.read_from_file_content("\n\n# only comments\n");
        assert!(list.is_empty());
    }
}
