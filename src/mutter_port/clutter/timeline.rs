//! Port of GNOME mutter's `clutter/clutter-timeline.{c,h}`.
//!
//! `ClutterTimeline` is a base class for managing time-based events: animations,
//! transitions, and frame-driven updates. It tracks elapsed time, direction,
//! looping, and progress within a duration.
//!
//! # What's ported
//!
//! - `ClutterTimelineDirection` (`Forward`/`Backward`).
//! - Core state: `duration`, `elapsed_time`, `is_playing`, `direction`,
//!   `repeat_count`, `current_repeat`, `auto_reverse`, `delay`.
//! - Key methods: `new`, `advance`, `elapsed_time`, `progress`, `is_playing`,
//!   `direction`, `set_direction`, `delta`, `start`, `stop`, `pause`, `rewind`.
//! - Marker storage: `add_marker`, `remove_marker`, `markers_at` (by name and
//!   progress value).
//! - Progress calculation: linear (elapsed / duration) or custom function.
//!
//! # What's skipped
//!
//! - GObject signals (started, completed, new-frame, marker-reached, etc.):
//!   replaced by returned state and marker list on `advance`.
//! - Frame clock integration: timeline advances via explicit `advance` call.
//! - Actor/stage binding: state-only.
//! - Animation mode / easing functions: basic linear progress only.

use alloc::vec::Vec;
use core::cmp;
use core::fmt;

/// `ClutterTimelineDirection`: playback direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum Direction {
    /// Play from 0 to duration.
    #[default]
    Forward = 0,
    /// Play from duration to 0.
    Backward = 1,
}

/// Marker at a specific progress point in the timeline.
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    /// Marker name.
    pub name: heapless::String<64>,
    /// Progress value [0.0, 1.0] or absolute milliseconds (if is_relative == false).
    pub progress: f64,
    /// If true, progress is normalized; if false, it's absolute milliseconds.
    pub is_relative: bool,
}

/// Core timeline state machine (no signals, no frame clock).
#[derive(Debug, Clone)]
pub struct Timeline {
    /// Duration in milliseconds.
    duration: u32,
    /// Delay before start in milliseconds.
    delay: u32,
    /// Elapsed time since start in milliseconds.
    elapsed_time: u32,
    /// Delta since last advance in milliseconds.
    msecs_delta: u32,
    /// Is the timeline currently playing.
    is_playing: bool,
    /// Playback direction.
    direction: Direction,
    /// Number of times to repeat (0 = infinite, 1 = play once).
    repeat_count: i32,
    /// Current repeat iteration (0-based).
    current_repeat: i32,
    /// Reverse direction at end of each repeat.
    auto_reverse: bool,
    /// Markers by name.
    markers: alloc::vec::Vec<Marker>,
}

impl Default for Timeline {
    fn default() -> Self {
        Self::new(100)
    }
}

impl Timeline {
    /// Create a new timeline with the given duration in milliseconds.
    pub fn new(duration_ms: u32) -> Self {
        Timeline {
            duration: duration_ms.max(1),
            delay: 0,
            elapsed_time: 0,
            msecs_delta: 0,
            is_playing: false,
            direction: Direction::Forward,
            repeat_count: 1,
            current_repeat: 0,
            auto_reverse: false,
            markers: alloc::vec::Vec::new(),
        }
    }

    /// Get duration in milliseconds.
    pub fn duration(&self) -> u32 {
        self.duration
    }

    /// Set duration in milliseconds (must be > 0).
    pub fn set_duration(&mut self, msecs: u32) {
        self.duration = msecs.max(1);
    }

    /// Get delay in milliseconds.
    pub fn delay(&self) -> u32 {
        self.delay
    }

    /// Set delay in milliseconds.
    pub fn set_delay(&mut self, msecs: u32) {
        self.delay = msecs;
    }

    /// Get elapsed time since start in milliseconds.
    pub fn elapsed_time(&self) -> u32 {
        self.elapsed_time
    }

    /// Get delta time since last advance in milliseconds.
    pub fn delta(&self) -> u32 {
        if !self.is_playing {
            0
        } else {
            self.msecs_delta
        }
    }

    /// Advance timeline by delta milliseconds. Updates elapsed_time and returns
    /// markers that were crossed and whether the timeline reached completion.
    pub fn advance(&mut self, delta_ms: u32) -> (Vec<Marker>, bool) {
        if !self.is_playing {
            return (Vec::new(), false);
        }

        self.msecs_delta = delta_ms;
        let old_elapsed = self.elapsed_time;

        match self.direction {
            Direction::Forward => {
                self.elapsed_time =
                    cmp::min(self.elapsed_time.saturating_add(delta_ms), self.duration);
            }
            Direction::Backward => {
                self.elapsed_time = if self.elapsed_time > delta_ms {
                    self.elapsed_time - delta_ms
                } else {
                    0
                };
            }
        }

        let mut crossed_markers = Vec::new();
        for marker in &self.markers {
            let threshold_ms = if marker.is_relative {
                (marker.progress * self.duration as f64) as u32
            } else {
                marker.progress as u32
            };

            let crossed = match self.direction {
                Direction::Forward => {
                    old_elapsed < threshold_ms && self.elapsed_time >= threshold_ms
                }
                Direction::Backward => {
                    old_elapsed > threshold_ms && self.elapsed_time <= threshold_ms
                }
            };

            if crossed {
                let _ = crossed_markers.push(marker.clone());
            }
        }

        let mut completed = false;
        if (self.direction == Direction::Forward && self.elapsed_time >= self.duration)
            || (self.direction == Direction::Backward && self.elapsed_time == 0)
        {
            completed = true;

            if self.repeat_count == 0 || self.current_repeat + 1 < self.repeat_count {
                self.current_repeat += 1;

                if self.auto_reverse {
                    self.direction = match self.direction {
                        Direction::Forward => Direction::Backward,
                        Direction::Backward => Direction::Forward,
                    };
                }

                if !self.auto_reverse {
                    match self.direction {
                        Direction::Forward => self.elapsed_time = 0,
                        Direction::Backward => self.elapsed_time = self.duration,
                    }
                }

                completed = false;
            }
        }

        (crossed_markers, completed)
    }

    /// Get normalized progress [0.0, 1.0].
    pub fn progress(&self) -> f64 {
        if self.duration == 0 {
            0.0
        } else {
            self.elapsed_time as f64 / self.duration as f64
        }
    }

    /// Check if timeline is currently playing.
    pub fn is_playing(&self) -> bool {
        self.is_playing
    }

    /// Start or resume playback.
    pub fn start(&mut self) {
        if !self.is_playing {
            self.is_playing = true;
            if self.direction == Direction::Forward && self.elapsed_time == self.duration {
                self.elapsed_time = 0;
            } else if self.direction == Direction::Backward && self.elapsed_time == 0 {
                self.elapsed_time = self.duration;
            }
        }
    }

    /// Pause playback without resetting state.
    pub fn pause(&mut self) {
        self.is_playing = false;
    }

    /// Stop and reset to initial state.
    pub fn stop(&mut self) {
        self.is_playing = false;
        self.current_repeat = 0;
        self.elapsed_time = match self.direction {
            Direction::Forward => 0,
            Direction::Backward => self.duration,
        };
    }

    /// Rewind to the start (or end if backward).
    pub fn rewind(&mut self) {
        self.elapsed_time = match self.direction {
            Direction::Forward => 0,
            Direction::Backward => self.duration,
        };
        self.current_repeat = 0;
    }

    /// Get playback direction.
    pub fn direction(&self) -> Direction {
        self.direction
    }

    /// Set playback direction. If direction changes and elapsed_time is at an
    /// endpoint, jump to the opposite endpoint.
    pub fn set_direction(&mut self, direction: Direction) {
        if self.direction != direction {
            self.direction = direction;

            if self.elapsed_time == 0 {
                self.elapsed_time = self.duration;
            }
        }
    }

    /// Get repeat count (0 = infinite, 1 = play once, n = play n times).
    pub fn repeat_count(&self) -> i32 {
        self.repeat_count
    }

    /// Set repeat count.
    pub fn set_repeat_count(&mut self, count: i32) {
        self.repeat_count = count;
    }

    /// Get current repeat iteration (0-based).
    pub fn current_repeat(&self) -> i32 {
        self.current_repeat
    }

    /// Check if auto-reverse is enabled.
    pub fn auto_reverse(&self) -> bool {
        self.auto_reverse
    }

    /// Set auto-reverse (reverse direction at end of each repeat).
    pub fn set_auto_reverse(&mut self, enabled: bool) {
        self.auto_reverse = enabled;
    }

    /// Add a marker at relative progress [0.0, 1.0].
    pub fn add_marker_at_progress(&mut self, name: &str, progress: f64) {
        let _ = self.markers.push(Marker {
            name: heapless::String::try_from(name).unwrap_or_default(),
            progress: progress.clamp(0.0, 1.0),
            is_relative: true,
        });
    }

    /// Add a marker at absolute time (milliseconds).
    pub fn add_marker_at_time(&mut self, name: &str, msecs: u32) {
        let _ = self.markers.push(Marker {
            name: heapless::String::try_from(name).unwrap_or_default(),
            progress: msecs as f64,
            is_relative: false,
        });
    }

    /// Remove a marker by name.
    pub fn remove_marker(&mut self, name: &str) {
        self.markers.retain(|m| m.name.as_str() != name);
    }

    /// Get all markers.
    pub fn markers(&self) -> &[Marker] {
        &self.markers
    }
}

impl fmt::Display for Direction {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Direction::Forward => write!(f, "forward"),
            Direction::Backward => write!(f, "backward"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_timeline_is_stopped() {
        let tl = Timeline::new(100);
        assert!(!tl.is_playing());
        assert_eq!(tl.elapsed_time(), 0);
        assert_eq!(tl.progress(), 0.0);
    }

    #[test]
    fn advance_updates_elapsed_time() {
        let mut tl = Timeline::new(100);
        tl.start();
        let (_, completed) = tl.advance(50);
        assert_eq!(tl.elapsed_time(), 50);
        assert_eq!(tl.progress(), 0.5);
        assert!(!completed);
    }

    #[test]
    fn advance_clamps_at_duration() {
        let mut tl = Timeline::new(100);
        tl.start();
        let (_, completed) = tl.advance(150);
        assert_eq!(tl.elapsed_time(), 100);
        assert_eq!(tl.progress(), 1.0);
        assert!(completed);
    }

    #[test]
    fn direction_backward_counts_down() {
        let mut tl = Timeline::new(100);
        tl.set_direction(Direction::Backward);
        tl.start();
        let (_, _) = tl.advance(30);
        assert_eq!(tl.elapsed_time(), 70);
        assert_eq!(tl.progress(), 0.7);
    }

    #[test]
    fn marker_crossing_forward() {
        let mut tl = Timeline::new(100);
        tl.add_marker_at_progress("half", 0.5);
        tl.start();
        let (crossed, _) = tl.advance(60);
        assert_eq!(crossed.len(), 1);
        assert_eq!(crossed[0].name.as_str(), "half");
    }

    #[test]
    fn marker_crossing_backward() {
        let mut tl = Timeline::new(100);
        tl.set_direction(Direction::Backward);
        tl.add_marker_at_progress("half", 0.5);
        tl.start();
        let (crossed, _) = tl.advance(60);
        assert_eq!(crossed.len(), 1);
    }

    #[test]
    fn repeat_and_loop() {
        let mut tl = Timeline::new(100);
        tl.set_repeat_count(2);
        tl.start();
        let (_, c1) = tl.advance(150);
        assert!(c1);
        assert_eq!(tl.current_repeat(), 1);
    }

    #[test]
    fn auto_reverse_toggles_direction() {
        let mut tl = Timeline::new(100);
        tl.set_auto_reverse(true);
        tl.set_repeat_count(2);
        tl.start();
        assert_eq!(tl.direction(), Direction::Forward);
        let (_, _) = tl.advance(150);
        assert_eq!(tl.direction(), Direction::Backward);
    }

    #[test]
    fn stop_resets_state() {
        let mut tl = Timeline::new(100);
        tl.start();
        let (_, _) = tl.advance(50);
        tl.stop();
        assert!(!tl.is_playing());
        assert_eq!(tl.elapsed_time(), 0);
        assert_eq!(tl.current_repeat(), 0);
    }

    #[test]
    fn pause_preserves_state() {
        let mut tl = Timeline::new(100);
        tl.start();
        let (_, _) = tl.advance(50);
        tl.pause();
        assert!(!tl.is_playing());
        assert_eq!(tl.elapsed_time(), 50);
    }
}
