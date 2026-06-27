//! GSocks4AProxy matching `gio/gsocks4aproxy.h`.
//! A SOCKS4A proxy. In this no_std port we model it with connect state.
//! Fully `no_std` compatible using `alloc`.

use spin::Mutex;

/// A SOCKS4A proxy (`GSocks4AProxy`).
pub struct Socks4AProxy {
    connected: Mutex<bool>,
}

impl Socks4AProxy {
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
        true
    }
}

impl Default for Socks4AProxy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_connect() {
        let p = Socks4AProxy::new();
        p.connect();
        assert!(p.is_connected());
        assert!(p.supports_hostname());
    }
}
