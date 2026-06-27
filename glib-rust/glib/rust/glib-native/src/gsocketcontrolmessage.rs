//! GSocketControlMessage matching `gio/gsocketcontrolmessage.h`.
//!
//! Represents an ancillary (control) message passed alongside socket data.
//! Common use: passing file descriptors over Unix domain sockets (`SCM_RIGHTS`).
//!
//! No_std compatible using `alloc`.

use alloc::vec::Vec;

/// CMSG level (SOL_SOCKET = 1, IPPROTO_IP = 0, etc.).
pub type CmsgLevel = i32;
/// CMSG type (SCM_RIGHTS = 1, SCM_CREDENTIALS = 2, etc.).
pub type CmsgType = i32;

/// A socket ancillary control message (`GSocketControlMessage`).
pub struct SocketControlMessage {
    level: CmsgLevel,
    msg_type: CmsgType,
    data: Vec<u8>,
}

impl SocketControlMessage {
    /// Creates a new control message with raw data.
    ///
    /// Mirrors `g_socket_control_message_serialize`.
    pub fn new(level: CmsgLevel, msg_type: CmsgType, data: Vec<u8>) -> Self {
        Self {
            level,
            msg_type,
            data,
        }
    }

    /// Creates a `SCM_RIGHTS` message carrying a set of file descriptors.
    ///
    /// Mirrors `g_unix_fd_message_new` / `GUnixFDMessage`.
    pub fn new_fd_list(fds: &[i32]) -> Self {
        // Each fd is stored as a 4-byte little-endian integer.
        let data: Vec<u8> = fds.iter().flat_map(|fd| fd.to_le_bytes()).collect();
        Self::new(1 /* SOL_SOCKET */, 1 /* SCM_RIGHTS */, data)
    }

    /// Returns the CMSG level.
    ///
    /// Mirrors `g_socket_control_message_get_level`.
    pub fn get_level(&self) -> CmsgLevel {
        self.level
    }

    /// Returns the CMSG type.
    ///
    /// Mirrors `g_socket_control_message_get_msg_type`.
    pub fn get_msg_type(&self) -> CmsgType {
        self.msg_type
    }

    /// Returns the serialized data size.
    ///
    /// Mirrors `g_socket_control_message_get_size`.
    pub fn get_size(&self) -> usize {
        self.data.len()
    }

    /// Returns the serialized data bytes.
    pub fn get_data(&self) -> &[u8] {
        &self.data
    }

    /// Deserializes the data as a list of file descriptors (for SCM_RIGHTS).
    ///
    /// Returns `None` if this is not an SCM_RIGHTS message or the data is malformed.
    pub fn as_fd_list(&self) -> Option<Vec<i32>> {
        if self.level != 1 || self.msg_type != 1 {
            return None;
        }
        if self.data.len() % 4 != 0 {
            return None;
        }
        Some(
            self.data
                .chunks_exact(4)
                .map(|c| i32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect(),
        )
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let m = SocketControlMessage::new(1, 1, vec![0, 0, 0, 3]);
        assert_eq!(m.get_level(), 1);
        assert_eq!(m.get_msg_type(), 1);
        assert_eq!(m.get_size(), 4);
    }

    #[test]
    fn test_fd_list_roundtrip() {
        let fds = [3i32, 7, 42];
        let m = SocketControlMessage::new_fd_list(&fds);
        let out = m.as_fd_list().unwrap();
        assert_eq!(out, fds);
    }

    #[test]
    fn test_fd_list_empty() {
        let m = SocketControlMessage::new_fd_list(&[]);
        let out = m.as_fd_list().unwrap();
        assert!(out.is_empty());
    }

    #[test]
    fn test_as_fd_list_wrong_type() {
        let m = SocketControlMessage::new(0, 2, vec![0, 0, 0, 1]);
        assert!(m.as_fd_list().is_none());
    }

    #[test]
    fn test_get_data() {
        let m = SocketControlMessage::new(1, 2, vec![1, 2, 3]);
        assert_eq!(m.get_data(), &[1, 2, 3]);
    }
}
