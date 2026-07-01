//! Mutter selection/clipboard management
//! Ported from meta/meta-selection*.h
use alloc::{string::String, vec::Vec, format};

use crate::mutter_port::meta::types::*;

/// Selection atom types (clipboard, primary, etc)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaSelectionType {
    Clipboard = 0,
    Primary = 1,
    Secondary = 2,
}

/// Manages selections (clipboard, primary selection, etc)
pub struct MetaSelection {
    pub clipboard_data: Option<Vec<u8>>,
    pub primary_selection_data: Option<Vec<u8>>,
    pub secondary_selection_data: Option<Vec<u8>>,
}

impl MetaSelection {
    pub fn new() -> Self {
        Self {
            clipboard_data: None,
            primary_selection_data: None,
            secondary_selection_data: None,
        }
    }

    /// Get current clipboard content as string
    pub fn get_clipboard(&self) -> Option<Vec<u8>> {
        self.clipboard_data.clone()
    }

    /// Set clipboard content
    pub fn set_clipboard(&mut self, data: &[u8]) {
        self.clipboard_data = Some(data.to_vec());
    }

    /// Get primary selection
    pub fn get_primary_selection(&self) -> Option<Vec<u8>> {
        self.primary_selection_data.clone()
    }

    /// Set primary selection
    pub fn set_primary_selection(&mut self, data: &[u8]) {
        self.primary_selection_data = Some(data.to_vec());
    }

    /// Clear all selections
    pub fn clear(&mut self) {
        self.clipboard_data = None;
        self.primary_selection_data = None;
        self.secondary_selection_data = None;
    }
}

impl Default for MetaSelection {
    fn default() -> Self {
        Self::new()
    }
}

/// Source for clipboard/selection data
pub struct MetaSelectionSource {
    pub data: Option<Vec<u8>>,
}

impl MetaSelectionSource {
    pub fn new() -> Self {
        Self { data: None }
    }

    /// Read selection data
    pub fn read(&self) -> Option<Vec<u8>> {
        self.data.clone()
    }
}

impl Default for MetaSelectionSource {
    fn default() -> Self {
        Self::new()
    }
}

/// Memory-backed selection source (owned clipboard data)
pub struct MetaSelectionSourceMemory {
    pub data: Vec<u8>,
}

impl MetaSelectionSourceMemory {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl Default for MetaSelectionSourceMemory {
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

// TODO: port remaining selection functions
