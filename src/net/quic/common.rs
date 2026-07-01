//! QUIC common primitives — variable-length integers, versions, and shared
//! constants (RFC 9000).
//!
//! Mirrors the helpers in `net/quic/common.{c,h}` of the in-kernel QUIC module
//! (github.com/lxin/quic). The variable-length integer codec is the foundation
//! the packet and frame parsers build on, so it is implemented exactly per
//! RFC 9000 §16.

/// QUIC version 1 (RFC 9000).
pub const QUIC_VERSION_1: u32 = 0x0000_0001;
/// QUIC version 2 (RFC 9369).
pub const QUIC_VERSION_2: u32 = 0x6b33_43cf;
/// Version Negotiation uses version 0 on the wire.
pub const QUIC_VERSION_NEGOTIATION: u32 = 0x0000_0000;

/// Largest value representable by a QUIC variable-length integer (2^62 - 1).
pub const VARINT_MAX: u64 = (1u64 << 62) - 1;

/// Number of bytes a variable-length integer occupies on the wire for `value`.
///
/// Returns 0 if `value` exceeds [`VARINT_MAX`] (not encodable).
pub fn varint_len(value: u64) -> usize {
    if value <= 63 {
        1
    } else if value <= 16383 {
        2
    } else if value <= 1_073_741_823 {
        4
    } else if value <= VARINT_MAX {
        8
    } else {
        0
    }
}

/// Encode `value` as a QUIC variable-length integer into `buf`.
///
/// Returns the number of bytes written, or `None` if `value` is too large or
/// `buf` is too small. The two most-significant bits of the first byte select
/// the length (00→1, 01→2, 10→4, 11→8 bytes); the remaining bits hold the
/// value in network byte order (RFC 9000 §16).
pub fn encode_varint(value: u64, buf: &mut [u8]) -> Option<usize> {
    let len = varint_len(value);
    if len == 0 || buf.len() < len {
        return None;
    }
    match len {
        1 => {
            buf[0] = value as u8;
        }
        2 => {
            let v = value as u16 | 0x4000;
            buf[0..2].copy_from_slice(&v.to_be_bytes());
        }
        4 => {
            let v = value as u32 | 0x8000_0000;
            buf[0..4].copy_from_slice(&v.to_be_bytes());
        }
        _ => {
            let v = value | 0xc000_0000_0000_0000;
            buf[0..8].copy_from_slice(&v.to_be_bytes());
        }
    }
    Some(len)
}

/// Decode a QUIC variable-length integer from the front of `buf`.
///
/// Returns `(value, bytes_consumed)` or `None` if `buf` does not hold a
/// complete encoding. The length prefix bits are masked off the first byte
/// before assembling the big-endian value.
pub fn decode_varint(buf: &[u8]) -> Option<(u64, usize)> {
    let first = *buf.first()?;
    let len = 1usize << (first >> 6); // 00→1, 01→2, 10→4, 11→8
    if buf.len() < len {
        return None;
    }
    let mut value = (first & 0x3f) as u64;
    for &b in &buf[1..len] {
        value = (value << 8) | b as u64;
    }
    Some((value, len))
}

/// QUIC transport error codes (RFC 9000 §20.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum TransportError {
    NoError = 0x00,
    InternalError = 0x01,
    ConnectionRefused = 0x02,
    FlowControlError = 0x03,
    StreamLimitError = 0x04,
    StreamStateError = 0x05,
    FinalSizeError = 0x06,
    FrameEncodingError = 0x07,
    TransportParameterError = 0x08,
    ConnectionIdLimitError = 0x09,
    ProtocolViolation = 0x0a,
    InvalidToken = 0x0b,
    ApplicationError = 0x0c,
    CryptoBufferExceeded = 0x0d,
    KeyUpdateError = 0x0e,
    AeadLimitReached = 0x0f,
    NoViablePath = 0x10,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varint_roundtrip() {
        // The four boundary values from RFC 9000 §A.1 plus edges.
        for &v in &[
            0u64,
            63,
            64,
            16383,
            16384,
            1_073_741_823,
            1_073_741_824,
            VARINT_MAX,
        ] {
            let mut buf = [0u8; 8];
            let n = encode_varint(v, &mut buf).unwrap();
            assert_eq!(n, varint_len(v));
            let (decoded, consumed) = decode_varint(&buf).unwrap();
            assert_eq!(decoded, v);
            assert_eq!(consumed, n);
        }
    }

    #[test]
    fn varint_too_large_is_rejected() {
        assert_eq!(varint_len(VARINT_MAX + 1), 0);
        assert!(encode_varint(VARINT_MAX + 1, &mut [0u8; 8]).is_none());
    }

    #[test]
    fn decode_known_encoding() {
        // RFC 9000 §A.1: 0x9d7f3e7d decodes to 494878333 (4-byte form).
        let (v, n) = decode_varint(&[0x9d, 0x7f, 0x3e, 0x7d]).unwrap();
        assert_eq!(n, 4);
        assert_eq!(v, 494_878_333);
    }
}
