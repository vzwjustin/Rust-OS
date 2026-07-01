//! In-memory selection source ported from GNOME Mutter's src/core/meta-selection-source-memory.c
//!
//! Provides clipboard data stored in memory, with support for various mimetypes
//! and async read operations.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-selection-source-memory.c

use crate::mutter_port::core::selection_source::SelectionSource;
use alloc::string::String;
use alloc::vec::Vec;

/// In-memory selection/clipboard source
#[derive(Debug, Clone)]
pub struct SelectionSourceMemory {
    pub base: SelectionSource,
    pub mimetype: String,
    pub content: Vec<u8>,
}

impl SelectionSourceMemory {
    /// Create new in-memory selection source from mimetype and data
    pub fn new(mimetype: String, content: Vec<u8>) -> Self {
        let mut base = SelectionSource::new(0);
        base.add_mimetype(mimetype.clone());

        SelectionSourceMemory {
            base,
            mimetype,
            content,
        }
    }

    /// Get the stored content as bytes
    pub fn get_content(&self) -> &[u8] {
        &self.content
    }

    /// Get the content length
    pub fn get_content_size(&self) -> usize {
        self.content.len()
    }

    /// Check if content is available for given mimetype
    pub fn has_mimetype(&self, mimetype: &str) -> bool {
        self.mimetype == mimetype
    }

    /// Read async: would return input stream in real implementation
    /// Stub: async stream handling requires infrastructure not available in no_std
    pub fn read_async(&self, _mimetype: &str) {
        // In real GLib implementation, would create GTask and return GInputStream
        // Stubbed for no_std kernel environment
    }
}
