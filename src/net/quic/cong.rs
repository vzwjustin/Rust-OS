//! QUIC congestion control (RFC 9002 §7) — NewReno.
//!
//! Mirrors `net/quic/cong.{c,h}`. Tracks the congestion window and slow-start
//! threshold, growing the window in slow start and additively in congestion
//! avoidance, and halving it on a loss (multiplicative decrease).

/// Default initial window: 10 packets, bounded by 14720 bytes (RFC 9002 §7.2).
const INITIAL_WINDOW: u64 = 10 * 1452;
/// Minimum congestion window: 2 packets (RFC 9002 §7.2).
const MIN_WINDOW: u64 = 2 * 1452;
/// Loss reduction factor (RFC 9002 §7.3.2).
const LOSS_REDUCTION_NUM: u64 = 1;
const LOSS_REDUCTION_DEN: u64 = 2;

#[derive(Debug)]
pub struct Cong {
    /// Congestion window in bytes.
    pub cwnd: u64,
    /// Slow-start threshold; `u64::MAX` means "no threshold yet" (slow start).
    pub ssthresh: u64,
    /// Bytes sent but not yet acknowledged.
    pub bytes_in_flight: u64,
    /// Max datagram size used to scale the window.
    pub max_datagram: u64,
    /// Packet number after which a new loss episode may begin (recovery).
    pub recovery_start_pn: Option<u64>,
}

impl Cong {
    pub fn new(max_datagram: u64) -> Self {
        Self {
            cwnd: INITIAL_WINDOW,
            ssthresh: u64::MAX,
            bytes_in_flight: 0,
            max_datagram: max_datagram.max(1200),
            recovery_start_pn: None,
        }
    }

    pub fn on_packet_sent(&mut self, bytes: u64) {
        self.bytes_in_flight += bytes;
    }

    /// Whether `bytes` more may be sent without exceeding the window.
    pub fn can_send(&self, bytes: u64) -> bool {
        self.bytes_in_flight + bytes <= self.cwnd
    }

    /// Process an acknowledgement of `acked` bytes.
    pub fn on_ack(&mut self, acked: u64) {
        self.bytes_in_flight = self.bytes_in_flight.saturating_sub(acked);
        if self.cwnd < self.ssthresh {
            // Slow start: exponential growth.
            self.cwnd += acked;
        } else {
            // Congestion avoidance: roughly one MSS per RTT.
            self.cwnd += self.max_datagram * acked / self.cwnd.max(1);
        }
    }

    /// Enter a loss episode for a packet numbered `pn`, halving the window
    /// once per episode (RFC 9002 §7.3.2).
    pub fn on_loss(&mut self, lost: u64, pn: u64) {
        self.bytes_in_flight = self.bytes_in_flight.saturating_sub(lost);
        let in_recovery = matches!(self.recovery_start_pn, Some(start) if pn <= start);
        if in_recovery {
            return; // already reduced for this episode
        }
        self.recovery_start_pn = Some(pn);
        self.ssthresh = (self.cwnd * LOSS_REDUCTION_NUM / LOSS_REDUCTION_DEN).max(MIN_WINDOW);
        self.cwnd = self.ssthresh;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slow_start_then_loss_halves() {
        let mut c = Cong::new(1452);
        let start = c.cwnd;
        c.on_packet_sent(3000);
        c.on_ack(3000);
        assert!(c.cwnd > start); // grew in slow start
        let before = c.cwnd;
        c.on_loss(0, 100);
        assert!(c.cwnd < before);
        assert_eq!(c.cwnd, c.ssthresh);
    }

    #[test]
    fn one_reduction_per_episode() {
        let mut c = Cong::new(1452);
        c.on_loss(0, 100);
        let after_first = c.cwnd;
        c.on_loss(0, 50); // same/earlier pn → no further reduction
        assert_eq!(c.cwnd, after_first);
    }
}
