//! Character set conversion and URI helpers matching `gconvert.h` / `gconvert.c`.
//!
//! Phase 6 covers the `GConvertError` type, URI helpers (`g_filename_to_uri`,
//! `g_filename_from_uri`, `g_uri_list_extract_uris`), and filename display helpers.
//!
//! Actual charset conversion (`g_convert`, `g_iconv`) requires iconv or an
//! equivalent and is deferred to a platform abstraction layer.

use crate::fileutils::{path_get_basename, path_is_absolute};
use crate::prelude::*;
use crate::quark::{quark_from_static_string, Quark};

/// Error codes for character set conversion (`GConvertError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ConvertError {
    /// Conversion between the requested character sets is not supported.
    NoConversion = 0,
    /// Invalid byte sequence in conversion input.
    IllegalSequence,
    /// Conversion failed for some reason.
    Failed,
    /// Partial character sequence at end of input.
    PartialInput,
    /// URI is invalid.
    BadUri,
    /// Pathname is not an absolute path.
    NotAbsolutePath,
    /// No memory available.
    NoMemory,
    /// Embedded NUL character in conversion output.
    EmbeddedNul,
}

/// Returns the quark for `G_CONVERT_ERROR`.
pub fn convert_error_quark() -> Quark {
    quark_from_static_string(Some("g-convert-error-quark"))
}

/// Converts a filename to a `file://` URI (`g_filename_to_uri`).
///
/// Returns `Err(ConvertError::NotAbsolutePath)` if `filename` is not absolute.
pub fn filename_to_uri(filename: &str, hostname: Option<&str>) -> Result<String, ConvertError> {
    if !path_is_absolute(filename) {
        return Err(ConvertError::NotAbsolutePath);
    }

    let mut uri = String::from("file://");
    if let Some(host) = hostname {
        if !host.is_empty() {
            uri.push_str(host);
        }
    }
    uri.push_str(&uri_escape(filename));
    Ok(uri)
}

/// Converts a `file://` URI back to a filename (`g_filename_from_uri`).
///
/// Returns `(filename, hostname)` on success.
pub fn filename_from_uri(uri: &str) -> Result<(String, Option<String>), ConvertError> {
    let rest = uri.strip_prefix("file://").ok_or(ConvertError::BadUri)?;

    // Find the start of the path (first `/` after the host portion)
    let (host, path) = if let Some(slash_pos) = rest.find('/') {
        let host = &rest[..slash_pos];
        let path = &rest[slash_pos..];
        (host, path)
    } else {
        // No path component
        return Err(ConvertError::BadUri);
    };

    let hostname = if host.is_empty() {
        None
    } else {
        Some(host.to_owned())
    };

    let filename = uri_unescape(path);
    if filename.is_empty() {
        return Err(ConvertError::BadUri);
    }

    Ok((filename, hostname))
}

/// Escapes a string for inclusion in a URI.
///
/// Escapes all characters except alphanumerics and `-_.~/`.
fn uri_escape(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                result.push(byte as char);
            }
            _ => {
                result.push('%');
                result.push_str(&format!("{:02X}", byte));
            }
        }
    }
    result
}

/// Unescapes percent-encoded sequences in a URI.
fn uri_unescape(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = hex_val(bytes[i + 1]);
            let lo = hex_val(bytes[i + 2]);
            if let (Some(h), Some(l)) = (hi, lo) {
                result.push((h << 4) | l);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

fn hex_val(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Extracts URIs from a URI list (`g_uri_list_extract_uris`).
///
/// A URI list is a text file where each line is either a URI (starting with
/// `file://`, `http://`, etc.) or a comment (starting with `#`). Blank lines
/// are skipped.
pub fn uri_list_extract_uris(uri_list: &str) -> Vec<String> {
    let mut result = Vec::new();
    for line in uri_list.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        result.push(trimmed.to_owned());
    }
    result
}

/// Returns a display name for `filename` (`g_filename_display_name`).
///
/// If the filename is valid UTF-8, it is returned as-is. Otherwise, each
/// invalid byte is replaced with `\xNN` escape sequences.
pub fn filename_display_name(filename: &str) -> String {
    if filename.is_ascii() {
        return filename.to_owned();
    }
    match core::str::from_utf8(filename.as_bytes()) {
        Ok(_) => filename.to_owned(),
        Err(_) => {
            let mut result = String::new();
            for byte in filename.bytes() {
                if byte.is_ascii() && byte >= 0x20 {
                    result.push(byte as char);
                } else {
                    result.push_str(&format!("\\x{:02x}", byte));
                }
            }
            result
        }
    }
}

/// Returns a display basename for `filename` (`g_filename_display_basename`).
///
/// Equivalent to `filename_display_name(path_get_basename(filename))`.
pub fn filename_display_basename(filename: &str) -> String {
    filename_display_name(&path_get_basename(filename))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_to_uri_simple() {
        assert_eq!(
            filename_to_uri("/home/user/file.txt", None).unwrap(),
            "file:///home/user/file.txt"
        );
        assert_eq!(
            filename_to_uri("/home/user/file.txt", Some("localhost")).unwrap(),
            "file://localhost/home/user/file.txt"
        );
    }

    #[test]
    fn filename_to_uri_rejects_relative() {
        assert_eq!(
            filename_to_uri("relative/path", None),
            Err(ConvertError::NotAbsolutePath)
        );
    }

    #[test]
    fn filename_from_uri_simple() {
        let (filename, host) = filename_from_uri("file:///home/user/file.txt").unwrap();
        assert_eq!(filename, "/home/user/file.txt");
        assert_eq!(host, None);
    }

    #[test]
    fn filename_from_uri_with_host() {
        let (filename, host) = filename_from_uri("file://server/share/file.txt").unwrap();
        assert_eq!(filename, "/share/file.txt");
        assert_eq!(host, Some("server".to_owned()));
    }

    #[test]
    fn filename_from_uri_rejects_non_file() {
        assert_eq!(
            filename_from_uri("http://example.com/"),
            Err(ConvertError::BadUri)
        );
    }

    #[test]
    fn uri_list_extracts() {
        let list = "# Comment\nfile:///a\n\nhttp://example.com\n";
        let uris = uri_list_extract_uris(list);
        assert_eq!(uris, vec!["file:///a", "http://example.com"]);
    }

    #[test]
    fn display_name_ascii() {
        assert_eq!(filename_display_name("/usr/bin/test"), "/usr/bin/test");
    }

    #[test]
    fn display_basename() {
        assert_eq!(filename_display_basename("/usr/bin/test"), "test");
    }

    #[test]
    fn convert_error_quark_is_nonzero() {
        assert_ne!(convert_error_quark(), 0);
    }
}
