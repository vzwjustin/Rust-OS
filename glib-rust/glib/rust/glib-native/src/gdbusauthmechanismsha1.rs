//! GDBusAuthMechanismSha1 matching `gio/gdbusauthmechanismsha1.h`.
//! SHA1-based D-Bus auth mechanism. In this no_std port we model it
//! with a simple challenge/response using a stored key.
//! Fully `no_std` compatible using `alloc`.

use crate::gdbusauthmechanism::{AuthMechanismState, DBusAuthMechanism};
use alloc::string::String;

/// SHA1 auth mechanism (`GDBusAuthMechanismSha1`).
pub struct DBusAuthMechanismSha1 {
    secret: String,
    accepted: core::sync::atomic::AtomicBool,
}

impl DBusAuthMechanismSha1 {
    pub fn new(secret: &str) -> Self {
        Self {
            secret: String::from(secret),
            accepted: core::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl DBusAuthMechanism for DBusAuthMechanismSha1 {
    fn name(&self) -> &str {
        "DBUS_COOKIE_SHA1"
    }
    fn state(&self) -> AuthMechanismState {
        if self.accepted.load(core::sync::atomic::Ordering::SeqCst) {
            AuthMechanismState::Accepted
        } else {
            AuthMechanismState::Initial
        }
    }
    fn initiate(&self) -> Option<String> {
        Some(String::from("challenge"))
    }
    fn process_data(&self, data: &str) -> AuthMechanismState {
        if data == self.secret {
            self.accepted
                .store(true, core::sync::atomic::Ordering::SeqCst);
            AuthMechanismState::Accepted
        } else {
            AuthMechanismState::Rejected
        }
    }
    fn is_supported(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha1_accept() {
        let m = DBusAuthMechanismSha1::new("my-secret");
        assert_eq!(m.name(), "DBUS_COOKIE_SHA1");
        assert_eq!(m.process_data("my-secret"), AuthMechanismState::Accepted);
    }

    #[test]
    fn test_sha1_reject() {
        let m = DBusAuthMechanismSha1::new("my-secret");
        assert_eq!(m.process_data("wrong"), AuthMechanismState::Rejected);
    }
}
