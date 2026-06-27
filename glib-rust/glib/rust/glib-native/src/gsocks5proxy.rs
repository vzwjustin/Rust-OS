//! GSocks5Proxy matching `gio/gsocks5proxy.h`.
//! A SOCKS5 proxy. In this no_std port we model it with connect state
//! and authentication support.
//! Fully `no_std` compatible using `alloc`.

use spin::Mutex;

/// A SOCKS5 proxy (`GSocks5Proxy`).
pub struct Socks5Proxy {
    connected: Mutex<bool>,
    authenticated: Mutex<bool>,
}

impl Socks5Proxy {
    pub fn new() -> Self {
        Self {
            connected: Mutex::new(false),
            authenticated: Mutex::new(false),
        }
    }

    pub fn is_connected(&self) -> bool {
        *self.connected.lock()
    }
    pub fn connect(&self) {
        *self.connected.lock() = true;
    }
    pub fn disconnect(&self) {
        *self.connected.lock() = false;
        *self.authenticated.lock() = false;
    }
    pub fn supports_hostname(&self) -> bool {
        true
    }
    pub fn is_authenticated(&self) -> bool {
        *self.authenticated.lock()
    }
    pub fn authenticate(&self) {
        *self.authenticated.lock() = true;
    }
}

impl Default for Socks5Proxy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_connect_auth() {
        let p = Socks5Proxy::new();
        p.connect();
        p.authenticate();
        assert!(p.is_connected());
        assert!(p.is_authenticated());
        assert!(p.supports_hostname());
    }
}
