//! `xdgmimeparent` matching `gio/xdgmime/xdgmimeparent.h`.
//!
//! XDG MIME parent list: stores MIME type hierarchy (subclass relationships).
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// XDG parent entry.
#[derive(Debug, Clone)]
struct ParentEntry {
    mime: String,
    parents: Vec<String>,
}

/// XDG parent list (mirrors `XdgParentList`).
#[derive(Debug, Default)]
pub struct XdgParentList {
    entries: Vec<ParentEntry>,
}

impl XdgParentList {
    /// Creates a new empty parent list (mirrors `_xdg_mime_parent_list_new`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Looks up the parents for a MIME type
    /// (mirrors `_xdg_mime_parent_list_lookup`).
    pub fn lookup(&self, mime: &str) -> Vec<String> {
        self.entries
            .iter()
            .find(|e| e.mime == mime)
            .map(|e| e.parents.clone())
            .unwrap_or_default()
    }

    /// Adds a parent relationship.
    pub fn add(&mut self, mime: &str, parent: &str) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.mime == mime) {
            if !entry.parents.contains(&parent.to_string()) {
                entry.parents.push(parent.to_string());
            }
        } else {
            self.entries.push(ParentEntry {
                mime: mime.to_string(),
                parents: vec![parent.to_string()],
            });
        }
    }

    /// Reads parent mappings from file content
    /// (mirrors `_xdg_mime_parent_read_from_file`).
    /// Each line: `mime_type=parent1 parent2 ...`
    pub fn read_from_file_content(&mut self, content: &str) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(eq) = line.find('=') {
                let mime = line[..eq].trim();
                let parents_str = line[eq + 1..].trim();
                for parent in parents_str.split_whitespace() {
                    self.add(mime, parent);
                }
            }
        }
    }

    /// Returns the number of MIME types with parents.
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
        let list = XdgParentList::new();
        assert!(list.is_empty());
    }

    #[test]
    fn test_add_and_lookup() {
        let mut list = XdgParentList::new();
        list.add("text/html", "text/plain");
        assert_eq!(list.lookup("text/html"), vec!["text/plain".to_string()]);
        assert_eq!(list.lookup("nonexistent"), Vec::<String>::new());
    }

    #[test]
    fn test_add_multiple_parents() {
        let mut list = XdgParentList::new();
        list.add("text/html", "text/plain");
        list.add("text/html", "application/xml");
        let parents = list.lookup("text/html");
        assert_eq!(parents.len(), 2);
        assert!(parents.contains(&"text/plain".to_string()));
        assert!(parents.contains(&"application/xml".to_string()));
    }

    #[test]
    fn test_add_duplicate_parent() {
        let mut list = XdgParentList::new();
        list.add("text/html", "text/plain");
        list.add("text/html", "text/plain");
        assert_eq!(list.lookup("text/html").len(), 1);
    }

    #[test]
    fn test_read_from_file_content() {
        let mut list = XdgParentList::new();
        list.read_from_file_content(
            "text/html=text/plain application/xml\napplication/json=text/plain\n# comment\n",
        );
        assert_eq!(list.lookup("text/html").len(), 2);
        assert_eq!(
            list.lookup("application/json"),
            vec!["text/plain".to_string()]
        );
    }

    #[test]
    fn test_read_from_file_content_ignores_empty() {
        let mut list = XdgParentList::new();
        list.read_from_file_content("\n\n# only comments\n");
        assert!(list.is_empty());
    }
}
