//! `xdgmimecache` matching `gio/xdgmime/xdgmimecache.h`.
//!
//! XDG MIME cache: mmapped cache for MIME type resolution.
//! In our no_std port, we use in-memory data structures.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

/// XDG MIME cache (mirrors `XdgMimeCache`).
#[derive(Debug, Default)]
pub struct XdgMimeCache {
    mime_types: Vec<(String, Vec<String>)>,
    aliases: Vec<(String, String)>,
    globs: Vec<(String, String, i32)>,
    icons: Vec<(String, String)>,
    generic_icons: Vec<(String, String)>,
    max_buffer_extents: usize,
}

impl XdgMimeCache {
    /// Creates a new cache from file content
    /// (mirrors `_xdg_mime_cache_new_from_file`).
    /// In our port, we create an empty cache.
    pub fn new() -> Self {
        Self {
            max_buffer_extents: 4096,
            ..Default::default()
        }
    }

    /// Returns the max buffer extents (mirrors `_xdg_mime_cache_get_max_buffer_extents`).
    pub fn get_max_buffer_extents(&self) -> usize {
        self.max_buffer_extents
    }

    /// Gets MIME type for data (mirrors `_xdg_mime_cache_get_mime_type_for_data`).
    pub fn get_mime_type_for_data(&self, data: &[u8]) -> String {
        if data.is_empty() {
            return "inode/x-empty".to_string();
        }
        if data.iter().take(1024).all(|&b| b != 0) {
            return "text/plain".to_string();
        }
        "application/octet-stream".to_string()
    }

    /// Gets MIME type for file (mirrors `_xdg_mime_cache_get_mime_type_for_file`).
    pub fn get_mime_type_for_file(&self, file_name: &str) -> String {
        self.get_mime_type_from_file_name(file_name)
    }

    /// Gets MIME type from file name (mirrors `_xdg_mime_cache_get_mime_type_from_file_name`).
    pub fn get_mime_type_from_file_name(&self, file_name: &str) -> String {
        for (pattern, mime, _) in &self.globs {
            if pattern.starts_with("*.") {
                if file_name.ends_with(&pattern[1..]) {
                    return mime.clone();
                }
            } else if file_name == pattern {
                return mime.clone();
            }
        }
        "application/octet-stream".to_string()
    }

    /// Gets multiple MIME types from file name
    /// (mirrors `_xdg_mime_cache_get_mime_types_from_file_name`).
    pub fn get_mime_types_from_file_name(&self, file_name: &str) -> Vec<String> {
        let mut results = Vec::new();
        for (pattern, mime, _) in &self.globs {
            let matches = if pattern.starts_with("*.") {
                file_name.ends_with(&pattern[1..])
            } else {
                file_name == pattern
            };
            if matches && !results.contains(mime) {
                results.push(mime.clone());
            }
        }
        results
    }

    /// Unaliases a MIME type (mirrors `_xdg_mime_cache_unalias_mime_type`).
    pub fn unalias_mime_type(&self, mime: &str) -> String {
        self.aliases
            .iter()
            .find(|(a, _)| a == mime)
            .map(|(_, c)| c.clone())
            .unwrap_or_else(|| mime.to_string())
    }

    /// Lists parents (mirrors `_xdg_mime_cache_list_mime_parents`).
    pub fn list_mime_parents(&self, mime: &str) -> Vec<String> {
        let unaliased = self.unalias_mime_type(mime);
        self.mime_types
            .iter()
            .find(|(m, _)| *m == unaliased)
            .map(|(_, p)| p.clone())
            .unwrap_or_default()
    }

    /// Checks subclass relationship (mirrors `_xdg_mime_cache_mime_type_subclass`).
    pub fn mime_type_subclass(&self, mime_a: &str, mime_b: &str) -> bool {
        if self.unalias_mime_type(mime_a) == self.unalias_mime_type(mime_b) {
            return true;
        }
        let parents = self.list_mime_parents(mime_a);
        parents.iter().any(|p| self.mime_type_subclass(p, mime_b))
    }

    /// Gets icon for MIME type (mirrors `_xdg_mime_cache_get_icon`).
    pub fn get_icon(&self, mime: &str) -> String {
        self.icons
            .iter()
            .find(|(m, _)| *m == mime)
            .map(|(_, i)| i.clone())
            .unwrap_or_default()
    }

    /// Gets generic icon for MIME type (mirrors `_xdg_mime_cache_get_generic_icon`).
    pub fn get_generic_icon(&self, mime: &str) -> String {
        self.generic_icons
            .iter()
            .find(|(m, _)| *m == mime)
            .map(|(_, i)| i.clone())
            .unwrap_or_default()
    }

    /// Adds a glob entry to the cache.
    pub fn add_glob(&mut self, pattern: &str, mime_type: &str, weight: i32) {
        self.globs
            .push((pattern.to_string(), mime_type.to_string(), weight));
    }

    /// Adds an alias to the cache.
    pub fn add_alias(&mut self, alias: &str, canonical: &str) {
        self.aliases
            .push((alias.to_string(), canonical.to_string()));
    }

    /// Adds a parent relationship to the cache.
    pub fn add_parent(&mut self, mime: &str, parent: &str) {
        if let Some((_, parents)) = self.mime_types.iter_mut().find(|(m, _)| m == mime) {
            if !parents.contains(&parent.to_string()) {
                parents.push(parent.to_string());
            }
        } else {
            self.mime_types
                .push((mime.to_string(), vec![parent.to_string()]));
        }
    }

    /// Adds an icon mapping to the cache.
    pub fn add_icon(&mut self, mime: &str, icon: &str) {
        self.icons.push((mime.to_string(), icon.to_string()));
    }

    /// Adds a generic icon mapping to the cache.
    pub fn add_generic_icon(&mut self, mime: &str, icon: &str) {
        self.generic_icons
            .push((mime.to_string(), icon.to_string()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let cache = XdgMimeCache::new();
        assert_eq!(cache.get_max_buffer_extents(), 4096);
    }

    #[test]
    fn test_get_mime_type_for_data() {
        let cache = XdgMimeCache::new();
        assert_eq!(cache.get_mime_type_for_data(b""), "inode/x-empty");
        assert_eq!(cache.get_mime_type_for_data(b"hello"), "text/plain");
    }

    #[test]
    fn test_get_mime_type_from_file_name() {
        let mut cache = XdgMimeCache::new();
        cache.add_glob("*.txt", "text/plain", 50);
        assert_eq!(cache.get_mime_type_from_file_name("file.txt"), "text/plain");
        assert_eq!(
            cache.get_mime_type_from_file_name("file.xyz"),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_unalias() {
        let mut cache = XdgMimeCache::new();
        cache.add_alias("application/x-text", "text/plain");
        assert_eq!(cache.unalias_mime_type("application/x-text"), "text/plain");
    }

    #[test]
    fn test_list_parents() {
        let mut cache = XdgMimeCache::new();
        cache.add_parent("text/html", "text/plain");
        let parents = cache.list_mime_parents("text/html");
        assert_eq!(parents, vec!["text/plain".to_string()]);
    }

    #[test]
    fn test_subclass() {
        let mut cache = XdgMimeCache::new();
        cache.add_parent("text/html", "text/plain");
        assert!(cache.mime_type_subclass("text/html", "text/plain"));
        assert!(cache.mime_type_subclass("text/plain", "text/plain"));
    }

    #[test]
    fn test_icons() {
        let mut cache = XdgMimeCache::new();
        cache.add_icon("text/plain", "text-x-generic");
        cache.add_generic_icon("text/plain", "text");
        assert_eq!(cache.get_icon("text/plain"), "text-x-generic");
        assert_eq!(cache.get_generic_icon("text/plain"), "text");
    }

    #[test]
    fn test_get_mime_types_from_file_name() {
        let mut cache = XdgMimeCache::new();
        cache.add_glob("*.html", "text/html", 50);
        cache.add_glob("*.htm", "text/html", 50);
        let types = cache.get_mime_types_from_file_name("page.html");
        assert_eq!(types, vec!["text/html".to_string()]);
    }
}
