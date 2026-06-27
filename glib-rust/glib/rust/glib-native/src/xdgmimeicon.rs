//! `xdgmimeicon` matching `gio/xdgmime/xdgmimeicon.h`.
//!
//! XDG MIME icon list: maps MIME types to icon names.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

/// XDG icon entry.
#[derive(Debug, Clone)]
struct IconEntry {
    mime: String,
    icon: String,
}

/// XDG icon list (mirrors `XdgIconList`).
#[derive(Debug, Default)]
pub struct XdgIconList {
    entries: Vec<IconEntry>,
}

impl XdgIconList {
    /// Creates a new empty icon list (mirrors `_xdg_mime_icon_list_new`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Looks up the icon for a MIME type
    /// (mirrors `_xdg_mime_icon_list_lookup`).
    pub fn lookup(&self, mime: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|e| e.mime == mime)
            .map(|e| e.icon.as_str())
    }

    /// Adds an icon mapping.
    pub fn add(&mut self, mime: &str, icon: &str) {
        if let Some(entry) = self.entries.iter_mut().find(|e| e.mime == mime) {
            entry.icon = icon.to_string();
        } else {
            self.entries.push(IconEntry {
                mime: mime.to_string(),
                icon: icon.to_string(),
            });
        }
    }

    /// Reads icon mappings from file content
    /// (mirrors `_xdg_mime_icon_read_from_file`).
    /// Each line: `mime_type:icon_name`
    pub fn read_from_file_content(&mut self, content: &str) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(colon) = line.find(':') {
                let mime = line[..colon].trim();
                let icon = line[colon + 1..].trim();
                if !mime.is_empty() && !icon.is_empty() {
                    self.add(mime, icon);
                }
            }
        }
    }

    /// Returns the number of entries.
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
        let list = XdgIconList::new();
        assert!(list.is_empty());
    }

    #[test]
    fn test_add_and_lookup() {
        let mut list = XdgIconList::new();
        list.add("text/plain", "text-x-generic");
        assert_eq!(list.lookup("text/plain"), Some("text-x-generic"));
        assert_eq!(list.lookup("nonexistent"), None);
    }

    #[test]
    fn test_add_overwrites() {
        let mut list = XdgIconList::new();
        list.add("text/plain", "icon1");
        list.add("text/plain", "icon2");
        assert_eq!(list.lookup("text/plain"), Some("icon2"));
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn test_read_from_file_content() {
        let mut list = XdgIconList::new();
        list.read_from_file_content(
            "text/plain:text-x-generic\napplication/json:application-json\n# comment\n",
        );
        assert_eq!(list.lookup("text/plain"), Some("text-x-generic"));
        assert_eq!(list.lookup("application/json"), Some("application-json"));
    }

    #[test]
    fn test_read_from_file_content_ignores_empty() {
        let mut list = XdgIconList::new();
        list.read_from_file_content("\n\n# only comments\n");
        assert!(list.is_empty());
    }
}
