//! X11 selection (clipboard) management.
//!
//! Ported from GNOME Mutter's src/x11/meta-x11-selection.c/.h,
//! src/x11/meta-selection-source-x11.c/.h, and selection stream files.
//! Manages clipboard/primary/secondary selections with streaming support.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/meta-x11-selection.c

use crate::mutter_port::x11::display::XWindow;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Selection type (clipboard, primary, secondary).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionType {
    Clipboard,
    Primary,
    Secondary,
}

/// Monotonically increasing identifier for an in-flight selection transfer.
pub type TransferId = u64;

/// State of a single selection read transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransferState {
    /// XConvertSelection has been requested but no SelectionNotify yet.
    Pending,
    /// The owner reported no data for the requested target.
    Empty,
    /// Data is being streamed in via PropertyNotify chunks.
    Streaming,
    /// All data has been received.
    Completed,
    /// The transfer was cancelled or failed.
    Failed,
}

/// Tracks one in-flight selection read initiated by XConvertSelection.
pub struct SelectionTransfer {
    pub transfer_id: TransferId,
    pub selection_type: SelectionType,
    pub owner_window: XWindow,
    pub mime_type: String,
    pub target_atom: u64,
    pub property_atom: u64,
    pub state: TransferState,
    /// Buffer accumulating the received bytes.
    pub buffer: Vec<u8>,
}

impl SelectionTransfer {
    pub fn new(
        transfer_id: TransferId,
        selection_type: SelectionType,
        owner_window: XWindow,
        mime_type: String,
        target_atom: u64,
        property_atom: u64,
    ) -> Self {
        Self {
            transfer_id,
            selection_type,
            owner_window,
            mime_type,
            target_atom,
            property_atom,
            state: TransferState::Pending,
            buffer: Vec::new(),
        }
    }

    /// Append a chunk of data received via PropertyNotify.
    pub fn append_chunk(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
        if self.state == TransferState::Pending || self.state == TransferState::Streaming {
            self.state = TransferState::Streaming;
        }
    }

    /// Mark the transfer complete after the final SelectionNotify.
    pub fn finish(&mut self) {
        self.state = TransferState::Completed;
    }

    /// Mark the transfer as having no data (owner returned None property).
    pub fn mark_empty(&mut self) {
        self.state = TransferState::Empty;
    }

    /// Mark the transfer failed.
    pub fn fail(&mut self) {
        self.state = TransferState::Failed;
    }
}

/// Selection source representing data offered by a window.
pub struct MetaSelectionSourceX11 {
    pub selection_type: SelectionType,
    pub owner_window: XWindow,
    pub timestamp: u32,

    /// Available MIME types for this selection.
    pub mime_types: Vec<String>,

    /// Offered targets.
    pub targets: Vec<u64>, // Atom handles

    /// In-flight read transfers keyed by transfer id.
    pub transfers: BTreeMap<TransferId, SelectionTransfer>,

    /// Next transfer id to hand out.
    next_transfer_id: TransferId,
}

impl MetaSelectionSourceX11 {
    /// Create a new X11 selection source.
    pub fn new(selection_type: SelectionType, owner_window: XWindow) -> Self {
        Self {
            selection_type,
            owner_window,
            timestamp: 0,
            mime_types: Vec::new(),
            targets: Vec::new(),
            transfers: BTreeMap::new(),
            next_transfer_id: 1,
        }
    }

    /// Get the list of MIME types available from this source.
    pub fn get_mime_types(&self) -> &[String] {
        &self.mime_types
    }

    /// Initiate an asynchronous read of the selection data for `mime_type`.
    ///
    /// A full implementation would call XConvertSelection on the owner window
    /// with the target atom derived from `mime_type`, then wait for the
    /// SelectionNotify event. Here we record the transfer state so the caller
    /// can drive it via `deliver_chunk` / `complete_transfer`. Returns the
    /// transfer id used to track the request.
    pub fn read_async(
        &mut self,
        mime_type: &str,
        target_atom: u64,
        property_atom: u64,
    ) -> TransferId {
        let transfer_id = self.next_transfer_id;
        self.next_transfer_id += 1;
        let transfer = SelectionTransfer::new(
            transfer_id,
            self.selection_type,
            self.owner_window,
            String::from(mime_type),
            target_atom,
            property_atom,
        );
        self.transfers.insert(transfer_id, transfer);
        transfer_id
    }

    /// Deliver a PropertyNotify chunk to a pending transfer.
    pub fn deliver_chunk(&mut self, transfer_id: TransferId, chunk: &[u8]) {
        if let Some(t) = self.transfers.get_mut(&transfer_id) {
            t.append_chunk(chunk);
        }
    }

    /// Complete a transfer after the final SelectionNotify arrives.
    pub fn complete_transfer(&mut self, transfer_id: TransferId, empty: bool) {
        if let Some(t) = self.transfers.get_mut(&transfer_id) {
            if empty {
                t.mark_empty();
            } else {
                t.finish();
            }
        }
    }

    /// Cancel a transfer (e.g. owner window destroyed).
    pub fn cancel_transfer(&mut self, transfer_id: TransferId) {
        if let Some(t) = self.transfers.get_mut(&transfer_id) {
            t.fail();
        }
    }

    /// Look up a transfer by id.
    pub fn get_transfer(&self, transfer_id: TransferId) -> Option<&SelectionTransfer> {
        self.transfers.get(&transfer_id)
    }

    /// Remove a finished transfer and return its buffered data.
    pub fn take_transfer(&mut self, transfer_id: TransferId) -> Option<SelectionTransfer> {
        self.transfers.remove(&transfer_id)
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
    pub fn new(owner_window: XWindow, selection: u64, target: u64, property: u64) -> Self {
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
