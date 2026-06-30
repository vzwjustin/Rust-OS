//! QUIC transport protocol (RFC 9000 / 9001 / 9002).
//!
//! A native Rust QUIC implementation for RustOS, structured to mirror the
//! in-kernel Linux QUIC module (github.com/lxin/quic, `net/quic/`): each C
//! translation unit there has a counterpart module here.
//!
//! | lxin/quic          | RustOS module        |
//! |--------------------|----------------------|
//! | common.{c,h}       | [`common`]           |
//! | connid.{c,h}       | [`connid`]           |
//! | pnspace.{c,h}      | [`pnspace`]          |
//! | packet.{c,h}       | [`packet`]           |
//! | frame.{c,h}        | [`frame`]            |
//! | stream.{c,h}       | [`stream`]           |
//! | cong.{c,h}         | [`cong`]             |
//! | path.{c,h}         | [`path`]             |
//! | timer.{c,h}        | [`timer`]            |
//! | crypto.{c,h}       | [`crypto`]           |
//! | socket/protocol.c  | [`Connection`]       |
//!
//! QUIC runs over UDP. As with the upstream module, the TLS 1.3 handshake is
//! offloaded to userspace and the kernel owns the data path (packet/frame
//! processing, streams, flow control, congestion control, loss recovery).
//!
//! ## Status
//! Implemented and unit-tested: variable-length integers, long/short packet
//! header parsing, the core frame wire formats, packet number spaces and PN
//! decoding, stream-ID semantics and per-stream flow control, NewReno
//! congestion control, RTT/PTO estimation, path validation, CRYPTO-stream
//! reassembly, and the connection state object.
//!
//! Follow-up phases: AEAD packet protection + header protection (RFC 9001),
//! the UDP socket glue in [`super::socket`], full ACK/loss-recovery scheduling,
//! and the userspace handshake hand-off.

pub mod common;
pub mod cong;
pub mod connection;
pub mod connid;
pub mod crypto;
pub mod endpoint;
pub mod frame;
pub mod io;
pub mod keys;
pub mod packet;
pub mod path;
pub mod pnspace;
pub mod protection;
pub mod recovery;
pub mod send;
pub mod stream;
pub mod timer;
pub mod udp;

pub use common::{QUIC_VERSION_1, QUIC_VERSION_2};
pub use connection::{ConnState, Connection, Role};
pub use connid::ConnectionId;
pub use endpoint::{DemuxResult, QuicEndpoint};
pub use packet::{parse_header, PacketHeader};

/// IP protocol number used by the in-kernel QUIC socket family
/// (matches lxin/quic's `IPPROTO_QUIC`).
pub const IPPROTO_QUIC: u8 = 144;

/// Monotonic millisecond timestamp used for RTT/loss recovery.
pub fn now_ms() -> u64 {
    crate::time::uptime_ms()
}

/// Default QUIC server UDP port for HTTP/3 (RFC 9114 uses 443; this is the
/// conventional test port).
pub const DEFAULT_QUIC_PORT: u16 = 443;

/// Route an inbound UDP payload that carries a QUIC packet to its destination
/// connection ID, returning the parsed public header.
///
/// This is the entry point the UDP layer calls once QUIC sockets are wired in;
/// `local_cid_len` is the connection-ID length this endpoint issued, needed to
/// locate the DCID in short-header (1-RTT) packets.
pub fn peek_destination(datagram: &[u8], local_cid_len: usize) -> Option<ConnectionId> {
    match parse_header(datagram, local_cid_len)? {
        PacketHeader::Long { dcid, .. } => Some(dcid),
        PacketHeader::Short { dcid, .. } => Some(dcid),
        PacketHeader::VersionNegotiation { dcid, .. } => Some(dcid),
    }
}
