//! GSocks4Proxy matching `gio/gsocks4proxy.h`.
//! A SOCKS4 proxy. In this no_std port we model it with connect state.
//! Fully `no_std` compatible using `alloc`.

use spin::Mutex;

/// A SOCKS4 proxy (`GSocks4Proxy`).
pub struct Socks4Proxy {
    connected: Mutex<bool>,
}

impl Socks4Proxy {
    pub fn new() -> Self {
        Self {
            connected: Mutex::new(false),
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
    }
    pub fn supports_hostname(&self) -> bool {
        false
    }
}

impl Default for Socks4Proxy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_connect() {
        let p = Socks4Proxy::new();
        p.connect();
        assert!(p.is_connected());
        assert!(!p.supports_hostname());
    }
}
