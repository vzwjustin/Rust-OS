//! Port of GNOME mutter's `clutter/clutter-frame-clock.{c,h}`.
//!
//! The frame clock drives the frame/vblank scheduling loop: tracks refresh rate,
//! last presentation time, computes next dispatch time, estimates frame budget
//! and deadline. Core state machine manages dispatch → present → idle cycles
//! with support for fixed-rate, variable-rate, and passive (externally-driven)
//! modes.
//!
//! # What's ported
//!
//! - `ClutterFrameClockMode` (Fixed/Variable/Passive) and
//!   `ClutterFrameClockState` (Idle/Scheduled/Dispatched*).
//! - `ClutterFrameListener` callback interface: before_frame, frame, new_frame.
//! - `ClutterFrameClock` state machine: frame pooling (3-frame ring), timeline
//!   tracking, refresh rate / vblank duration management.
//! - Public scheduling methods: `schedule_update`, `schedule_update_now`,
//!   `schedule_update_later`, `notify_presented`, `notify_ready`.
//! - Timing calculations: max update time estimates, next presentation time
//!   based on refresh interval, frame deadline computation, triple-buffering
//!   detection.
//!
//! # What's skipped
//!
//! - GObject / GSource dispatch: replaced with plain `i64` microsecond times.
//! - Signal dispatch: skipped (no callback mechanism outside the listener iface).
//! - Debug tracing/logging: minimal notes inline; caller can instrument.
//! - Deferred times queue: simplified to core scheduling logic.
//! - Backend frame resource callbacks: Rust ownership replaces GObject ref-count.
//!
//! As with `mutter_port::clutter`, this is no_std Rust: plain structs/enums,
//! `core::` types, no unsafe. Designed for static linking into the kernel.

use core::cmp::{max, min};

use crate::mutter_port::clutter::frame::{Frame, FrameResult};

/// Operation mode for the frame clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum FrameClockMode {
    /// Fixed-rate: dispatch at regular intervals derived from refresh rate.
    Fixed = 0,
    /// Variable-rate (VRR): adaptive refresh to match presentation demand.
    Variable = 1,
    /// Passive: external driver calls `schedule_update` to trigger dispatch.
    Passive = 2,
}

/// Internal state machine state for the frame clock.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum FrameClockState {
    /// Initialization; no frames dispatched yet.
    Init = 0,
    /// Idle; no frame scheduled.
    Idle = 1,
    /// Update scheduled at a calculated time.
    Scheduled = 2,
    /// Update scheduled for immediate dispatch (now).
    ScheduledNow = 3,
    /// Update scheduled for a deferred later time.
    ScheduledLater = 4,
    /// One frame dispatched; awaiting presentation.
    DispatchedOne = 5,
    /// Frame dispatched, and another update already scheduled.
    DispatchedOneAndScheduled = 6,
    /// Dispatched frame + scheduled-now update pending.
    DispatchedOneAndScheduledNow = 7,
    /// Dispatched frame + scheduled-later update pending.
    DispatchedOneAndScheduledLater = 8,
    /// Two frames queued (triple-buffering): one presented, one pending.
    DispatchedTwo = 9,
}

/// Callback interface for the frame clock: invoked on each dispatch cycle.
pub trait FrameClockListener {
    /// Called before any frame processing (pre-dispatch hook).
    fn before_frame(&mut self, frame: &Frame);

    /// Called to process the frame. Returns the result
    /// (PendingPresented/Idle/Ignored).
    fn frame(&mut self, frame: &Frame) -> FrameResult;

    /// Called to obtain a fresh frame object for the next dispatch.
    fn new_frame(&mut self) -> Frame;
}

/// Callback interface for passive-mode drivers.
pub trait FrameClockDriver {
    /// Notify the driver to schedule an update (called in passive mode).
    fn schedule_update(&mut self);
}

/// The frame clock: manages dispatch timing, frame pooling, and state transitions.
pub struct FrameClock {
    // Basic config
    refresh_rate: f32,
    refresh_interval_us: i64,
    vblank_duration_us: i64,
    mode: FrameClockMode,

    // State machine
    state: FrameClockState,
    next_update_time_us: i64,

    // Frame pooling (3-frame ring)
    frame_pool: [Frame; 3],
    frame_pool_index: usize,
    prev_dispatch: Option<Frame>,
    next_presentation: Option<Frame>,
    next_next_presentation: Option<Frame>,
    prev_presentation: Option<Frame>,

    // Presentation time tracking
    is_next_presentation_time_valid: bool,
    is_target_presentation_time: bool,
    next_presentation_time_us: i64,

    // Frame deadline
    has_next_frame_deadline: bool,
    next_frame_deadline_us: i64,

    // Render time estimation (short-term / long-term max)
    longterm_promotion_us: i64,
    longterm_max_update_duration_us: i64,
    shortterm_max_update_duration_us: i64,
    ever_got_measurements: bool,

    // Scheduling state
    pending_reschedule: bool,
    pending_reschedule_now: bool,

    // Inhibit count (pausing mechanism)
    inhibit_count: usize,

    // Metadata
    frame_count: i64,
    deadline_evasion_us: i64,
    output_name: &'static str,
}

impl FrameClock {
    /// Create a new frame clock with the given refresh rate (Hz) and vblank
    /// duration (microseconds).
    pub fn new(refresh_rate: f32, vblank_duration_us: i64, output_name: &'static str) -> Self {
        let refresh_interval_us = (0.5 + 1_000_000.0 / refresh_rate) as i64;

        FrameClock {
            refresh_rate: refresh_rate.max(30.0), // Clamp to minimum
            refresh_interval_us,
            vblank_duration_us,
            mode: FrameClockMode::Fixed,

            state: FrameClockState::Init,
            next_update_time_us: 0,

            frame_pool: [Frame::new(), Frame::new(), Frame::new()],
            frame_pool_index: 0,
            prev_dispatch: None,
            next_presentation: None,
            next_next_presentation: None,
            prev_presentation: None,

            is_next_presentation_time_valid: false,
            is_target_presentation_time: false,
            next_presentation_time_us: 0,

            has_next_frame_deadline: false,
            next_frame_deadline_us: 0,

            longterm_promotion_us: 0,
            longterm_max_update_duration_us: 0,
            shortterm_max_update_duration_us: 0,
            ever_got_measurements: false,

            pending_reschedule: false,
            pending_reschedule_now: false,

            inhibit_count: 0,

            frame_count: 0,
            deadline_evasion_us: 0,
            output_name,
        }
    }

    /// Get the current refresh rate (Hz).
    pub fn refresh_rate(&self) -> f32 {
        self.refresh_rate
    }

    /// Set the refresh rate (Hz).
    pub fn set_refresh_rate(&mut self, refresh_rate: f32) {
        self.refresh_rate = refresh_rate;
        self.refresh_interval_us = (0.5 + 1_000_000.0 / refresh_rate) as i64;
    }

    /// Set the frame clock operating mode (Fixed/Variable/Passive).
    pub fn set_mode(&mut self, mode: FrameClockMode) {
        self.mode = mode;
    }

    /// Get the current operating mode.
    pub fn mode(&self) -> FrameClockMode {
        self.mode
    }

    /// Get the current state machine state.
    pub fn state(&self) -> FrameClockState {
        self.state
    }

    /// Get the current frame count.
    pub fn frame_count(&self) -> i64 {
        self.frame_count
    }

    /// Get the next update time (in monotonic microseconds, or -1 if none).
    pub fn next_update_time(&self) -> i64 {
        if self.inhibit_count > 0 && self.next_update_time_us > 0 {
            -1
        } else {
            self.next_update_time_us
        }
    }

    /// Set the deadline evasion margin (microseconds) for adaptive timing.
    pub fn set_deadline_evasion(&mut self, deadline_evasion_us: i64) {
        self.deadline_evasion_us = deadline_evasion_us;
    }

    /// Notify the frame clock that a frame was presented.
    ///
    /// Updates presentation time tracking, frame pool state, and transitions
    /// the state machine based on buffering mode.
    pub fn notify_presented(&mut self, presentation_time_us: i64) {
        if self.next_presentation.is_none() {
            return;
        }

        // Rotate frame pool: prev_dispatch → prev_presentation,
        // next_presentation → prev_dispatch, next_next → next_presentation.
        self.prev_presentation.take();
        if let Some(mut frame) = self.next_presentation.take() {
            frame.expected_presentation_time_us = Some(presentation_time_us);
            self.prev_presentation = Some(frame);
        }

        self.next_presentation = self.next_next_presentation.take();

        // Update refresh rate if provided in presentation feedback.
        // (In full mutter, this comes from ClutterFrameInfo; here we
        // accept it as a parameter. Caller would extract it from KMS.)

        // State transitions on presentation:
        self.state = match self.state {
            FrameClockState::DispatchedOne => {
                self.pending_reschedule = false;
                FrameClockState::Idle
            }
            FrameClockState::DispatchedOneAndScheduled => {
                self.pending_reschedule = false;
                FrameClockState::Scheduled
            }
            FrameClockState::DispatchedOneAndScheduledNow => {
                self.pending_reschedule = false;
                FrameClockState::ScheduledNow
            }
            FrameClockState::DispatchedOneAndScheduledLater => {
                self.pending_reschedule = false;
                FrameClockState::ScheduledLater
            }
            FrameClockState::DispatchedTwo => {
                // Transition to DispatchedOne; next frame still pending.
                self.pending_reschedule = false;
                FrameClockState::DispatchedOne
            }
            other => other,
        };
    }

    /// Notify the frame clock that the GPU/KMS is ready for the next frame.
    ///
    /// Clears queued frames and transitions state.
    pub fn notify_ready(&mut self) {
        // Clear the oldest queued frame (either next_next or next).
        if self.next_next_presentation.is_some() {
            self.next_next_presentation = None;
        } else if self.next_presentation.is_some() {
            self.next_presentation = None;
        }

        // State transitions on ready:
        self.state = match self.state {
            FrameClockState::DispatchedOne => {
                self.pending_reschedule = false;
                FrameClockState::Idle
            }
            FrameClockState::DispatchedOneAndScheduled => {
                self.pending_reschedule = false;
                FrameClockState::Scheduled
            }
            FrameClockState::DispatchedOneAndScheduledNow => {
                self.pending_reschedule = false;
                FrameClockState::ScheduledNow
            }
            FrameClockState::DispatchedOneAndScheduledLater => {
                self.pending_reschedule = false;
                FrameClockState::ScheduledLater
            }
            FrameClockState::DispatchedTwo => {
                self.pending_reschedule = false;
                FrameClockState::DispatchedOne
            }
            other => other,
        };
    }

    /// Schedule an update at the computed next presentation time.
    ///
    /// Transitions from Idle → Scheduled and sets next_update_time
    /// based on the fixed or variable-rate calculation.
    pub fn schedule_update(&mut self) {
        if self.inhibit_count > 0 {
            self.pending_reschedule = true;
            return;
        }

        match self.mode {
            FrameClockMode::Passive => return, // Passive mode: no self-scheduling
            _ => {}
        }

        match self.state {
            FrameClockState::Init => {
                self.state = FrameClockState::Scheduled;
                self.next_update_time_us = self.now_us();
            }
            FrameClockState::Idle | FrameClockState::ScheduledLater => {
                self.state = FrameClockState::Scheduled;
                self.calculate_next_update_time();
            }
            FrameClockState::Scheduled
            | FrameClockState::ScheduledNow
            | FrameClockState::DispatchedOneAndScheduled
            | FrameClockState::DispatchedOneAndScheduledNow => {
                // Already scheduled; no change.
            }
            FrameClockState::DispatchedOne | FrameClockState::DispatchedOneAndScheduledLater => {
                if self.want_triple_buffering() {
                    self.state = FrameClockState::DispatchedOneAndScheduled;
                    self.calculate_next_update_time();
                } else {
                    self.pending_reschedule = true;
                }
            }
            FrameClockState::DispatchedTwo => {
                self.pending_reschedule = true;
            }
            FrameClockState::ScheduledLater => {
                // Handled above
            }
        }
    }

    /// Schedule an immediate update (dispatch as soon as possible).
    pub fn schedule_update_now(&mut self) {
        if self.inhibit_count > 0 {
            self.pending_reschedule = true;
            self.pending_reschedule_now = true;
            return;
        }

        match self.mode {
            FrameClockMode::Passive => return,
            _ => {}
        }

        match self.state {
            FrameClockState::Init
            | FrameClockState::Idle
            | FrameClockState::Scheduled
            | FrameClockState::ScheduledLater => {
                self.state = FrameClockState::ScheduledNow;
            }
            FrameClockState::ScheduledNow | FrameClockState::DispatchedOneAndScheduledNow => {
                // Already scheduled-now; no change.
                return;
            }
            FrameClockState::DispatchedOneAndScheduled
            | FrameClockState::DispatchedOneAndScheduledLater => {
                self.state = FrameClockState::DispatchedOneAndScheduledNow;
            }
            FrameClockState::DispatchedOne => {
                if self.want_triple_buffering() {
                    self.state = FrameClockState::DispatchedOneAndScheduledNow;
                } else {
                    self.pending_reschedule = true;
                    self.pending_reschedule_now = true;
                    return;
                }
            }
            FrameClockState::DispatchedTwo => {
                self.pending_reschedule = true;
                self.pending_reschedule_now = true;
                return;
            }
        }

        self.next_update_time_us = self.now_us();
        self.is_next_presentation_time_valid = false;
        self.is_target_presentation_time = false;
    }

    /// Schedule an update for a specific future time.
    pub fn schedule_update_later(&mut self, target_us: i64) {
        if self.inhibit_count > 0 {
            self.pending_reschedule = true;
            return;
        }

        match self.mode {
            FrameClockMode::Passive => return,
            _ => {}
        }

        match self.state {
            FrameClockState::Init | FrameClockState::Idle | FrameClockState::ScheduledLater => {
                self.state = FrameClockState::ScheduledLater;
                self.next_update_time_us = target_us;
            }
            FrameClockState::Scheduled | FrameClockState::ScheduledNow => {
                // Don't downgrade from Scheduled/ScheduledNow to Later.
            }
            FrameClockState::DispatchedOneAndScheduled
            | FrameClockState::DispatchedOneAndScheduledNow => {
                // Already have a pending update.
            }
            FrameClockState::DispatchedOne | FrameClockState::DispatchedOneAndScheduledLater => {
                if self.want_triple_buffering() {
                    self.state = FrameClockState::DispatchedOneAndScheduled;
                    self.calculate_next_update_time();
                } else {
                    self.pending_reschedule = true;
                }
            }
            FrameClockState::DispatchedTwo => {
                self.pending_reschedule = true;
            }
        }
    }

    /// Inhibit frame updates (pause the clock).
    pub fn inhibit(&mut self) {
        if self.inhibit_count == 0 {
            // Transition scheduled states to idle, recording intent to reschedule.
            match self.state {
                FrameClockState::Scheduled | FrameClockState::ScheduledLater => {
                    self.pending_reschedule = true;
                    self.state = FrameClockState::Idle;
                }
                FrameClockState::ScheduledNow => {
                    self.pending_reschedule = true;
                    self.pending_reschedule_now = true;
                    self.state = FrameClockState::Idle;
                }
                FrameClockState::DispatchedOneAndScheduled => {
                    self.pending_reschedule = true;
                    self.state = FrameClockState::DispatchedOne;
                }
                FrameClockState::DispatchedOneAndScheduledNow => {
                    self.pending_reschedule = true;
                    self.pending_reschedule_now = true;
                    self.state = FrameClockState::DispatchedOne;
                }
                FrameClockState::DispatchedOneAndScheduledLater => {
                    self.pending_reschedule = true;
                    self.state = FrameClockState::DispatchedOne;
                }
                _ => {}
            }
            self.next_update_time_us = -1;
        }
        self.inhibit_count += 1;
    }

    /// Uninhibit frame updates (resume the clock).
    pub fn uninhibit(&mut self) {
        if self.inhibit_count > 0 {
            self.inhibit_count -= 1;
            if self.inhibit_count == 0 && self.pending_reschedule {
                self.pending_reschedule = false;
                if self.pending_reschedule_now {
                    self.pending_reschedule_now = false;
                    self.schedule_update_now();
                } else {
                    self.schedule_update();
                }
            }
        }
    }

    /// Dispatch the frame clock: invoke the listener and return the frame
    /// result. The caller should call this when `next_update_time_us` is
    /// reached.
    pub fn dispatch(&mut self, time_us: i64, listener: &mut dyn FrameClockListener) -> FrameResult {
        // Allocate a frame from the pool.
        let mut frame = listener.new_frame();
        frame.frame_count = self.frame_count;
        self.frame_count += 1;

        if self.is_next_presentation_time_valid {
            frame.expected_presentation_time_us = Some(self.next_presentation_time_us);
            frame.is_target_presentation_time = self.is_target_presentation_time;
        }

        if self.has_next_frame_deadline {
            frame.frame_deadline_us = Some(self.next_frame_deadline_us);
        }

        // Invoke before_frame hook.
        listener.before_frame(&frame);

        // Process the frame.
        let result = listener.frame(&frame);

        // Update state based on result.
        self.prev_dispatch = Some(frame);

        self.state = match (self.state, result) {
            (FrameClockState::ScheduledNow, _) | (FrameClockState::Scheduled, _) => {
                FrameClockState::DispatchedOne
            }
            (FrameClockState::ScheduledLater, _) => FrameClockState::DispatchedOne,
            (FrameClockState::Init, _) => FrameClockState::DispatchedOne,
            (state, _) => state,
        };

        // Store the frame for presentation tracking if it's active.
        if result != FrameResult::Ignored {
            if self.next_next_presentation.is_none() {
                self.next_next_presentation = self.prev_dispatch.take();
            } else if self.next_presentation.is_none() {
                self.next_presentation = self.prev_dispatch.take();
            }
        }

        result
    }

    // --- Private helpers ---

    fn now_us(&self) -> i64 {
        // Placeholder: in production, call monotonic clock.
        // For testing, this would be injected.
        0
    }

    fn calculate_next_update_time(&mut self) {
        match self.mode {
            FrameClockMode::Fixed => {
                if let Some(last) = &self.prev_presentation {
                    if let Some(pres_time) = last.expected_presentation_time_us {
                        self.next_presentation_time_us = pres_time + self.refresh_interval_us;
                        self.has_next_frame_deadline = true;
                        self.next_frame_deadline_us =
                            self.next_presentation_time_us - self.vblank_duration_us;
                        self.is_next_presentation_time_valid = true;
                        self.is_target_presentation_time = true;

                        let max_update_time = self.get_max_update_duration_us();
                        self.next_update_time_us = max(
                            self.now_us(),
                            self.next_presentation_time_us - max_update_time,
                        );
                        return;
                    }
                }
                // Fallback: schedule soon
                self.next_update_time_us = self.now_us();
                self.is_next_presentation_time_valid = false;
            }
            FrameClockMode::Variable => {
                if let Some(last) = &self.prev_presentation {
                    if let Some(pres_time) = last.expected_presentation_time_us {
                        self.next_presentation_time_us = pres_time + self.refresh_interval_us;
                        let max_update_time = self.get_max_update_duration_us();
                        self.next_update_time_us = max(
                            self.now_us(),
                            self.next_presentation_time_us - max_update_time,
                        );
                        self.has_next_frame_deadline = true;
                        self.next_frame_deadline_us = self.next_update_time_us;
                        self.is_next_presentation_time_valid = true;
                        self.is_target_presentation_time = false;
                        return;
                    }
                }
                // Fallback
                self.next_update_time_us = self.now_us();
                self.is_next_presentation_time_valid = false;
            }
            FrameClockMode::Passive => {
                // No internal scheduling in passive mode.
            }
        }
    }

    fn get_max_update_duration_us(&self) -> i64 {
        max(
            self.longterm_max_update_duration_us,
            self.shortterm_max_update_duration_us,
        )
    }

    fn want_triple_buffering(&self) -> bool {
        if let Some(last) = &self.prev_presentation {
            if last.expected_presentation_time_us.is_some() {
                let max_update = self.get_max_update_duration_us();
                return max_update >= self.refresh_interval_us;
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_frame_clock() {
        let clock = FrameClock::new(60.0, 1000, "HDMI-1");
        assert_eq!(clock.refresh_rate(), 60.0);
        assert_eq!(clock.mode(), FrameClockMode::Fixed);
        assert_eq!(clock.state(), FrameClockState::Init);
        assert_eq!(clock.frame_count(), 0);
    }

    #[test]
    fn set_mode_and_refresh_rate() {
        let mut clock = FrameClock::new(60.0, 1000, "HDMI-1");
        clock.set_mode(FrameClockMode::Variable);
        assert_eq!(clock.mode(), FrameClockMode::Variable);

        clock.set_refresh_rate(144.0);
        assert!((clock.refresh_rate() - 144.0).abs() < 0.1);
    }

    #[test]
    fn inhibit_and_uninhibit() {
        let mut clock = FrameClock::new(60.0, 1000, "HDMI-1");
        clock.inhibit();
        assert_eq!(clock.inhibit_count, 1);

        clock.inhibit();
        assert_eq!(clock.inhibit_count, 2);

        clock.uninhibit();
        assert_eq!(clock.inhibit_count, 1);

        clock.uninhibit();
        assert_eq!(clock.inhibit_count, 0);
    }

    #[test]
    fn schedule_update_transitions_state() {
        let mut clock = FrameClock::new(60.0, 1000, "HDMI-1");
        clock.schedule_update();
        assert_eq!(clock.state(), FrameClockState::Scheduled);
    }

    #[test]
    fn schedule_update_now_takes_precedence() {
        let mut clock = FrameClock::new(60.0, 1000, "HDMI-1");
        clock.schedule_update();
        assert_eq!(clock.state(), FrameClockState::Scheduled);

        clock.schedule_update_now();
        assert_eq!(clock.state(), FrameClockState::ScheduledNow);
    }

    #[test]
    fn inhibit_blocks_scheduling() {
        let mut clock = FrameClock::new(60.0, 1000, "HDMI-1");
        clock.inhibit();
        clock.schedule_update();
        assert!(clock.pending_reschedule);
        assert_eq!(clock.state(), FrameClockState::Init);
    }
}
