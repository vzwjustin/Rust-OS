//! `gcontenttypeprivate` matching `gio/gcontenttypeprivate.h`.
//!
//! Private content type API: sniff length, unalias, parent lookup,
//! and `_impl` wrappers for the public content type functions.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Sniff length for content type guessing (mirrors `_g_unix_content_type_get_sniff_len`).
pub fn unix_content_type_get_sniff_len() -> usize {
    4096
}

/// Unaliases a content type (e.g. `text/x-csrc` → `text/x-csrc`).
///
/// Mirrors `_g_unix_content_type_unalias`.
pub fn unix_content_type_unalias(content_type: &str) -> String {
    let aliases = ALIASES.lock();
    for (alias, canonical) in aliases.iter() {
        if alias == content_type {
            return canonical.clone();
        }
    }
    content_type.to_string()
}

/// Returns parent content types for a given type.
///
/// Mirrors `_g_unix_content_type_get_parents`.
pub fn unix_content_type_get_parents(content_type: &str) -> Vec<String> {
    let unaliased = unix_content_type_unalias(content_type);

    // Standard MIME hierarchy: text/plain is parent of all text/*
    if let Some(subtype) = unaliased.strip_prefix("text/") {
        if subtype != "plain" {
            return vec!["text/plain".to_string()];
        }
    }

    // application/octet-stream is the fallback parent
    if unaliased != "application/octet-stream" {
        return vec!["application/octet-stream".to_string()];
    }

    Vec::new()
}

/// MIME directories for content type lookups.
static MIME_DIRS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Sets MIME directories (mirrors `g_content_type_set_mime_dirs_impl`).
pub fn set_mime_dirs(dirs: &[&str]) {
    let mut mime_dirs = MIME_DIRS.lock();
    mime_dirs.clear();
    for d in dirs {
        mime_dirs.push(d.to_string());
    }
}

/// Gets MIME directories (mirrors `g_content_type_get_mime_dirs_impl`).
pub fn get_mime_dirs() -> Vec<String> {
    MIME_DIRS.lock().clone()
}

/// Checks if two content types are equal (mirrors `g_content_type_equals_impl`).
pub fn equals_impl(type1: &str, type2: &str) -> bool {
    unix_content_type_unalias(type1) == unix_content_type_unalias(type2)
}

/// Checks if a content type is a subtype of another (mirrors `g_content_type_is_a_impl`).
pub fn is_a_impl(content_type: &str, supertype: &str) -> bool {
    if equals_impl(content_type, supertype) {
        return true;
    }
    let parents = unix_content_type_get_parents(content_type);
    parents.iter().any(|p| equals_impl(p, supertype))
}

/// Checks if a content type is a MIME type (mirrors `g_content_type_is_mime_type_impl`).
pub fn is_mime_type_impl(content_type: &str, mime_type: &str) -> bool {
    is_a_impl(content_type, mime_type)
}

/// Checks if a content type is unknown (mirrors `g_content_type_is_unknown_impl`).
pub fn is_unknown_impl(content_type: &str) -> bool {
    content_type == "application/octet-stream" || content_type.is_empty()
}

/// Gets a description for a content type (mirrors `g_content_type_get_description_impl`).
pub fn get_description_impl(content_type: &str) -> String {
    let unaliased = unix_content_type_unalias(content_type);
    if unaliased == "application/octet-stream" {
        "Unknown type".to_string()
    } else if unaliased.starts_with("text/") {
        "Text document".to_string()
    } else if unaliased.starts_with("image/") {
        "Image".to_string()
    } else if unaliased.starts_with("audio/") {
        "Audio".to_string()
    } else if unaliased.starts_with("video/") {
        "Video".to_string()
    } else if unaliased.starts_with("application/") {
        "Application data".to_string()
    } else {
        unaliased
    }
}

/// Gets the MIME type for a content type (mirrors `g_content_type_get_mime_type_impl`).
pub fn get_mime_type_impl(content_type: &str) -> String {
    unix_content_type_unalias(content_type)
}

/// Checks if a content type can be executable (mirrors `g_content_type_can_be_executable_impl`).
pub fn can_be_executable_impl(content_type: &str) -> bool {
    let unaliased = unix_content_type_unalias(content_type);
    unaliased == "application/x-executable"
        || unaliased == "application/x-shellscript"
        || unaliased.starts_with("text/")
        || unaliased == "application/octet-stream"
}

/// Creates a content type from a MIME type (mirrors `g_content_type_from_mime_type_impl`).
pub fn from_mime_type_impl(mime_type: &str) -> String {
    mime_type.to_string()
}

/// Guesses a content type from filename and/or data (mirrors `g_content_type_guess_impl`).
pub fn guess_impl(filename: Option<&str>, data: &[u8]) -> (String, bool) {
    if let Some(fname) = filename {
        if let Some(pos) = fname.rfind('.') {
            let ext = &fname[pos + 1..];
            match ext {
                "txt" => return ("text/plain".to_string(), false),
                "html" | "htm" => return ("text/html".to_string(), false),
                "png" => return ("image/png".to_string(), false),
                "jpg" | "jpeg" => return ("image/jpeg".to_string(), false),
                "pdf" => return ("application/pdf".to_string(), false),
                "sh" => return ("application/x-shellscript".to_string(), false),
                _ => {}
            }
        }
    }

    // Sniff data
    if data.len() >= 4 {
        if &data[..4] == b"\x89PNG" {
            return ("image/png".to_string(), false);
        }
        if data.starts_with(b"%PDF") {
            return ("application/pdf".to_string(), false);
        }
        if data.starts_with(b"#!/") || data.starts_with(b"#! ") {
            return ("application/x-shellscript".to_string(), true);
        }
    }

    ("application/octet-stream".to_string(), true)
}

/// Returns all registered content types (mirrors `g_content_types_get_registered_impl`).
pub fn get_registered_impl() -> Vec<String> {
    let types = REGISTERED_TYPES.lock();
    types.clone()
}

/// Registers a content type.
pub fn register_content_type(ct: &str) {
    REGISTERED_TYPES.lock().push(ct.to_string());
}

/// Alias table: (alias → canonical).
static ALIASES: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Registered content types.
static REGISTERED_TYPES: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Adds an alias mapping.
pub fn add_alias(alias: &str, canonical: &str) {
    ALIASES
        .lock()
        .push((alias.to_string(), canonical.to_string()));
}

/// Clears all aliases and registered types (for testing).
pub fn clear_all() {
    ALIASES.lock().clear();
    REGISTERED_TYPES.lock().clear();
    MIME_DIRS.lock().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sniff_len() {
        assert_eq!(unix_content_type_get_sniff_len(), 4096);
    }

    #[test]
    fn test_unalias() {
        clear_all();
        add_alias("text/x-csrc", "text/x-csrc");
        assert_eq!(unix_content_type_unalias("text/x-csrc"), "text/x-csrc");
        assert_eq!(unix_content_type_unalias("unknown/type"), "unknown/type");
    }

    #[test]
    fn test_parents() {
        assert_eq!(
            unix_content_type_get_parents("text/html"),
            vec!["text/plain"]
        );
        assert_eq!(
            unix_content_type_get_parents("image/png"),
            vec!["application/octet-stream"]
        );
        assert!(unix_content_type_get_parents("application/octet-stream").is_empty());
    }

    #[test]
    fn test_equals() {
        clear_all();
        assert!(equals_impl("text/plain", "text/plain"));
        assert!(!equals_impl("text/plain", "image/png"));
    }

    #[test]
    fn test_is_a() {
        assert!(is_a_impl("text/html", "text/plain"));
        assert!(is_a_impl("image/png", "application/octet-stream"));
        assert!(!is_a_impl("text/plain", "image/png"));
    }

    #[test]
    fn test_guess_from_filename() {
        let (ct, uncertain) = guess_impl(Some("file.txt"), &[]);
        assert_eq!(ct, "text/plain");
        assert!(!uncertain);

        let (ct, _) = guess_impl(Some("image.png"), &[]);
        assert_eq!(ct, "image/png");
    }

    #[test]
    fn test_guess_from_data() {
        let (ct, _) = guess_impl(None, b"\x89PNG\r\n\x1a\n");
        assert_eq!(ct, "image/png");

        let (ct, uncertain) = guess_impl(None, &[0xFF, 0xD8, 0xFF]);
        assert_eq!(ct, "application/octet-stream");
        assert!(uncertain);
    }

    #[test]
    fn test_can_be_executable() {
        assert!(can_be_executable_impl("application/x-executable"));
        assert!(can_be_executable_impl("text/plain"));
        assert!(!can_be_executable_impl("image/png"));
    }

    #[test]
    fn test_description() {
        assert_eq!(
            get_description_impl("application/octet-stream"),
            "Unknown type"
        );
        assert_eq!(get_description_impl("text/plain"), "Text document");
        assert_eq!(get_description_impl("image/png"), "Image");
    }
}
