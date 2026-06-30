//! QUIC key derivation (RFC 9001 §5).
//!
//! Derives the per-encryption-level packet-protection keys. The Initial keys
//! are computed in-kernel from the client's Destination Connection ID and the
//! version-1 salt (no handshake needed); Handshake and 1-RTT keys are derived
//! from the traffic secrets the userspace TLS handshake installs.

use crate::crypto::hkdf::{hkdf_expand_label, hkdf_extract};
use alloc::vec::Vec;

/// QUIC v1 Initial salt (RFC 9001 §5.2).
pub const INITIAL_SALT_V1: [u8; 20] = [
    0x38, 0x76, 0x2c, 0xf7, 0xf5, 0x59, 0x34, 0xb3, 0x4d, 0x17, 0x9a, 0xe6, 0xa4, 0xc8, 0x0c, 0xad,
    0xcc, 0xbb, 0x7f, 0x0a,
];

/// AEAD key length for AES-128-GCM (the Initial cipher suite).
pub const KEY_LEN: usize = 16;
/// Packet-protection IV length.
pub const IV_LEN: usize = 12;
/// Header-protection key length (AES-128).
pub const HP_LEN: usize = 16;

/// The packet-protection material for one direction at one encryption level.
#[derive(Debug, Clone)]
pub struct PacketKeys {
    pub key: Vec<u8>,
    pub iv: Vec<u8>,
    pub hp: Vec<u8>,
}

/// Derive AEAD key / IV / header-protection key from a traffic secret
/// (RFC 9001 §5.1) — the "quic key" / "quic iv" / "quic hp" labels.
pub fn derive_packet_keys(secret: &[u8]) -> PacketKeys {
    PacketKeys {
        key: hkdf_expand_label(secret, b"quic key", b"", KEY_LEN),
        iv: hkdf_expand_label(secret, b"quic iv", b"", IV_LEN),
        hp: hkdf_expand_label(secret, b"quic hp", b"", HP_LEN),
    }
}

/// Derive the client and server Initial packet keys from the client's
/// Destination Connection ID (RFC 9001 §5.2).
///
/// Returns `(client_initial, server_initial)`.
pub fn initial_keys(dcid: &[u8]) -> (PacketKeys, PacketKeys) {
    let initial_secret = hkdf_extract(&INITIAL_SALT_V1, dcid);
    let client_secret = hkdf_expand_label(&initial_secret, b"client in", b"", 32);
    let server_secret = hkdf_expand_label(&initial_secret, b"server in", b"", 32);
    (
        derive_packet_keys(&client_secret),
        derive_packet_keys(&server_secret),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn h(s: &str) -> Vec<u8> {
        let b = s.as_bytes();
        let v = |c: u8| match c {
            b'0'..=b'9' => c - b'0',
            b'a'..=b'f' => c - b'a' + 10,
            _ => 0,
        };
        let mut o = Vec::new();
        let mut i = 0;
        while i + 1 < b.len() {
            o.push((v(b[i]) << 4) | v(b[i + 1]));
            i += 2;
        }
        o
    }

    // RFC 9001 Appendix A.1: DCID 0x8394c8f03e515708.
    #[test]
    fn initial_keys_rfc9001_a1() {
        let dcid = h("8394c8f03e515708");
        let (client, server) = initial_keys(&dcid);

        assert_eq!(client.key, h("1f369613dd76d5467730efcbe3b1a22d"));
        assert_eq!(client.iv, h("fa044b2f42a3fd3b46fb255c"));
        assert_eq!(client.hp, h("9f50449e04a0e810283a1e9933adedd2"));

        assert_eq!(server.key, h("cf3a5331653c364c88f0f379b6067e37"));
        assert_eq!(server.iv, h("0ac1493ca1905853b0bba03e"));
        assert_eq!(server.hp, h("c206b8d9b9f0f37644430b490eeaa314"));
    }
}
