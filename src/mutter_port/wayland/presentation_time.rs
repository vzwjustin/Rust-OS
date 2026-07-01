//! GNOME src/wayland/meta-wayland-presentation-time.c
//!
//! Implements wp_presentation / wp_presentation_feedback. A client requests
//! feedback for a surface's next commit; the compositor later either
//! `presented` it (with a timestamp, refresh interval and output sequence) or
//! `discarded` it (the content was superseded before display).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-presentation-time.c

use alloc::{collections::BTreeMap, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};

/// wp_presentation_feedback.presented `flags` bitfield.
pub const KIND_VSYNC: u32 = 0x1;
pub const KIND_HW_CLOCK: u32 = 0x2;
pub const KIND_HW_COMPLETION: u32 = 0x4;
pub const KIND_ZERO_COPY: u32 = 0x8;

/// Lifecycle of a feedback object.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FeedbackState {
    Pending,
    Presented,
    Discarded,
}

/// A single wp_presentation_feedback request tied to a surface commit.
#[derive(Debug, Clone, Copy)]
pub struct PresentationFeedback {
    pub id: u32,
    pub surface_id: u32,
    pub state: FeedbackState,
    /// Presentation timestamp (seconds, nanoseconds) once presented.
    pub tv_sec: u64,
    pub tv_nsec: u32,
    /// Refresh interval in nanoseconds.
    pub refresh_ns: u32,
    /// Output sequence counter (64-bit, split hi/lo on the wire).
    pub seq: u64,
    pub flags: u32,
}

impl PresentationFeedback {
    pub fn new(id: u32, surface_id: u32) -> Self {
        PresentationFeedback {
            id,
            surface_id,
            state: FeedbackState::Pending,
            tv_sec: 0,
            tv_nsec: 0,
            refresh_ns: 0,
            seq: 0,
            flags: 0,
        }
    }

    /// wp_presentation_feedback.presented.
    pub fn present(&mut self, tv_sec: u64, tv_nsec: u32, refresh_ns: u32, seq: u64, flags: u32) {
        self.state = FeedbackState::Presented;
        self.tv_sec = tv_sec;
        self.tv_nsec = tv_nsec;
        self.refresh_ns = refresh_ns;
        self.seq = seq;
        self.flags = flags;
    }

    /// meta_wayland_presentation_feedback_discard.
    pub fn discard(&mut self) {
        self.state = FeedbackState::Discarded;
    }
}

/// Tracks feedback objects awaiting the next presentation of each surface.
pub struct PresentationTimeManager {
    /// feedback id -> feedback.
    feedbacks: BTreeMap<u32, PresentationFeedback>,
    next_id: AtomicU32,
    /// Per-surface output sequence tracking for validity checks.
    last_sequence: BTreeMap<u32, u32>,
    refresh_ns: u32,
}

impl PresentationTimeManager {
    pub fn new() -> Self {
        PresentationTimeManager {
            feedbacks: BTreeMap::new(),
            next_id: AtomicU32::new(1),
            last_sequence: BTreeMap::new(),
            refresh_ns: 16_666_666, // ~60Hz default
        }
    }

    /// wp_presentation.feedback - register a feedback for a surface commit.
    pub fn request_feedback(&mut self, surface_id: u32) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::Release);
        self.feedbacks
            .insert(id, PresentationFeedback::new(id, surface_id));
        id
    }

    pub fn get(&self, id: u32) -> Option<&PresentationFeedback> {
        self.feedbacks.get(&id)
    }

    /// Present all pending feedbacks for a surface and remove them, returning
    /// the completed feedback records for wire delivery.
    ///
    /// STUB: timing values would come from ClutterFrameInfo; here they're
    /// supplied by the caller.
    pub fn present_surface(
        &mut self,
        surface_id: u32,
        tv_sec: u64,
        tv_nsec: u32,
        sequence: u32,
        flags: u32,
    ) -> Vec<PresentationFeedback> {
        self.last_sequence.insert(surface_id, sequence);
        let refresh = self.refresh_ns;
        let ids: Vec<u32> = self
            .feedbacks
            .iter()
            .filter(|(_, f)| f.surface_id == surface_id && f.state == FeedbackState::Pending)
            .map(|(id, _)| *id)
            .collect();

        let mut done = Vec::new();
        for id in ids {
            if let Some(mut f) = self.feedbacks.remove(&id) {
                f.present(tv_sec, tv_nsec, refresh, sequence as u64, flags);
                done.push(f);
            }
        }
        done
    }

    /// Discard all pending feedbacks for a surface (content superseded).
    pub fn discard_surface(&mut self, surface_id: u32) -> Vec<PresentationFeedback> {
        let ids: Vec<u32> = self
            .feedbacks
            .iter()
            .filter(|(_, f)| f.surface_id == surface_id && f.state == FeedbackState::Pending)
            .map(|(id, _)| *id)
            .collect();

        let mut done = Vec::new();
        for id in ids {
            if let Some(mut f) = self.feedbacks.remove(&id) {
                f.discard();
                done.push(f);
            }
        }
        done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_present() {
        let mut mgr = PresentationTimeManager::new();
        let _f = mgr.request_feedback(10);
        let done = mgr.present_surface(10, 100, 5, 42, KIND_VSYNC);
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].state, FeedbackState::Presented);
        assert_eq!(done[0].seq, 42);
        assert_eq!(done[0].flags, KIND_VSYNC);
        // Consumed.
        assert!(mgr.present_surface(10, 0, 0, 0, 0).is_empty());
    }

    #[test]
    fn test_discard() {
        let mut mgr = PresentationTimeManager::new();
        mgr.request_feedback(10);
        let done = mgr.discard_surface(10);
        assert_eq!(done.len(), 1);
        assert_eq!(done[0].state, FeedbackState::Discarded);
    }

    #[test]
    fn test_isolated_surfaces() {
        let mut mgr = PresentationTimeManager::new();
        mgr.request_feedback(10);
        mgr.request_feedback(20);
        assert_eq!(mgr.present_surface(10, 1, 1, 1, 0).len(), 1);
        // Surface 20's feedback untouched.
        assert_eq!(mgr.discard_surface(20).len(), 1);
    }
}
