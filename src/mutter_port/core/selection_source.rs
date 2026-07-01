//! Selection source base type ported from GNOME Mutter's src/core/meta-selection-source.c
//!
//! Abstract base for clipboard/selection sources. Manages state and signals for
//! clipboard ownership and data transfer.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-selection-source.c

use alloc::string::String;
use alloc::vec::Vec;

/// Selection type (clipboard, primary, secondary)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionType {
    Clipboard,
    Primary,
    Secondary,
}

/// Abstract base for selection/clipboard sources
#[derive(Debug, Clone)]
pub struct SelectionSource {
    pub id: u32,
    pub active: bool,
    pub mimetypes: Vec<String>,
}

impl SelectionSource {
    /// Create new selection source
    pub fn new(id: u32) -> Self {
        SelectionSource {
            id,
            active: false,
            mimetypes: Vec::new(),
        }
    }

    /// Activate this source (take clipboard ownership)
    pub fn activate(&mut self) {
        self.active = true;
    }

    /// Deactivate this source (release clipboard ownership)
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// Check if this source is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Add supported mimetype
    pub fn add_mimetype(&mut self, mimetype: String) {
        if !self.mimetypes.contains(&mimetype) {
            self.mimetypes.push(mimetype);
        }
    }

    /// Get list of supported mimetypes
    pub fn get_mimetypes(&self) -> &[String] {
        &self.mimetypes
    }

    /// Read data asynchronously from source for given mimetype
    /// Stub: depends on async callback infrastructure not in no_std kernel
    pub fn read_async(&self, _mimetype: &str) {
        // Async read would require callback registration; stubbed for no_std
    }
}
