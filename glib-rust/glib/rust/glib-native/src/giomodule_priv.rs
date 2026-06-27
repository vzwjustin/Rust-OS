//! giomodule_priv matching `gio/giomodule-priv.c`.
//!
//! Provides the `_g_io_module_extract_name` function which extracts the
//! plugin name from a module filename. It removes optional "lib" or "libgio"
//! prefix, replaces '-' with '_', and removes everything after the first dot.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};

/// Extract the plugin name from a module filename.
///
/// Removes optional "lib" or "libgio" prefix, replaces '-' with '_',
/// and removes everything after the first dot.
///
/// Examples:
/// - "libgiognutls.so" → "gnutls"
/// - "libgioopenssl.so" → "openssl"
/// - "libgnutls.so" → "gnutls"
/// - "gnutls.so" → "gnutls"
/// - "gnutls" → "gnutls"
///
/// Mirrors `_g_io_module_extract_name`.
pub fn extract_name(filename: &str) -> String {
    // Get basename (everything after last '/' or '\\')
    let bname = filename
        .rsplit(|c| c == '/' || c == '\\')
        .next()
        .unwrap_or(filename);

    // Replace '-' with '_'
    let bname: String = bname
        .chars()
        .map(|c| if c == '-' { '_' } else { c })
        .collect();

    // Determine prefix length
    let prefix_len = if bname.starts_with("libgio") {
        6
    } else if bname.starts_with("lib") || bname.starts_with("gio") {
        3
    } else {
        0
    };

    // Find first dot
    let dot_pos = bname.find('.');
    let end = dot_pos.unwrap_or(bname.len());
    let name_end = if end >= prefix_len { end } else { bname.len() };

    bname[prefix_len..name_end].to_string()
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_libgio_prefix() {
        assert_eq!(extract_name("libgiognutls.so"), "gnutls");
        assert_eq!(extract_name("libgioopenssl.so"), "openssl");
    }

    #[test]
    fn test_lib_prefix() {
        assert_eq!(extract_name("libgnutls.so"), "gnutls");
        assert_eq!(extract_name("libfoo.so"), "foo");
    }

    #[test]
    fn test_gio_prefix() {
        assert_eq!(extract_name("giognutls.so"), "gnutls");
        assert_eq!(extract_name("giofoo.so"), "foo");
    }

    #[test]
    fn test_no_prefix() {
        assert_eq!(extract_name("gnutls.so"), "gnutls");
        assert_eq!(extract_name("gnutls"), "gnutls");
    }

    #[test]
    fn test_with_path() {
        assert_eq!(
            extract_name("/usr/lib/gio/modules/libgiognutls.so"),
            "gnutls"
        );
        assert_eq!(
            extract_name("C:\\gio\\modules\\libgioopenssl.dll"),
            "openssl"
        );
    }

    #[test]
    fn test_hyphen_replacement() {
        assert_eq!(extract_name("libgiofoo-bar.so"), "foo_bar");
        assert_eq!(extract_name("libfoo-bar.so"), "foo_bar");
    }

    #[test]
    fn test_no_dot() {
        assert_eq!(extract_name("libgiognutls"), "gnutls");
        assert_eq!(extract_name("gnutls"), "gnutls");
    }

    #[test]
    fn test_dll() {
        assert_eq!(extract_name("giognutls.dll"), "gnutls");
        assert_eq!(extract_name("gnutls.dll"), "gnutls");
    }
}
