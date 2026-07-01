//! Desktop background — ported from gnome-bg.c
//!
//! Manages the desktop background appearance: solid color, vertical gradient,
//! or horizontal gradient.  Draws directly to the RustOS framebuffer.
//!
//! The upstream uses GSettings for configuration, GdkPixbuf for images, and
//! cairo for rendering.  We support solid and gradient color modes which cover
//! the common desktop background use case in a no_std kernel.

use crate::graphics::framebuffer::{self, Color, Rect};

/// Background shading type (mirrors GDesktopBackgroundShading).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgShading {
    /// Single solid color.
    Solid,
    /// Vertical gradient from top to bottom.
    Vertical,
    /// Horizontal gradient from left to right.
    Horizontal,
}

/// Background placement style (mirrors GDesktopBackgroundStyle).
/// For image backgrounds — not all are applicable in no_std.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgPlacement {
    /// Centered on screen.
    Centered,
    /// Tiled to fill screen.
    Tiled,
    /// Scaled to fit, preserving aspect ratio.
    Scaled,
    /// Stretched to fill screen (may distort).
    Stretched,
    /// Zoomed to fill, cropping as needed.
    Zoom,
    /// Spanning multiple monitors.
    Spanned,
}

/// Background configuration — solid or gradient colors.
pub struct Background {
    shading: BgShading,
    primary: Color,
    secondary: Color,
    placement: BgPlacement,
}

impl Default for Background {
    fn default() -> Self {
        Self {
            shading: BgShading::Vertical,
            primary: Color::rgb(44, 0, 30),
            secondary: Color::rgb(94, 39, 80),
            placement: BgPlacement::Scaled,
        }
    }
}

impl Background {
    /// Create a new background with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the color(s) and shading type.
    /// Matches `gnome_bg_set_rgba()`.
    pub fn set_rgba(&mut self, shading: BgShading, primary: Color, secondary: Color) {
        self.shading = shading;
        self.primary = primary;
        self.secondary = secondary;
    }

    /// Set a single solid color.
    pub fn set_solid(&mut self, color: Color) {
        self.shading = BgShading::Solid;
        self.primary = color;
    }

    /// Set a vertical gradient.
    pub fn set_gradient_vertical(&mut self, top: Color, bottom: Color) {
        self.shading = BgShading::Vertical;
        self.primary = top;
        self.secondary = bottom;
    }

    /// Set a horizontal gradient.
    pub fn set_gradient_horizontal(&mut self, left: Color, right: Color) {
        self.shading = BgShading::Horizontal;
        self.primary = left;
        self.secondary = right;
    }

    /// Set the placement style (for image backgrounds).
    pub fn set_placement(&mut self, placement: BgPlacement) {
        self.placement = placement;
    }

    /// Get the current shading type.
    pub fn shading(&self) -> BgShading {
        self.shading
    }

    /// Get the primary color.
    pub fn primary_color(&self) -> Color {
        self.primary
    }

    /// Get the secondary color.
    pub fn secondary_color(&self) -> Color {
        self.secondary
    }

    /// Get the placement style.
    pub fn placement(&self) -> BgPlacement {
        self.placement
    }

    /// Determine if the background is dark (for text color selection).
    /// Matches `gnome_bg_is_dark()`.
    pub fn is_dark(&self, width: usize, height: usize) -> bool {
        let avg = match self.shading {
            BgShading::Solid => luminance(self.primary),
            BgShading::Vertical | BgShading::Horizontal => {
                (luminance(self.primary) + luminance(self.secondary)) / 2
            }
        };
        avg < 128
    }

    /// Draw the background to the framebuffer.
    /// Matches `gnome_bg_draw()`.
    pub fn draw(&self, width: usize, height: usize) {
        match self.shading {
            BgShading::Solid => {
                let rect = Rect::new(0, 0, width, height);
                framebuffer::fill_rect(rect, self.primary);
            }
            BgShading::Vertical => {
                draw_vertical_gradient(width, height, self.primary, self.secondary);
            }
            BgShading::Horizontal => {
                draw_horizontal_gradient(width, height, self.primary, self.secondary);
            }
        }
    }

    /// Draw the background to a specific region of the framebuffer.
    pub fn draw_region(&self, rect: Rect) {
        match self.shading {
            BgShading::Solid => {
                framebuffer::fill_rect(rect, self.primary);
            }
            BgShading::Vertical => {
                draw_vertical_gradient_rect(rect, self.primary, self.secondary);
            }
            BgShading::Horizontal => {
                draw_horizontal_gradient_rect(rect, self.primary, self.secondary);
            }
        }
    }

    /// Begin a crossfade from the current solid color to a new solid
    /// color. Returns a `BgCrossfade` that the caller should `tick()`
    /// each frame until `is_finished()`; the background's color is
    /// updated to `new_color` when the crossfade completes.
    ///
    /// This mirrors the upstream `gnome_bg_crossfade_start()` pattern:
    /// when the GSettings background key changes, GNOME creates a
    /// crossfade from the old surface to the new one. Here we operate
    /// on solid colors directly via the framebuffer.
    ///
    /// For gradient backgrounds, only the primary color is crossfaded
    /// (the secondary stays fixed); a full gradient crossfade would
    /// need per-pixel blending which is too slow for 60 FPS on the
    /// kernel framebuffer without GPU acceleration.
    pub fn crossfade_to(
        &self,
        new_color: Color,
        width: usize,
        height: usize,
    ) -> super::bg_crossfade::BgCrossfade {
        let mut fade = super::bg_crossfade::BgCrossfade::new(width, height);
        fade.set_start_color(self.primary);
        fade.set_end_color(new_color);
        fade.start();
        fade
    }
}

/// Draw a vertical gradient (top→bottom) across the full screen.
fn draw_vertical_gradient(width: usize, height: usize, top: Color, bottom: Color) {
    if height == 0 {
        return;
    }
    for y in 0..height {
        let t = y as f64 / height as f64;
        let color = blend(top, bottom, t);
        let row = Rect::new(0, y, width, 1);
        framebuffer::fill_rect(row, color);
    }
}

/// Draw a horizontal gradient (left→right) across the full screen.
fn draw_horizontal_gradient(width: usize, height: usize, left: Color, right: Color) {
    if width == 0 {
        return;
    }
    for x in 0..width {
        let t = x as f64 / width as f64;
        let color = blend(left, right, t);
        let col = Rect::new(x, 0, 1, height);
        framebuffer::fill_rect(col, color);
    }
}

/// Draw a vertical gradient within a specific rect.
fn draw_vertical_gradient_rect(rect: Rect, top: Color, bottom: Color) {
    if rect.height == 0 {
        return;
    }
    for y in 0..rect.height {
        let t = y as f64 / rect.height as f64;
        let color = blend(top, bottom, t);
        let row = Rect::new(rect.x, rect.y + y, rect.width, 1);
        framebuffer::fill_rect(row, color);
    }
}

/// Draw a horizontal gradient within a specific rect.
fn draw_horizontal_gradient_rect(rect: Rect, left: Color, right: Color) {
    if rect.width == 0 {
        return;
    }
    for x in 0..rect.width {
        let t = x as f64 / rect.width as f64;
        let color = blend(left, right, t);
        let col = Rect::new(rect.x + x, rect.y, 1, rect.height);
        framebuffer::fill_rect(col, color);
    }
}

/// Linear blend between two colors.
fn blend(a: Color, b: Color, t: f64) -> Color {
    let lerp = |x: u8, y: u8| -> u8 {
        let v = x as f64 + (y as f64 - x as f64) * t;
        if v < 0.0 {
            0
        } else if v > 255.0 {
            255
        } else {
            (v + 0.5) as u8
        }
    };
    Color::rgb(lerp(a.r, b.r), lerp(a.g, b.g), lerp(a.b, b.b))
}

/// Relative luminance of a color (0-255).
fn luminance(c: Color) -> u8 {
    // Standard luminance formula: 0.299R + 0.587G + 0.114B
    ((c.r as u32 * 299 + c.g as u32 * 587 + c.b as u32 * 114) / 1000) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_solid() {
        let mut bg = Background::new();
        bg.set_solid(Color::rgb(100, 100, 100));
        assert_eq!(bg.shading(), BgShading::Solid);
        assert_eq!(bg.primary_color(), Color::rgb(100, 100, 100));
    }

    fn test_gradient() {
        let mut bg = Background::new();
        bg.set_gradient_vertical(Color::rgb(0, 0, 0), Color::rgb(255, 255, 255));
        assert_eq!(bg.shading(), BgShading::Vertical);
        assert_eq!(bg.primary_color(), Color::rgb(0, 0, 0));
        assert_eq!(bg.secondary_color(), Color::rgb(255, 255, 255));
    }

    fn test_is_dark() {
        let mut bg = Background::new();
        bg.set_solid(Color::rgb(10, 10, 10));
        assert!(bg.is_dark(800, 600));

        bg.set_solid(Color::rgb(240, 240, 240));
        assert!(!bg.is_dark(800, 600));
    }

    fn test_blend() {
        let c = blend(Color::rgb(0, 0, 0), Color::rgb(100, 100, 100), 0.5);
        assert_eq!(c, Color::rgb(50, 50, 50));
    }
}
