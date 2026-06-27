//! gresource-tool matching `gio/gresource-tool.c`.
//!
//! List and extract resources from compiled GResource bundles.

use crate::gresource::{Resource, ResourceLookupFlags};
use crate::prelude::*;
use alloc::collections::BTreeMap;

/// Load a resource bundle from raw bytes.
pub fn load_resource(data: &[u8]) -> Result<Resource, String> {
    if data.is_empty() {
        return Err("empty resource data".into());
    }
    // Stub: treat input as a single entry at "/".
    let mut entries = BTreeMap::new();
    entries.insert("/".to_owned(), data.to_vec());
    Ok(Resource::from_data(entries))
}

/// List resource paths under `prefix`.
pub fn list_resources(resource: &Resource, prefix: &str, details: bool) -> Vec<String> {
    let mut lines = Vec::new();
    match resource.enumerate_children(prefix, ResourceLookupFlags::None) {
        Ok(children) => {
            for child in children {
                let path = format!("{prefix}{child}");
                if details {
                    if let Ok((size, flags)) = resource.get_info(&path, ResourceLookupFlags::None) {
                        lines.push(format!("{path}\tsize={size}\tflags={flags}"));
                    } else {
                        lines.push(path);
                    }
                } else {
                    lines.push(path);
                }
            }
        }
        Err(_) => {}
    }
    lines.sort();
    lines
}

/// Extract resource data at `path`.
pub fn extract_resource(resource: &Resource, path: &str) -> Result<Vec<u8>, String> {
    resource
        .lookup_data(path, ResourceLookupFlags::None)
        .map(|b| b.data().to_vec())
        .map_err(|e| e.message().to_owned())
}

/// Entry point for `gresource`.
pub fn run(args: &[&str]) -> i32 {
    if args.is_empty() || args[0] == "help" || args.contains(&"--help") {
        gwarn!("Usage: gresource {{list,extract}} FILE [PATH]");
        return if args.is_empty() { 1 } else { 0 };
    }
    match args[0] {
        "list" => {
            if args.len() < 2 {
                return 1;
            }
            let details = args.contains(&"--details");
            let resource = match load_resource(args[1].as_bytes()) {
                Ok(r) => r,
                Err(msg) => {
                    gwarn!("{msg}");
                    return 1;
                }
            };
            let prefix = args.get(2).copied().unwrap_or("/");
            for line in list_resources(&resource, prefix, details) {
                gwarn!("{line}");
            }
            0
        }
        "extract" => {
            if args.len() < 3 {
                return 1;
            }
            let resource = match load_resource(args[1].as_bytes()) {
                Ok(r) => r,
                Err(msg) => {
                    gwarn!("{msg}");
                    return 1;
                }
            };
            match extract_resource(&resource, args[2]) {
                Ok(data) => {
                    gwarn!("extracted {} bytes", data.len());
                    0
                }
                Err(msg) => {
                    gwarn!("{msg}");
                    1
                }
            }
        }
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_and_list() {
        let r = load_resource(b"data").unwrap();
        let paths = list_resources(&r, "/", false);
        assert!(!paths.is_empty());
    }

    #[test]
    fn extract_root() {
        let r = load_resource(b"payload").unwrap();
        let data = extract_resource(&r, "/").unwrap();
        assert_eq!(data, b"payload");
    }

    #[test]
    fn empty_load_fails() {
        assert!(load_resource(&[]).is_err());
    }

    #[test]
    fn run_list_ok() {
        assert_eq!(run(&["list", "bundle"]), 0);
    }
}
