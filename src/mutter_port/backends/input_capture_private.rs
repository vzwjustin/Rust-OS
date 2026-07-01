//! Input Capture Private — ported from GNOME Mutter
//!
//! Private activation/deactivation interface for input capture sessions.
//! Used by screencast and remote access protocols to grab input.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-input-capture-private.h

/// Opaque input capture type.
pub struct InputCapture;

/// Opaque input capture session type.
pub struct InputCaptureSession;

/// Activate an input capture session.
pub fn meta_input_capture_activate(_input_capture: &InputCapture, _session: &InputCaptureSession) {
    // TODO: implementation
}

/// Deactivate an input capture session.
pub fn meta_input_capture_deactivate(_input_capture: &InputCapture, _session: &InputCaptureSession) {
    // TODO: implementation
}
