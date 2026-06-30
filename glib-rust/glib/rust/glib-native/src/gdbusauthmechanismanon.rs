//! GDBusAuthMechanismAnon matching `gio/gdbusauthmechanismanon.h`.
//! ANONYMOUS D-Bus auth mechanism. In this no_std port we model it
//! as a simple accepted mechanism.
//! Fully `no_std` compatible using `alloc`.

use crate::gdbusauthmechanism::{AuthMechanismState, DBusAuthMechanism};
use alloc::string::String;

/// ANONYMOUS auth mechanism (`GDBusAuthMechanismAnon`).
pub struct DBusAuthMechanismAnon {
    state: core::sync::atomic::AtomicU8,
}

impl DBusAuthMechanismAnon {
    pub fn new() -> Self {
        Self {
            state: core::sync::atomic::AtomicU8::new(AuthMechanismState::Initial as u8),
        }
    }

    fn get_state(&self) -> AuthMechanismState {
        match self.state.load(core::sync::atomic::Ordering::SeqCst) {
            0 => AuthMechanismState::Initial,
            1 => AuthMechanismState::WaitingForData,
            2 => AuthMechanismState::HaveDataToSend,
            3 => AuthMechanismState::Accepted,
            _ => AuthMechanismState::Rejected,
        }
    }
}

impl Default for DBusAuthMechanismAnon {
    fn default() -> Self {
        Self::new()
    }
}

impl DBusAuthMechanism for DBusAuthMechanismAnon {
    fn name(&self) -> &str {
        "ANONYMOUS"
    }
    fn state(&self) -> AuthMechanismState {
        self.get_state()
    }
    fn initiate(&self) -> Option<String> {
        Some(String::from("anonymous"))
    }
    fn process_data(&self, _data: &str) -> AuthMechanismState {
        self.state.store(
            AuthMechanismState::Accepted as u8,
            core::sync::atomic::Ordering::SeqCst,
        );
        AuthMechanismState::Accepted
    }
    fn is_supported(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anonymous() {
        let m = DBusAuthMechanismAnon::new();
        assert_eq!(m.name(), "ANONYMOUS");
        assert!(m.is_supported());
        assert!(m.initiate().is_some());
        assert_eq!(m.process_data("anything"), AuthMechanismState::Accepted);
    }
}
