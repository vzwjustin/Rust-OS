//! Remote selection source ported from GNOME Mutter's src/core/meta-selection-source-remote.c
//!
//! Provides clipboard data from a remote source (e.g., Wayland clipboard session),
//! with async transfer support.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-selection-source-remote.c

use crate::mutter_port::core::selection_source::SelectionSource;
use alloc::string::String;
use alloc::vec::Vec;

/// Remote selection source backed by a clipboard session
#[derive(Debug, Clone)]
pub struct SelectionSourceRemote {
    pub base: SelectionSource,
    pub session_id: u32,
    pub mime_types: Vec<String>,
}

impl SelectionSourceRemote {
    /// Create new remote selection source
    pub fn new(session_id: u32, mime_types: Vec<String>) -> Self {
        let mut base = SelectionSource::new(0);
        for mime_type in &mime_types {
            base.add_mimetype(mime_type.clone());
        }

        SelectionSourceRemote {
            base,
            session_id,
            mime_types,
        }
    }

    /// Check if mimetype is available
    pub fn has_mimetype(&self, mimetype: &str) -> bool {
        self.mime_types.iter().any(|m| m == mimetype)
    }

    /// Get list of available mimetypes
    pub fn get_mimetypes(&self) -> &[String] {
        &self.mime_types
    }

    /// Request async transfer from remote session for given mimetype
    /// Stub: requires async task infrastructure not available in no_std
    pub fn read_async(&self, _mimetype: &str) {
        // In real GLib, would create GTask and request transfer from session
        // Stubbed for no_std kernel environment
    }

    /// Complete an async transfer (called by session when ready)
    /// Stub: file descriptor handling requires async I/O infrastructure
    pub fn complete_transfer(&self, _fd: i32) {
        // Would create input stream from fd and complete async task
    }

    /// Cancel an in-progress transfer
    pub fn cancel_transfer(&self) {
        // Would error the pending task
    }
}
