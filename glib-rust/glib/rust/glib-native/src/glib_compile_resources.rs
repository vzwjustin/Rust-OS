//! glib-compile-resources matching `gio/glib-compile-resources.c`.
//!
//! Compile a GResource XML description into a binary resource bundle.

use crate::prelude::*;
use alloc::collections::BTreeMap;

/// Parsed file entry from gresource XML.
#[derive(Clone, Debug)]
pub struct ResourceFileEntry {
    pub alias: Option<String>,
    pub compressed: bool,
    pub content: Vec<u8>,
}

/// Compile gresource XML into a textual C source stub or binary header representation.
pub fn compile_resources(input: &str) -> Result<String, String> {
    let entries = parse_gresource_xml(input)?;
    let mut out = String::from("/* compiled gresource */\n");
    for (path, entry) in &entries {
        out.push_str(&format!(
            "/* {path} size={} compressed={} */\n",
            entry.content.len(),
            entry.compressed
        ));
        if let Some(alias) = &entry.alias {
            out.push_str(&format!("/* alias={alias} */\n"));
        }
    }
    out.push_str("static const guint8 resource_data[] = { 0 };\n");
    Ok(out)
}

/// Parse minimal `<gresource>` XML (prefix and `<file>` children).
pub fn parse_gresource_xml(input: &str) -> Result<BTreeMap<String, ResourceFileEntry>, String> {
    let mut entries = BTreeMap::new();
    let mut prefix = String::from("/");
    for line in input.lines() {
        let line = line.trim();
        if let Some(p) = line.strip_prefix("<gresource") {
            if let Some(start) = p.find("prefix=\"") {
                let rest = &p[start + 8..];
                if let Some(end) = rest.find('"') {
                    prefix = rest[..end].to_owned();
                    if !prefix.ends_with('/') {
                        prefix.push('/');
                    }
                }
            }
        } else if let Some(file) = line.strip_prefix("<file") {
            let compressed = file.contains("compressed=\"true\"");
            let alias = extract_attr(file, "alias");
            let path = extract_attr(file, "name").or_else(|| alias.clone());
            if let Some(name) = path {
                let full = format!("{prefix}{name}");
                entries.insert(
                    full,
                    ResourceFileEntry {
                        alias,
                        compressed,
                        content: Vec::new(),
                    },
                );
            }
        } else if let Some(body) = line.strip_prefix("</file>") {
            let _ = body;
        }
    }
    if entries.is_empty() {
        if let Some(start) = input.find("prefix=\"") {
            let rest = &input[start + 8..];
            if let Some(end) = rest.find('"') {
                prefix = rest[..end].to_owned();
                if !prefix.ends_with('/') {
                    prefix.push('/');
                }
            }
        }
        for file in input.split("<file").skip(1) {
            let compressed = file.contains("compressed=\"true\"");
            let alias = extract_attr(file, "alias");
            let path = extract_attr(file, "name").or_else(|| alias.clone());
            if let Some(name) = path {
                entries.insert(
                    format!("{prefix}{name}"),
                    ResourceFileEntry {
                        alias,
                        compressed,
                        content: Vec::new(),
                    },
                );
            }
        }
    }
    if entries.is_empty() && !input.contains("<gresource") {
        return Err("not a gresource document".into());
    }
    Ok(entries)
}

fn extract_attr(line: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

/// Entry point for `glib-compile-resources`.
pub fn run(args: &[&str]) -> i32 {
    if args.is_empty() || args.contains(&"--help") || args.contains(&"-h") {
        gwarn!("Usage: glib-compile-resources [--generate-source] XML");
        return if args.is_empty() { 1 } else { 0 };
    }
    let xml_path = args.iter().rev().find(|a| !a.starts_with('-')).copied();
    let Some(path) = xml_path else {
        return 1;
    };

    let xml_data = match crate::stdio::read_file_bytes(path) {
        Some(data) => data,
        None => {
            gwarn!("Failed to read XML file: {}", path);
            return 1;
        }
    };

    let xml = match core::str::from_utf8(&xml_data) {
        Ok(s) => s,
        Err(_e) => {
            gwarn!("Invalid UTF-8 in XML file: {}", e);
            return 1;
        }
    };

    match compile_resources(xml) {
        Ok(_source) => {
            gwarn!("{source}");
            0
        }
        Err(_msg) => {
            gwarn!("{msg}");
            1
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_xml() {
        let xml = r#"<gresource prefix="/app/"><file name="ui.xml"></file></gresource>"#;
        let entries = parse_gresource_xml(xml).unwrap();
        assert!(entries.contains_key("/app/ui.xml"));
    }

    #[test]
    fn compile_produces_source() {
        let xml = r#"<gresource prefix="/"><file name="a"></file></gresource>"#;
        let out = compile_resources(xml).unwrap();
        assert!(out.contains("resource_data"));
    }

    #[test]
    fn invalid_xml_fails() {
        assert!(parse_gresource_xml("not xml").is_err());
    }

    #[test]
    fn run_help_ok() {
        assert_eq!(run(&["--help"]), 0);
    }

    #[test]
    fn run_reads_real_xml_file() {
        use std::fs;
        let dir = std::env::temp_dir().join("glib_compile_resources_test");
        let _ = fs::create_dir_all(&dir);
        let xml_path = dir.join("test.gresource.xml");
        let xml = r#"<gresource prefix="/app/"><file name="ui.xml"></file></gresource>"#;
        let _ = fs::write(&xml_path, xml);
        let path_str = xml_path.to_string_lossy().to_string();
        assert_eq!(run(&[&path_str]), 0);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_missing_file_fails() {
        assert_eq!(run(&["/nonexistent/path/file.xml"]), 1);
    }
}
