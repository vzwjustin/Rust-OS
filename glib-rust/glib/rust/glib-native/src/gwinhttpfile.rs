//! `gwinhttpfile` matching `gio/win32/gwinhttpfile.h`.
//!
//! WinHTTP file: represents a file accessed via HTTP/HTTPS.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::winhttp::UrlComponents;
use alloc::string::{String, ToString};
use spin::Mutex;

/// WinHTTP file (mirrors `GWinHttpFile`).
pub struct WinHttpFile {
    url: Mutex<UrlComponents>,
    uri: Mutex<String>,
}

impl WinHttpFile {
    /// Creates a new WinHTTP file from a URI
    /// (mirrors `_g_winhttp_file_new`).
    pub fn new(uri: &str) -> Self {
        let url = crate::winhttp::crack_url(uri).unwrap_or_default();
        Self {
            url: Mutex::new(url),
            uri: Mutex::new(uri.to_string()),
        }
    }

    /// Returns the URI.
    pub fn uri(&self) -> String {
        self.uri.lock().clone()
    }

    /// Returns the parsed URL components.
    pub fn url_components(&self) -> UrlComponents {
        self.url.lock().clone()
    }

    /// Returns the scheme.
    pub fn scheme(&self) -> String {
        self.url.lock().scheme.clone()
    }

    /// Returns the host name.
    pub fn host_name(&self) -> String {
        self.url.lock().host_name.clone()
    }

    /// Returns the URL path.
    pub fn url_path(&self) -> String {
        self.url.lock().url_path.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let f = WinHttpFile::new("http://example.com/path");
        assert_eq!(f.uri(), "http://example.com/path");
        assert_eq!(f.scheme(), "http");
        assert_eq!(f.host_name(), "example.com");
        assert_eq!(f.url_path(), "/path");
    }

    #[test]
    fn test_https() {
        let f = WinHttpFile::new("https://secure.example.com/api");
        assert_eq!(f.scheme(), "https");
        assert_eq!(f.host_name(), "secure.example.com");
    }
}
