//! QUIC packet number spaces (RFC 9000 §12.3, §17.1, Appendix A).
//!
//! Mirrors `net/quic/pnspace.{c,h}`. Each encryption level has an independent
//! packet number space: numbers start at 0, increase by one per packet sent,
//! and are acknowledged independently. The wire encoding truncates the number
//! to the fewest bytes that unambiguously express it relative to the largest
//! acknowledged number.

/// The three packet number spaces (RFC 9000 §12.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PnSpaceKind {
    Initial = 0,
    Handshake = 1,
    Application = 2,
}

/// Per-space sender/receiver state.
#[derive(Debug, Default)]
pub struct PnSpace {
    /// Next packet number to assign when sending.
    pub next_pn: u64,
    /// Largest packet number acknowledged by the peer (for this space).
    pub largest_acked: Option<u64>,
    /// Largest packet number received from the peer (drives PN decoding and
    /// ACK generation).
    pub largest_received: Option<u64>,
}

impl PnSpace {
    pub fn new() -> Self {
        Self::default()
    }

    /// Allocate the next packet number for an outgoing packet.
    pub fn take_pn(&mut self) -> u64 {
        let pn = self.next_pn;
        self.next_pn += 1;
        pn
    }

    /// Record receipt of `pn`, advancing `largest_received`.
    pub fn on_received(&mut self, pn: u64) {
        self.largest_received = Some(match self.largest_received {
            Some(cur) => cur.max(pn),
            None => pn,
        });
    }
}

/// Number of bytes needed to encode `pn` given the `largest_acked` number, per
/// RFC 9000 §17.1: the encoding must cover more than twice the range between
/// the new number and the largest acked.
pub fn pn_encode_len(pn: u64, largest_acked: Option<u64>) -> usize {
    let range = match largest_acked {
        Some(acked) => pn.saturating_sub(acked).saturating_mul(2),
        None => pn.saturating_mul(2).max(1),
    };
    if range < (1 << 8) {
        1
    } else if range < (1 << 16) {
        2
    } else if range < (1 << 24) {
        3
    } else {
        4
    }
}

/// Reconstruct the full packet number from the `truncated` value of
/// `pn_nbits` bits, given the `largest_pn` already processed in this space.
///
/// This is the `DecodePacketNumber` algorithm from RFC 9000 Appendix A.3:
/// pick the candidate congruent to `truncated` (mod 2^nbits) that is closest
/// to the expected next number.
pub fn pn_decode(largest_pn: u64, truncated: u64, pn_nbits: u32) -> u64 {
    let expected = largest_pn.wrapping_add(1);
    let pn_win = 1u64 << pn_nbits;
    let pn_hwin = pn_win / 2;
    let pn_mask = pn_win - 1;

    let candidate = (expected & !pn_mask) | truncated;
    if candidate.wrapping_add(pn_hwin) <= expected
        && candidate < (1u64 << 62).wrapping_sub(pn_win)
    {
        candidate.wrapping_add(pn_win)
    } else if candidate > expected.wrapping_add(pn_hwin) && candidate >= pn_win {
        candidate.wrapping_sub(pn_win)
    } else {
        candidate
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_example_from_rfc() {
        // RFC 9000 §A.3: largest_pn = 0xa82f30ea, truncated 2 bytes 0x9b32
        // → 0xa82f9b32.
        let pn = pn_decode(0xa82f_30ea, 0x9b32, 16);
        assert_eq!(pn, 0xa82f_9b32);
    }

    #[test]
    fn encode_len_grows_with_range() {
        assert_eq!(pn_encode_len(1, Some(0)), 1);
        assert_eq!(pn_encode_len(1000, Some(0)), 2);
        assert_eq!(pn_encode_len(1 << 20, Some(0)), 3);
    }
}
