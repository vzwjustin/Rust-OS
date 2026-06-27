//! gcontenttype_win32 matching `gio/gcontenttype-win32.c`.
//!
//! Windows-specific content type implementation using the registry
//! (HKEY_CLASSES_ROOT) to determine file type associations, descriptions,
//! and icons.
//!
//! In this no_std port, we model the registry-based content type lookups
//! with in-memory maps. Actual Windows registry access is not available.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use spin::Mutex;

/// Registry-based ProgID map: extension/type → ProgID
static PROGIDS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

/// ProgID → description map
static DESCRIPTIONS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

/// ProgID → parent ProgID map (for equals/is_a checks)
static PARENTS: Mutex<BTreeMap<String, String>> = Mutex::new(BTreeMap::new());

/// Sets MIME directories (no-op on Windows).
pub fn set_mime_dirs(_dirs: &[&str]) {
    // No-op on Windows, as in the C implementation
}

/// Gets MIME directories (returns empty on Windows).
pub fn get_mime_dirs() -> Vec<String> {
    Vec::new()
}

/// Registers a ProgID for a content type.
pub fn register_progid(content_type: &str, progid: &str) {
    PROGIDS
        .lock()
        .insert(content_type.to_ascii_lowercase(), progid.to_string());
}

/// Registers a description for a ProgID.
pub fn register_description(progid: &str, description: &str) {
    DESCRIPTIONS
        .lock()
        .insert(progid.to_string(), description.to_string());
}

/// Registers a parent ProgID relationship.
pub fn register_parent_progid(progid: &str, parent: &str) {
    PARENTS
        .lock()
        .insert(progid.to_string(), parent.to_string());
}

/// Gets the ProgID for a content type from the registry.
///
/// Mirrors `get_registry_classes_key`.
fn get_registry_classes_key(subdir: &str) -> Option<String> {
    PROGIDS.lock().get(&subdir.to_ascii_lowercase()).cloned()
}

/// Checks if two content types are equal.
///
/// On Windows, this compares case-insensitively, then checks if both
/// types resolve to the same ProgID.
///
/// Mirrors `g_content_type_equals_impl`.
pub fn equals(type1: &str, type2: &str) -> bool {
    if type1.eq_ignore_ascii_case(type2) {
        return true;
    }

    let progid1 = get_registry_classes_key(type1);
    let progid2 = get_registry_classes_key(type2);

    if let (Some(p1), Some(p2)) = (progid1, progid2) {
        return p1.eq_ignore_ascii_case(&p2);
    }

    false
}

/// Checks if `content_type` is a subtype of `supertype`.
///
/// On Windows, this follows the ProgID parent chain.
///
/// Mirrors `g_content_type_is_a_impl`.
pub fn is_a(content_type: &str, supertype: &str) -> bool {
    if equals(content_type, supertype) {
        return true;
    }

    let progid = match get_registry_classes_key(content_type) {
        Some(p) => p,
        None => return false,
    };

    let mut current = progid;
    let mut visited = Vec::new();
    loop {
        if visited.contains(&current) {
            break;
        }
        visited.push(current.clone());

        if current.eq_ignore_ascii_case(supertype) {
            return true;
        }

        match PARENTS.lock().get(&current) {
            Some(parent) => {
                if parent.eq_ignore_ascii_case(supertype) {
                    return true;
                }
                current = parent.clone();
            }
            None => break,
        }
    }

    false
}

/// Checks if a content type is a specific MIME type.
pub fn is_mime_type(content_type: &str, mime_type: &str) -> bool {
    is_a(content_type, mime_type)
}

/// Checks if a content type is unknown.
pub fn is_unknown(content_type: &str) -> bool {
    content_type.eq_ignore_ascii_case("application/octet-stream")
        || content_type.eq_ignore_ascii_case("unknown")
}

/// Gets the description for a content type.
///
/// Mirrors `g_content_type_get_description_impl`.
pub fn get_description(content_type: &str) -> String {
    if let Some(progid) = get_registry_classes_key(content_type) {
        if let Some(desc) = DESCRIPTIONS.lock().get(&progid) {
            return desc.clone();
        }
    }
    content_type.to_string()
}

/// Gets the MIME type for a filename based on its extension.
///
/// Mirrors `g_content_type_guess_impl` (filename portion).
pub fn guess_from_filename(filename: &str) -> String {
    let ext = match filename.rfind('.') {
        Some(pos) => &filename[pos + 1..],
        None => return "application/octet-stream".to_string(),
    };

    // Check registered ProgIDs first
    let ext_lower = format!(".{}", ext.to_ascii_lowercase());
    if let Some(progid) = get_registry_classes_key(&ext_lower) {
        return progid;
    }

    // Fallback to common types
    match ext.to_ascii_lowercase().as_str() {
        "txt" => "txtfile",
        "exe" => "exefile",
        "dll" => "dllfile",
        "bmp" => "image/bmp",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "html" | "htm" => "htmlfile",
        "xml" => "xmlfile",
        "pdf" => "application/pdf",
        "zip" => "application/zip",
        _ => "application/octet-stream",
    }
    .to_string()
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equals_case_insensitive() {
        assert!(equals("text/plain", "TEXT/PLAIN"));
        assert!(equals("image/png", "Image/PNG"));
        assert!(!equals("text/plain", "text/html"));
    }

    #[test]
    fn test_equals_via_progid() {
        register_progid(".jpg", "jpegfile");
        register_progid(".jpeg", "jpegfile");
        assert!(equals(".jpg", ".jpeg"));
    }

    #[test]
    fn test_is_a() {
        register_progid(".html", "htmlfile");
        register_parent_progid("htmlfile", "textfile");
        assert!(is_a(".html", "htmlfile"));
        assert!(is_a(".html", "textfile"));
        assert!(!is_a(".html", "imagefile"));
    }

    #[test]
    fn test_get_description() {
        register_progid(".txt", "txtfile");
        register_description("txtfile", "Text Document");
        assert_eq!(get_description(".txt"), "Text Document");
    }

    #[test]
    fn test_guess_from_filename() {
        assert_eq!(guess_from_filename("file.txt"), "txtfile");
        assert_eq!(guess_from_filename("file.exe"), "exefile");
        assert_eq!(guess_from_filename("file.png"), "image/png");
        assert_eq!(
            guess_from_filename("file.unknown"),
            "application/octet-stream"
        );
        assert_eq!(
            guess_from_filename("noextension"),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_is_unknown() {
        assert!(is_unknown("application/octet-stream"));
        assert!(is_unknown("unknown"));
        assert!(!is_unknown("text/plain"));
    }

    #[test]
    fn test_mime_dirs_noop() {
        set_mime_dirs(&["/custom"]);
        assert!(get_mime_dirs().is_empty());
    }
}
