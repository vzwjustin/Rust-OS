//! GPortalSupport matching `gio/gportalsupport.h`.
//! Portal support detection. In this no_std port we model it with
//! a static availability flag.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// Portal support (`GPortalSupport`).
pub struct PortalSupport {
    available: Mutex<bool>,
    desktop: Mutex<String>,
}

impl PortalSupport {
    pub fn new() -> Self {
        Self {
            available: Mutex::new(false),
            desktop: Mutex::new(String::new()),
        }
    }

    pub fn is_available(&self) -> bool {
        *self.available.lock()
    }
    pub fn set_available(&self, available: bool) {
        *self.available.lock() = available;
    }

    pub fn get_desktop(&self) -> String {
        self.desktop.lock().clone()
    }
    pub fn set_desktop(&self, desktop: &str) {
        *self.desktop.lock() = desktop.to_string();
    }
}

impl Default for PortalSupport {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let p = PortalSupport::new();
        assert!(!p.is_available());
    }

    #[test]
    fn test_set_available() {
        let p = PortalSupport::new();
        p.set_available(true);
        p.set_desktop("gnome");
        assert!(p.is_available());
        assert_eq!(p.get_desktop(), "gnome");
    }
}
