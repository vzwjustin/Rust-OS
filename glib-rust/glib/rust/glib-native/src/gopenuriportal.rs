//! GOpenURIPortal matching `gio/gopenuriportal.h`.
//! Portal-based URI opener. In this no_std port we model it with
//! a record of opened URIs.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// An open-URI portal (`GOpenURIPortal`).
pub struct OpenURIPortal {
    opened: Mutex<Vec<String>>,
}

impl OpenURIPortal {
    pub fn new() -> Self {
        Self {
            opened: Mutex::new(Vec::new()),
        }
    }

    pub fn open_uri(&self, uri: &str) -> bool {
        self.opened.lock().push(uri.to_string());
        true
    }

    pub fn open_uri_async(&self, uri: &str) -> bool {
        self.open_uri(uri)
    }

    pub fn get_opened_uris(&self) -> Vec<String> {
        self.opened.lock().clone()
    }

    pub fn is_available(&self) -> bool {
        true
    }
}

impl Default for OpenURIPortal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_uri() {
        let p = OpenURIPortal::new();
        p.open_uri("https://example.com");
        p.open_uri("file:///tmp/test.txt");
        assert_eq!(p.get_opened_uris().len(), 2);
    }
}
