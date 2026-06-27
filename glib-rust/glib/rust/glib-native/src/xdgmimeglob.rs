//! `xdgmimeglob` matching `gio/xdgmime/xdgmimeglob.h`.
//!
//! XDG MIME glob hash: filename-based MIME type matching using glob patterns.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

/// Glob type (mirrors `XdgGlobType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XdgGlobType {
    Literal,
    Simple,
    Full,
}

/// Glob entry.
#[derive(Debug, Clone)]
struct GlobEntry {
    glob: String,
    mime_type: String,
    weight: i32,
    case_sensitive: bool,
    glob_type: XdgGlobType,
}

/// XDG glob hash (mirrors `XdgGlobHash`).
#[derive(Debug, Default)]
pub struct XdgGlobHash {
    entries: Vec<GlobEntry>,
}

impl XdgGlobHash {
    /// Creates a new empty glob hash (mirrors `_xdg_glob_hash_new`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a glob pattern (mirrors `_xdg_glob_hash_append_glob`).
    pub fn append_glob(&mut self, glob: &str, mime_type: &str, weight: i32, case_sensitive: bool) {
        let glob_type = determine_type(glob);
        self.entries.push(GlobEntry {
            glob: glob.to_string(),
            mime_type: mime_type.to_string(),
            weight,
            case_sensitive,
            glob_type,
        });
    }

    /// Looks up MIME types for a file name (mirrors `_xdg_glob_hash_lookup_file_name`).
    pub fn lookup_file_name(&self, file_name: &str) -> Vec<String> {
        let mut results = Vec::new();
        let mut best_weight = -1;
        let mut best_match: Option<&GlobEntry> = None;
        for entry in &self.entries {
            if self.matches(file_name, entry) {
                if entry.weight > best_weight {
                    best_weight = entry.weight;
                    best_match = Some(entry);
                }
                if !results.contains(&entry.mime_type) {
                    results.push(entry.mime_type.clone());
                }
            }
        }
        let _ = best_match;
        results
    }

    /// Determines the glob type for a pattern (mirrors `_xdg_glob_determine_type`).
    pub fn determine_type(glob: &str) -> XdgGlobType {
        determine_type(glob)
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns true if the hash is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    fn matches(&self, file_name: &str, entry: &GlobEntry) -> bool {
        match entry.glob_type {
            XdgGlobType::Literal => file_name == entry.glob,
            XdgGlobType::Simple => {
                if entry.glob.starts_with("*.") {
                    file_name.ends_with(&entry.glob[1..])
                } else {
                    false
                }
            }
            XdgGlobType::Full => full_glob_match(file_name, &entry.glob),
        }
    }
}

/// Determines the glob type for a pattern (mirrors `_xdg_glob_determine_type`).
pub fn determine_type(glob: &str) -> XdgGlobType {
    if !glob.contains('*') {
        XdgGlobType::Literal
    } else if glob.starts_with("*.")
        && !glob[2..].contains('*')
        && !glob[2..].contains('?')
        && !glob[2..].contains('[')
    {
        XdgGlobType::Simple
    } else {
        XdgGlobType::Full
    }
}

/// Full glob matching with `*`, `?`, and bracket character classes.
fn full_glob_match(text: &str, pattern: &str) -> bool {
    full_glob_match_at(text.as_bytes(), pattern.as_bytes(), 0, 0)
}

fn full_glob_match_at(text: &[u8], pattern: &[u8], ti: usize, pi: usize) -> bool {
    if pi == pattern.len() {
        return ti == text.len();
    }

    match pattern[pi] {
        b'*' => {
            let mut next_ti = ti;
            while next_ti <= text.len() {
                if full_glob_match_at(text, pattern, next_ti, pi + 1) {
                    return true;
                }
                next_ti += 1;
            }
            false
        }
        b'?' => ti < text.len() && full_glob_match_at(text, pattern, ti + 1, pi + 1),
        b'[' => {
            if ti >= text.len() {
                return false;
            }
            if let Some(end) = pattern[pi + 1..].iter().position(|&b| b == b']') {
                let class_start = pi + 1;
                let class_end = class_start + end;
                pattern[class_start..class_end].contains(&text[ti])
                    && full_glob_match_at(text, pattern, ti + 1, class_end + 1)
            } else {
                text[ti] == b'[' && full_glob_match_at(text, pattern, ti + 1, pi + 1)
            }
        }
        byte => {
            ti < text.len() && text[ti] == byte && full_glob_match_at(text, pattern, ti + 1, pi + 1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_determine_type() {
        assert_eq!(determine_type("Makefile"), XdgGlobType::Literal);
        assert_eq!(determine_type("*.txt"), XdgGlobType::Simple);
        assert_eq!(determine_type("x*.[ch]"), XdgGlobType::Full);
        assert_eq!(determine_type("*.tar.gz"), XdgGlobType::Simple);
    }

    #[test]
    fn test_new() {
        let hash = XdgGlobHash::new();
        assert!(hash.is_empty());
    }

    #[test]
    fn test_append_and_lookup() {
        let mut hash = XdgGlobHash::new();
        hash.append_glob("*.txt", "text/plain", 50, false);
        hash.append_glob("Makefile", "text/x-makefile", 50, false);
        let results = hash.lookup_file_name("readme.txt");
        assert_eq!(results, vec!["text/plain".to_string()]);
        let results = hash.lookup_file_name("Makefile");
        assert_eq!(results, vec!["text/x-makefile".to_string()]);
    }

    #[test]
    fn test_lookup_no_match() {
        let hash = XdgGlobHash::new();
        let results = hash.lookup_file_name("file.xyz");
        assert!(results.is_empty());
    }

    #[test]
    fn test_full_glob() {
        let mut hash = XdgGlobHash::new();
        hash.append_glob("x*.[ch]", "text/x-csrc", 50, false);
        let results = hash.lookup_file_name("xfoo.c");
        assert_eq!(results, vec!["text/x-csrc".to_string()]);
    }

    #[test]
    fn test_multiple_matches() {
        let mut hash = XdgGlobHash::new();
        hash.append_glob("*.html", "text/html", 50, false);
        hash.append_glob("*.htm", "text/html", 50, false);
        let results = hash.lookup_file_name("page.html");
        assert_eq!(results, vec!["text/html".to_string()]);
    }
}
