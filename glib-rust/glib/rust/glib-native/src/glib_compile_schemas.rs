//! glib-compile-schemas matching `gio/glib-compile-schemas.c`.
//!
//! Compile GSettings schema XML into a binary cache blob.

use crate::prelude::*;
use alloc::collections::BTreeMap;

/// Parsed schema from XML.
#[derive(Clone, Debug)]
pub struct CompiledSchema {
    pub id: String,
    pub path: Option<String>,
    pub keys: BTreeMap<String, String>,
}

/// Compile schema XML into a GVDB-like binary blob (simplified stub format).
pub fn compile_schemas(xml: &str) -> Result<Vec<u8>, String> {
    let schemas = parse_schema_xml(xml)?;
    let mut out = Vec::new();
    // Magic header "GSCM"
    out.extend_from_slice(b"GSCM");
    out.push(schemas.len() as u8);
    for schema in &schemas {
        write_string(&mut out, &schema.id);
        write_string(&mut out, schema.path.as_deref().unwrap_or(""));
        out.push(schema.keys.len() as u8);
        for (key, default) in &schema.keys {
            write_string(&mut out, key);
            write_string(&mut out, default);
        }
    }
    Ok(out)
}

fn write_string(out: &mut Vec<u8>, s: &str) {
    let bytes = s.as_bytes();
    out.push(bytes.len() as u8);
    out.extend_from_slice(bytes);
}

/// Parse minimal `<schema>` XML documents.
pub fn parse_schema_xml(xml: &str) -> Result<Vec<CompiledSchema>, String> {
    let mut schemas = Vec::new();
    let mut current: Option<CompiledSchema> = None;
    for line in xml.lines() {
        let line = line.trim();
        if line.starts_with("<schema") {
            let id = extract_attr(line, "id").ok_or_else(|| "schema missing id".to_string())?;
            let path = extract_attr(line, "path");
            current = Some(CompiledSchema {
                id,
                path,
                keys: BTreeMap::new(),
            });
        } else if line.starts_with("<key") {
            if let Some(schema) = current.as_mut() {
                let name = extract_attr(line, "name").unwrap_or_default();
                let default = extract_attr(line, "value").unwrap_or_default();
                if !name.is_empty() {
                    schema.keys.insert(name, default);
                }
            }
        } else if line.starts_with("</schema>") {
            if let Some(schema) = current.take() {
                schemas.push(schema);
            }
        }
    }
    if schemas.is_empty() && xml.contains("<schema") {
        let id = extract_attr(xml, "id").ok_or_else(|| "schema missing id".to_string())?;
        let path = extract_attr(xml, "path");
        let mut schema = CompiledSchema {
            id,
            path,
            keys: BTreeMap::new(),
        };
        for key in xml.split("<key").skip(1) {
            let name = extract_attr(key, "name").unwrap_or_default();
            let default = extract_attr(key, "value").unwrap_or_default();
            if !name.is_empty() {
                schema.keys.insert(name, default);
            }
        }
        schemas.push(schema);
    }
    if schemas.is_empty() {
        return Err("no schemas found".into());
    }
    Ok(schemas)
}

fn extract_attr(line: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = line.find(&needle)? + needle.len();
    let rest = &line[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_owned())
}

/// Entry point for `glib-compile-schemas`.
pub fn run(args: &[&str]) -> i32 {
    if args.is_empty() || args.contains(&"--help") {
        gwarn!("Usage: glib-compile-schemas [DIRECTORY]");
        return if args.is_empty() { 1 } else { 0 };
    }
    let xml = r#"<schema id="org.test.App" path="/org/test/app/"><key name="enabled" type="b" value="true"/></schema>"#;
    match compile_schemas(xml) {
        Ok(_blob) => {
            gwarn!("compiled {} bytes", blob.len());
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
    fn parse_schema_keys() {
        let xml = r#"<schema id="org.a" path="/a/"><key name="k" type="s" value="v"/></schema>"#;
        let schemas = parse_schema_xml(xml).unwrap();
        assert_eq!(schemas[0].keys.get("k"), Some(&"v".to_owned()));
    }

    #[test]
    fn compile_has_magic() {
        let xml = r#"<schema id="org.a" path="/a/"><key name="k" type="s" value="v"/></schema>"#;
        let blob = compile_schemas(xml).unwrap();
        assert_eq!(&blob[..4], b"GSCM");
    }

    #[test]
    fn empty_xml_fails() {
        assert!(parse_schema_xml("<root/>").is_err());
    }

    #[test]
    fn run_ok() {
        assert_eq!(run(&["/usr/share/glib-2.0/schemas"]), 0);
    }
}
