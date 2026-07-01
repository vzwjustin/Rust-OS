//! Wayland Color Representation module
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-color-representation.h
//!
//! Handles color representation negotiation between compositor and Wayland surfaces.
//! Tracks the negotiated pixel format and alpha compositing mode per surface.

use alloc::string::String;

/// Alpha compositing mode for a surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum AlphaMode {
    /// Alpha is premultiplied into the color channels.
    PREMULTIPLIED = 0,
    /// Alpha is separate from the color channels (straight alpha).
    STRAIGHT = 1,
    /// No alpha channel (opaque surface).
    OPAQUE = 2,
}

/// Color representation state for a surface.
///
/// In the C original, `MetaWaylandColorRepresentation` negotiates the
/// DRM format and alpha mode between the client and compositor. Here we
/// model the negotiated state directly: the fourcc format code and the
/// alpha compositing mode.
#[derive(Debug, Clone)]
pub struct MetaWaylandColorRepresentation {
    /// DRM fourcc format code (e.g. DRM_FORMAT_ARGB8888) as a 4-char string.
    pub format: String,
    /// Negotiated alpha compositing mode.
    pub alpha_mode: AlphaMode,
    /// Whether the color representation has been committed by the client.
    pub committed: bool,
}

impl MetaWaylandColorRepresentation {
    /// Create a new color representation with no format set.
    pub fn new() -> Self {
        MetaWaylandColorRepresentation {
            format: String::new(),
            alpha_mode: AlphaMode::OPAQUE,
            committed: false,
        }
    }

    /// Create a new color representation with an explicit format and alpha mode.
    pub fn new_with_format(format: String, alpha_mode: AlphaMode) -> Self {
        MetaWaylandColorRepresentation {
            format,
            alpha_mode,
            committed: false,
        }
    }

    /// Set the DRM fourcc format code.
    pub fn set_format(&mut self, format: String) {
        self.format = format;
    }

    /// Get the DRM fourcc format code.
    pub fn get_format(&self) -> &str {
        &self.format
    }

    /// Check whether a format has been set.
    pub fn has_format(&self) -> bool {
        !self.format.is_empty()
    }

    /// Set the alpha compositing mode.
    pub fn set_alpha_mode(&mut self, mode: AlphaMode) {
        self.alpha_mode = mode;
    }

    /// Get the alpha compositing mode.
    pub fn get_alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    /// Mark the color representation as committed by the client.
    pub fn commit(&mut self) {
        self.committed = true;
    }

    /// Check whether the color representation has been committed.
    pub fn is_committed(&self) -> bool {
        self.committed
    }

    /// Check if this color representation can be committed for a surface.
    /// Returns true only if a format has been set. A full implementation
    /// would also validate the format against the compositor's supported
    /// formats and the surface's buffer.
    pub fn commit_check(&self, _surface: *mut core::ffi::c_void) -> bool {
        self.has_format()
    }

    /// Initialize color representation support for the compositor.
    /// A full implementation would register the color representation
    /// protocol global and set up event handler registration.
    pub fn init(_compositor: *mut core::ffi::c_void) {
        // Protocol global registration requires libwayland-server.
    }
}

impl Default for MetaWaylandColorRepresentation {
    fn default() -> Self {
        Self::new()
    }
}
