//! X11 selection (clipboard) management.
//!
//! Ported from GNOME Mutter's src/x11/meta-x11-selection.c/.h,
//! src/x11/meta-selection-source-x11.c/.h, and selection stream files.
//! Manages clipboard/primary/secondary selections with streaming support.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/meta-x11-selection.c

use crate::mutter_port::x11::display::XWindow;
use alloc::vec::Vec;

/// Selection type (clipboard, primary, secondary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionType {
    Clipboard,
    Primary,
    Secondary,
}

/// Selection source representing data offered by a window.
pub struct MetaSelectionSourceX11 {
    pub selection_type: SelectionType,
    pub owner_window: XWindow,
    pub timestamp: u32,

    /// Available MIME types for this selection.
    pub mime_types: Vec<alloc::string::String>,

    /// Offered targets.
    pub targets: Vec<u64>, // Atom handles
}

impl MetaSelectionSourceX11 {
    /// Create a new X11 selection source.
    /// # TODO: port logic from meta_selection_source_x11_new()
    pub fn new(selection_type: SelectionType, owner_window: XWindow) -> Self {
        Self {
            selection_type,
            owner_window,
            timestamp: 0,
            mime_types: Vec::new(),
            targets: Vec::new(),
        }
    }

    /// Get the list of MIME types available from this source.
    pub fn get_mime_types(&self) -> &[alloc::string::String] {
        &self.mime_types
    }

    /// Read selection data asynchronously.
    /// # TODO: port logic from meta_selection_source_read_async()
    pub fn read_async(&self, _mime_type: &str) {
        // TODO: initiate read via XConvertSelection
    }
}

/// Selection input stream for reading selection data.
pub struct MetaX11SelectionInputStream {
    pub owner_window: XWindow,
    pub selection: u64, // Atom
    pub target: u64,    // Atom
    pub property: u64,  // Atom

    /// Data accumulated so far.
    pub data: Vec<u8>,

    /// Whether the read is complete.
    pub complete: bool,
}

impl MetaX11SelectionInputStream {
    /// Create a new selection input stream.
    /// # TODO: port logic from meta_x11_selection_input_stream_new()
    pub fn new(
        owner_window: XWindow,
        selection: u64,
        target: u64,
        property: u64,
    ) -> Self {
        Self {
            owner_window,
            selection,
            target,
            property,
            data: Vec::new(),
            complete: false,
        }
    }

    /// Append data from a PropertyNotify event.
    /// # TODO: port logic from PropertyNotify handling
    pub fn append_data(&mut self, data: &[u8]) {
        self.data.extend_from_slice(data);
    }

    /// Mark this stream as complete.
    pub fn mark_complete(&mut self) {
        self.complete = true;
    }

    /// Get the accumulated data.
    pub fn get_data(&self) -> &[u8] {
        &self.data
    }
}

/// Selection output stream for writing selection data.
pub struct MetaX11SelectionOutputStream {
    pub owner_window: XWindow,
    pub requestor: XWindow,
    pub selection: u64, // Atom
    pub target: u64,    // Atom
    pub property: u64,  // Atom

    /// Data to send.
    pub data: Vec<u8>,
    pub position: usize,
}

impl MetaX11SelectionOutputStream {
    /// Create a new selection output stream.
    /// # TODO: port logic from meta_x11_selection_output_stream_new()
    pub fn new(
        owner_window: XWindow,
        requestor: XWindow,
        selection: u64,
        target: u64,
        property: u64,
    ) -> Self {
        Self {
            owner_window,
            requestor,
            selection,
            target,
            property,
            data: Vec::new(),
            position: 0,
        }
    }

    /// Set the data to send.
    pub fn set_data(&mut self, data: Vec<u8>) {
        self.data = data;
        self.position = 0;
    }

    /// Get the next chunk of data to send.
    pub fn get_next_chunk(&mut self, max_size: usize) -> &[u8] {
        let start = self.position;
        let end = (start + max_size).min(self.data.len());
        self.position = end;
        &self.data[start..end]
    }

    /// Check if all data has been sent.
    pub fn is_complete(&self) -> bool {
        self.position >= self.data.len()
    }
}
