//! QR code widget — ported from gnome-qr-gtk/gnome-qr-widget.c
//!
//! The upstream is a GtkWidget that displays a QR code.  We provide a
//! framebuffer renderer that draws a QR code at a given position and size,
//! with optional fallback text display.

use super::qr::{generate_qr_code, QrCode, QrColor, QrEccLevel, QrPixelFormat};
use crate::graphics::framebuffer::{fill_rect, Color, Rect};
use crate::graphics::get_default_font;
use alloc::string::{String, ToString};

/// QR code widget for framebuffer rendering.
pub struct QrWidget {
    text: String,
    alternative_text: String,
    size: usize,
    ecc: QrEccLevel,
    cached_code: Option<QrCode>,
}

impl QrWidget {
    /// Create a new QR widget with the given text.
    /// Matches `gnome_qr_widget_new()`.
    pub fn new(text: &str) -> Self {
        let mut widget = Self {
            text: text.to_string(),
            alternative_text: String::new(),
            size: 128,
            ecc: QrEccLevel::Medium,
            cached_code: None,
        };
        widget.regenerate();
        widget
    }

    /// Set the QR code text.
    /// Matches `gnome_qr_widget_set_text()`.
    pub fn set_text(&mut self, text: &str) {
        self.text = text.to_string();
        self.regenerate();
    }

    /// Get the QR code text.
    /// Matches `gnome_qr_widget_get_text()`.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Set alternative text (displayed when QR generation fails).
    /// Matches `gnome_qr_widget_set_alternative_text()`.
    pub fn set_alternative_text(&mut self, text: &str) {
        self.alternative_text = text.to_string();
    }

    /// Get alternative text.
    pub fn alternative_text(&self) -> &str {
        &self.alternative_text
    }

    /// Set the rendered size in pixels.
    /// Matches `gnome_qr_widget_set_size()`.
    pub fn set_size(&mut self, size: usize) {
        self.size = size;
    }

    /// Get the rendered size.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Set the error correction level.
    /// Matches `gnome_qr_widget_set_ecc_level()`.
    pub fn set_ecc_level(&mut self, ecc: QrEccLevel) {
        self.ecc = ecc;
        self.regenerate();
    }

    /// Get the error correction level.
    pub fn ecc_level(&self) -> QrEccLevel {
        self.ecc
    }

    /// Regenerate the QR code cache.
    fn regenerate(&mut self) {
        self.cached_code = generate_qr_code(&self.text, self.ecc);
    }

    /// Check if the QR code was successfully generated.
    pub fn is_valid(&self) -> bool {
        self.cached_code.is_some()
    }

    /// Render the QR code to the framebuffer at the given position.
    /// The QR code is centered within a `size × size` area at (x, y).
    pub fn render(&self, x: usize, y: usize) {
        // Draw white background
        fill_rect(
            Rect::new(x, y, self.size, self.size),
            Color::rgb(255, 255, 255),
        );

        if let Some(ref code) = self.cached_code {
            let (pixels, pixel_size) = code.render(
                self.size,
                QrColor::WHITE,
                QrColor::BLACK,
                QrPixelFormat::Rgb888,
            );

            // Center the QR code in the allocated area
            let offset_x = x + (self.size - pixel_size) / 2;
            let offset_y = y + (self.size - pixel_size) / 2;

            for row in 0..pixel_size {
                for col in 0..pixel_size {
                    let idx = (row * pixel_size + col) * 3;
                    let r = pixels[idx];
                    let g = pixels[idx + 1];
                    let b = pixels[idx + 2];
                    let color = Color::rgb(r, g, b);
                    crate::graphics::framebuffer::set_pixel(offset_x + col, offset_y + row, color);
                }
            }
        } else if !self.alternative_text.is_empty() {
            // Draw alternative text
            let font = get_default_font();
            let text = &self.alternative_text;
            let char_w = font.char_width;
            let max_chars = self.size / char_w;
            let start_y = y + self.size / 2;
            let mut cx = x + 4;
            for ch in text.chars().take(max_chars) {
                crate::graphics::draw_char(ch, cx, start_y, Color::rgb(0, 0, 0), font);
                cx += char_w;
            }
        }
    }

    /// Get the actual pixel size of the rendered QR code.
    pub fn rendered_size(&self) -> Option<usize> {
        let code = self.cached_code.as_ref()?;
        let (_, size) = code.render(
            self.size,
            QrColor::WHITE,
            QrColor::BLACK,
            QrPixelFormat::Rgb888,
        );
        Some(size)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_new_widget() {
        let widget = QrWidget::new("https://example.com");
        assert!(widget.is_valid());
        assert_eq!(widget.text(), "https://example.com");
    }

    fn test_set_text() {
        let mut widget = QrWidget::new("test");
        widget.set_text("new text");
        assert_eq!(widget.text(), "new text");
        assert!(widget.is_valid());
    }

    fn test_empty_text() {
        let widget = QrWidget::new("");
        assert!(!widget.is_valid());
    }

    fn test_alternative_text() {
        let mut widget = QrWidget::new("");
        widget.set_alternative_text("Scan failed");
        assert_eq!(widget.alternative_text(), "Scan failed");
    }
}
