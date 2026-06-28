//! `xdgmime` matching `gio/xdgmime/xdgmime.h`.
//!
//! XDG Mime Spec mime resolver. Provides MIME type detection from
//! file data, file names, and hierarchy lookups.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use spin::Mutex;

/// Well-known MIME type constants (mirrors `xdg_mime_type_*`).
pub const XDG_MIME_TYPE_UNKNOWN: &str = "application/octet-stream";
pub const XDG_MIME_TYPE_EMPTY: &str = "inode/x-empty";
pub const XDG_MIME_TYPE_TEXTPLAIN: &str = "text/plain";

/// Reload callback ID counter.
static CALLBACK_ID: Mutex<i32> = Mutex::new(0);

/// Registered reload callbacks.
static CALLBACKS: Mutex<Vec<(i32, fn())>> = Mutex::new(Vec::new());

/// MIME type aliases registry.
static ALIASES: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// MIME type parents registry.
static PARENTS: Mutex<Vec<(String, Vec<String>)>> = Mutex::new(Vec::new());

/// MIME type icons registry.
static ICONS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// MIME type generic icons registry.
static GENERIC_ICONS: Mutex<Vec<(String, String)>> = Mutex::new(Vec::new());

/// Glob patterns for filename-based MIME detection.
static GLOBS: Mutex<Vec<GlobEntry>> = Mutex::new(Vec::new());

/// A glob pattern entry for MIME type matching.
#[derive(Debug, Clone)]
struct GlobEntry {
    pattern: String,
    mime_type: String,
    weight: i32,
    case_sensitive: bool,
}

/// Returns the MIME type for the given data (mirrors `xdg_mime_get_mime_type_for_data`).
pub fn get_mime_type_for_data(data: &[u8], result_prio: Option<&mut i32>) -> String {
    if data.is_empty() {
        if let Some(p) = result_prio {
            *p = 0;
        }
        return XDG_MIME_TYPE_EMPTY.to_string();
    }
    if let Some(p) = result_prio {
        *p = 50;
    }
    if is_text_data(data) {
        return XDG_MIME_TYPE_TEXTPLAIN.to_string();
    }
    XDG_MIME_TYPE_UNKNOWN.to_string()
}

/// Returns the MIME type for the given file name (mirrors `xdg_mime_get_mime_type_from_file_name`).
pub fn get_mime_type_from_file_name(file_name: &str) -> String {
    let globs = GLOBS.lock();
    let mut best_match: Option<&GlobEntry> = None;
    let mut best_weight = -1;
    for entry in globs.iter() {
        if matches_glob(file_name, &entry.pattern) {
            if entry.weight > best_weight {
                best_weight = entry.weight;
                best_match = Some(entry);
            }
        }
    }
    match best_match {
        Some(e) => e.mime_type.clone(),
        None => XDG_MIME_TYPE_UNKNOWN.to_string(),
    }
}

/// Returns the MIME type for the given file (mirrors `xdg_mime_get_mime_type_for_file`).
/// In our no_std port, we only use the filename for detection.
pub fn get_mime_type_for_file(file_name: &str) -> String {
    get_mime_type_from_file_name(file_name)
}

/// Returns multiple MIME types for the given file name
/// (mirrors `xdg_mime_get_mime_types_from_file_name`).
pub fn get_mime_types_from_file_name(file_name: &str) -> Vec<String> {
    let globs = GLOBS.lock();
    let mut results = Vec::new();
    for entry in globs.iter() {
        if matches_glob(file_name, &entry.pattern) && !results.contains(&entry.mime_type) {
            results.push(entry.mime_type.clone());
        }
    }
    results
}

/// Checks if a MIME type is valid (mirrors `xdg_mime_is_valid_mime_type`).
pub fn is_valid_mime_type(mime_type: &str) -> bool {
    if let Some(slash) = mime_type.find('/') {
        slash > 0 && slash < mime_type.len() - 1
    } else {
        false
    }
}

/// Checks if two MIME types are equal (mirrors `xdg_mime_mime_type_equal`).
pub fn mime_type_equal(mime_a: &str, mime_b: &str) -> bool {
    unalias_mime_type(mime_a) == unalias_mime_type(mime_b)
}

/// Checks if two media types are equal (mirrors `xdg_mime_media_type_equal`).
pub fn media_type_equal(mime_a: &str, mime_b: &str) -> bool {
    let a_media = mime_a.split('/').next().unwrap_or("");
    let b_media = mime_b.split('/').next().unwrap_or("");
    a_media == b_media
}

/// Checks if `mime_a` is a subclass of `mime_b` (mirrors `xdg_mime_mime_type_subclass`).
pub fn mime_type_subclass(mime_a: &str, mime_b: &str) -> bool {
    if mime_type_equal(mime_a, mime_b) {
        return true;
    }
    let parents = PARENTS.lock();
    let unaliased = unalias_mime_type(mime_a);
    if let Some((_, parent_list)) = parents.iter().find(|(m, _)| *m == unaliased) {
        return parent_list.iter().any(|p| mime_type_subclass(p, mime_b));
    }
    false
}

/// Lists parents of a MIME type (mirrors `xdg_mime_list_mime_parents`).
pub fn list_mime_parents(mime: &str) -> Vec<String> {
    let parents = PARENTS.lock();
    let unaliased = unalias_mime_type(mime);
    parents
        .iter()
        .find(|(m, _)| *m == unaliased)
        .map(|(_, p)| p.clone())
        .unwrap_or_default()
}

/// Unaliases a MIME type (mirrors `xdg_mime_unalias_mime_type`).
pub fn unalias_mime_type(mime: &str) -> String {
    let aliases = ALIASES.lock();
    for (alias, canonical) in aliases.iter() {
        if alias == mime {
            return canonical.clone();
        }
    }
    mime.to_string()
}

/// Returns the icon for a MIME type (mirrors `xdg_mime_get_icon`).
pub fn get_icon(mime: &str) -> String {
    let icons = ICONS.lock();
    let unaliased = unalias_mime_type(mime);
    icons
        .iter()
        .find(|(m, _)| *m == unaliased)
        .map(|(_, i)| i.clone())
        .unwrap_or_default()
}

/// Returns the generic icon for a MIME type (mirrors `xdg_mime_get_generic_icon`).
pub fn get_generic_icon(mime: &str) -> String {
    let icons = GENERIC_ICONS.lock();
    let unaliased = unalias_mime_type(mime);
    icons
        .iter()
        .find(|(m, _)| *m == unaliased)
        .map(|(_, i)| i.clone())
        .unwrap_or_default()
}

/// Returns the max buffer extents needed for magic-based detection
/// (mirrors `xdg_mime_get_max_buffer_extents`).
pub fn get_max_buffer_extents() -> usize {
    4096
}

/// Shuts down the XDG mime system (mirrors `xdg_mime_shutdown`).
pub fn shutdown() {
    ALIASES.lock().clear();
    PARENTS.lock().clear();
    ICONS.lock().clear();
    GENERIC_ICONS.lock().clear();
    GLOBS.lock().clear();
    CALLBACKS.lock().clear();
    MIME_DIRS.lock().clear();
}

/// Registers a reload callback (mirrors `xdg_mime_register_reload_callback`).
pub fn register_reload_callback(callback: fn()) -> i32 {
    let mut id = CALLBACK_ID.lock();
    *id += 1;
    let callback_id = *id;
    CALLBACKS.lock().push((callback_id, callback));
    callback_id
}

/// Removes a reload callback by ID (mirrors `xdg_mime_remove_callback`).
pub fn remove_callback(callback_id: i32) {
    CALLBACKS.lock().retain(|(id, _)| *id != callback_id);
}

/// MIME search directories (mirrors `xdg_dirs` in xdgmime.c).
static MIME_DIRS: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Sets the search directories (mirrors `xdg_mime_set_dirs`).
///
/// Stores the provided directory list and triggers reload callbacks
/// so that registered consumers can re-initialize their caches.
pub fn set_dirs(dirs: &[&str]) {
    let mut mime_dirs = MIME_DIRS.lock();
    mime_dirs.clear();
    for dir in dirs {
        mime_dirs.push(dir.to_string());
    }
    drop(mime_dirs);

    let callbacks = CALLBACKS.lock();
    for (_, callback) in callbacks.iter() {
        callback();
    }
}

/// Returns the currently configured MIME search directories.
pub fn get_dirs() -> Vec<String> {
    MIME_DIRS.lock().clone()
}

/// Dumps the internal state (mirrors `xdg_mime_dump`).
///
/// Outputs the current globs, aliases, parents, icons, and search
/// directories via `gwarn!` for debugging purposes.
pub fn dump() {
    let dirs = MIME_DIRS.lock();
    gwarn!("xdgmime dump: {} search directories", dirs.len());
    for dir in dirs.iter() {
        gwarn!("  dir: {}", dir);
    }

    let globs = GLOBS.lock();
    gwarn!("xdgmime dump: {} glob patterns", globs.len());
    for entry in globs.iter() {
        gwarn!(
            "  glob: {} -> {} (weight={}, cs={})",
            entry.pattern,
            entry.mime_type,
            entry.weight,
            entry.case_sensitive
        );
    }

    let aliases = ALIASES.lock();
    gwarn!("xdgmime dump: {} aliases", aliases.len());
    for (alias, canonical) in aliases.iter() {
        gwarn!("  alias: {} -> {}", alias, canonical);
    }

    let parents = PARENTS.lock();
    gwarn!("xdgmime dump: {} parent mappings", parents.len());
    for (mime, parent_list) in parents.iter() {
        gwarn!("  parent: {} -> {:?}", mime, parent_list);
    }

    let icons = ICONS.lock();
    gwarn!("xdgmime dump: {} icon mappings", icons.len());
    for (mime, icon) in icons.iter() {
        gwarn!("  icon: {} -> {}", mime, icon);
    }

    let generic_icons = GENERIC_ICONS.lock();
    gwarn!("xdgmime dump: {} generic icon mappings", generic_icons.len());
    for (mime, icon) in generic_icons.iter() {
        gwarn!("  generic_icon: {} -> {}", mime, icon);
    }
}

// ── Internal helpers ───────────────────────────────────────────────────────

/// Registers a glob pattern for MIME type detection.
pub fn register_glob(pattern: &str, mime_type: &str, weight: i32, case_sensitive: bool) {
    GLOBS.lock().push(GlobEntry {
        pattern: pattern.to_string(),
        mime_type: mime_type.to_string(),
        weight,
        case_sensitive,
    });
}

/// Registers a MIME type alias.
pub fn register_alias(alias: &str, canonical: &str) {
    ALIASES
        .lock()
        .push((alias.to_string(), canonical.to_string()));
}

/// Registers a MIME type parent.
pub fn register_parent(mime: &str, parent: &str) {
    let mut parents = PARENTS.lock();
    let unaliased = unalias_mime_type(mime);
    if let Some((_, list)) = parents.iter_mut().find(|(m, _)| *m == unaliased) {
        if !list.contains(&parent.to_string()) {
            list.push(parent.to_string());
        }
    } else {
        parents.push((unaliased, vec![parent.to_string()]));
    }
}

/// Registers an icon for a MIME type.
pub fn register_icon(mime: &str, icon: &str) {
    ICONS.lock().push((mime.to_string(), icon.to_string()));
}

/// Registers a generic icon for a MIME type.
pub fn register_generic_icon(mime: &str, icon: &str) {
    GENERIC_ICONS
        .lock()
        .push((mime.to_string(), icon.to_string()));
}

/// Checks if data appears to be text.
fn is_text_data(data: &[u8]) -> bool {
    if data.is_empty() {
        return false;
    }
    let mut non_text = 0;
    let len = data.len().min(1024);
    for &b in &data[..len] {
        if b == 0 {
            return false;
        }
        if b < 0x09 || (b > 0x0d && b < 0x20) {
            non_text += 1;
        }
    }
    (non_text as f64 / len as f64) < 0.3
}

/// Matches a filename against a glob pattern.
fn matches_glob(file_name: &str, pattern: &str) -> bool {
    if pattern.starts_with("*.") {
        let suffix = &pattern[1..];
        file_name.ends_with(suffix)
    } else if pattern.contains('*') {
        simple_glob_match(file_name, pattern)
    } else {
        file_name == pattern
    }
}

/// Simple glob matching with `*` wildcards.
fn simple_glob_match(text: &str, pattern: &str) -> bool {
    let text_bytes = text.as_bytes();
    let pattern_bytes = pattern.as_bytes();
    let mut ti = 0;
    let mut pi = 0;
    let mut star_pi = None;
    let mut star_ti = 0;
    while ti < text_bytes.len() {
        if pi < pattern_bytes.len()
            && (pattern_bytes[pi] == b'?' || pattern_bytes[pi] == text_bytes[ti])
        {
            ti += 1;
            pi += 1;
        } else if pi < pattern_bytes.len() && pattern_bytes[pi] == b'*' {
            star_pi = Some(pi);
            star_ti = ti;
            pi += 1;
        } else if let Some(spi) = star_pi {
            pi = spi + 1;
            star_ti += 1;
            ti = star_ti;
        } else {
            return false;
        }
    }
    while pi < pattern_bytes.len() && pattern_bytes[pi] == b'*' {
        pi += 1;
    }
    pi == pattern_bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_mime_type_for_data_empty() {
        assert_eq!(get_mime_type_for_data(&[], None), XDG_MIME_TYPE_EMPTY);
    }

    #[test]
    fn test_get_mime_type_for_data_text() {
        assert_eq!(
            get_mime_type_for_data(b"Hello, world!", None),
            XDG_MIME_TYPE_TEXTPLAIN
        );
    }

    #[test]
    fn test_get_mime_type_for_data_binary() {
        let data = [0u8, 1, 2, 3, 4, 5];
        assert_eq!(get_mime_type_for_data(&data, None), XDG_MIME_TYPE_UNKNOWN);
    }

    #[test]
    fn test_get_mime_type_from_file_name() {
        shutdown();
        register_glob("*.txt", "text/plain", 50, false);
        assert_eq!(get_mime_type_from_file_name("readme.txt"), "text/plain");
    }

    #[test]
    fn test_get_mime_type_from_file_name_unknown() {
        shutdown();
        assert_eq!(
            get_mime_type_from_file_name("file.xyzunknown"),
            XDG_MIME_TYPE_UNKNOWN
        );
    }

    #[test]
    fn test_is_valid_mime_type() {
        assert!(is_valid_mime_type("text/plain"));
        assert!(!is_valid_mime_type("invalid"));
        assert!(!is_valid_mime_type("/plain"));
        assert!(!is_valid_mime_type("text/"));
    }

    #[test]
    fn test_mime_type_equal() {
        shutdown();
        register_alias("application/x-text", "text/plain");
        assert!(mime_type_equal("application/x-text", "text/plain"));
        assert!(!mime_type_equal("text/plain", "application/json"));
    }

    #[test]
    fn test_media_type_equal() {
        assert!(media_type_equal("text/plain", "text/html"));
        assert!(!media_type_equal("text/plain", "application/json"));
    }

    #[test]
    fn test_mime_type_subclass() {
        shutdown();
        register_parent("text/html", "text/plain");
        assert!(mime_type_subclass("text/html", "text/plain"));
        assert!(mime_type_subclass("text/plain", "text/plain"));
        assert!(!mime_type_subclass("text/plain", "text/html"));
    }

    #[test]
    fn test_list_mime_parents() {
        shutdown();
        register_parent("text/html", "text/plain");
        let parents = list_mime_parents("text/html");
        assert_eq!(parents, vec!["text/plain".to_string()]);
    }

    #[test]
    fn test_unalias() {
        shutdown();
        register_alias("application/x-text", "text/plain");
        assert_eq!(unalias_mime_type("application/x-text"), "text/plain");
        assert_eq!(unalias_mime_type("text/plain"), "text/plain");
    }

    #[test]
    fn test_register_and_get_icon() {
        shutdown();
        register_icon("text/plain", "text-x-generic");
        assert_eq!(get_icon("text/plain"), "text-x-generic");
    }

    #[test]
    fn test_register_and_get_generic_icon() {
        shutdown();
        register_generic_icon("text/plain", "text");
        assert_eq!(get_generic_icon("text/plain"), "text");
    }

    #[test]
    fn test_max_buffer_extents() {
        assert_eq!(get_max_buffer_extents(), 4096);
    }

    #[test]
    fn test_reload_callback() {
        shutdown();
        let id = register_reload_callback(|| {});
        assert!(id > 0);
        remove_callback(id);
    }

    #[test]
    fn test_get_mime_types_from_file_name() {
        shutdown();
        register_glob("*.html", "text/html", 50, false);
        register_glob("*.htm", "text/html", 50, false);
        let types = get_mime_types_from_file_name("page.html");
        assert_eq!(types, vec!["text/html".to_string()]);
    }

    #[test]
    fn test_glob_match() {
        assert!(matches_glob("file.txt", "*.txt"));
        assert!(matches_glob("Makefile", "Makefile"));
        assert!(!matches_glob("file.txt", "*.c"));
    }

    #[test]
    fn test_set_dirs_and_get_dirs() {
        shutdown();
        set_dirs(&["/usr/share/mime", "/home/user/.local/share/mime"]);
        let dirs = get_dirs();
        assert_eq!(dirs.len(), 2);
        assert!(dirs.contains(&"/usr/share/mime".to_string()));
        assert!(dirs.contains(&"/home/user/.local/share/mime".to_string()));
        shutdown();
    }

    #[test]
    fn test_set_dirs_triggers_callback() {
        shutdown();
        use core::sync::atomic::{AtomicBool, Ordering};
        static CALLED: AtomicBool = AtomicBool::new(false);
        CALLED.store(false, Ordering::SeqCst);
        let id = register_reload_callback(|| {
            CALLED.store(true, Ordering::SeqCst);
        });
        set_dirs(&["/test/mime"]);
        assert!(CALLED.load(Ordering::SeqCst));
        remove_callback(id);
        shutdown();
    }

    #[test]
    fn test_set_dirs_clears_previous() {
        shutdown();
        set_dirs(&["/first", "/second"]);
        assert_eq!(get_dirs().len(), 2);
        set_dirs(&["/third"]);
        assert_eq!(get_dirs().len(), 1);
        assert_eq!(get_dirs()[0], "/third");
        shutdown();
    }
}
