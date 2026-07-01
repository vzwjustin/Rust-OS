//! Gles3 Table — Function pointer dispatch table for GLES3 extensions from GNOME Mutter
//!
//! Holds pointers to GL (GLES3) functions, both core entry points and
//! optional/extension functions. Dynamic loading happens in MetaGles3
//! (via `eglGetProcAddress`); this struct is the data container with typed
//! function-pointer signatures matching the Khronos GLES3 headers.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-gles3-table.h

use core::ffi::c_void;

/// GL boolean type (matches Khronos `GLboolean`).
pub type GLboolean = u8;
/// GL enum type (matches Khronos `GLenum`).
pub type GLenum = u32;
/// GL bitfield type (matches Khronos `GLbitfield`).
pub type GLbitfield = u32;
/// GL signed int type (matches Khronos `GLint`).
pub type GLint = i32;
/// GL unsigned int type (matches Khronos `GLuint`).
pub type GLuint = u32;
/// GL signed size type (matches Khronos `GLsizei`).
pub type GLsizei = i32;
/// GL signed char pointer (matches Khronos `GLchar`).
pub type GLchar = core::ffi::c_char;
/// GL float type (matches Khronos `GLfloat`).
pub type GLfloat = f32;

/// EGLClientWindow / EGLImage opaque pointer.
pub type EGLImage = *mut c_void;

/// MetaGles3Table — Function pointer table for GLES3 core and extension
/// functions. Populated dynamically by MetaGles3 during extension loading
/// via `eglGetProcAddress`. Each field is `Option<...>` so unavailable
/// extensions are simply `None`.
#[repr(C)]
pub struct MetaGles3Table {
    // --- Core GLES3 entry points ---
    /// `glClear(bitfield)` — clears buffers to preset values.
    pub gl_clear: Option<unsafe extern "C" fn(mask: GLbitfield)>,
    /// `glEnable(cap)` — enables server-side GL capability.
    pub gl_enable: Option<unsafe extern "C" fn(cap: GLenum)>,
    /// `glDisable(cap)` — disables server-side GL capability.
    pub gl_disable: Option<unsafe extern "C" fn(cap: GLenum)>,
    /// `glViewport(x, y, width, height)` — sets the viewport rectangle.
    pub gl_viewport:
        Option<unsafe extern "C" fn(x: GLint, y: GLint, width: GLsizei, height: GLsizei)>,
    /// `glClearColor(r, g, b, a)` — sets the color-buffer clear value.
    pub gl_clear_color:
        Option<unsafe extern "C" fn(r: GLfloat, g: GLfloat, b: GLfloat, a: GLfloat)>,
    /// `glGetError()` — returns the current error code.
    pub gl_get_error: Option<unsafe extern "C" fn() -> GLenum>,
    /// `glBindTexture(target, texture)` — binds a named texture to a target.
    pub gl_bind_texture: Option<unsafe extern "C" fn(target: GLenum, texture: GLuint)>,
    /// `glTexImage2D(...)` — specifies a two-dimensional texture image.
    pub gl_tex_image_2d: Option<
        unsafe extern "C" fn(
            target: GLenum,
            level: GLint,
            internalformat: GLint,
            width: GLsizei,
            height: GLsizei,
            border: GLint,
            format: GLenum,
            type_: GLenum,
            data: *const c_void,
        ),
    >,
    /// `glFinish()` — blocks until all GL execution is complete.
    pub gl_finish: Option<unsafe extern "C" fn()>,
    /// `glFlush()` — forces GL command execution in finite time.
    pub gl_flush: Option<unsafe extern "C" fn()>,

    // --- Extension entry points ---
    /// `glEGLImageTargetTexture2DOES(target, image)` — OES EGL image
    /// extension function. Maps an EGL image to a texture target.
    pub gl_egl_image_target_texture_2d_oes:
        Option<unsafe extern "C" fn(target: GLenum, image: EGLImage)>,
}

impl Default for MetaGles3Table {
    fn default() -> Self {
        Self {
            gl_clear: None,
            gl_enable: None,
            gl_disable: None,
            gl_viewport: None,
            gl_clear_color: None,
            gl_get_error: None,
            gl_bind_texture: None,
            gl_tex_image_2d: None,
            gl_finish: None,
            gl_flush: None,
            gl_egl_image_target_texture_2d_oes: None,
        }
    }
}
