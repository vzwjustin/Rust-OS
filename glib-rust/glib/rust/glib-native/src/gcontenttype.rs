//! GContentType matching `gio/gcontenttype.h`.
//!
//! Upstream `GContentType` provides functions for working with content
//! types (MIME types on Unix, file extensions on Windows). We port it
//! as a module of free functions with a simple in-memory type registry.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A registered content type entry.
struct ContentTypeEntry {
    content_type: String,
    mime_type: String,
    description: String,
    can_be_executable: bool,
}

static REGISTRY: Mutex<Vec<ContentTypeEntry>> = Mutex::new(Vec::new());

/// Initializes the registry with common types.
fn ensure_registry() {
    let mut reg = REGISTRY.lock();
    if !reg.is_empty() {
        return;
    }
    reg.push(ContentTypeEntry {
        content_type: "text/plain".to_string(),
        mime_type: "text/plain".to_string(),
        description: "Plain text document".to_string(),
        can_be_executable: true,
    });
    reg.push(ContentTypeEntry {
        content_type: "application/octet-stream".to_string(),
        mime_type: "application/octet-stream".to_string(),
        description: "Unknown binary data".to_string(),
        can_be_executable: true,
    });
    reg.push(ContentTypeEntry {
        content_type: "image/png".to_string(),
        mime_type: "image/png".to_string(),
        description: "PNG image".to_string(),
        can_be_executable: false,
    });
    reg.push(ContentTypeEntry {
        content_type: "application/x-shellscript".to_string(),
        mime_type: "application/x-shellscript".to_string(),
        description: "Shell script".to_string(),
        can_be_executable: true,
    });
}

/// Checks if two content types are equal.
///
/// Mirrors `g_content_type_equals`.
pub fn content_type_equals(type1: &str, type2: &str) -> bool {
    type1 == type2
}

/// Checks if a content type is a subtype of another.
///
/// Mirrors `g_content_type_is_a`.
pub fn content_type_is_a(ctype: &str, supertype: &str) -> bool {
    if ctype == supertype {
        return true;
    }
    if supertype == "text/plain" && ctype.starts_with("text/") {
        return true;
    }
    if supertype == "application/octet-stream" {
        return true;
    }
    false
}

/// Checks if a content type is a MIME type.
///
/// Mirrors `g_content_type_is_mime_type`.
pub fn content_type_is_mime_type(ctype: &str, mime_type: &str) -> bool {
    ctype == mime_type
}

/// Checks if a content type is unknown.
///
/// Mirrors `g_content_type_is_unknown`.
pub fn content_type_is_unknown(ctype: &str) -> bool {
    ctype == "application/octet-stream" || ctype.is_empty()
}

/// Gets the description for a content type.
///
/// Mirrors `g_content_type_get_description`.
pub fn content_type_get_description(ctype: &str) -> String {
    ensure_registry();
    let reg = REGISTRY.lock();
    for entry in reg.iter() {
        if entry.content_type == ctype {
            return entry.description.clone();
        }
    }
    "Unknown".to_string()
}

/// Gets the MIME type for a content type.
///
/// Mirrors `g_content_type_get_mime_type`.
pub fn content_type_get_mime_type(ctype: &str) -> Option<String> {
    ensure_registry();
    let reg = REGISTRY.lock();
    for entry in reg.iter() {
        if entry.content_type == ctype {
            return Some(entry.mime_type.clone());
        }
    }
    if ctype.contains('/') {
        Some(ctype.to_string())
    } else {
        None
    }
}

/// Checks if a content type can be executable.
///
/// Mirrors `g_content_type_can_be_executable`.
pub fn content_type_can_be_executable(ctype: &str) -> bool {
    ensure_registry();
    let reg = REGISTRY.lock();
    for entry in reg.iter() {
        if entry.content_type == ctype {
            return entry.can_be_executable;
        }
    }
    ctype.starts_with("text/") || ctype == "application/octet-stream"
}

/// Converts a MIME type to a content type.
///
/// Mirrors `g_content_type_from_mime_type`.
pub fn content_type_from_mime_type(mime_type: &str) -> String {
    mime_type.to_string()
}

/// Guesses a content type from a filename and/or data.
///
/// Mirrors `g_content_type_guess`.
pub fn content_type_guess(filename: Option<&str>, data: &[u8]) -> (String, bool) {
    if let Some(name) = filename {
        let lower = name.to_lowercase();
        if lower.ends_with(".txt") {
            return ("text/plain".to_string(), false);
        }
        if lower.ends_with(".png") {
            return ("image/png".to_string(), false);
        }
        if lower.ends_with(".sh") {
            return ("application/x-shellscript".to_string(), false);
        }
        if lower.ends_with(".json") {
            return ("application/json".to_string(), false);
        }
        if lower.ends_with(".xml") {
            return ("application/xml".to_string(), false);
        }
        if lower.ends_with(".html") || lower.ends_with(".htm") {
            return ("text/html".to_string(), false);
        }
    }
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return ("image/png".to_string(), false);
    }
    if data.starts_with(b"#!/") || data.starts_with(b"#! ") {
        return ("application/x-shellscript".to_string(), true);
    }
    ("application/octet-stream".to_string(), true)
}

/// Gets all registered content types.
///
/// Mirrors `g_content_types_get_registered`.
pub fn content_types_get_registered() -> Vec<String> {
    ensure_registry();
    let reg = REGISTRY.lock();
    reg.iter().map(|e| e.content_type.clone()).collect()
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_type_equals() {
        assert!(content_type_equals("text/plain", "text/plain"));
        assert!(!content_type_equals("text/plain", "image/png"));
    }

    #[test]
    fn test_content_type_is_a() {
        assert!(content_type_is_a("text/plain", "text/plain"));
        assert!(content_type_is_a("text/html", "text/plain"));
        assert!(content_type_is_a("image/png", "application/octet-stream"));
        assert!(!content_type_is_a("image/png", "text/plain"));
    }

    #[test]
    fn test_content_type_is_mime_type() {
        assert!(content_type_is_mime_type("text/plain", "text/plain"));
        assert!(!content_type_is_mime_type("text/plain", "image/png"));
    }

    #[test]
    fn test_content_type_is_unknown() {
        assert!(content_type_is_unknown("application/octet-stream"));
        assert!(content_type_is_unknown(""));
        assert!(!content_type_is_unknown("text/plain"));
    }

    #[test]
    fn test_content_type_get_description() {
        assert_eq!(
            content_type_get_description("text/plain"),
            "Plain text document"
        );
        assert_eq!(content_type_get_description("image/png"), "PNG image");
        assert_eq!(content_type_get_description("nonexistent"), "Unknown");
    }

    #[test]
    fn test_content_type_get_mime_type() {
        assert_eq!(
            content_type_get_mime_type("text/plain").unwrap(),
            "text/plain"
        );
        assert_eq!(
            content_type_get_mime_type("image/png").unwrap(),
            "image/png"
        );
        assert!(content_type_get_mime_type("nonexistent").is_none());
    }

    #[test]
    fn test_content_type_can_be_executable() {
        assert!(content_type_can_be_executable("text/plain"));
        assert!(content_type_can_be_executable("application/octet-stream"));
        assert!(!content_type_can_be_executable("image/png"));
    }

    #[test]
    fn test_content_type_from_mime_type() {
        assert_eq!(content_type_from_mime_type("text/html"), "text/html");
    }

    #[test]
    fn test_content_type_guess_filename() {
        let (ct, uncertain) = content_type_guess(Some("test.txt"), &[]);
        assert_eq!(ct, "text/plain");
        assert!(!uncertain);

        let (ct, _) = content_type_guess(Some("photo.png"), &[]);
        assert_eq!(ct, "image/png");

        let (ct, _) = content_type_guess(Some("script.sh"), &[]);
        assert_eq!(ct, "application/x-shellscript");
    }

    #[test]
    fn test_content_type_guess_data() {
        let png_header = b"\x89PNG\r\n\x1a\n";
        let (ct, _) = content_type_guess(None, png_header);
        assert_eq!(ct, "image/png");

        let shell_header = b"#!/bin/sh\n";
        let (ct, uncertain) = content_type_guess(None, shell_header);
        assert_eq!(ct, "application/x-shellscript");
        assert!(uncertain);
    }

    #[test]
    fn test_content_type_guess_unknown() {
        let (ct, uncertain) = content_type_guess(None, b"random bytes");
        assert_eq!(ct, "application/octet-stream");
        assert!(uncertain);
    }

    #[test]
    fn test_content_types_get_registered() {
        let types = content_types_get_registered();
        assert!(types.contains(&"text/plain".to_string()));
        assert!(types.contains(&"image/png".to_string()));
    }
}
