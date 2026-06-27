//! gwin32sid matching `gio/gwin32sid.c`.
//!
//! Windows Security Identifier (SID) utilities. Provides functions to
//! copy SIDs, get the SID from an access token, and convert between
//! SID and string representations.
//!
//! In this no_std port, we model SIDs as byte vectors.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;
use spin::Mutex;

/// A Windows Security Identifier (SID).
///
/// In the C implementation, this wraps the Windows `SID` struct.
/// Here we store the raw SID bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sid {
    bytes: Vec<u8>,
}

impl Sid {
    /// Creates a SID from raw bytes.
    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
        }
    }

    /// Returns the raw SID bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Returns the length of the SID in bytes.
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    /// Returns whether the SID is empty.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Converts the SID to its string representation (SDDL format).
    ///
    /// Format: S-R-I-S-S-S...
    /// Where R = revision, I = identifier authority, S = sub-authorities.
    ///
    /// Mirrors `ConvertSidToSidToString`.
    pub fn to_string(&self) -> String {
        if self.bytes.len() < 8 {
            return String::new();
        }

        let revision = self.bytes[0];
        let sub_authority_count = self.bytes[1] as usize;

        // Identifier authority (bytes 2-7, big-endian)
        let id_auth: u64 = ((self.bytes[2] as u64) << 40)
            | ((self.bytes[3] as u64) << 32)
            | ((self.bytes[4] as u64) << 24)
            | ((self.bytes[5] as u64) << 16)
            | ((self.bytes[6] as u64) << 8)
            | (self.bytes[7] as u64);

        let mut result = format!("S-{}-{}", revision, id_auth);

        // Sub-authorities (each 4 bytes, little-endian, starting at byte 8)
        for i in 0..sub_authority_count {
            let offset = 8 + i * 4;
            if offset + 4 <= self.bytes.len() {
                let sub_auth = u32::from_le_bytes([
                    self.bytes[offset],
                    self.bytes[offset + 1],
                    self.bytes[offset + 2],
                    self.bytes[offset + 3],
                ]);
                result.push_str(&format!("-{}", sub_auth));
            }
        }

        result
    }

    /// Parses a SID from its SDDL string representation.
    ///
    /// Mirrors `ConvertStringSidToSid`.
    pub fn from_string(s: &str) -> Option<Self> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() < 3 || parts[0] != "S" {
            return None;
        }

        let revision: u8 = parts[1].parse().ok()?;
        let id_auth: u64 = parts[2].parse().ok()?;

        let sub_auths: Vec<u32> = parts[3..]
            .iter()
            .filter_map(|s| s.parse::<u32>().ok())
            .collect();

        let sub_count = sub_auths.len();
        if sub_count > 15 {
            return None;
        }

        let mut bytes = Vec::with_capacity(8 + sub_count * 4);
        bytes.push(revision);
        bytes.push(sub_count as u8);
        bytes.push((id_auth >> 40) as u8);
        bytes.push((id_auth >> 32) as u8);
        bytes.push((id_auth >> 24) as u8);
        bytes.push((id_auth >> 16) as u8);
        bytes.push((id_auth >> 8) as u8);
        bytes.push(id_auth as u8);
        for sa in &sub_auths {
            bytes.extend_from_slice(&sa.to_le_bytes());
        }

        Some(Self { bytes })
    }
}

/// Gets the user SID from an access token.
///
/// In the C implementation, this calls `GetTokenInformation(TokenUser, ...)`
/// and extracts the SID. Here we model it with a configurable value.
///
/// Mirrors `_g_win32_token_get_sid`.
pub fn token_get_sid(token: &Win32Token) -> Option<Sid> {
    token.user_sid.clone()
}

/// A Windows access token.
pub struct Win32Token {
    user_sid: Option<Sid>,
}

impl Win32Token {
    /// Creates a new token with the given user SID.
    pub fn new(user_sid: Option<Sid>) -> Self {
        Self { user_sid }
    }

    /// Returns the user SID.
    pub fn user_sid(&self) -> Option<&Sid> {
        self.user_sid.as_ref()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sid_roundtrip() {
        // S-1-5-32-544 (Administrators group)
        let sid_str = "S-1-5-32-544";
        let sid = Sid::from_string(sid_str).unwrap();
        assert_eq!(sid.to_string(), sid_str);
    }

    #[test]
    fn test_sid_from_bytes() {
        let bytes = [1u8, 1, 0, 0, 0, 0, 0, 5, 32, 0, 0, 0];
        let sid = Sid::from_bytes(&bytes);
        assert_eq!(sid.to_string(), "S-1-5-32");
    }

    #[test]
    fn test_sid_invalid_string() {
        assert!(Sid::from_string("invalid").is_none());
        assert!(Sid::from_string("S-").is_none());
        assert!(Sid::from_string("X-1-5").is_none());
    }

    #[test]
    fn test_token_get_sid() {
        let sid = Sid::from_string("S-1-5-32-544").unwrap();
        let token = Win32Token::new(Some(sid.clone()));
        let result = token_get_sid(&token);
        assert!(result.is_some());
        assert_eq!(result.unwrap().to_string(), "S-1-5-32-544");
    }

    #[test]
    fn test_token_no_sid() {
        let token = Win32Token::new(None);
        assert!(token_get_sid(&token).is_none());
    }
}
