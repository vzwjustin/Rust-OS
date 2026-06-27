//! GCredentials matching `gio/gcredentials.h`.
//!
//! `GCredentials` carries Unix process credentials (PID, UID, GID) exchanged
//! over Unix domain sockets. We port it as a plain struct with optional fields
//! (None when unknown or on non-Unix targets).
//!
//! Fully `no_std` compatible.

use crate::error::Error;
use crate::quark::{quark_from_string, Quark};

fn credentials_quark() -> Quark {
    quark_from_string(Some("g-credentials-error-quark"))
}

/// Error codes for credential operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialsError {
    /// Credentials not supported on this OS.
    NotSupported,
    /// The native credentials type is wrong.
    WrongNativeType,
}

impl CredentialsError {
    fn to_code(self) -> i32 {
        self as i32
    }
}

/// Process credentials (`GCredentials`).
///
/// Holds the PID, UID, and GID of a peer process. All fields are `Option`
/// because credentials may be unavailable (e.g. non-Unix platforms or
/// ancillary data not received yet).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Credentials {
    pid: Option<i32>,
    uid: Option<u32>,
    gid: Option<u32>,
}

impl Credentials {
    /// Creates credentials with all fields unknown.
    ///
    /// Mirrors `g_credentials_new`.
    pub fn new() -> Self {
        Self {
            pid: None,
            uid: None,
            gid: None,
        }
    }

    /// Creates credentials with explicit PID/UID/GID.
    pub fn new_with(pid: i32, uid: u32, gid: u32) -> Self {
        Self {
            pid: Some(pid),
            uid: Some(uid),
            gid: Some(gid),
        }
    }

    /// Returns the peer PID, or `None` if unknown.
    ///
    /// Mirrors `g_credentials_get_unix_pid`.
    pub fn get_unix_pid(&self) -> Result<i32, Error> {
        self.pid.ok_or_else(|| {
            Error::new(
                credentials_quark(),
                CredentialsError::NotSupported.to_code(),
                "PID not available",
            )
        })
    }

    /// Returns the peer UID, or error if unknown.
    ///
    /// Mirrors `g_credentials_get_unix_user`.
    pub fn get_unix_user(&self) -> Result<u32, Error> {
        self.uid.ok_or_else(|| {
            Error::new(
                credentials_quark(),
                CredentialsError::NotSupported.to_code(),
                "UID not available",
            )
        })
    }

    /// Sets the UID.
    ///
    /// Mirrors `g_credentials_set_unix_user`.
    pub fn set_unix_user(&mut self, uid: u32) -> Result<(), Error> {
        self.uid = Some(uid);
        Ok(())
    }

    /// Returns true if this and `other` represent the same process.
    ///
    /// Mirrors `g_credentials_is_same_user`.
    pub fn is_same_user(&self, other: &Credentials) -> Result<bool, Error> {
        match (self.uid, other.uid) {
            (Some(a), Some(b)) => Ok(a == b),
            _ => Err(Error::new(
                credentials_quark(),
                CredentialsError::NotSupported.to_code(),
                "UID not available",
            )),
        }
    }

    /// Returns a human-readable string representation.
    ///
    /// Mirrors `g_credentials_to_string`.
    pub fn to_string(&self) -> alloc::string::String {
        alloc::format!(
            "credentials: pid={}, uid={}, gid={}",
            self.pid.map_or("n/a".into(), |v| alloc::format!("{}", v)),
            self.uid.map_or("n/a".into(), |v| alloc::format!("{}", v)),
            self.gid.map_or("n/a".into(), |v| alloc::format!("{}", v)),
        )
    }
}

impl Default for Credentials {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_all_unknown() {
        let c = Credentials::new();
        assert!(c.get_unix_pid().is_err());
        assert!(c.get_unix_user().is_err());
    }

    #[test]
    fn test_new_with() {
        let c = Credentials::new_with(1234, 1000, 1000);
        assert_eq!(c.get_unix_pid().unwrap(), 1234);
        assert_eq!(c.get_unix_user().unwrap(), 1000);
    }

    #[test]
    fn test_set_unix_user() {
        let mut c = Credentials::new();
        c.set_unix_user(501).unwrap();
        assert_eq!(c.get_unix_user().unwrap(), 501);
    }

    #[test]
    fn test_is_same_user_true() {
        let a = Credentials::new_with(1, 1000, 1000);
        let b = Credentials::new_with(2, 1000, 100);
        assert!(a.is_same_user(&b).unwrap());
    }

    #[test]
    fn test_is_same_user_false() {
        let a = Credentials::new_with(1, 1000, 1000);
        let b = Credentials::new_with(1, 501, 501);
        assert!(!a.is_same_user(&b).unwrap());
    }

    #[test]
    fn test_is_same_user_unknown() {
        let a = Credentials::new();
        let b = Credentials::new_with(1, 1000, 1000);
        assert!(a.is_same_user(&b).is_err());
    }

    #[test]
    fn test_to_string() {
        let c = Credentials::new_with(42, 1000, 1000);
        let s = c.to_string();
        assert!(s.contains("pid=42"));
        assert!(s.contains("uid=1000"));
    }

    #[test]
    fn test_to_string_unknown() {
        let c = Credentials::new();
        let s = c.to_string();
        assert!(s.contains("n/a"));
    }

    #[test]
    fn test_default() {
        let c = Credentials::default();
        assert!(c.get_unix_pid().is_err());
    }

    #[test]
    fn test_clone_eq() {
        let a = Credentials::new_with(1, 100, 100);
        let b = a.clone();
        assert_eq!(a, b);
    }
}
