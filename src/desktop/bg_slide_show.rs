//! Background slide show — ported from gnome-bg-slide-show.c
//!
//! Manages a timed sequence of background slides, each either a fixed image
//! or a transition between two images.  The upstream parses XML wallpaper
//! configuration files; we accept a programmatic slide list and compute
//! the current slide based on wall-clock time.
//!
//! In a no_std kernel without image file support, slides reference color
//! pairs (for gradient backgrounds) rather than file paths.  The slide show
//! integrates with `background::Background` and `bg_crossfade::BgCrossfade`.

use crate::graphics::framebuffer::Color;

/// A single slide in the slideshow.
#[derive(Debug, Clone, Copy)]
pub struct Slide {
    /// Duration in seconds.
    pub duration: f64,
    /// Whether this is a fixed slide (true) or a transition (false).
    pub fixed: bool,
    /// Primary color (the "from" or static color).
    pub color1: Color,
    /// Secondary color (the "to" color for transitions; ignored if fixed).
    pub color2: Color,
}

/// Result of querying the current slide.
#[derive(Debug, Clone, Copy)]
pub struct CurrentSlide {
    /// Progress through the current slide (0.0 to 1.0).
    pub progress: f64,
    /// Duration of the current slide in seconds.
    pub duration: f64,
    /// Whether the current slide is fixed (not a transition).
    pub is_fixed: bool,
    /// Primary color.
    pub color1: Color,
    /// Secondary color (for transitions).
    pub color2: Color,
    /// Index of the current slide.
    pub slide_index: usize,
}

/// Background slide show state.
pub struct BgSlideShow {
    slides: alloc::vec::Vec<Slide>,
    start_time: f64,
    total_duration: f64,
    has_multiple_sizes: bool,
}

impl BgSlideShow {
    /// Create a new empty slide show.
    pub fn new() -> Self {
        Self {
            slides: alloc::vec::Vec::new(),
            start_time: 0.0,
            total_duration: 0.0,
            has_multiple_sizes: false,
        }
    }

    /// Create a slide show from a list of slides.  Starts at the current
    /// system time.
    pub fn with_slides(slides: alloc::vec::Vec<Slide>) -> Self {
        let total_duration: f64 = slides.iter().map(|s| s.duration).sum();
        let start_time = current_time_secs();
        Self {
            slides,
            start_time,
            total_duration,
            has_multiple_sizes: false,
        }
    }

    /// Add a slide to the end of the show.
    pub fn add_slide(&mut self, slide: Slide) {
        self.total_duration += slide.duration;
        self.slides.push(slide);
    }

    /// Get the number of slides.
    pub fn num_slides(&self) -> usize {
        self.slides.len()
    }

    /// Get the total duration in seconds.
    pub fn total_duration(&self) -> f64 {
        self.total_duration
    }

    /// Get the start time in seconds (Unix timestamp).
    pub fn start_time(&self) -> f64 {
        self.start_time
    }

    /// Whether the slideshow has multiple sizes.
    pub fn has_multiple_sizes(&self) -> bool {
        self.has_multiple_sizes
    }

    /// Set the start time (Unix timestamp in seconds).
    pub fn set_start_time(&mut self, time: f64) {
        self.start_time = time;
    }

    /// Get a specific slide by index.
    pub fn get_slide(&self, index: usize) -> Option<&Slide> {
        self.slides.get(index)
    }

    /// Compute the current slide based on wall-clock time.
    /// Matches `gnome_bg_slide_show_get_current_slide()`.
    pub fn get_current_slide(&self) -> Option<CurrentSlide> {
        if self.slides.is_empty() || self.total_duration <= 0.0 {
            return None;
        }

        let now = current_time_secs();
        let mut delta = now - self.start_time;
        // Wrap around using fmod
        delta = mod_f64(delta, self.total_duration);
        if delta < 0.0 {
            delta += self.total_duration;
        }

        let mut elapsed = 0.0;
        for (i, slide) in self.slides.iter().enumerate() {
            if elapsed + slide.duration > delta {
                let progress = if slide.duration > 0.0 {
                    (delta - elapsed) / slide.duration
                } else {
                    0.0
                };
                return Some(CurrentSlide {
                    progress,
                    duration: slide.duration,
                    is_fixed: slide.fixed,
                    color1: slide.color1,
                    color2: slide.color2,
                    slide_index: i,
                });
            }
            elapsed += slide.duration;
        }

        // Fallback: return last slide
        let last = self.slides.last()?;
        Some(CurrentSlide {
            progress: 1.0,
            duration: last.duration,
            is_fixed: last.fixed,
            color1: last.color1,
            color2: last.color2,
            slide_index: self.slides.len() - 1,
        })
    }

    /// Get a specific slide by frame number (only fixed slides count as frames,
    /// matching upstream `gnome_bg_slide_show_get_slide()`).
    pub fn get_slide_by_frame(&self, frame_number: usize) -> Option<CurrentSlide> {
        if self.slides.is_empty() || self.total_duration <= 0.0 {
            return None;
        }

        let now = current_time_secs();
        let mut delta = mod_f64(now - self.start_time, self.total_duration);
        if delta < 0.0 {
            delta += self.total_duration;
        }

        let mut elapsed = 0.0;
        let mut frame_idx = 0usize;
        for (i, slide) in self.slides.iter().enumerate() {
            if !slide.fixed {
                elapsed += slide.duration;
                continue;
            }
            if frame_idx == frame_number {
                let progress = if slide.duration > 0.0 {
                    ((delta - elapsed) / slide.duration).max(0.0)
                } else {
                    0.0
                };
                return Some(CurrentSlide {
                    progress,
                    duration: slide.duration,
                    is_fixed: slide.fixed,
                    color1: slide.color1,
                    color2: slide.color2,
                    slide_index: i,
                });
            }
            frame_idx += 1;
            elapsed += slide.duration;
        }
        None
    }
}

impl Default for BgSlideShow {
    fn default() -> Self {
        Self::new()
    }
}

/// Get current time in seconds as f64 (from system_time).
fn current_time_secs() -> f64 {
    crate::time::system_time() as f64
}

/// Floating-point modulo (like C's fmod).
fn mod_f64(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        return a;
    }
    let quotient = a / b;
    let whole = if quotient < 0.0 {
        -((-quotient) as u64 as f64)
    } else {
        quotient as u64 as f64
    };
    let r = a - whole * b;
    if r < 0.0 && b > 0.0 {
        r + b
    } else if r > 0.0 && b < 0.0 {
        r + b
    } else {
        r
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_empty() {
        let show = BgSlideShow::new();
        assert_eq!(show.num_slides(), 0);
        assert!(show.get_current_slide().is_none());
    }

    fn test_single_slide() {
        let mut show = BgSlideShow::new();
        show.add_slide(Slide {
            duration: 10.0,
            fixed: true,
            color1: Color::rgb(44, 0, 30),
            color2: Color::rgb(0, 0, 0),
        });
        assert_eq!(show.num_slides(), 1);
        let current = show.get_current_slide();
        assert!(current.is_some());
        assert!(current.unwrap().is_fixed);
    }

    fn test_mod_f64() {
        assert_eq!(mod_f64(10.0, 3.0), 1.0);
        assert_eq!(mod_f64(-1.0, 3.0), 2.0);
        assert_eq!(mod_f64(7.5, 2.5), 0.0);
    }
}
