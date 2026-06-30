//! QUIC encryption levels and key state (RFC 9001).
//!
//! Mirrors `net/quic/crypto.{c,h}`. Like the upstream in-kernel QUIC module,
//! the TLS 1.3 handshake itself is offloaded to userspace: the kernel receives
//! the negotiated traffic secrets per encryption level and is responsible for
//! the AEAD packet protection and header protection on the data path.
//!
//! This module defines the encryption levels, the per-level key material slots,
//! and the CRYPTO-stream reassembly buffer that feeds the handshake. The AEAD
//! and header-protection transforms are wired in a follow-up phase once the
//! kernel crypto API exposes the required AEAD (AES-GCM / ChaCha20-Poly1305).

use alloc::vec::Vec;

/// QUIC encryption levels (RFC 9001 §2). Each maps to a packet number space
/// except 0-RTT and 1-RTT which share the Application space.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncryptionLevel {
    Initial,
    ZeroRtt,
    Handshake,
    OneRtt,
}

/// Negotiated key material for one direction at one encryption level.
#[derive(Debug, Clone, Default)]
pub struct KeyMaterial {
    /// AEAD key.
    pub key: Vec<u8>,
    /// AEAD IV (XORed with the packet number to form the nonce).
    pub iv: Vec<u8>,
    /// Header-protection key.
    pub hp: Vec<u8>,
    pub installed: bool,
}

impl KeyMaterial {
    pub fn install(&mut self, key: Vec<u8>, iv: Vec<u8>, hp: Vec<u8>) {
        self.key = key;
        self.iv = iv;
        self.hp = hp;
        self.installed = true;
    }
}

/// Per-connection crypto state: the secrets at each level plus the CRYPTO
/// stream reassembly buffer that carries TLS handshake messages.
#[derive(Debug, Default)]
pub struct CryptoState {
    pub tx_initial: KeyMaterial,
    pub rx_initial: KeyMaterial,
    pub tx_handshake: KeyMaterial,
    pub rx_handshake: KeyMaterial,
    pub tx_app: KeyMaterial,
    pub rx_app: KeyMaterial,
    /// Current 1-RTT key phase bit (RFC 9001 §6).
    pub key_phase: bool,
    /// In-order CRYPTO data delivered to the handshake.
    pub crypto_recv_offset: u64,
    /// Buffered out-of-order CRYPTO data keyed by offset.
    crypto_reasm: Vec<(u64, Vec<u8>)>,
}

impl CryptoState {
    pub fn tx(&mut self, level: EncryptionLevel) -> &mut KeyMaterial {
        match level {
            EncryptionLevel::Initial => &mut self.tx_initial,
            EncryptionLevel::Handshake => &mut self.tx_handshake,
            EncryptionLevel::ZeroRtt | EncryptionLevel::OneRtt => &mut self.tx_app,
        }
    }

    pub fn rx(&mut self, level: EncryptionLevel) -> &mut KeyMaterial {
        match level {
            EncryptionLevel::Initial => &mut self.rx_initial,
            EncryptionLevel::Handshake => &mut self.rx_handshake,
            EncryptionLevel::ZeroRtt | EncryptionLevel::OneRtt => &mut self.rx_app,
        }
    }

    /// Accept a CRYPTO frame at `offset`; returns any newly in-order bytes the
    /// handshake can now consume, reassembling out-of-order pieces.
    pub fn recv_crypto(&mut self, offset: u64, data: &[u8]) -> Vec<u8> {
        if offset + data.len() as u64 <= self.crypto_recv_offset {
            return Vec::new(); // fully duplicate
        }
        self.crypto_reasm.push((offset, data.to_vec()));
        self.crypto_reasm.sort_by_key(|(o, _)| *o);

        let mut out = Vec::new();
        let mut progressed = true;
        while progressed {
            progressed = false;
            let mut i = 0;
            while i < self.crypto_reasm.len() {
                let (o, d) = &self.crypto_reasm[i];
                let (o, end) = (*o, *o + d.len() as u64);
                if end <= self.crypto_recv_offset {
                    self.crypto_reasm.remove(i); // stale
                    continue;
                }
                if o <= self.crypto_recv_offset {
                    let skip = (self.crypto_recv_offset - o) as usize;
                    out.extend_from_slice(&d[skip..]);
                    self.crypto_recv_offset = end;
                    self.crypto_reasm.remove(i);
                    progressed = true;
                    continue;
                }
                i += 1;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crypto_reassembly_orders_pieces() {
        let mut cs = CryptoState::default();
        // Deliver [4..8) first (out of order), then [0..4): handshake should
        // then see the whole 0..8 range in order.
        assert!(cs.recv_crypto(4, b"defg").is_empty());
        let out = cs.recv_crypto(0, b"abcd");
        assert_eq!(out, b"abcddefg");
        assert_eq!(cs.crypto_recv_offset, 8);
    }
}
