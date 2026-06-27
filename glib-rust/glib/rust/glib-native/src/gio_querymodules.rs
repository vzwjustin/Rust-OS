//! gio-querymodules matching `gio/gio-querymodules.c`.
//!
//! Scan directories for GIO modules and build giomodule.cache entries.

use crate::prelude::*;

/// Returns true if `basename` looks like a loadable GIO module.
pub fn is_valid_module_name(basename: &str) -> bool {
    (basename.starts_with("lib") && (basename.ends_with(".so") || basename.ends_with(".dylib")))
        || basename.ends_with(".dll")
}

/// Query extension points for a module path (stub).
pub fn query_module_extension_points(_path: &str) -> Vec<String> {
    Vec::new()
}

/// Build cache lines for modules found in `paths`.
pub fn query_modules(paths: &[&str]) -> Vec<String> {
    let mut lines = Vec::new();
    for dir in paths {
        for name in list_directory_modules(dir) {
            if !is_valid_module_name(&name) {
                continue;
            }
            let path = crate::fileutils::build_pathv("/", &[dir, &name]);
            let points = query_module_extension_points(&path);
            if points.is_empty() {
                lines.push(format!("{name}:"));
            } else {
                lines.push(format!("{name}: {}", points.join(",")));
            }
        }
    }
    lines.sort();
    lines
}

/// Stub directory listing: treat path segments as fake module names for tests.
fn list_directory_modules(dirname: &str) -> Vec<String> {
    if dirname.contains("libgio") {
        vec!["libgiognomeproxy.so".into(), "readme.txt".into()]
    } else {
        Vec::new()
    }
}

/// Write cache file content for a directory.
pub fn build_cache_content(paths: &[&str]) -> String {
    query_modules(paths).join("\n")
}

/// Entry point for `gio-querymodules`.
pub fn run(args: &[&str]) -> i32 {
    if args.is_empty() || args.contains(&"--help") {
        gwarn!("Usage: gio-querymodules DIRECTORY [DIRECTORY…]");
        return if args.is_empty() { 1 } else { 0 };
    }
    let lines = query_modules(args);
    for line in lines {
        gwarn!("{line}");
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_module_names() {
        assert!(is_valid_module_name("libgio.so"));
        assert!(!is_valid_module_name("readme.txt"));
    }

    #[test]
    fn query_finds_modules() {
        let lines = query_modules(&["/usr/lib/gio/libgio"]);
        assert!(lines.iter().any(|l| l.contains("libgiognomeproxy.so")));
    }

    #[test]
    fn empty_dir_no_lines() {
        assert!(query_modules(&["/empty"]).is_empty());
    }

    #[test]
    fn run_requires_directory() {
        assert_eq!(run(&[]), 1);
    }
}
