//! Renderer Context EGL — OpenGL ES rendering context.
//!
//! Wraps EGL context for GPU-accelerated rendering (inherits from CoglContextEGL).
//! Upstream has no public fields beyond the parent class.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-renderer-context-egl.c

pub struct RendererContextEgl {
    // Inherits from CoglContextEGL; no additional public fields in upstream.
}

impl RendererContextEgl {
    pub fn new() -> Self {
        RendererContextEgl {}
    }
}

impl Default for RendererContextEgl {
    fn default() -> Self {
        Self::new()
    }
}
