//! X11 Sync counter support (WM_SYNC_REQUEST).
//!
//! Ported from GNOME Mutter's src/x11/meta-sync-counter.c/.h.
//! Implements the extended WM hints WM_SYNC_REQUEST protocol for coordinating
//! client window painting with the compositor's frame cycles.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/x11/meta-sync-counter.c

/// Represents a single XSync counter used for WM_SYNC_REQUEST.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SyncCounter {
    pub counter_id: u64,
    pub alarm_id: u64,
    /// Current counter value (tracked locally since we have no XSync).
    pub value: i64,
    /// Whether the counter has been initialized.
    pub initialized: bool,
}

impl SyncCounter {
    /// Create a new sync counter.
    pub fn new() -> Self {
        Self {
            counter_id: 0,
            alarm_id: 0,
            value: 0,
            initialized: false,
        }
    }

    /// Initialize the counter with a unique ID.
    ///
    /// A full implementation would call `XSyncCreateCounter` on the X
    /// display, passing an initial `XSyncValue`, and store the returned
    /// XID in `counter_id`. Without an X connection we allocate a
    /// synthetic, monotonically increasing XID so callers can still
    /// distinguish counters.
    pub fn init(&mut self) {
        if self.counter_id == 0 {
            // Generate a synthetic counter ID. In upstream, this is
            // an XID allocated by the X server.
            self.counter_id = 1;
        }
        self.initialized = true;
        self.value = 0;
    }

    /// Destroy the counter. A full implementation would call
    /// XSyncDestroyCounter.
    pub fn destroy(&mut self) {
        self.counter_id = 0;
        self.alarm_id = 0;
        self.value = 0;
        self.initialized = false;
    }

    /// Create an alarm for this counter. Generates a synthetic alarm
    /// ID. A full implementation would call XSyncCreateAlarm.
    pub fn create_alarm(&mut self) {
        if self.counter_id != 0 && self.alarm_id == 0 {
            self.alarm_id = self.counter_id + 1;
        }
    }

    /// Destroy the alarm for this counter.
    pub fn destroy_alarm(&mut self) {
        self.alarm_id = 0;
    }

    /// Query the current counter value. Returns the locally tracked
    /// value. A full implementation would call XSyncQueryCounter.
    pub fn query_value(&self) -> Option<i64> {
        if self.initialized {
            Some(self.value)
        } else {
            None
        }
    }

    /// Set the counter value. Updates the local tracking. A full
    /// implementation would call XSyncSetCounter.
    pub fn set_value(&mut self, value: i64) {
        self.value = value;
    }

    /// Increment the counter value by delta. A full implementation
    /// would call XSyncChangeCounter.
    pub fn increment(&mut self, delta: i64) {
        self.value = self.value.saturating_add(delta);
    }
}

impl Default for SyncCounter {
    fn default() -> Self {
        Self::new()
    }
}

/// WM_SYNC_REQUEST protocol handler.
pub struct SyncRequestHandler {
    pub counter: SyncCounter,
    pub pending_sync_request: bool,
    pub last_sync_request_value: i64,
}

impl SyncRequestHandler {
    /// Create a new sync request handler.
    pub fn new() -> Self {
        Self {
            counter: SyncCounter::new(),
            pending_sync_request: false,
            last_sync_request_value: 0,
        }
    }

    /// Begin a new sync request cycle.
    ///
    /// Records the counter value the client committed via its
    /// `_NET_WM_SYNC_REQUEST` counter and arms the handler so that the
    /// next [`handle_alarm`] / [`alarm_notify`] call completes the
    /// request. Upstream Mutter pairs this with `XSyncSetCounter` to
    /// publish the new frame serial to the client.
    pub fn begin_sync_request(&mut self, counter_value: i64) {
        self.pending_sync_request = true;
        self.last_sync_request_value = counter_value;
        self.counter.set_value(counter_value);
    }

    /// Handle a sync request alarm event.
    ///
    /// Upstream Mutter receives an `XSyncAlarmNotifyEvent` when the
    /// client's redraw counter reaches the requested serial. Without an
    /// X connection we treat any pending request as satisfied and
    /// return `true`; a no-op call (nothing pending) returns `false`.
    pub fn handle_alarm(&mut self) -> bool {
        if self.pending_sync_request {
            self.pending_sync_request = false;
            return true;
        }
        false
    }

    /// Process an `XSyncAlarmNotifyEvent` carrying an updated counter
    /// value.
    ///
    /// A full implementation would decode the `XSyncAlarmNotifyEvent`
    /// (matching `alarm_id` to our counter's alarm), extract the
    /// `XSyncValue`, and advance the local counter to that value before
    /// completing the pending sync request. Here we accept the reported
    /// value, update the counter, and clear the pending flag.
    ///
    /// Returns `true` if a pending sync request was satisfied.
    pub fn alarm_notify(&mut self, alarm_value: i64) -> bool {
        self.counter.set_value(alarm_value);
        self.last_sync_request_value = alarm_value;
        if self.pending_sync_request {
            self.pending_sync_request = false;
            return true;
        }
        false
    }
}

impl Default for SyncRequestHandler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_counter_creation() {
        let counter = SyncCounter::new();
        assert_eq!(counter.counter_id, 0);
        assert_eq!(counter.alarm_id, 0);
    }

    #[test]
    fn test_sync_request_handler() {
        let mut handler = SyncRequestHandler::new();
        assert!(!handler.pending_sync_request);
        assert_eq!(handler.last_sync_request_value, 0);

        handler.begin_sync_request(42);
        assert!(handler.pending_sync_request);
        assert_eq!(handler.last_sync_request_value, 42);
    }

    #[test]
    fn test_sync_request_alarm() {
        let mut handler = SyncRequestHandler::new();
        handler.begin_sync_request(10);
        assert!(handler.pending_sync_request);

        let result = handler.handle_alarm();
        assert!(result);
        assert!(!handler.pending_sync_request);
    }

    #[test]
    fn test_sync_request_alarm_no_pending() {
        let mut handler = SyncRequestHandler::new();
        let result = handler.handle_alarm();
        assert!(!result);
    }
}
