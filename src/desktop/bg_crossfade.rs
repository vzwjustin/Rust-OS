//! Background crossfade — ported from gnome-bg-crossfade.c
//!
//! Animates a smooth transition between two background colors/gradients on the
//! framebuffer.  The upstream uses cairo surfaces and GdkWindow; we operate
//! directly on the RustOS framebuffer using alpha blending.
//!
//! The crossfade runs for ~0.75 seconds (matching upstream's `total_duration`),
//! blending from the start color to the end color per-pixel.

use crate::graphics::framebuffer::{self, Color, Rect};
use crate::mutter_port::clutter::easing::{easing_for_mode, AnimationMode};

/// Total crossfade duration in seconds (matches upstream default).
const DEFAULT_DURATION_MS: u64 = 750;

/// Frame interval in milliseconds (~60 FPS, matching upstream's 1000/60).
const FRAME_INTERVAL_MS: u64 = 16;

/// Background crossfade state.
pub struct BgCrossfade {
    width: usize,
    height: usize,
    start_color: Color,
    end_color: Color,
    start_time_ms: u64,
    total_duration_ms: u64,
    started: bool,
    finished: bool,
    last_frame_time: u64,
    /// Easing curve applied to the linear progress before blending.
    /// Defaults to `EaseInOutCubic` for a smooth, natural fade (the
    /// upstream `gnome-bg-crossfade` uses a linear blend, but the
    /// Mutter/Clutter animation framework applies easing to all
    /// transitions; `EaseInOutCubic` is the Clutter default for
    /// background-class animations).
    easing_mode: AnimationMode,
}

impl BgCrossfade {
    /// Create a new crossfade for the given screen dimensions.
    /// Matches `gnome_bg_crossfade_new(width, height)`.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            start_color: Color::rgb(0, 0, 0),
            end_color: Color::rgb(0, 0, 0),
            start_time_ms: 0,
            total_duration_ms: DEFAULT_DURATION_MS,
            started: false,
            finished: false,
            last_frame_time: 0,
            easing_mode: AnimationMode::EaseInOutCubic,
        }
    }

    /// Set the start (fading-from) color.
    /// Matches `gnome_bg_crossfade_set_start_surface()`.
    pub fn set_start_color(&mut self, color: Color) {
        self.start_color = color;
    }

    /// Set the end (fading-to) color.
    /// Matches `gnome_bg_crossfade_set_end_surface()`.
    pub fn set_end_color(&mut self, color: Color) {
        self.end_color = color;
    }

    /// Start the crossfade animation.
    /// Matches `gnome_bg_crossfade_start()`.
    pub fn start(&mut self) {
        self.started = true;
        self.finished = false;
        self.start_time_ms = crate::time::uptime_ms();
        self.last_frame_time = self.start_time_ms;
    }

    /// Check if the crossfade is currently running.
    /// Matches `gnome_bg_crossfade_is_started()`.
    pub fn is_started(&self) -> bool {
        self.started && !self.finished
    }

    /// Stop the crossfade immediately.
    /// Matches `gnome_bg_crossfade_stop()`.
    pub fn stop(&mut self) {
        self.started = false;
        self.finished = false;
    }

    /// Check if the crossfade has completed.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Advance the crossfade by one frame.  Returns true if the crossfade is
    /// still running (more frames needed), false if it has finished.
    ///
    /// This performs per-pixel alpha blending on the framebuffer between the
    /// start and end colors.  Call this from the desktop tick at ~60 FPS.
    pub fn tick(&mut self) -> bool {
        if !self.started || self.finished {
            return false;
        }

        let now = crate::time::uptime_ms();
        let elapsed = now.saturating_sub(self.start_time_ms);

        // Only render at ~60 FPS
        if now.saturating_sub(self.last_frame_time) < FRAME_INTERVAL_MS {
            return true;
        }
        self.last_frame_time = now;

        // Calculate progress (0.0 to 1.0)
        let linear_progress = if self.total_duration_ms > 0 {
            (elapsed as f64 / self.total_duration_ms as f64).clamp(0.0, 1.0)
        } else {
            1.0
        };
        // Apply the easing curve (e.g. EaseInOutCubic) to the linear
        // progress for a smoother, more natural fade. The easing
        // function takes (t, d) where t is elapsed and d is total
        // duration; we pass the raw elapsed/duration so the easing
        // curve's acceleration/deceleration profile is applied
        // correctly.
        let progress =
            if self.total_duration_ms > 0 && linear_progress > 0.0 && linear_progress < 1.0 {
                easing_for_mode(
                    self.easing_mode,
                    elapsed as f64,
                    self.total_duration_ms as f64,
                )
                .clamp(0.0, 1.0)
            } else {
                linear_progress
            };

        if progress >= 1.0 {
            // Final frame — paint the end color solid
            let rect = Rect::new(0, 0, self.width, self.height);
            framebuffer::fill_rect(rect, self.end_color);
            self.finished = true;
            self.started = false;
            return false;
        }

        // Blend start→end colors across the full screen
        let blended = blend_color(self.start_color, self.end_color, progress);
        let rect = Rect::new(0, 0, self.width, self.height);
        framebuffer::fill_rect(rect, blended);

        true
    }

    /// Set a custom duration in milliseconds.
    pub fn set_duration_ms(&mut self, ms: u64) {
        self.total_duration_ms = ms;
    }

    /// Set the easing curve applied to the fade progress. Default is
    /// `EaseInOutCubic`; use `Linear` for the original upstream
    /// behavior (no acceleration/deceleration).
    pub fn set_easing_mode(&mut self, mode: AnimationMode) {
        self.easing_mode = mode;
    }
}

/// Linear interpolation between two colors.
/// `t` is 0.0 (start) to 1.0 (end).
fn blend_color(start: Color, end: Color, t: f64) -> Color {
    let lerp = |a: u8, b: u8| -> u8 {
        let a = a as f64;
        let b = b as f64;
        let v = a + (b - a) * t;
        if v < 0.0 {
            0
        } else if v > 255.0 {
            255
        } else {
            (v + 0.5) as u8
        }
    };
    Color::rgb(
        lerp(start.r, end.r),
        lerp(start.g, end.g),
        lerp(start.b, end.b),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_blend_start() {
        let c = blend_color(Color::rgb(0, 0, 0), Color::rgb(255, 255, 255), 0.0);
        assert_eq!(c, Color::rgb(0, 0, 0));
    }

    fn test_blend_end() {
        let c = blend_color(Color::rgb(0, 0, 0), Color::rgb(255, 255, 255), 1.0);
        assert_eq!(c, Color::rgb(255, 255, 255));
    }

    fn test_blend_mid() {
        let c = blend_color(Color::rgb(0, 0, 0), Color::rgb(100, 100, 100), 0.5);
        assert_eq!(c, Color::rgb(50, 50, 50));
    }

    fn test_crossfade_lifecycle() {
        let mut fade = BgCrossfade::new(800, 600);
        fade.set_start_color(Color::rgb(44, 0, 30));
        fade.set_end_color(Color::rgb(94, 39, 80));
        assert!(!fade.is_started());
        fade.start();
        assert!(fade.is_started());
        fade.stop();
        assert!(!fade.is_started());
    }
}
