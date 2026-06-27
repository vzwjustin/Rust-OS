//! GTlsPassword matching `gio/gtlspassword.h`.
//!
//! A password used for TLS operations. In this no_std port we model
//! it with password value, description, and flags.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Flags for `GTlsPassword`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TlsPasswordFlags(pub u32);

impl TlsPasswordFlags {
    pub const NONE: Self = Self(0);
    pub const RETRY: Self = Self(1 << 0);
    pub const MANY_TRIES: Self = Self(1 << 1);
    pub const FINAL_TRY: Self = Self(1 << 2);
    pub const PKCS11_USER: Self = Self(1 << 3);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// A TLS password (`GTlsPassword`).
pub struct TlsPassword {
    value: Mutex<Vec<u8>>,
    description: Mutex<String>,
    flags: Mutex<TlsPasswordFlags>,
    warning: Mutex<Option<String>>,
}

impl TlsPassword {
    /// Creates a new TLS password.
    pub fn new() -> Self {
        Self {
            value: Mutex::new(Vec::new()),
            description: Mutex::new(String::new()),
            flags: Mutex::new(TlsPasswordFlags::NONE),
            warning: Mutex::new(None),
        }
    }

    /// Creates a new TLS password with description and flags.
    pub fn new_with(flags: TlsPasswordFlags, description: &str) -> Self {
        Self {
            value: Mutex::new(Vec::new()),
            description: Mutex::new(description.to_string()),
            flags: Mutex::new(flags),
            warning: Mutex::new(None),
        }
    }

    /// Gets the password value as bytes.
    pub fn get_value(&self) -> Vec<u8> {
        self.value.lock().clone()
    }

    /// Sets the password value.
    pub fn set_value(&self, value: &[u8]) {
        *self.value.lock() = value.to_vec();
    }

    /// Gets the password as a UTF-8 string.
    pub fn get_password(&self) -> String {
        String::from_utf8_lossy(&self.value.lock()).to_string()
    }

    /// Sets the password from a string.
    pub fn set_password(&self, password: &str) {
        *self.value.lock() = password.as_bytes().to_vec();
    }

    /// Gets the description.
    pub fn get_description(&self) -> String {
        self.description.lock().clone()
    }

    /// Sets the description.
    pub fn set_description(&self, description: &str) {
        *self.description.lock() = description.to_string();
    }

    /// Gets the flags.
    pub fn get_flags(&self) -> TlsPasswordFlags {
        *self.flags.lock()
    }

    /// Sets the flags.
    pub fn set_flags(&self, flags: TlsPasswordFlags) {
        *self.flags.lock() = flags;
    }

    /// Gets the warning, if any.
    pub fn get_warning(&self) -> Option<String> {
        self.warning.lock().clone()
    }

    /// Sets a warning.
    pub fn set_warning(&self, warning: &str) {
        *self.warning.lock() = Some(warning.to_string());
    }
}

impl Default for TlsPassword {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let pw = TlsPassword::new();
        assert!(pw.get_value().is_empty());
        assert!(pw.get_description().is_empty());
        assert_eq!(pw.get_flags(), TlsPasswordFlags::NONE);
    }

    #[test]
    fn test_new_with() {
        let pw = TlsPassword::new_with(TlsPasswordFlags::RETRY, "Enter password");
        assert_eq!(pw.get_description(), "Enter password");
        assert!(pw.get_flags().contains(TlsPasswordFlags::RETRY));
    }

    #[test]
    fn test_set_get_password() {
        let pw = TlsPassword::new();
        pw.set_password("secret123");
        assert_eq!(pw.get_password(), "secret123");
        assert_eq!(pw.get_value(), b"secret123".to_vec());
    }

    #[test]
    fn test_warning() {
        let pw = TlsPassword::new();
        pw.set_warning("Invalid password");
        assert_eq!(pw.get_warning(), Some("Invalid password".to_string()));
    }
}
