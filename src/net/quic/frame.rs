//! QUIC frame parsing and encoding (RFC 9000 §19).
//!
//! Mirrors `net/quic/frame.{c,h}`. Implements the wire format for the frame
//! types that make up the data path; each parser consumes exactly one frame
//! and reports how many bytes it used so a packet payload can be walked
//! frame-by-frame.

use super::common::{decode_varint, encode_varint};
use alloc::vec::Vec;

/// Frame type codes (RFC 9000 §19). STREAM and ACK occupy ranges because their
/// low bits carry flags.
pub mod frame_type {
    pub const PADDING: u64 = 0x00;
    pub const PING: u64 = 0x01;
    pub const ACK: u64 = 0x02; // 0x02..=0x03 (0x01 bit = ECN counts present)
    pub const ACK_ECN: u64 = 0x03;
    pub const RESET_STREAM: u64 = 0x04;
    pub const STOP_SENDING: u64 = 0x05;
    pub const CRYPTO: u64 = 0x06;
    pub const NEW_TOKEN: u64 = 0x07;
    pub const STREAM: u64 = 0x08; // 0x08..=0x0f (low 3 bits = OFF|LEN|FIN)
    pub const MAX_DATA: u64 = 0x10;
    pub const MAX_STREAM_DATA: u64 = 0x11;
    pub const MAX_STREAMS_BIDI: u64 = 0x12;
    pub const MAX_STREAMS_UNI: u64 = 0x13;
    pub const DATA_BLOCKED: u64 = 0x14;
    pub const STREAM_DATA_BLOCKED: u64 = 0x15;
    pub const STREAMS_BLOCKED_BIDI: u64 = 0x16;
    pub const STREAMS_BLOCKED_UNI: u64 = 0x17;
    pub const NEW_CONNECTION_ID: u64 = 0x18;
    pub const RETIRE_CONNECTION_ID: u64 = 0x19;
    pub const PATH_CHALLENGE: u64 = 0x1a;
    pub const PATH_RESPONSE: u64 = 0x1b;
    pub const CONNECTION_CLOSE: u64 = 0x1c; // 0x1c (transport) / 0x1d (application)
    pub const CONNECTION_CLOSE_APP: u64 = 0x1d;
    pub const HANDSHAKE_DONE: u64 = 0x1e;
}

/// STREAM frame flag bits (low 3 bits of the 0x08..0x0f type).
const STREAM_FIN: u64 = 0x01;
const STREAM_LEN: u64 = 0x02;
const STREAM_OFF: u64 = 0x04;

/// A parsed QUIC frame. Variable-length data is borrowed from the input buffer
/// to avoid copying during packet processing.
#[derive(Debug, Clone)]
pub enum Frame<'a> {
    Padding(usize),
    Ping,
    Ack {
        largest: u64,
        delay: u64,
        first_range: u64,
        ranges: Vec<(u64, u64)>, // (gap, ack_range_length)
    },
    ResetStream {
        stream_id: u64,
        error_code: u64,
        final_size: u64,
    },
    StopSending {
        stream_id: u64,
        error_code: u64,
    },
    Crypto {
        offset: u64,
        data: &'a [u8],
    },
    Stream {
        stream_id: u64,
        offset: u64,
        fin: bool,
        data: &'a [u8],
    },
    MaxData(u64),
    MaxStreamData {
        stream_id: u64,
        max: u64,
    },
    ConnectionClose {
        error_code: u64,
        frame_type: u64,
        reason: &'a [u8],
        application: bool,
    },
    HandshakeDone,
}

/// Parse a single frame from the front of `buf`, returning the frame and the
/// number of bytes consumed.
pub fn parse_frame(buf: &[u8]) -> Option<(Frame<'_>, usize)> {
    let (ty, mut off) = decode_varint(buf)?;

    match ty {
        frame_type::PADDING => {
            // Coalesce a run of PADDING bytes into one frame.
            let mut n = off;
            while buf.get(n) == Some(&0) {
                n += 1;
            }
            Some((Frame::Padding(n), n))
        }
        frame_type::PING => Some((Frame::Ping, off)),
        frame_type::ACK | frame_type::ACK_ECN => {
            let (largest, n) = decode_varint(&buf[off..])?;
            off += n;
            let (delay, n) = decode_varint(&buf[off..])?;
            off += n;
            let (range_count, n) = decode_varint(&buf[off..])?;
            off += n;
            let (first_range, n) = decode_varint(&buf[off..])?;
            off += n;
            let mut ranges = Vec::new();
            for _ in 0..range_count {
                let (gap, n) = decode_varint(&buf[off..])?;
                off += n;
                let (len, n) = decode_varint(&buf[off..])?;
                off += n;
                ranges.push((gap, len));
            }
            if ty == frame_type::ACK_ECN {
                // ECT0, ECT1, ECN-CE counts.
                for _ in 0..3 {
                    let (_v, n) = decode_varint(&buf[off..])?;
                    off += n;
                }
            }
            Some((
                Frame::Ack {
                    largest,
                    delay,
                    first_range,
                    ranges,
                },
                off,
            ))
        }
        frame_type::RESET_STREAM => {
            let (stream_id, n) = decode_varint(&buf[off..])?;
            off += n;
            let (error_code, n) = decode_varint(&buf[off..])?;
            off += n;
            let (final_size, n) = decode_varint(&buf[off..])?;
            off += n;
            Some((
                Frame::ResetStream {
                    stream_id,
                    error_code,
                    final_size,
                },
                off,
            ))
        }
        frame_type::STOP_SENDING => {
            let (stream_id, n) = decode_varint(&buf[off..])?;
            off += n;
            let (error_code, n) = decode_varint(&buf[off..])?;
            off += n;
            Some((
                Frame::StopSending {
                    stream_id,
                    error_code,
                },
                off,
            ))
        }
        frame_type::CRYPTO => {
            let (offset, n) = decode_varint(&buf[off..])?;
            off += n;
            let (len, n) = decode_varint(&buf[off..])?;
            off += n;
            let end = off.checked_add(len as usize)?;
            if end > buf.len() {
                return None;
            }
            Some((
                Frame::Crypto {
                    offset,
                    data: &buf[off..end],
                },
                end,
            ))
        }
        t if (frame_type::STREAM..=frame_type::STREAM + 7).contains(&t) => {
            let (stream_id, n) = decode_varint(&buf[off..])?;
            off += n;
            let offset = if t & STREAM_OFF != 0 {
                let (v, n) = decode_varint(&buf[off..])?;
                off += n;
                v
            } else {
                0
            };
            let data = if t & STREAM_LEN != 0 {
                let (len, n) = decode_varint(&buf[off..])?;
                off += n;
                let end = off.checked_add(len as usize)?;
                if end > buf.len() {
                    return None;
                }
                let d = &buf[off..end];
                off = end;
                d
            } else {
                // No length → data runs to the end of the packet payload.
                let d = &buf[off..];
                off = buf.len();
                d
            };
            Some((
                Frame::Stream {
                    stream_id,
                    offset,
                    fin: t & STREAM_FIN != 0,
                    data,
                },
                off,
            ))
        }
        frame_type::MAX_DATA => {
            let (v, n) = decode_varint(&buf[off..])?;
            Some((Frame::MaxData(v), off + n))
        }
        frame_type::MAX_STREAM_DATA => {
            let (stream_id, n) = decode_varint(&buf[off..])?;
            off += n;
            let (max, n) = decode_varint(&buf[off..])?;
            off += n;
            Some((Frame::MaxStreamData { stream_id, max }, off))
        }
        frame_type::CONNECTION_CLOSE | frame_type::CONNECTION_CLOSE_APP => {
            let application = ty == frame_type::CONNECTION_CLOSE_APP;
            let (error_code, n) = decode_varint(&buf[off..])?;
            off += n;
            let frame_type = if application {
                0
            } else {
                let (ft, n) = decode_varint(&buf[off..])?;
                off += n;
                ft
            };
            let (reason_len, n) = decode_varint(&buf[off..])?;
            off += n;
            let end = off.checked_add(reason_len as usize)?;
            if end > buf.len() {
                return None;
            }
            Some((
                Frame::ConnectionClose {
                    error_code,
                    frame_type,
                    reason: &buf[off..end],
                    application,
                },
                end,
            ))
        }
        frame_type::HANDSHAKE_DONE => Some((Frame::HandshakeDone, off)),
        _ => None, // Unknown/unsupported frame type.
    }
}

/// Encode a CRYPTO frame into `buf`, returning bytes written.
pub fn encode_crypto(offset: u64, data: &[u8], buf: &mut [u8]) -> Option<usize> {
    let mut off = 0;
    off += encode_varint(frame_type::CRYPTO, &mut buf[off..])?;
    off += encode_varint(offset, &mut buf[off..])?;
    off += encode_varint(data.len() as u64, &mut buf[off..])?;
    if buf.len() < off + data.len() {
        return None;
    }
    buf[off..off + data.len()].copy_from_slice(data);
    Some(off + data.len())
}

/// Encode a STREAM frame with explicit offset and length into `buf`.
pub fn encode_stream(
    stream_id: u64,
    offset: u64,
    fin: bool,
    data: &[u8],
    buf: &mut [u8],
) -> Option<usize> {
    let mut ty = frame_type::STREAM | STREAM_LEN;
    if offset != 0 {
        ty |= STREAM_OFF;
    }
    if fin {
        ty |= STREAM_FIN;
    }
    let mut off = 0;
    off += encode_varint(ty, &mut buf[off..])?;
    off += encode_varint(stream_id, &mut buf[off..])?;
    if offset != 0 {
        off += encode_varint(offset, &mut buf[off..])?;
    }
    off += encode_varint(data.len() as u64, &mut buf[off..])?;
    if buf.len() < off + data.len() {
        return None;
    }
    buf[off..off + data.len()].copy_from_slice(data);
    Some(off + data.len())
}

/// Encode a single-byte PING frame.
pub fn encode_ping(buf: &mut [u8]) -> Option<usize> {
    encode_varint(frame_type::PING, buf)
}

/// Encode a HANDSHAKE_DONE frame (server → client, 1-RTT).
pub fn encode_handshake_done(buf: &mut [u8]) -> Option<usize> {
    encode_varint(frame_type::HANDSHAKE_DONE, buf)
}

/// Encode a MAX_DATA frame advertising the connection-level flow-control limit.
pub fn encode_max_data(max: u64, buf: &mut [u8]) -> Option<usize> {
    let mut off = encode_varint(frame_type::MAX_DATA, buf)?;
    off += encode_varint(max, &mut buf[off..])?;
    Some(off)
}

/// Encode an ACK frame (RFC 9000 §19.3). `ranges` are the `(gap, ack_range_len)`
/// pairs after the first range, in descending packet-number order.
pub fn encode_ack(
    largest: u64,
    ack_delay: u64,
    first_range: u64,
    ranges: &[(u64, u64)],
    buf: &mut [u8],
) -> Option<usize> {
    let mut off = encode_varint(frame_type::ACK, buf)?;
    off += encode_varint(largest, &mut buf[off..])?;
    off += encode_varint(ack_delay, &mut buf[off..])?;
    off += encode_varint(ranges.len() as u64, &mut buf[off..])?;
    off += encode_varint(first_range, &mut buf[off..])?;
    for &(gap, len) in ranges {
        off += encode_varint(gap, &mut buf[off..])?;
        off += encode_varint(len, &mut buf[off..])?;
    }
    Some(off)
}

/// Encode a CONNECTION_CLOSE frame. `application` selects the 0x1d (application)
/// vs 0x1c (transport) type; `frame_type_field` is ignored for the
/// application variant.
pub fn encode_connection_close(
    error_code: u64,
    frame_type_field: u64,
    reason: &[u8],
    application: bool,
    buf: &mut [u8],
) -> Option<usize> {
    let ty = if application {
        frame_type::CONNECTION_CLOSE_APP
    } else {
        frame_type::CONNECTION_CLOSE
    };
    let mut off = encode_varint(ty, buf)?;
    off += encode_varint(error_code, &mut buf[off..])?;
    if !application {
        off += encode_varint(frame_type_field, &mut buf[off..])?;
    }
    off += encode_varint(reason.len() as u64, &mut buf[off..])?;
    if buf.len() < off + reason.len() {
        return None;
    }
    buf[off..off + reason.len()].copy_from_slice(reason);
    Some(off + reason.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ping_and_padding() {
        let buf = [0x00u8, 0x00, 0x01]; // PADDING x2 then PING
        let (f, n) = parse_frame(&buf).unwrap();
        assert!(matches!(f, Frame::Padding(2)));
        assert_eq!(n, 2);
        let (f, n) = parse_frame(&buf[2..]).unwrap();
        assert!(matches!(f, Frame::Ping));
        assert_eq!(n, 1);
    }

    #[test]
    fn stream_frame_roundtrip() {
        let mut buf = [0u8; 32];
        let n = encode_stream(4, 8, true, b"hello", &mut buf).unwrap();
        let (f, consumed) = parse_frame(&buf[..n]).unwrap();
        assert_eq!(consumed, n);
        match f {
            Frame::Stream { stream_id, offset, fin, data } => {
                assert_eq!(stream_id, 4);
                assert_eq!(offset, 8);
                assert!(fin);
                assert_eq!(data, b"hello");
            }
            _ => panic!("expected stream frame"),
        }
    }

    #[test]
    fn ack_frame_roundtrip() {
        let mut buf = [0u8; 32];
        // Largest 100, first range covers 100..=98, then one more range.
        let n = encode_ack(100, 3, 2, &[(1, 4)], &mut buf).unwrap();
        let (f, consumed) = parse_frame(&buf[..n]).unwrap();
        assert_eq!(consumed, n);
        match f {
            Frame::Ack {
                largest,
                delay,
                first_range,
                ranges,
            } => {
                assert_eq!(largest, 100);
                assert_eq!(delay, 3);
                assert_eq!(first_range, 2);
                assert_eq!(ranges, alloc::vec![(1u64, 4u64)]);
            }
            _ => panic!("expected ack frame"),
        }
    }

    #[test]
    fn crypto_frame_roundtrip() {
        let mut buf = [0u8; 32];
        let n = encode_crypto(0, b"\x16\x03", &mut buf).unwrap();
        let (f, consumed) = parse_frame(&buf[..n]).unwrap();
        assert_eq!(consumed, n);
        match f {
            Frame::Crypto { offset, data } => {
                assert_eq!(offset, 0);
                assert_eq!(data, b"\x16\x03");
            }
            _ => panic!("expected crypto frame"),
        }
    }
}
