//! Gles3 Table — Function pointer dispatch table for GLES3 extensions from GNOME Mutter
//!
//! Holds pointers to optional/extension GL functions. Dynamic loading happens
//! in MetaGles3; this is the data container.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-gles3-table.h

/// MetaGles3Table — Function pointer table for GLES3 extensions.
/// Populated dynamically by MetaGles3 during extension loading.
#[repr(C)]
pub struct MetaGles3Table {
    /// glEGLImageTargetTexture2DOES — OES EGL image extension function.
    /// Maps an EGL image to a texture target.
    /// TODO: GL function pointer type.
    pub gl_egl_image_target_texture_2d_oes: usize, // TODO: proper function pointer type
}

impl Default for MetaGles3Table {
    fn default() -> Self {
        Self {
            gl_egl_image_target_texture_2d_oes: 0,
        }
    }
}
