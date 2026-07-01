//! Mutter compositor subsystem
//! Ported from meta/compositor.h and meta-background*.h, meta-shaped-texture.h
//!
//! MetaCompositor manages the rendering pipeline and window compositing.
//! MetaBackground, MetaWindowActor, and MetaShapedTexture support visual rendering.

use crate::mutter_port::meta::display::MetaDisplay;
use alloc::{boxed::Box, string::String, vec::Vec};
// Use the rich window type (types::* only provides an opaque stub); this
// matches what `meta::MetaWindow` re-exports.
use crate::mutter_port::meta::window::MetaWindow;

/// Main compositor object managing rendering pipeline
pub struct MetaCompositor {
    display: Option<Box<MetaDisplay>>,
    is_enabled: bool,
    managed_windows: Vec<*mut MetaWindow>,
    /// Whether a redraw is pending (dirty flag).
    needs_redraw: bool,
}

impl MetaCompositor {
    /// Create a new MetaCompositor
    pub fn new() -> Self {
        Self {
            display: None,
            is_enabled: false,
            managed_windows: Vec::new(),
            needs_redraw: false,
        }
    }

    /// Get the display this compositor is managing
    pub fn get_display(&self) -> Option<&MetaDisplay> {
        self.display.as_ref().map(|b| &**b)
    }

    /// Enable/disable compositing
    pub fn set_enabled(&mut self, enabled: bool) {
        self.is_enabled = enabled;
    }

    /// Check if compositor is active
    pub fn is_enabled(&self) -> bool {
        self.is_enabled
    }

    /// Manage a new window for compositing (tracked by identity; deduplicated).
    pub fn manage_window(&mut self, window: &MetaWindow) {
        let ptr = window as *const MetaWindow as *mut MetaWindow;
        if !self.managed_windows.contains(&ptr) {
            self.managed_windows.push(ptr);
        }
    }

    /// Unmanage a window (remove from compositing). No-op if not managed.
    pub fn unmanage_window(&mut self, window: &MetaWindow) {
        let ptr = window as *const MetaWindow as *mut MetaWindow;
        self.managed_windows.retain(|&w| w != ptr);
    }

    /// Number of windows currently managed for compositing.
    pub fn managed_window_count(&self) -> usize {
        self.managed_windows.len()
    }

    /// Mark the compositor as needing a redraw.
    pub fn schedule_redraw(&mut self) {
        self.needs_redraw = true;
    }

    /// Whether a redraw is pending.
    pub fn needs_redraw(&self) -> bool {
        self.needs_redraw
    }

    /// Redraw/composite the screen. Iterates over all managed windows
    /// and clears the dirty flag. A full implementation would drive the
    /// Clutter paint pipeline to composite each window actor.
    pub fn redraw(&mut self) {
        if !self.is_enabled {
            return;
        }
        // The actual paint loop would walk `managed_windows` and call
        // each window actor's paint method via the Clutter scene graph.
        // For now, we clear the dirty flag to signal the redraw completed.
        self.needs_redraw = false;
    }
}

impl Default for MetaCompositor {
    fn default() -> Self {
        Self::new()
    }
}

/// Background image/content for desktop or monitors
pub struct MetaBackground {
    red: f32,
    green: f32,
    blue: f32,
    image_path: Option<String>,
}

impl MetaBackground {
    pub fn new() -> Self {
        Self {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            image_path: None,
        }
    }

    /// Set background color
    pub fn set_color(&mut self, red: f32, green: f32, blue: f32) {
        self.red = red;
        self.green = green;
        self.blue = blue;
        // Setting a solid color clears any image.
        self.image_path = None;
    }

    /// Get background color
    pub fn get_color(&self) -> (f32, f32, f32) {
        (self.red, self.green, self.blue)
    }

    /// Load background image from file path. A full implementation would
    /// decode the image via a codec and upload it to a GPU texture.
    /// For now, the path is stored for later resolution.
    pub fn set_image(&mut self, path: &str) {
        self.image_path = Some(String::from(path));
    }

    /// Get the current background image path, if any.
    pub fn get_image_path(&self) -> Option<&str> {
        self.image_path.as_deref()
    }
}

impl Default for MetaBackground {
    fn default() -> Self {
        Self::new()
    }
}

/// Actor for rendering a window's contents
pub struct MetaWindowActor {
    window: Option<Box<MetaWindow>>,
    is_visible: bool,
    opacity: f32,
}

impl MetaWindowActor {
    /// Create a new MetaWindowActor
    pub fn new() -> Self {
        Self {
            window: None,
            is_visible: true,
            opacity: 1.0,
        }
    }

    /// Get the window this actor represents
    pub fn get_window(&self) -> Option<&MetaWindow> {
        self.window.as_ref().map(|b| &**b)
    }

    /// Show the actor
    pub fn show(&mut self) {
        self.is_visible = true;
    }

    /// Hide the actor
    pub fn hide(&mut self) {
        self.is_visible = false;
    }

    /// Check if actor is visible
    pub fn is_visible(&self) -> bool {
        self.is_visible
    }

    /// Set opacity (0.0 - 1.0)
    pub fn set_opacity(&mut self, opacity: f32) {
        self.opacity = opacity;
    }

    /// Get current opacity
    pub fn get_opacity(&self) -> f32 {
        self.opacity
    }
}

impl Default for MetaWindowActor {
    fn default() -> Self {
        Self::new()
    }
}

/// Shaped texture for rendering window content
pub struct MetaShapedTexture {
    is_valid: bool,
    is_dirty: bool,
}

impl MetaShapedTexture {
    pub fn new() -> Self {
        Self {
            is_valid: false,
            is_dirty: true,
        }
    }

    /// Mark the texture as needing an update.
    pub fn invalidate(&mut self) {
        self.is_dirty = true;
    }

    /// Whether the texture content needs to be re-uploaded.
    pub fn is_dirty(&self) -> bool {
        self.is_dirty
    }

    /// Update the texture content. Marks the texture as valid and
    /// clears the dirty flag. A full implementation would re-upload
    /// the window's pixel data to a GPU texture.
    pub fn update(&mut self) {
        self.is_valid = true;
        self.is_dirty = false;
    }
}

impl Default for MetaShapedTexture {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mutter_port::meta::enums::MetaWindowType;

    #[test]
    fn test_manage_and_unmanage_windows() {
        let mut c = MetaCompositor::new();
        let w1 = MetaWindow::new(MetaWindowType::Normal);
        let w2 = MetaWindow::new(MetaWindowType::Dialog);
        assert_eq!(c.managed_window_count(), 0);

        c.manage_window(&w1);
        c.manage_window(&w2);
        assert_eq!(c.managed_window_count(), 2);

        // Managing the same window again is deduplicated.
        c.manage_window(&w1);
        assert_eq!(c.managed_window_count(), 2);

        c.unmanage_window(&w1);
        assert_eq!(c.managed_window_count(), 1);
        // Unmanaging a window that isn't managed is a no-op.
        c.unmanage_window(&w1);
        assert_eq!(c.managed_window_count(), 1);
    }
}
