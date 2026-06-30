//! QUIC streams (RFC 9000 §2, §3).
//!
//! Mirrors `net/quic/stream.{c,h}`. A stream ID's two least-significant bits
//! encode the initiator (client/server) and directionality (bidi/uni); the
//! remaining bits are the per-category sequence number.

use alloc::collections::{BTreeMap, VecDeque};
use alloc::vec::Vec;

/// Which endpoint opened a stream (RFC 9000 §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamInitiator {
    Client,
    Server,
}

/// Stream directionality (RFC 9000 §2.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamDir {
    Bidirectional,
    Unidirectional,
}

/// Decompose a stream ID into (initiator, direction).
pub fn stream_kind(stream_id: u64) -> (StreamInitiator, StreamDir) {
    let initiator = if stream_id & 0x01 == 0 {
        StreamInitiator::Client
    } else {
        StreamInitiator::Server
    };
    let dir = if stream_id & 0x02 == 0 {
        StreamDir::Bidirectional
    } else {
        StreamDir::Unidirectional
    };
    (initiator, dir)
}

/// Compose the Nth stream ID for the given initiator/direction.
pub fn make_stream_id(index: u64, initiator: StreamInitiator, dir: StreamDir) -> u64 {
    let init_bit = if initiator == StreamInitiator::Server { 1 } else { 0 };
    let dir_bit = if dir == StreamDir::Unidirectional { 2 } else { 0 };
    (index << 2) | dir_bit | init_bit
}

/// Send-side stream state machine (RFC 9000 §3.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SendState {
    Ready,
    Send,
    DataSent,
    DataReceived, // all data acknowledged
    ResetSent,
    ResetReceived,
}

/// Receive-side stream state machine (RFC 9000 §3.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecvState {
    Recv,
    SizeKnown,
    DataReceived,
    DataRead,
    ResetReceived,
    ResetRead,
}

/// A single QUIC stream with independent flow-controlled send/receive halves.
#[derive(Debug)]
pub struct Stream {
    pub id: u64,
    pub send_state: SendState,
    pub recv_state: RecvState,
    /// Bytes handed to the send half (next send offset).
    pub send_offset: u64,
    /// Peer's flow-control limit for our sending on this stream.
    pub send_max_data: u64,
    /// In-order bytes delivered to the application from the receive half.
    pub recv_offset: u64,
    /// Flow-control limit we advertise to the peer for this stream.
    pub recv_max_data: u64,
    /// Final size once a FIN/RESET fixes it.
    pub final_size: Option<u64>,
    /// Buffered outgoing bytes not yet packetized.
    pub send_buf: VecDeque<u8>,
    /// Out-of-order received chunks keyed by their start offset, awaiting the
    /// gap before them to be filled.
    recv_buf: BTreeMap<u64, Vec<u8>>,
}

impl Stream {
    pub fn new(id: u64, initial_max_data: u64) -> Self {
        Self {
            id,
            send_state: SendState::Ready,
            recv_state: RecvState::Recv,
            send_offset: 0,
            send_max_data: initial_max_data,
            recv_offset: 0,
            recv_max_data: initial_max_data,
            final_size: None,
            send_buf: VecDeque::new(),
            recv_buf: BTreeMap::new(),
        }
    }

    /// Accept received STREAM data at `offset`, buffering out-of-order pieces,
    /// and return the bytes that are now deliverable in order (advancing
    /// `recv_offset`). `fin` fixes the final size at the end of this chunk.
    ///
    /// Duplicate or already-delivered bytes are dropped. Chunks are keyed by
    /// start offset; contiguous chunks are coalesced as gaps fill.
    pub fn recv(&mut self, offset: u64, data: &[u8], fin: bool) -> Vec<u8> {
        let end = offset + data.len() as u64;
        if fin {
            self.final_size = Some(end);
            if self.recv_state == RecvState::Recv {
                self.recv_state = RecvState::SizeKnown;
            }
        }
        // Drop data entirely below what we've already delivered.
        if end <= self.recv_offset {
            return Vec::new();
        }
        // Trim a prefix that overlaps already-delivered bytes.
        let (offset, data) = if offset < self.recv_offset {
            let skip = (self.recv_offset - offset) as usize;
            (self.recv_offset, &data[skip..])
        } else {
            (offset, data)
        };
        if !data.is_empty() {
            // Keep the longer chunk if one already starts here.
            let replace = self
                .recv_buf
                .get(&offset)
                .map_or(true, |existing| existing.len() < data.len());
            if replace {
                self.recv_buf.insert(offset, data.to_vec());
            }
        }

        // Deliver every chunk that now starts exactly at recv_offset, skipping
        // any portion already covered by a previous (longer) chunk.
        let mut delivered = Vec::new();
        while let Some((&start, _)) = self.recv_buf.range(..=self.recv_offset).next_back() {
            let chunk = self.recv_buf.remove(&start).unwrap();
            let chunk_end = start + chunk.len() as u64;
            if chunk_end <= self.recv_offset {
                continue; // fully superseded
            }
            let skip = (self.recv_offset - start) as usize;
            delivered.extend_from_slice(&chunk[skip..]);
            self.recv_offset = chunk_end;
        }

        if let Some(fs) = self.final_size {
            if self.recv_offset >= fs && self.recv_state == RecvState::SizeKnown {
                self.recv_state = RecvState::DataReceived;
            }
        }
        delivered
    }

    /// Queue `data` for sending, respecting the peer's flow-control limit.
    /// Returns the number of bytes accepted.
    pub fn write(&mut self, data: &[u8]) -> usize {
        let window = self.send_max_data.saturating_sub(self.send_offset);
        let n = core::cmp::min(window as usize, data.len());
        self.send_buf.extend(&data[..n]);
        self.send_offset += n as u64;
        if n > 0 && self.send_state == SendState::Ready {
            self.send_state = SendState::Send;
        }
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_id_roundtrip() {
        // Client-initiated bidi stream #0 is id 0; server uni #0 is id 3.
        assert_eq!(
            make_stream_id(0, StreamInitiator::Client, StreamDir::Bidirectional),
            0
        );
        assert_eq!(
            make_stream_id(0, StreamInitiator::Server, StreamDir::Unidirectional),
            3
        );
        let (init, dir) = stream_kind(3);
        assert_eq!(init, StreamInitiator::Server);
        assert_eq!(dir, StreamDir::Unidirectional);
    }

    #[test]
    fn write_respects_flow_control() {
        let mut s = Stream::new(0, 4);
        assert_eq!(s.write(b"hello"), 4); // capped at send_max_data
        assert_eq!(s.send_offset, 4);
        assert_eq!(s.write(b"x"), 0); // window exhausted
    }

    #[test]
    fn recv_reassembles_out_of_order() {
        let mut s = Stream::new(0, 1024);
        // Out-of-order: [4..8) arrives first and is buffered.
        assert_eq!(s.recv(4, b"defg", false), b"");
        assert_eq!(s.recv_offset, 0);
        // The gap [0..4) fills → both chunks deliver in order.
        assert_eq!(s.recv(0, b"abcd", false), b"abcddefg");
        assert_eq!(s.recv_offset, 8);
    }

    #[test]
    fn recv_drops_duplicate_and_trims_overlap() {
        let mut s = Stream::new(0, 1024);
        assert_eq!(s.recv(0, b"abcd", false), b"abcd");
        // Fully duplicate.
        assert_eq!(s.recv(0, b"abcd", false), b"");
        // Overlapping: [2..6) — only [4..6) is new.
        assert_eq!(s.recv(2, b"cdef", false), b"ef");
        assert_eq!(s.recv_offset, 6);
    }

    #[test]
    fn recv_fin_marks_final_size() {
        let mut s = Stream::new(0, 1024);
        assert_eq!(s.recv(0, b"hello", true), b"hello");
        assert_eq!(s.final_size, Some(5));
        assert_eq!(s.recv_state, RecvState::DataReceived);
    }
}
