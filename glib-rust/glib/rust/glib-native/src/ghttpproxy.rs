//! GHttpProxy matching `gio/ghttpproxy.h`.
//! An HTTP proxy implementation. In this no_std port we model it
//! with a proxy URI and connect state.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// An HTTP proxy (`GHttpProxy`).
pub struct HttpProxy {
    uri: Mutex<String>,
    connected: Mutex<bool>,
}

impl HttpProxy {
    pub fn new(uri: &str) -> Self {
        Self {
            uri: Mutex::new(uri.to_string()),
            connected: Mutex::new(false),
        }
    }

    pub fn get_uri(&self) -> String {
        self.uri.lock().clone()
    }
    pub fn is_connected(&self) -> bool {
        *self.connected.lock()
    }
    pub fn connect(&self) {
        *self.connected.lock() = true;
    }
    pub fn disconnect(&self) {
        *self.connected.lock() = false;
    }

    pub fn supports_hostname(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let p = HttpProxy::new("http://proxy:8080");
        assert_eq!(p.get_uri(), "http://proxy:8080");
        assert!(!p.is_connected());
    }

    #[test]
    fn test_connect() {
        let p = HttpProxy::new("http://proxy:8080");
        p.connect();
        assert!(p.is_connected());
        assert!(p.supports_hostname());
    }
}
