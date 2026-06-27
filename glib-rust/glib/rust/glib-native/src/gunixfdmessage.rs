//! GUnixFDMessage matching `gio/gunixfdmessage.h` / `gio/gunixfdmessage.c`.
//!
//! A `GSocketControlMessage` subclass carrying file descriptors via
//! `SCM_RIGHTS`. Serializes through [`SocketControlMessage`].
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gsocketcontrolmessage::SocketControlMessage;
use crate::gunixfdlist::UnixFDList;
use alloc::sync::Arc;

/// SOL_SOCKET level for Unix fd messages.
const SOL_SOCKET: i32 = 1;
/// SCM_RIGHTS control message type.
const SCM_RIGHTS: i32 = 1;

/// A socket control message carrying a list of file descriptors (`GUnixFDMessage`).
pub struct UnixFDMessage {
    fd_list: Arc<UnixFDList>,
}

impl UnixFDMessage {
    /// Creates a new empty fd message.
    ///
    /// Mirrors `g_unix_fd_message_new`.
    pub fn new() -> Self {
        Self {
            fd_list: Arc::new(UnixFDList::new()),
        }
    }

    /// Creates an fd message wrapping an existing [`UnixFDList`].
    pub fn new_with_fd_list(fd_list: UnixFDList) -> Self {
        Self {
            fd_list: Arc::new(fd_list),
        }
    }

    /// Creates an fd message from a slice of file descriptors.
    ///
    /// Mirrors `g_unix_fd_message_new_with_fd_list`.
    pub fn new_from_fds(fds: &[i32]) -> Self {
        Self::new_with_fd_list(UnixFDList::new_from_array(fds))
    }

    /// Returns the fd list carried by this message.
    ///
    /// Mirrors `g_unix_fd_message_get_fd_list`.
    pub fn get_fd_list(&self) -> Arc<UnixFDList> {
        Arc::clone(&self.fd_list)
    }

    /// Appends `fd` to the message's fd list.
    ///
    /// Mirrors `g_unix_fd_message_append_fd`.
    pub fn append_fd(&self, fd: i32) -> Result<usize, Error> {
        self.fd_list.add(fd)
    }

    /// Returns the number of fds in the message.
    pub fn get_fd_count(&self) -> usize {
        self.fd_list.get_length()
    }

    /// Serializes to a `SocketControlMessage` (`SCM_RIGHTS` payload).
    ///
    /// Mirrors `g_socket_control_message_serialize` on `GUnixFDMessage`.
    pub fn to_control_message(&self) -> SocketControlMessage {
        let fds: alloc::vec::Vec<i32> = (0..self.fd_list.get_length())
            .filter_map(|i| self.fd_list.get(i).ok())
            .collect();
        SocketControlMessage::new_fd_list(&fds)
    }

    /// Deserializes from a `SocketControlMessage`.
    ///
    /// Returns `None` if the message is not `SCM_RIGHTS` or the payload is
    /// malformed. Mirrors `g_unix_fd_message_new_with_fd_list` /
    /// `g_socket_control_message_deserialize`.
    pub fn from_control_message(msg: &SocketControlMessage) -> Option<Self> {
        if msg.get_level() != SOL_SOCKET || msg.get_msg_type() != SCM_RIGHTS {
            return None;
        }
        let fds = msg.as_fd_list()?;
        Some(Self::new_from_fds(&fds))
    }
}

impl Default for UnixFDMessage {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for UnixFDMessage {
    fn clone(&self) -> Self {
        Self {
            fd_list: Arc::clone(&self.fd_list),
        }
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let msg = UnixFDMessage::new();
        assert_eq!(msg.get_fd_count(), 0);
    }

    #[test]
    fn test_append_fd() {
        let msg = UnixFDMessage::new();
        assert_eq!(msg.append_fd(3).unwrap(), 0);
        assert_eq!(msg.append_fd(5).unwrap(), 1);
        assert_eq!(msg.get_fd_list().get(0).unwrap(), 3);
    }

    #[test]
    fn test_new_from_fds() {
        let msg = UnixFDMessage::new_from_fds(&[1, 2, 3]);
        assert_eq!(msg.get_fd_count(), 3);
        assert_eq!(msg.get_fd_list().get(1).unwrap(), 2);
    }

    #[test]
    fn test_control_message_roundtrip() {
        let msg = UnixFDMessage::new_from_fds(&[3, 7, 42]);
        let ctrl = msg.to_control_message();
        assert_eq!(ctrl.get_level(), SOL_SOCKET);
        assert_eq!(ctrl.get_msg_type(), SCM_RIGHTS);
        let restored = UnixFDMessage::from_control_message(&ctrl).unwrap();
        assert_eq!(restored.get_fd_count(), 3);
        assert_eq!(restored.get_fd_list().get(2).unwrap(), 42);
    }

    #[test]
    fn test_from_control_message_wrong_type() {
        let ctrl = SocketControlMessage::new(1, 2, alloc::vec![0u8; 4]);
        assert!(UnixFDMessage::from_control_message(&ctrl).is_none());
    }

    #[test]
    fn test_append_invalid_fd() {
        use crate::gioerror::IOErrorEnum;
        let msg = UnixFDMessage::new();
        assert!(msg.append_fd(-1).is_err());
        assert_eq!(
            msg.append_fd(-1).unwrap_err().code(),
            IOErrorEnum::InvalidArgument.to_code()
        );
    }
}
