//! Mutter selection/clipboard management
//! Ported from meta/meta-selection*.h
use alloc::{format, string::String, vec::Vec};

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
    /// MIME types offered by this source (e.g. "text/plain;charset=utf-8",
    /// "image/png"). Tracked so consumers can negotiate a format before
    /// requesting the bytes.
    pub mime_types: Vec<String>,
    /// Whether an async read is currently in flight. Used to serialize
    /// overlapping read requests and to reject re-entrant reads.
    pub read_in_progress: bool,
    /// Bytes already delivered to the most recent async read consumer.
    /// Reset when a new read is started.
    pub read_buffer: Option<Vec<u8>>,
    /// Offset into `read_buffer` for incremental delivery.
    pub read_offset: usize,
}

impl MetaSelectionSource {
    pub fn new() -> Self {
        Self {
            data: None,
            mime_types: Vec::new(),
            read_in_progress: false,
            read_buffer: None,
            read_offset: 0,
        }
    }

    /// Read selection data
    pub fn read(&self) -> Option<Vec<u8>> {
        self.data.clone()
    }

    /// Get the list of MIME types offered by this source.
    pub fn get_mime_types(&self) -> &[String] {
        &self.mime_types
    }

    /// Add a MIME type to the offered set. Duplicates are ignored.
    pub fn add_mime_type(&mut self, mime_type: &str) {
        let owned = String::from(mime_type);
        if !self.mime_types.contains(&owned) {
            self.mime_types.push(owned);
        }
    }

    /// Returns true if this source can satisfy the requested MIME type.
    pub fn has_mime_type(&self, mime_type: &str) -> bool {
        self.mime_types.iter().any(|m| m == mime_type)
    }

    /// Begin an asynchronous read of the selection data. Copies the
    /// current `data` into an internal delivery buffer and marks a read
    /// as in progress. Returns `false` if a read is already in flight or
    /// there is no data to deliver.
    pub fn read_async_start(&mut self) -> bool {
        if self.read_in_progress {
            return false;
        }
        let data = match self.data.clone() {
            Some(d) => d,
            None => return false,
        };
        self.read_buffer = Some(data);
        self.read_offset = 0;
        self.read_in_progress = true;
        true
    }

    /// Pull the next chunk of an in-progress async read. `max_bytes` is
    /// the maximum number of bytes to return in this call. Returns
    /// `Some(chunk)` while data remains, and `None` once the read is
    /// fully consumed (which also finalizes the read).
    pub fn read_async_poll(&mut self, max_bytes: usize) -> Option<Vec<u8>> {
        if !self.read_in_progress {
            return None;
        }
        let buffer = self.read_buffer.as_ref()?;
        if self.read_offset >= buffer.len() {
            // Read fully consumed — finalize.
            self.read_in_progress = false;
            self.read_buffer = None;
            self.read_offset = 0;
            return None;
        }
        let end = core::cmp::min(self.read_offset + max_bytes, buffer.len());
        let chunk = buffer[self.read_offset..end].to_vec();
        self.read_offset = end;
        Some(chunk)
    }

    /// Cancel any in-progress async read, discarding the delivery buffer.
    pub fn read_async_cancel(&mut self) {
        self.read_in_progress = false;
        self.read_buffer = None;
        self.read_offset = 0;
    }

    /// Whether an async read is currently in flight.
    pub fn is_read_in_progress(&self) -> bool {
        self.read_in_progress
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
