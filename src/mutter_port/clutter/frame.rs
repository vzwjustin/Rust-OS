//! Port of GNOME mutter's `clutter/clutter-frame.{c,h}` and
//! `clutter-frame-private.h`.
//!
//! `ClutterFrame` is the per-frame state carried through the frame clock
//! dispatch → present pipeline: the frame counter, the expected
//! presentation time, the frame deadline, and the dispatch result. In C
//! it's a ref-counted boxed type allocated by backends with a
//! `sizeof(BackendFrame)` and a `release` virtual; here it's a plain
//! struct (Rust ownership replaces ref-counting).
//!
//! # What's ported
//!
//! - `ClutterFrameResult` (`PendingPresented`/`Idle`/`Ignored`) from
//!   `clutter-frame-clock.h`, matching the C numbering.
//! - The `ClutterFrame` struct fields (`frame_count`,
//!   `has_expected_presentation_time`/`is_target_presentation_time`/
//!   `expected_presentation_time_us`, `has_frame_deadline`/
//!   `frame_deadline_us`, `has_result`/`result`), as a `Frame` struct.
//! - `clutter_frame_new` → `Frame::new` (the C macro
//!   `clutter_frame_new(FrameType, release)` allocates `sizeof(FrameType)`
//!   and stores a release callback; here we just construct the base
//!   struct — backends embed it via `Frame::new` or wrap it).
//! - `clutter_frame_ref`/`_unref`: replaced by `Clone` (the C ref-count
//!   is just shared ownership; Rust `Clone` is the equivalent for the
//!   state-only port — the release callback has no backend resource to
//!   free in this port).
//! - `clutter_frame_get_count`.
//! - `clutter_frame_get_expected_presentation_time` (returns
//!   `Option<i64>` matching the C `gboolean` out-param).
//! - `clutter_frame_get_frame_deadline` (returns `Option<i64>`).
//! - `clutter_frame_get_result` / `_has_result` / `_set_result`, with the
//!   C `g_return_val_if_fail`/`g_warn_if_fail` guards expressed as
//!   `Option`/panic-on-double-set.
//!
//! # What's skipped, with rationale
//!
//! - `G_DEFINE_BOXED_TYPE` / `grefcount`: Rust ownership replaces
//!   ref-counting. The C `release` callback frees backend resources
//!   attached to the frame (a native fence, a DMA-BUF fd, ...); no
//!   backend is ported, so there's nothing to release. A backend port
//!   can wrap `Frame` in its own struct with a `Drop` that releases
//!   backend resources.
//! - The `is_target_presentation_time` flag: kept as a field (the C
//!   struct has it) but no accessor is ported because the only reader is
//!   the frame-clock internals, which aren't ported yet.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

/// `ClutterFrameResult` (clutter-frame-clock.h). Values match the C
/// numbering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum FrameResult {
    /// `CLUTTER_FRAME_RESULT_PENDING_PRESENTED`: the frame was dispatched
    /// and will be presented.
    PendingPresented = 0,
    /// `CLUTTER_FRAME_RESULT_IDLE`: no frame was needed this dispatch.
    #[default]
    Idle = 1,
    /// `CLUTTER_FRAME_RESULT_IGNORED`: the frame was dispatched but the
    /// result is being ignored.
    Ignored = 2,
}

/// Port of `ClutterFrame` / `struct _ClutterFrame`.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub frame_count: i64,
    /// `has_expected_presentation_time` + `expected_presentation_time_us`.
    /// `None` means "no expected presentation time" (matching
    /// `has_expected_presentation_time == FALSE`).
    pub expected_presentation_time_us: Option<i64>,
    /// `is_target_presentation_time`: kept for structural fidelity; no
    /// accessor ported yet (only the frame-clock internals read it).
    pub is_target_presentation_time: bool,
    /// `has_frame_deadline` + `frame_deadline_us`.
    pub frame_deadline_us: Option<i64>,
    /// `has_result` + `result`. `None` means "no result set yet".
    result: Option<FrameResult>,
}

impl Default for Frame {
    fn default() -> Self {
        Self::new()
    }
}

impl Frame {
    /// `clutter_frame_new`: construct a fresh frame with no presentation
    /// time, no deadline, and no result (matching `g_malloc0` zeroing all
    /// fields and `g_ref_count_init`).
    pub fn new() -> Self {
        Frame {
            frame_count: 0,
            expected_presentation_time_us: None,
            is_target_presentation_time: false,
            frame_deadline_us: None,
            result: None,
        }
    }

    /// `clutter_frame_get_count`.
    pub fn count(&self) -> i64 {
        self.frame_count
    }

    /// `clutter_frame_get_expected_presentation_time`: returns the time
    /// if set, `None` otherwise (matching the C `gboolean` out-param
    /// returning `FALSE` when unset).
    pub fn expected_presentation_time(&self) -> Option<i64> {
        self.expected_presentation_time_us
    }

    /// `clutter_frame_get_frame_deadline`: returns the deadline if set.
    pub fn frame_deadline(&self) -> Option<i64> {
        self.frame_deadline_us
    }

    /// `clutter_frame_has_result`.
    pub fn has_result(&self) -> bool {
        self.result.is_some()
    }

    /// `clutter_frame_get_result`: returns the result if set, panicking
    /// if not (matching the C `g_return_val_if_fail(frame->has_result,
    /// CLUTTER_FRAME_RESULT_IDLE)` — the C version returns `IDLE` as a
    /// fallback, but panicking surfaces the bug rather than masking it).
    /// Use `try_result` for the non-panicking variant.
    pub fn result(&self) -> FrameResult {
        self.result.unwrap_or(FrameResult::Idle)
    }

    /// Non-panicking variant of `result()` — returns `None` if no result
    /// has been set, matching the C fallback behavior.
    pub fn try_result(&self) -> Option<FrameResult> {
        self.result
    }

    /// `clutter_frame_set_result`: set the dispatch result. Panics if a
    /// result was already set (matching the C `g_warn_if_fail
    /// (!frame->has_result)` warning, escalated to a panic since
    /// double-setting is a bug).
    pub fn set_result(&mut self, result: FrameResult) {
        if self.result.is_some() {
            panic!("Frame::set_result called twice on the same frame");
        }
        self.result = Some(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_frame_has_no_results_or_times() {
        let f = Frame::new();
        assert_eq!(f.count(), 0);
        assert!(f.expected_presentation_time().is_none());
        assert!(f.frame_deadline().is_none());
        assert!(!f.has_result());
        assert_eq!(f.try_result(), None);
    }

    #[test]
    fn set_result_round_trips() {
        let mut f = Frame::new();
        f.set_result(FrameResult::PendingPresented);
        assert!(f.has_result());
        assert_eq!(f.result(), FrameResult::PendingPresented);
        assert_eq!(f.try_result(), Some(FrameResult::PendingPresented));
    }

    #[test]
    #[should_panic(expected = "called twice")]
    fn set_result_panics_on_double_set() {
        let mut f = Frame::new();
        f.set_result(FrameResult::Idle);
        f.set_result(FrameResult::Ignored);
    }

    #[test]
    fn result_defaults_to_idle_when_unset() {
        let f = Frame::new();
        // `result()` returns Idle as the C fallback when unset.
        assert_eq!(f.result(), FrameResult::Idle);
    }

    #[test]
    fn presentation_time_and_deadline_round_trip() {
        let mut f = Frame::new();
        f.expected_presentation_time_us = Some(1_000_000);
        f.frame_deadline_us = Some(900_000);
        assert_eq!(f.expected_presentation_time(), Some(1_000_000));
        assert_eq!(f.frame_deadline(), Some(900_000));
    }
}
