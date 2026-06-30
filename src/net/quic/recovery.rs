//! QUIC loss detection and recovery (RFC 9002).
//!
//! Operates on a packet number space's sent-packet map together with the
//! congestion controller and RTT estimator: records sent packets, processes
//! incoming ACKs (RTT sample + congestion feedback), detects lost packets by
//! the packet-number and time thresholds, and computes the Probe Timeout.

use super::cong::Cong;
use super::pnspace::{PnSpace, SentInfo};
use super::timer::RttEstimator;
use alloc::vec::Vec;

/// A packet is declared lost if a later packet this many numbers ahead is
/// acknowledged (RFC 9002 §6.1.1).
pub const PACKET_THRESHOLD: u64 = 3;
/// Time threshold = 9/8 × max(smoothed_rtt, latest_rtt) (RFC 9002 §6.1.2).
const TIME_THRESHOLD_NUM: u64 = 9;
const TIME_THRESHOLD_DEN: u64 = 8;

/// Record a sent packet for loss recovery. The caller separately updates the
/// congestion controller's in-flight bytes via `Cong::on_packet_sent`.
pub fn on_packet_sent(space: &mut PnSpace, pn: u64, now: u64, ack_eliciting: bool, size: u64) {
    space.sent.insert(
        pn,
        SentInfo {
            time_sent: now,
            ack_eliciting,
            size,
        },
    );
}

/// Process an incoming ACK for this space: remove acknowledged packets, feed
/// their bytes to the congestion controller, and take an RTT sample from the
/// largest newly-acknowledged ack-eliciting packet (RFC 9002 §5).
///
/// `acked` are inclusive `(low, high)` packet-number ranges; `ack_delay` is the
/// peer's reported delay in the same time unit as `now` (ms here).
pub fn on_ack_received(
    space: &mut PnSpace,
    cong: &mut Cong,
    rtt: &mut RttEstimator,
    largest_acked: u64,
    ack_delay: u64,
    now: u64,
    acked: &[(u64, u64)],
) {
    space.largest_acked = Some(match space.largest_acked {
        Some(c) => c.max(largest_acked),
        None => largest_acked,
    });

    let mut largest_newly_acked_time: Option<u64> = None;
    for &(lo, hi) in acked {
        for pn in lo..=hi {
            if let Some(info) = space.sent.remove(&pn) {
                if info.ack_eliciting {
                    cong.on_ack(info.size);
                }
                if pn == largest_acked {
                    largest_newly_acked_time = Some(info.time_sent);
                }
            }
        }
    }

    if let Some(ts) = largest_newly_acked_time {
        let sample = now.saturating_sub(ts);
        rtt.update(sample, ack_delay);
    }
}

/// Detect and remove lost packets, feeding their bytes to the congestion
/// controller (`Cong::on_loss`). Returns the lost packet numbers.
///
/// Lost packets are processed in descending order so the congestion window is
/// halved at most once per loss episode (keyed off the largest lost number).
pub fn detect_lost(space: &mut PnSpace, cong: &mut Cong, now: u64, rtt: &RttEstimator) -> Vec<u64> {
    let largest_acked = match space.largest_acked {
        Some(l) => l,
        None => return Vec::new(),
    };

    let loss_delay =
        (TIME_THRESHOLD_NUM * rtt.smoothed_rtt.max(rtt.latest_rtt) / TIME_THRESHOLD_DEN).max(1);

    let mut lost: Vec<u64> = space
        .sent
        .iter()
        .filter(|(&pn, info)| {
            pn <= largest_acked
                && (largest_acked - pn >= PACKET_THRESHOLD
                    || now.saturating_sub(info.time_sent) >= loss_delay)
        })
        .map(|(&pn, _)| pn)
        .collect();

    lost.sort_unstable_by(|a, b| b.cmp(a)); // descending
    for &pn in &lost {
        if let Some(info) = space.sent.remove(&pn) {
            if info.ack_eliciting {
                cong.on_loss(info.size, pn);
            }
        }
    }
    lost
}

/// Probe Timeout for the current consecutive-PTO `pto_count` (RFC 9002 §6.2.1):
/// the base PTO doubled per backoff step.
pub fn pto_duration(rtt: &RttEstimator, pto_count: u32) -> u64 {
    rtt.pto().saturating_mul(1u64 << pto_count.min(16))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::pnspace::PnSpace;

    #[test]
    fn ack_removes_sent_and_samples_rtt() {
        let mut space = PnSpace::new();
        let mut cong = Cong::new(1452);
        let mut rtt = RttEstimator::new(0);
        on_packet_sent(&mut space, 0, 0, true, 1200);
        on_packet_sent(&mut space, 1, 10, true, 1200);
        cong.on_packet_sent(2400);

        on_ack_received(&mut space, &mut cong, &mut rtt, 1, 0, 60, &[(0, 1)]);
        assert!(space.sent.is_empty());
        // RTT sampled from the largest acked (pn 1, sent at t=10, acked at 60).
        assert_eq!(rtt.smoothed_rtt, 50);
    }

    #[test]
    fn packet_threshold_declares_loss() {
        let mut space = PnSpace::new();
        let mut cong = Cong::new(1452);
        let mut rtt = RttEstimator::new(0);
        for pn in 0..=4u64 {
            on_packet_sent(&mut space, pn, pn, true, 1200);
        }
        cong.on_packet_sent(6000);
        // Ack pn 4 → pns 0 and 1 are >= 3 behind, declared lost.
        on_ack_received(&mut space, &mut cong, &mut rtt, 4, 0, 100, &[(4, 4)]);
        let lost = detect_lost(&mut space, &mut cong, 100, &rtt);
        assert!(lost.contains(&0) && lost.contains(&1));
        assert!(!lost.contains(&2)); // only 2 behind the largest acked
    }
}
