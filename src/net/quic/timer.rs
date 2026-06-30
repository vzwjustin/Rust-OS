//! QUIC loss-detection and RTT estimation (RFC 9002 §5, §6).
//!
//! Mirrors `net/quic/timer.{c,h}`. Maintains the smoothed RTT estimate and
//! derives the Probe Timeout (PTO) used to arm retransmission/probe timers.

/// RTT estimator state (RFC 9002 §5). Times are in milliseconds.
#[derive(Debug)]
pub struct RttEstimator {
    pub latest_rtt: u64,
    pub smoothed_rtt: u64,
    pub rttvar: u64,
    pub min_rtt: u64,
    pub max_ack_delay: u64,
    has_sample: bool,
}

/// Default initial RTT before any sample (RFC 9002 §6.2.2): 333ms.
const INITIAL_RTT: u64 = 333;
/// Timer granularity (RFC 9002 §6.2): 1ms.
const GRANULARITY: u64 = 1;

impl RttEstimator {
    pub fn new(max_ack_delay: u64) -> Self {
        Self {
            latest_rtt: 0,
            smoothed_rtt: INITIAL_RTT,
            rttvar: INITIAL_RTT / 2,
            min_rtt: 0,
            max_ack_delay,
            has_sample: false,
        }
    }

    /// Fold a new RTT sample in, applying the peer's `ack_delay` (RFC 9002
    /// §5.3).
    pub fn update(&mut self, rtt_sample: u64, ack_delay: u64) {
        self.latest_rtt = rtt_sample;
        if !self.has_sample {
            self.min_rtt = rtt_sample;
            self.smoothed_rtt = rtt_sample;
            self.rttvar = rtt_sample / 2;
            self.has_sample = true;
            return;
        }
        self.min_rtt = self.min_rtt.min(rtt_sample);
        // Only subtract ack_delay if doing so keeps the sample above min_rtt.
        let adjusted = if rtt_sample >= self.min_rtt + ack_delay {
            rtt_sample - ack_delay
        } else {
            rtt_sample
        };
        // rttvar = 3/4 rttvar + 1/4 |smoothed - adjusted|
        let diff = self.smoothed_rtt.abs_diff(adjusted);
        self.rttvar = (3 * self.rttvar + diff) / 4;
        // smoothed = 7/8 smoothed + 1/8 adjusted
        self.smoothed_rtt = (7 * self.smoothed_rtt + adjusted) / 8;
    }

    /// Probe Timeout duration (RFC 9002 §6.2.1):
    /// smoothed_rtt + max(4*rttvar, granularity) + max_ack_delay.
    pub fn pto(&self) -> u64 {
        self.smoothed_rtt + (4 * self.rttvar).max(GRANULARITY) + self.max_ack_delay
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_sample_sets_smoothed() {
        let mut r = RttEstimator::new(25);
        r.update(100, 0);
        assert_eq!(r.smoothed_rtt, 100);
        assert_eq!(r.min_rtt, 100);
        assert!(r.pto() >= 100);
    }
}
