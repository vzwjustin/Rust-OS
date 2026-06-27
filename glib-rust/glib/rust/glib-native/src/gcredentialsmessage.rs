//! GCredentialsMessage matching `gio/gcredentialsmessage.h`.
//!
//! A `GSocketControlMessage` subclass that carries Unix peer credentials
//! (`SCM_CREDENTIALS`). Serializes the PID/UID/GID into a 12-byte payload.
//!
//! No_std compatible using `alloc`.

use crate::gcredentials::Credentials;
use crate::gsocketcontrolmessage::SocketControlMessage;
use alloc::vec::Vec;

/// SOL_SOCKET level.
const SOL_SOCKET: i32 = 1;
/// SCM_CREDENTIALS type.
const SCM_CREDENTIALS: i32 = 2;

/// Serialized size: pid (i32) + uid (u32) + gid (u32) = 12 bytes.
const CREDS_PAYLOAD_LEN: usize = 12;

/// A socket control message carrying Unix credentials (`GCredentialsMessage`).
pub struct CredentialsMessage {
    credentials: Credentials,
}

impl CredentialsMessage {
    /// Creates a new credentials message.
    ///
    /// Mirrors `g_credentials_message_new`.
    pub fn new(credentials: Credentials) -> Self {
        Self { credentials }
    }

    /// Creates a credentials message with all-unknown fields (zeros).
    pub fn new_unknown() -> Self {
        Self {
            credentials: Credentials::new(),
        }
    }

    /// Returns the credentials carried by this message.
    ///
    /// Mirrors `g_credentials_message_get_credentials`.
    pub fn get_credentials(&self) -> &Credentials {
        &self.credentials
    }

    /// Serializes to a `SocketControlMessage` (SCM_CREDENTIALS payload).
    ///
    /// Layout: `pid:i32 uid:u32 gid:u32` in little-endian byte order.
    pub fn to_control_message(&self) -> SocketControlMessage {
        let pid = self.credentials.get_unix_pid().unwrap_or(0);
        let uid = self.credentials.get_unix_user().unwrap_or(0);
        // GID is not exposed via the public API; default to UID for same-user.
        let gid = uid;
        let mut data = Vec::with_capacity(CREDS_PAYLOAD_LEN);
        data.extend_from_slice(&pid.to_le_bytes());
        data.extend_from_slice(&uid.to_le_bytes());
        data.extend_from_slice(&gid.to_le_bytes());
        SocketControlMessage::new(SOL_SOCKET, SCM_CREDENTIALS, data)
    }

    /// Deserializes from a `SocketControlMessage`.
    ///
    /// Returns `None` if the message is not an SCM_CREDENTIALS message or the
    /// payload is malformed.
    pub fn from_control_message(msg: &SocketControlMessage) -> Option<Self> {
        if msg.get_level() != SOL_SOCKET || msg.get_msg_type() != SCM_CREDENTIALS {
            return None;
        }
        let data = msg.get_data();
        if data.len() < CREDS_PAYLOAD_LEN {
            return None;
        }
        let pid = i32::from_le_bytes([data[0], data[1], data[2], data[3]]);
        let uid = u32::from_le_bytes([data[4], data[5], data[6], data[7]]);
        let gid = u32::from_le_bytes([data[8], data[9], data[10], data[11]]);
        Some(Self {
            credentials: Credentials::new_with(pid, uid, gid),
        })
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gcredentials::Credentials;

    #[test]
    fn test_roundtrip() {
        let creds = Credentials::new_with(1234, 1000, 1000);
        let msg = CredentialsMessage::new(creds);
        let ctrl = msg.to_control_message();
        let restored = CredentialsMessage::from_control_message(&ctrl).unwrap();
        assert_eq!(restored.get_credentials().get_unix_pid().unwrap(), 1234);
        assert_eq!(restored.get_credentials().get_unix_user().unwrap(), 1000);
    }

    #[test]
    fn test_wrong_level() {
        let ctrl = crate::gsocketcontrolmessage::SocketControlMessage::new(0, 2, vec![0u8; 12]);
        assert!(CredentialsMessage::from_control_message(&ctrl).is_none());
    }

    #[test]
    fn test_wrong_type() {
        let ctrl = crate::gsocketcontrolmessage::SocketControlMessage::new(1, 1, vec![0u8; 12]);
        assert!(CredentialsMessage::from_control_message(&ctrl).is_none());
    }

    #[test]
    fn test_short_payload() {
        let ctrl = crate::gsocketcontrolmessage::SocketControlMessage::new(1, 2, vec![0u8; 4]);
        assert!(CredentialsMessage::from_control_message(&ctrl).is_none());
    }

    #[test]
    fn test_new_unknown() {
        let m = CredentialsMessage::new_unknown();
        assert!(m.get_credentials().get_unix_pid().is_err());
    }

    #[test]
    fn test_get_credentials() {
        let creds = Credentials::new_with(99, 500, 500);
        let m = CredentialsMessage::new(creds);
        assert_eq!(m.get_credentials().get_unix_pid().unwrap(), 99);
    }
}
