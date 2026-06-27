//! GUnixCredentialsMessage matching `gio/gunixcredentialsmessage.h`.
//!
//! On Unix platforms this is the concrete [`CredentialsMessage`] used to pass
//! process credentials over domain sockets (`SCM_CREDENTIALS`). This module
//! provides the `g_unix_credentials_message_*` API as a thin wrapper.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gcredentials::Credentials;
use crate::gcredentialsmessage::CredentialsMessage;
use crate::gsocketcontrolmessage::SocketControlMessage;

/// Unix credentials control message (`GUnixCredentialsMessage`).
///
/// Wraps [`CredentialsMessage`] with the GUnixCredentialsMessage API surface.
pub struct UnixCredentialsMessage {
    inner: CredentialsMessage,
}

impl UnixCredentialsMessage {
    /// Creates a new credentials message.
    ///
    /// Mirrors `g_unix_credentials_message_new`.
    pub fn new(credentials: Credentials) -> Self {
        Self {
            inner: CredentialsMessage::new(credentials),
        }
    }

    /// Creates a credentials message with all-unknown fields.
    ///
    /// Mirrors `g_credentials_message_new` with empty credentials.
    pub fn new_unknown() -> Self {
        Self {
            inner: CredentialsMessage::new_unknown(),
        }
    }

    /// Returns the credentials carried by this message.
    ///
    /// Mirrors `g_unix_credentials_message_get_credentials`.
    pub fn get_credentials(&self) -> &Credentials {
        self.inner.get_credentials()
    }

    /// Returns a reference to the underlying [`CredentialsMessage`].
    pub fn as_credentials_message(&self) -> &CredentialsMessage {
        &self.inner
    }

    /// Consumes the wrapper and returns the inner [`CredentialsMessage`].
    pub fn into_credentials_message(self) -> CredentialsMessage {
        self.inner
    }

    /// Serializes to a `SocketControlMessage` (`SCM_CREDENTIALS` payload).
    pub fn to_control_message(&self) -> SocketControlMessage {
        self.inner.to_control_message()
    }

    /// Deserializes from a `SocketControlMessage`.
    pub fn from_control_message(msg: &SocketControlMessage) -> Option<Self> {
        CredentialsMessage::from_control_message(msg).map(|inner| Self { inner })
    }
}

impl Clone for UnixCredentialsMessage {
    fn clone(&self) -> Self {
        Self {
            inner: CredentialsMessage::new(self.inner.get_credentials().clone()),
        }
    }
}

impl From<CredentialsMessage> for UnixCredentialsMessage {
    fn from(inner: CredentialsMessage) -> Self {
        Self { inner }
    }
}

impl From<UnixCredentialsMessage> for CredentialsMessage {
    fn from(msg: UnixCredentialsMessage) -> Self {
        msg.inner
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_and_get_credentials() {
        let creds = Credentials::new_with(1234, 1000, 1000);
        let msg = UnixCredentialsMessage::new(creds);
        assert_eq!(msg.get_credentials().get_unix_pid().unwrap(), 1234);
        assert_eq!(msg.get_credentials().get_unix_user().unwrap(), 1000);
    }

    #[test]
    fn test_new_unknown() {
        let msg = UnixCredentialsMessage::new_unknown();
        assert!(msg.get_credentials().get_unix_pid().is_err());
    }

    #[test]
    fn test_control_message_roundtrip() {
        let creds = Credentials::new_with(42, 500, 500);
        let msg = UnixCredentialsMessage::new(creds);
        let ctrl = msg.to_control_message();
        let restored = UnixCredentialsMessage::from_control_message(&ctrl).unwrap();
        assert_eq!(restored.get_credentials().get_unix_pid().unwrap(), 42);
        assert_eq!(restored.get_credentials().get_unix_user().unwrap(), 500);
    }

    #[test]
    fn test_from_credentials_message() {
        let creds = Credentials::new_with(1, 2, 3);
        let inner = CredentialsMessage::new(creds);
        let unix = UnixCredentialsMessage::from(inner);
        assert_eq!(unix.get_credentials().get_unix_pid().unwrap(), 1);
    }

    #[test]
    fn test_into_credentials_message() {
        let creds = Credentials::new_with(9, 8, 7);
        let unix = UnixCredentialsMessage::new(creds);
        let inner: CredentialsMessage = unix.into();
        assert_eq!(inner.get_credentials().get_unix_pid().unwrap(), 9);
    }
}
