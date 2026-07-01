//! Gles3 — OpenGL ES 3.0 renderer wrapper from GNOME Mutter
//!
//! Provides a wrapper around EGL/GLES3, managing function pointers,
//! extension detection, and error state. EGL/GL I/O (context creation,
//! `eglGetProcAddress`, `glGetString`) is documented in the methods but not
//! issued here since there is no EGL implementation in `no_std`.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-gles3.h

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use super::gles3_table::MetaGles3Table;

/// Opaque EGL context type.
pub struct MetaEgl;

/// GL error code (matches Khronos `GLenum` / `glGetError` return).
pub type GlError = u32;

/// MetaGles3 — Wrapper for EGL context and GLES3 function pointers.
/// Tracks extension availability and GL error state.
pub struct MetaGles3 {
    pub egl: *mut MetaEgl,
    /// Function-pointer table populated during initialization. `None` until
    /// `initialize` is called.
    pub table: Option<Box<MetaGles3Table>>,
    /// Whether the GLES3 wrapper has been initialized (function pointers
    /// loaded and extensions queried).
    is_initialized: bool,
    /// List of supported GL extension names, populated by querying
    /// `glGetString(GL_EXTENSIONS)` during initialization and appended to
    /// via `add_extension`.
    extensions: Vec<String>,
    /// Last GL error recorded via `check_error` (0 = `GL_NO_ERROR`).
    error_state: GlError,
}

impl MetaGles3 {
    /// Create a new GLES3 wrapper with a given EGL context.
    pub fn new(egl: *mut MetaEgl) -> Self {
        MetaGles3 {
            egl,
            table: None,
            is_initialized: false,
            extensions: Vec::new(),
            error_state: 0,
        }
    }

    /// Returns whether the GLES3 wrapper has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.is_initialized
    }

    /// Marks the wrapper as initialized and stores the function-pointer
    /// table. A full implementation would also call `eglGetProcAddress`
    /// for each entry in the table and query `glGetString(GL_EXTENSIONS)`
    /// to populate `extensions`.
    pub fn initialize(&mut self, table: Box<MetaGles3Table>) {
        self.table = Some(table);
        self.is_initialized = true;
    }

    /// Adds an extension name to the supported-extensions list. Called
    /// during initialization after parsing the `GL_EXTENSIONS` string, and
    /// may be called later if extensions are dynamically enabled.
    pub fn add_extension(&mut self, name: String) {
        if !self.has_extension(&name) {
            self.extensions.push(name);
        }
    }

    /// Returns `true` if the named extension is in the supported list.
    pub fn has_extension(&self, name: &str) -> bool {
        self.extensions.iter().any(|e| e == name)
    }

    /// Returns the list of supported extension names.
    pub fn extensions(&self) -> &[String] {
        &self.extensions
    }

    /// Returns the last recorded GL error (0 = no error).
    pub fn get_error_state(&self) -> GlError {
        self.error_state
    }

    /// Records a GL error. A full implementation would call `glGetError()`
    /// to drain the error queue and store the first non-zero result.
    pub fn set_error_state(&mut self, error: GlError) {
        self.error_state = error;
    }

    /// Clears the recorded error state (sets it to 0 / `GL_NO_ERROR`).
    pub fn clear_error(&mut self) {
        self.error_state = 0;
    }
}

impl Default for MetaGles3 {
    fn default() -> Self {
        MetaGles3 {
            egl: core::ptr::null_mut(),
            table: None,
            is_initialized: false,
            extensions: Vec::new(),
            error_state: 0,
        }
    }
}
