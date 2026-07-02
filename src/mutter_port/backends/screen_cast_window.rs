//! Screen Cast Window — ported from GNOME Mutter
//!
//! Interface trait for window objects that can be screen-cast, defining capture,
//! damage tracking, and cursor transformation methods for remote clients.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-screen-cast-window.h

/// Represents a rectangular region using integers (mirrors `MtkRectangle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MtkRectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl MtkRectangle {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        MtkRectangle {
            x,
            y,
            width,
            height,
        }
    }
}

/// Floating-point point (mirrors `graphene_point_t`).
#[derive(Debug, Clone, Copy)]
pub struct GraphenePoint {
    pub x: f32,
    pub y: f32,
}

impl GraphenePoint {
    pub fn new(x: f32, y: f32) -> Self {
        GraphenePoint { x, y }
    }
}

/// Opaque cursor object placeholder (mirrors `ClutterCursor`).
pub struct ClutterCursor;

/// Opaque framebuffer object placeholder (mirrors `CoglFramebuffer`).
pub struct CoglFramebuffer;

/// Interface for objects that can provide screen cast capture.
///
/// This trait defines the contract for window/surface objects that support
/// being captured and screencasted to remote clients. Implementations handle
/// buffer bounds, cursor positioning, and frame capture.
pub trait MetaScreenCastWindow {
    /// Get the buffer bounds of this window.
    fn get_buffer_bounds(&self) -> MtkRectangle;

    /// Transform a relative position within this window.
    fn transform_relative_position(&self, x: f64, y: f64) -> (f64, f64);

    /// Transform cursor position to this window's local coordinate system.
    fn transform_cursor_position(
        &self,
        cursor: &ClutterCursor,
        cursor_position: &GraphenePoint,
    ) -> Option<(GraphenePoint, f32)>;

    /// Capture framebuffer contents into a provided buffer.
    fn capture_into(&self, bounds: &MtkRectangle, data: &mut [u8]);

    /// Blit window contents to a Cogl framebuffer.
    fn blit_to_framebuffer(&self, bounds: &MtkRectangle, framebuffer: &CoglFramebuffer) -> bool;

    /// Check if this window has pending damage.
    fn has_damage(&self) -> bool;

    /// Increment usage counter (for reference tracking).
    fn inc_usage(&self);

    /// Decrement usage counter.
    fn dec_usage(&self);
}

/// Default implementation stub for screen cast window operations.
pub struct ScreenCastWindowStub;

impl ScreenCastWindowStub {
    pub fn new() -> Self {
        ScreenCastWindowStub
    }
}

impl Default for ScreenCastWindowStub {
    fn default() -> Self {
        Self::new()
    }
}
