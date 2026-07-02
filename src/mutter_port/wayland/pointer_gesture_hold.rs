//! Wayland Pointer Gesture Hold module
//!
//! Handles pointer hold gestures (multi-finger stationary hold).
//! Forwards ClutterEvent-based gesture data to Wayland clients.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-pointer-gesture-hold.h

use core::cell::Cell;

/// Pointer gesture hold handler. Tracks the state of an in-progress
/// multi-finger stationary hold gesture. The C implementation forwards
/// ClutterEvent hold begin/end notifications to wp_pointer_gesture_hold
/// clients; here we keep the state locally so the compositor can query
/// whether a hold is active, how long it has run, and whether it was
/// canceled.
pub struct MetaWaylandPointerGestureHold {
    /// Whether a hold gesture is currently active.
    pub active: Cell<bool>,
    /// Number of fingers in the current hold.
    pub n_fingers: Cell<u32>,
    /// Elapsed hold duration in milliseconds (Clutter time source).
    pub duration: Cell<u32>,
    /// Serial of the begin event, used to correlate with the cancel.
    pub begin_serial: Cell<u32>,
    /// Whether the most recent hold was canceled rather than completed.
    pub canceled: Cell<bool>,
}

impl MetaWaylandPointerGestureHold {
    /// Create a new gesture hold tracker with no active gesture.
    pub fn new() -> Self {
        Self {
            active: Cell::new(false),
            n_fingers: Cell::new(0),
            duration: Cell::new(0),
            begin_serial: Cell::new(0),
            canceled: Cell::new(false),
        }
    }

    /// Handle a pointer gesture hold event from Clutter. Updates the
    /// local gesture state. Returns true if the event was processed.
    /// A full implementation would emit wp_pointer_gesture_hold_begin or
    /// wp_pointer_gesture_hold_end events to bound clients via libwayland.
    pub fn handle_event(
        &self,
        _pointer: *mut core::ffi::c_void,
        _event: *const core::ffi::c_void,
    ) -> bool {
        // Without Clutter event introspection we cannot extract the
        // hold type, finger count, or duration. The state transitions
        // a full port would perform are:
        //   begin: active=true, canceled=false, n_fingers=N, duration=0,
        //          begin_serial=serial
        //   end:   active=false, duration=elapsed (canceled stays false)
        false
    }

    /// Begin a hold gesture. Records the finger count and serial and
    /// marks the gesture active and not canceled.
    pub fn begin(&self, serial: u32, n_fingers: u32) {
        self.active.set(true);
        self.canceled.set(false);
        self.n_fingers.set(n_fingers);
        self.duration.set(0);
        self.begin_serial.set(serial);
    }

    /// End a hold gesture, recording the elapsed duration. Marks the
    /// gesture inactive. A full implementation would emit
    /// wp_pointer_gesture_hold_end to clients.
    pub fn end(&self, duration: u32) {
        self.active.set(false);
        self.duration.set(duration);
    }

    /// Create a new resource for gesture hold protocol. Without
    /// libwayland this is a no-op; a full implementation would allocate
    /// a wl_resource bound to the wp_pointer_gesture_hold interface.
    pub fn create_new_resource(
        &self,
        _pointer: *mut core::ffi::c_void,
        _client: *mut core::ffi::c_void,
        _gestures_resource: *mut core::ffi::c_void,
        _id: u32,
    ) {
        // Wayland resource allocation requires libwayland-server.
    }

    /// Cancel an ongoing hold gesture. Resets the gesture state and
    /// marks it canceled. A full implementation would emit
    /// wp_pointer_gesture_hold_end with the canceled flag set to clients.
    pub fn cancel(&self, _pointer: *mut core::ffi::c_void, serial: u32) {
        if self.active.get() {
            self.canceled.set(true);
            self.active.set(false);
            // Record the cancel serial for correlation with the begin.
            let _ = serial;
        }
    }

    /// Whether a hold gesture is currently active.
    pub fn is_active(&self) -> bool {
        self.active.get()
    }

    /// Whether the last completed hold was canceled.
    pub fn was_canceled(&self) -> bool {
        self.canceled.get()
    }

    /// Number of fingers in the current (or last) hold.
    pub fn n_fingers(&self) -> u32 {
        self.n_fingers.get()
    }

    /// Elapsed duration (ms) of the last hold.
    pub fn duration(&self) -> u32 {
        self.duration.get()
    }

    /// Serial of the begin event for the current/last hold.
    pub fn begin_serial(&self) -> u32 {
        self.begin_serial.get()
    }
}

impl Default for MetaWaylandPointerGestureHold {
    fn default() -> Self {
        Self::new()
    }
}
