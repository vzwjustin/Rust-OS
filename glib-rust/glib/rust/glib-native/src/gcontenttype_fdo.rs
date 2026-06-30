//! gcontenttype_fdo matching `gio/gcontenttype-fdo.c`.
//!
//! Freedesktop.org (XDG) content type implementation. Provides content type
//! equality checks, subtype checks, parent lookup, MIME type sniffing,
//! and content type description/icon lookup based on the XDG MIME database.
//!
//! In this no_std port, we implement the core logic using in-memory MIME
//! type tables and alias/parent maps. File system access for loading
//! MIME databases is deferred.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// The unknown MIME type string.
pub const XDG_MIME_TYPE_UNKNOWN: &str = "application/octet-stream";

/// Global MIME directory list.
static MIME_DIRS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Global alias map: alias → canonical type.
static ALIASES: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

/// Global parent map: type → list of parent types.
static PARENTS: Mutex<BTreeMap<String, Vec<String>>> = Mutex::new(BTreeMap::new());

/// Global content type descriptions.
static DESCRIPTIONS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

/// Initializes the MIME directories with default system paths.
///
/// Mirrors `g_content_type_set_mime_dirs_impl(NULL)`.
pub fn set_default_mime_dirs() {
    let mut dirs = MIME_DIRS.lock();
    dirs.clear();
    dirs.push("/usr/share/mime".to_string());
    dirs.push("/usr/local/share/mime".to_string());
}

/// Sets custom MIME directories.
///
/// Mirrors `g_content_type_set_mime_dirs_impl`.
pub fn set_mime_dirs(dirs: &[&str]) {
    let mut d = MIME_DIRS.lock();
    d.clear();
    for dir in dirs {
        d.push(dir.to_string());
    }
}

/// Returns the current MIME directories.
///
/// Mirrors `g_content_type_get_mime_dirs_impl`.
pub fn get_mime_dirs() -> Vec<String> {
    let dirs = MIME_DIRS.lock();
    if dirs.is_empty() {
        drop(dirs);
        set_default_mime_dirs();
        return MIME_DIRS.lock().clone();
    }
    dirs.clone()
}

/// Registers a MIME type alias.
///
/// In the C implementation, this is done by parsing `aliases` files from
/// the XDG MIME database. Here we provide a programmatic interface.
pub fn register_alias(alias: &str, canonical: &str) {
    ALIASES
        .lock()
        .insert(alias.to_string(), canonical.to_string());
}

/// Returns the canonical (unaliased) form of a content type.
///
/// Mirrors `_g_unix_content_type_unalias`.
pub fn unalias(content_type: &str) -> String {
    ALIASES
        .lock()
        .get(content_type)
        .cloned()
        .unwrap_or_else(|| content_type.to_string())
}

/// Returns the maximum buffer size needed for content type sniffing.
///
/// Mirrors `_g_unix_content_type_get_sniff_len`.
pub fn get_sniff_len() -> usize {
    4096
}

/// Checks if two content types are equal (after unaliasing).
///
/// Mirrors `g_content_type_equals_impl`.
pub fn equals(type1: &str, type2: &str) -> bool {
    let t1 = unalias(type1);
    let t2 = unalias(type2);
    t1 == t2
}

/// Checks if `content_type` is a subtype of `supertype`.
///
/// Mirrors `g_content_type_is_a_impl`.
pub fn is_a(content_type: &str, supertype: &str) -> bool {
    if equals(content_type, supertype) {
        return true;
    }

    let ct = unalias(content_type);
    let st = unalias(supertype);

    // Check the text/* hierarchy
    if ct.starts_with("text/") && st == "text/plain" {
        return true;
    }

    // Check application/octet-stream as a catch-all
    if st == "application/octet-stream" {
        return true;
    }

    // Check registered parents
    if let Some(parents) = PARENTS.lock().get(&ct) {
        for parent in parents {
            if parent == &st || is_a(parent, &st) {
                return true;
            }
        }
    }

    // Check generic supertypes
    if st == "text/plain" && ct.starts_with("text/") {
        return true;
    }
    if st == "application/octet-stream" {
        return true;
    }
    if st == "text" && ct.starts_with("text/") {
        return true;
    }
    if st == "application" && ct.starts_with("application/") {
        return true;
    }
    if st == "image" && ct.starts_with("image/") {
        return true;
    }
    if st == "audio" && ct.starts_with("audio/") {
        return true;
    }
    if st == "video" && ct.starts_with("video/") {
        return true;
    }

    false
}

/// Checks if a content type is a specific MIME type.
///
/// Mirrors `g_content_type_is_mime_type_impl`.
pub fn is_mime_type(content_type: &str, mime_type: &str) -> bool {
    is_a(content_type, mime_type)
}

/// Checks if a content type is the unknown type.
///
/// Mirrors `g_content_type_is_unknown_impl`.
pub fn is_unknown(content_type: &str) -> bool {
    content_type == XDG_MIME_TYPE_UNKNOWN
}

/// Gets the parent content types for a given type.
///
/// Mirrors `_g_unix_content_type_get_parents`.
pub fn get_parents(content_type: &str) -> Vec<String> {
    let mut result = Vec::new();
    let canonical = unalias(content_type);
    result.push(canonical.clone());

    // Check registered parents
    if let Some(parents) = PARENTS.lock().get(&canonical) {
        for p in parents {
            result.push(p.clone());
        }
    }

    // Add generic parents based on the type hierarchy
    if canonical.starts_with("text/") && canonical != "text/plain" {
        result.push("text/plain".to_string());
    }

    result
}

/// Registers a parent relationship.
pub fn register_parent(child: &str, parent: &str) {
    PARENTS
        .lock()
        .entry(child.to_string())
        .or_default()
        .push(parent.to_string());
}

/// Gets the description for a content type.
///
/// Mirrors `g_content_type_get_description_impl`.
pub fn get_description(content_type: &str) -> String {
    let canonical = unalias(content_type);
    DESCRIPTIONS
        .lock()
        .get(&canonical)
        .cloned()
        .unwrap_or_else(|| canonical.clone())
}

/// Registers a description for a content type.
pub fn register_description(content_type: &str, description: &str) {
    DESCRIPTIONS
        .lock()
        .insert(content_type.to_string(), description.to_string());
}

/// Gets the MIME type for a file name based on its extension.
///
/// This is a simplified version of the XDG MIME type detection.
pub fn get_mime_type_from_filename(filename: &str) -> String {
    let ext = filename.rsplit('.').next().unwrap_or("");
    match ext.to_ascii_lowercase().as_str() {
        "txt" => "text/plain",
        "html" | "htm" => "text/html",
        "xml" => "text/xml",
        "css" => "text/css",
        "js" => "application/javascript",
        "json" => "application/json",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        "gz" => "application/gzip",
        "tar" => "application/x-tar",
        "rs" => "text/x-rust",
        "c" => "text/x-csrc",
        "h" => "text/x-chdr",
        "cpp" | "cxx" | "cc" => "text/x-c++src",
        "hpp" | "hxx" | "hh" => "text/x-c++hdr",
        "py" => "text/x-python",
        "sh" => "application/x-shellscript",
        "so" => "application/x-sharedlib",
        "o" => "application/x-object",
        "a" => "application/x-archive",
        "wasm" => "application/wasm",
        _ => XDG_MIME_TYPE_UNKNOWN,
    }
    .to_string()
}

/// Guesses the content type from data sniffing.
///
/// Mirrors `g_content_type_guess_impl` (data-based portion).
pub fn guess_from_data(data: &[u8]) -> String {
    if data.is_empty() {
        return XDG_MIME_TYPE_UNKNOWN.to_string();
    }

    // Check for common magic numbers
    if data.len() >= 4 {
        if &data[0..4] == b"\x89PNG" {
            return "image/png".to_string();
        }
        if &data[0..4] == b"GIF8" {
            return "image/gif".to_string();
        }
        if &data[0..2] == b"\xff\xd8" {
            return "image/jpeg".to_string();
        }
        if &data[0..4] == b"%PDF" {
            return "application/pdf".to_string();
        }
        if &data[0..4] == b"PK\x03\x04" {
            return "application/zip".to_string();
        }
        if &data[0..5] == b"\x1f\x8b\x08\x00\x00" || (data[0] == 0x1f && data[1] == 0x8b) {
            return "application/gzip".to_string();
        }
        if &data[0..4] == b"\x7fELF" {
            return "application/x-executable".to_string();
        }
        if &data[0..4] == b"\x00\x61\x73\x6d" {
            return "application/wasm".to_string();
        }
    }

    // Check for text
    if data
        .iter()
        .take(64)
        .all(|&b| b == 0 || (b >= 0x20 && b <= 0x7e) || b == b'\n' || b == b'\r' || b == b'\t')
    {
        // Check for HTML
        let lower: String = data
            .iter()
            .take(256)
            .map(|&b| {
                if b.is_ascii_uppercase() {
                    b.to_ascii_lowercase() as char
                } else {
                    b as char
                }
            })
            .collect();
        if lower.contains("<!doctype html") || lower.contains("<html") {
            return "text/html".to_string();
        }
        if lower.contains("<?xml") {
            return "text/xml".to_string();
        }
        if lower.contains("{") || lower.contains("[") {
            // Could be JSON
            let trimmed = data
                .iter()
                .skip_while(|&&b| b.is_ascii_whitespace())
                .take(1)
                .next();
            if let Some(&b'{') | Some(&b'[') = trimmed {
                return "application/json".to_string();
            }
        }
        return "text/plain".to_string();
    }

    XDG_MIME_TYPE_UNKNOWN.to_string()
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equals() {
        assert!(equals("text/plain", "text/plain"));
        assert!(!equals("text/plain", "text/html"));
    }

    #[test]
    fn test_equals_with_alias() {
        register_alias("text/x-csrc", "text/x-c");
        assert!(equals("text/x-csrc", "text/x-c"));
    }

    #[test]
    fn test_unalias() {
        register_alias("application/x-zip", "application/zip");
        assert_eq!(unalias("application/x-zip"), "application/zip");
        assert_eq!(unalias("text/plain"), "text/plain");
    }

    #[test]
    fn test_is_a() {
        assert!(is_a("text/html", "text/plain"));
        assert!(is_a("image/png", "application/octet-stream"));
        assert!(!is_a("image/png", "text/plain"));
    }

    #[test]
    fn test_is_a_with_registered_parent() {
        register_parent("image/svg+xml", "text/xml");
        assert!(is_a("image/svg+xml", "text/xml"));
        assert!(is_a("image/svg+xml", "text/plain"));
    }

    #[test]
    fn test_is_unknown() {
        assert!(is_unknown(XDG_MIME_TYPE_UNKNOWN));
        assert!(!is_unknown("text/plain"));
    }

    #[test]
    fn test_get_parents() {
        let parents = get_parents("text/html");
        assert!(parents.contains(&"text/html".to_string()));
        assert!(parents.contains(&"text/plain".to_string()));
    }

    #[test]
    fn test_get_mime_type_from_filename() {
        assert_eq!(get_mime_type_from_filename("file.txt"), "text/plain");
        assert_eq!(get_mime_type_from_filename("image.png"), "image/png");
        assert_eq!(
            get_mime_type_from_filename("archive.tar.gz"),
            "application/gzip"
        );
        assert_eq!(
            get_mime_type_from_filename("unknown.xyz"),
            XDG_MIME_TYPE_UNKNOWN
        );
    }

    #[test]
    fn test_guess_from_data_png() {
        let data = b"\x89PNG\r\n\x1a\n";
        assert_eq!(guess_from_data(data), "image/png");
    }

    #[test]
    fn test_guess_from_data_pdf() {
        let data = b"%PDF-1.4\n";
        assert_eq!(guess_from_data(data), "application/pdf");
    }

    #[test]
    fn test_guess_from_data_text() {
        let data = b"Hello, World!\n";
        assert_eq!(guess_from_data(data), "text/plain");
    }

    #[test]
    fn test_guess_from_data_html() {
        let data = b"<!DOCTYPE html>\n<html><body></body></html>";
        assert_eq!(guess_from_data(data), "text/html");
    }

    #[test]
    fn test_guess_from_data_empty() {
        assert_eq!(guess_from_data(&[]), XDG_MIME_TYPE_UNKNOWN);
    }

    #[test]
    fn test_mime_dirs() {
        set_mime_dirs(&["/custom/mime"]);
        let dirs = get_mime_dirs();
        assert_eq!(dirs, vec!["/custom/mime".to_string()]);

        set_default_mime_dirs();
        let dirs = get_mime_dirs();
        assert!(dirs.contains(&"/usr/share/mime".to_string()));
    }
}
