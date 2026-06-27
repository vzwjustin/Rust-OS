//! GDBusAuthMechanismExternal matching `gio/gdbusauthmechanismexternal.h`.
//! EXTERNAL D-Bus auth mechanism (uses external credentials). In this
//! no_std port we model it checking a user ID.
//! Fully `no_std` compatible using `alloc`.

use crate::gdbusauthmechanism::{AuthMechanismState, DBusAuthMechanism};
use alloc::string::String;

/// EXTERNAL auth mechanism (`GDBusAuthMechanismExternal`).
pub struct DBusAuthMechanismExternal {
    user_id: String,
    accepted: core::sync::atomic::AtomicBool,
}

impl DBusAuthMechanismExternal {
    pub fn new(user_id: &str) -> Self {
        Self {
            user_id: String::from(user_id),
            accepted: core::sync::atomic::AtomicBool::new(false),
        }
    }
}

impl DBusAuthMechanism for DBusAuthMechanismExternal {
    fn name(&self) -> &str {
        "EXTERNAL"
    }
    fn state(&self) -> AuthMechanismState {
        if self.accepted.load(core::sync::atomic::Ordering::SeqCst) {
            AuthMechanismState::Accepted
        } else {
            AuthMechanismState::Initial
        }
    }
    fn initiate(&self) -> Option<String> {
        Some(self.user_id.clone())
    }
    fn process_data(&self, data: &str) -> AuthMechanismState {
        if data == self.user_id {
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
    fn test_external_accept() {
        let m = DBusAuthMechanismExternal::new("1000");
        assert_eq!(m.name(), "EXTERNAL");
        assert_eq!(m.process_data("1000"), AuthMechanismState::Accepted);
    }

    #[test]
    fn test_external_reject() {
        let m = DBusAuthMechanismExternal::new("1000");
        assert_eq!(m.process_data("9999"), AuthMechanismState::Rejected);
    }
}
