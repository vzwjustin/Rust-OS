//! X11 event source for main loop integration.
//!
//! Ported from GNOME Mutter's src/x11/meta-x11-event-source.c/.h.
//! Provides GLib GSource integration for X11 events (now adapted for Rust).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/meta-x11-event-source.c

/// Opaque event source handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventSourceId(pub u64);

/// X11 event source for main loop integration.
pub struct MetaX11EventSource {
    pub source_id: EventSourceId,

    /// Display handle (opaque).
    pub xdisplay: u64,

    /// Event queue for buffered events.
    pub pending_events: alloc::vec::Vec<u64>,

    /// Last event serial processed.
    pub last_serial: u64,

    /// Whether the event source is active.
    pub active: bool,
}

impl MetaX11EventSource {
    /// Create a new X11 event source.
    /// # TODO: port logic from meta_x11_event_source_new()
    pub fn new(xdisplay: u64) -> Self {
        Self {
            source_id: EventSourceId(0),
            xdisplay,
            pending_events: alloc::vec::Vec::new(),
            last_serial: 0,
            active: false,
        }
    }

    /// Prepare the event source (check for pending events).
    /// # TODO: port logic from source prepare callback
    pub fn prepare(&self) -> bool {
        // TODO: check XPending() for events
        !self.pending_events.is_empty()
    }

    /// Query the event source (compute timeout).
    /// # TODO: port logic from source query callback
    pub fn query(&self) -> Option<u32> {
        if self.prepare() {
            Some(0) // Don't wait
        } else {
            None // Wait indefinitely
        }
    }

    /// Process the event source (dispatch events).
    /// # TODO: port logic from source dispatch callback
    pub fn dispatch(&mut self) -> bool {
        let mut processed = false;
        while !self.pending_events.is_empty() {
            // TODO: pop event from queue
            // TODO: dispatch to handlers
            processed = true;
        }
        processed
    }

    /// Add an event to the pending queue.
    pub fn queue_event(&mut self, event: u64) {
        self.pending_events.push(event);
    }

    /// Clear all pending events.
    pub fn clear_events(&mut self) {
        self.pending_events.clear();
    }

    /// Get the number of pending events.
    pub fn pending_count(&self) -> usize {
        self.pending_events.len()
    }

    /// Activate the event source.
    pub fn activate(&mut self) {
        self.active = true;
    }

    /// Deactivate the event source.
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// Check if the event source is active.
    pub fn is_active(&self) -> bool {
        self.active
    }
}
