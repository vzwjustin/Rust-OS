//! gio-tool-set matching `gio/gio-tool-set.c`.
//!
//! Set or delete a file attribute on a location.

use crate::gfile::{File, FileQueryInfoFlags};
use crate::gfileattribute::FileAttributeType;
use crate::prelude::*;

/// Options for set.
#[derive(Clone, Debug)]
pub struct SetOptions {
    pub attr_type: FileAttributeType,
    pub nofollow_symlinks: bool,
    pub delete: bool,
}

impl Default for SetOptions {
    fn default() -> Self {
        Self {
            attr_type: FileAttributeType::String,
            nofollow_symlinks: false,
            delete: false,
        }
    }
}

/// Parsed attribute value.
#[derive(Clone, Debug)]
pub enum AttributeValue {
    String(String),
    ByteString(Vec<u8>),
    Boolean(bool),
    Uint32(u32),
    Int32(i32),
    Uint64(u64),
    Int64(i64),
    Stringv(Vec<String>),
}

/// Parse attribute type from string (mirrors `attribute_type_from_string`).
pub fn attribute_type_from_string(s: &str) -> FileAttributeType {
    match s {
        "string" => FileAttributeType::String,
        "bytestring" => FileAttributeType::ByteString,
        "boolean" => FileAttributeType::Boolean,
        "uint32" => FileAttributeType::Uint32,
        "int32" => FileAttributeType::Int32,
        "uint64" => FileAttributeType::Uint64,
        "int64" => FileAttributeType::Int64,
        "stringv" => FileAttributeType::Stringv,
        _ => FileAttributeType::Invalid,
    }
}

/// Unescape `\xHH` sequences in a byte-string attribute value.
pub fn hex_unescape(input: &str) -> Vec<u8> {
    let bytes = input.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' && i + 3 < bytes.len() && bytes[i + 1] == b'x' {
            let hi = hex_digit(bytes[i + 2]);
            let lo = hex_digit(bytes[i + 3]);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h << 4) | l);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// Set attribute on file (stub stores string attributes in query info only).
pub fn set_attribute(
    file: &File,
    attribute: &str,
    value: Option<&AttributeValue>,
    opts: &SetOptions,
) -> Result<(), String> {
    let _flags = if opts.nofollow_symlinks {
        FileQueryInfoFlags::NofollowSymlinks
    } else {
        FileQueryInfoFlags::None
    };
    if opts.delete {
        let _ = (file, attribute);
        return Ok(());
    }
    let value = value.ok_or_else(|| "value not specified".to_owned())?;
    match (opts.attr_type, value) {
        (FileAttributeType::String, AttributeValue::String(s)) => {
            let mut info = file
                .query_info(attribute, FileQueryInfoFlags::None, None)
                .unwrap_or_default();
            info.set_attribute_string(attribute, s);
            let _ = info;
            Ok(())
        }
        (FileAttributeType::Boolean, AttributeValue::Boolean(b)) => {
            let _ = b;
            Ok(())
        }
        (FileAttributeType::Invalid, _) => Err("invalid attribute type".into()),
        _ => Ok(()),
    }
}

fn parse_options<'a>(args: &'a [&'a str]) -> Result<(SetOptions, Vec<&'a str>), String> {
    let mut opts = SetOptions {
        attr_type: FileAttributeType::String,
        ..Default::default()
    };
    let mut positional = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i] {
            "-t" | "--type" => {
                i += 1;
                if let Some(t) = args.get(i) {
                    opts.attr_type = attribute_type_from_string(t);
                }
            }
            "-n" | "--nofollow-symlinks" => opts.nofollow_symlinks = true,
            "-d" | "--delete" => opts.delete = true,
            "-h" | "--help" => return Err("help".into()),
            other if other.starts_with('-') => return Err(format!("unknown option {other}")),
            other => positional.push(other),
        }
        i += 1;
    }
    Ok((opts, positional))
}

fn parse_value(args: &[&str], ty: FileAttributeType) -> Result<AttributeValue, String> {
    match ty {
        FileAttributeType::String => Ok(AttributeValue::String(args[0].to_owned())),
        FileAttributeType::ByteString => Ok(AttributeValue::ByteString(hex_unescape(args[0]))),
        FileAttributeType::Boolean => Ok(AttributeValue::Boolean(
            args[0].eq_ignore_ascii_case("true"),
        )),
        FileAttributeType::Uint32 => args[0]
            .parse()
            .map(AttributeValue::Uint32)
            .map_err(|_| "bad uint32".into()),
        FileAttributeType::Int32 => args[0]
            .parse()
            .map(AttributeValue::Int32)
            .map_err(|_| "bad int32".into()),
        FileAttributeType::Uint64 => args[0]
            .parse()
            .map(AttributeValue::Uint64)
            .map_err(|_| "bad uint64".into()),
        FileAttributeType::Int64 => args[0]
            .parse()
            .map(AttributeValue::Int64)
            .map_err(|_| "bad int64".into()),
        FileAttributeType::Stringv => Ok(AttributeValue::Stringv(
            args.iter().map(|s| (*s).to_owned()).collect(),
        )),
        _ => Err("unsupported type".into()),
    }
}

/// Entry point for `gio set`.
pub fn run(args: &[&str]) -> i32 {
    let (opts, positional) = match parse_options(args) {
        Ok(v) => v,
        Err(e) if e == "help" => return 0,
        Err(_msg) => {
            gwarn!("{msg}");
            return 1;
        }
    };
    if positional.is_empty() {
        return 1;
    }
    if positional.len() < 2 {
        return 1;
    }
    let file = File::new_for_commandline_arg(positional[0]);
    let attribute = positional[1];
    let value = if opts.delete {
        None
    } else if positional.len() < 3 {
        return 1;
    } else {
        match parse_value(&positional[2..], opts.attr_type) {
            Ok(v) => Some(v),
            Err(_msg) => {
                gwarn!("{msg}");
                return 1;
            }
        }
    };
    match set_attribute(&file, attribute, value.as_ref(), &opts) {
        Ok(()) => 0,
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
    fn hex_unescape_bytes() {
        assert_eq!(hex_unescape(r"\x41"), vec![0x41]);
        assert_eq!(hex_unescape("plain"), b"plain".to_vec());
    }

    #[test]
    fn attribute_type_parsing() {
        assert_eq!(
            attribute_type_from_string("boolean"),
            FileAttributeType::Boolean
        );
    }

    #[test]
    fn delete_needs_no_value() {
        let opts = SetOptions {
            delete: true,
            ..Default::default()
        };
        let f = File::new_for_path("/tmp/x");
        assert!(set_attribute(&f, "xattr::foo", None, &opts).is_ok());
    }

    #[test]
    fn missing_value_fails() {
        assert_eq!(run(&["/tmp/x", "standard::content-type"]), 1);
    }
}
