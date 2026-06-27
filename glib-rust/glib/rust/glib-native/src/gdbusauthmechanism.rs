//! GDBusAuthMechanism matching `gio/gdbusauthmechanism.h`.
//! Base D-Bus auth mechanism. In this no_std port we model it as a trait.
//! Fully `no_std` compatible using `alloc`.

use alloc::string::String;

/// Auth mechanism state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthMechanismState {
    Initial,
    WaitingForData,
    HaveDataToSend,
    Accepted,
    Rejected,
}

/// A D-Bus auth mechanism (`GDBusAuthMechanism`).
pub trait DBusAuthMechanism {
    /// Gets the mechanism name (e.g. "ANONYMOUS", "EXTERNAL", "SHA1").
    fn name(&self) -> &str;

    /// Gets the current auth state.
    fn state(&self) -> AuthMechanismState;

    /// Initiates the mechanism, returning initial data to send.
    fn initiate(&self) -> Option<String>;

    /// Processes data received from the peer.
    fn process_data(&self, data: &str) -> AuthMechanismState;

    /// Checks if the mechanism is supported.
    fn is_supported(&self) -> bool;
}

/// A simple auth mechanism wrapper.
pub struct SimpleAuthMechanism {
    mech_name: String,
    supported: bool,
    state: core::sync::atomic::AtomicU8,
}

impl SimpleAuthMechanism {
    pub fn new(name: &str, supported: bool) -> Self {
        Self {
            mech_name: String::from(name),
            supported,
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

    fn set_state(&self, s: AuthMechanismState) {
        self.state
            .store(s as u8, core::sync::atomic::Ordering::SeqCst);
    }
}

impl DBusAuthMechanism for SimpleAuthMechanism {
    fn name(&self) -> &str {
        &self.mech_name
    }
    fn state(&self) -> AuthMechanismState {
        self.get_state()
    }
    fn initiate(&self) -> Option<String> {
        None
    }
    fn process_data(&self, _data: &str) -> AuthMechanismState {
        self.set_state(AuthMechanismState::Accepted);
        AuthMechanismState::Accepted
    }
    fn is_supported(&self) -> bool {
        self.supported
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_mechanism() {
        let m = SimpleAuthMechanism::new("ANONYMOUS", true);
        assert_eq!(m.name(), "ANONYMOUS");
        assert!(m.is_supported());
        assert_eq!(m.state(), AuthMechanismState::Initial);
        assert_eq!(m.process_data("test"), AuthMechanismState::Accepted);
        assert_eq!(m.state(), AuthMechanismState::Accepted);
    }
}
