//! Input Capture Private — ported from GNOME Mutter
//!
//! Private activation/deactivation interface for input capture sessions.
//! Used by screencast and remote access protocols to grab input.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-input-capture-private.h

use core::cell::Cell;

/// Opaque input capture type. Tracks active sessions.
pub struct InputCapture {
    active_session_count: Cell<u32>,
}

impl InputCapture {
    pub fn new() -> Self {
        Self {
            active_session_count: Cell::new(0),
        }
    }

    /// Number of currently active capture sessions.
    pub fn active_session_count(&self) -> u32 {
        self.active_session_count.get()
    }
}

impl Default for InputCapture {
    fn default() -> Self {
        Self::new()
    }
}

/// Input capture session. Tracks whether the session is actively
/// capturing input.
pub struct InputCaptureSession {
    active: Cell<bool>,
}

impl InputCaptureSession {
    pub fn new() -> Self {
        Self {
            active: Cell::new(false),
        }
    }

    /// Whether this session is actively capturing input.
    pub fn is_active(&self) -> bool {
        self.active.get()
    }
}

impl Default for InputCaptureSession {
    fn default() -> Self {
        Self::new()
    }
}

/// Activate an input capture session. Marks the session as active
/// and increments the capture's active session count.
pub fn meta_input_capture_activate(input_capture: &InputCapture, session: &InputCaptureSession) {
    if !session.is_active() {
        session.active.set(true);
        input_capture
            .active_session_count
            .set(input_capture.active_session_count.get() + 1);
    }
}

/// Deactivate an input capture session. Marks the session as inactive
/// and decrements the capture's active session count.
pub fn meta_input_capture_deactivate(input_capture: &InputCapture, session: &InputCaptureSession) {
    if session.is_active() {
        session.active.set(false);
        let count = input_capture.active_session_count.get();
        if count > 0 {
            input_capture.active_session_count.set(count - 1);
        }
    }
}
