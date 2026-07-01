//! GNOME Mutter's src/backends/meta-orientation-manager.c
//!
//! Screen orientation manager: reads the current device orientation from an
//! accelerometer (via iio-sensor-proxy) and exposes it, plus an
//! "orientation-lock" gsetting that inhibits tracking. Drives auto-rotation.
//!
//! Stubbed: the D-Bus iio-sensor-proxy (net.hadess.SensorProxy), the g_bus
//! name watch, GSettings, and GObject signals are all unavailable in the
//! kernel. The orientation state machine (claim/inhibit bookkeeping, has-accel
//! transitions, orientation change detection) is ported faithfully; sensor
//! input is fed in via `update_from_sensor()` and lock state via `set_locked()`.
//! Signal emission is replaced by returning the emitted events.
//!
//! Reference:
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-orientation-manager.c

use alloc::vec::Vec;

/// MetaOrientation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Undefined,
    Normal,
    BottomUp,
    LeftUp,
    RightUp,
}

/// MtkMonitorTransform (subset produced by orientation).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorTransform {
    Normal,
    Transform90,
    Transform180,
    Transform270,
}

/// meta_orientation_to_transform()
pub fn orientation_to_transform(orientation: Orientation) -> MonitorTransform {
    match orientation {
        Orientation::BottomUp => MonitorTransform::Transform180,
        Orientation::LeftUp => MonitorTransform::Transform90,
        Orientation::RightUp => MonitorTransform::Transform270,
        Orientation::Undefined | Orientation::Normal => MonitorTransform::Normal,
    }
}

/// orientation_from_string(): map the iio-sensor-proxy
/// AccelerometerOrientation string to a MetaOrientation.
pub fn orientation_from_string(orientation: &str) -> Orientation {
    match orientation {
        "normal" => Orientation::Normal,
        "bottom-up" => Orientation::BottomUp,
        "left-up" => Orientation::LeftUp,
        "right-up" => Orientation::RightUp,
        _ => Orientation::Undefined,
    }
}

/// Events emitted by the manager, replacing the "orientation-changed" and
/// "sensor-active" GObject signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrientationEvent {
    OrientationChanged,
    SensorActive,
}

/// MetaOrientationManager. Mirrors struct _MetaOrientationManager (minus the
/// D-Bus proxy / cancellable / GSettings handles, which are stubbed).
#[derive(Debug)]
pub struct OrientationManager {
    /// Whether the iio-sensor-proxy is currently present on the bus.
    iio_present: bool,
    orientation: Orientation,
    has_accel: bool,
    orientation_locked: bool,
    should_claim: bool,
    is_claimed: bool,
    inhibited_count: i32,
}

impl Default for OrientationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl OrientationManager {
    /// meta_orientation_manager_init()
    pub fn new() -> Self {
        OrientationManager {
            iio_present: false,
            orientation: Orientation::Undefined,
            has_accel: false,
            orientation_locked: false,
            should_claim: false,
            is_claimed: false,
            inhibited_count: 0,
        }
    }

    /// meta_orientation_manager_get_orientation()
    pub fn orientation(&self) -> Orientation {
        self.orientation
    }

    /// meta_orientation_manager_has_accelerometer()
    pub fn has_accelerometer(&self) -> bool {
        self.has_accel
    }

    pub fn is_claimed(&self) -> bool {
        self.is_claimed
    }

    /// sync_accelerometer_claimed(): claim/release the sensor based on whether
    /// the proxy is present and tracking is not inhibited. The async D-Bus
    /// ClaimAccelerometer/ReleaseAccelerometer round-trip is stubbed, so the
    /// claimed state resolves synchronously.
    fn sync_accelerometer_claimed(&mut self) {
        let should_claim = self.iio_present && self.inhibited_count == 0;
        if self.should_claim == should_claim {
            return;
        }
        self.should_claim = should_claim;
        self.is_claimed = should_claim && self.iio_present;
    }

    /// update_has_accel(): update the has-accelerometer flag and, if the sensor
    /// went away, clear the orientation.
    fn update_has_accel(&mut self, has_accel: bool, events: &mut Vec<OrientationEvent>) {
        if self.has_accel == has_accel {
            return;
        }
        self.has_accel = has_accel;
        if !has_accel && self.orientation != Orientation::Undefined {
            self.orientation = Orientation::Undefined;
            events.push(OrientationEvent::OrientationChanged);
        }
    }

    /// sync_state(): recompute orientation from a sensor reading string and
    /// emit orientation-changed when it differs.
    fn sync_state(&mut self, reading: &str, events: &mut Vec<OrientationEvent>) {
        let new_orientation = orientation_from_string(reading);
        if self.orientation == new_orientation {
            return;
        }
        self.orientation = new_orientation;
        events.push(OrientationEvent::OrientationChanged);
    }

    /// iio_sensor_appeared_cb() + iio_proxy_ready(): the iio-sensor-proxy became
    /// available and reported its HasAccelerometer property.
    pub fn on_sensor_appeared(&mut self, has_accel: bool) -> Vec<OrientationEvent> {
        let mut events = Vec::new();
        self.iio_present = true;
        self.update_has_accel(has_accel, &mut events);
        self.sync_accelerometer_claimed();
        events
    }

    /// iio_sensor_vanished_cb(): the iio-sensor-proxy went away.
    pub fn on_sensor_vanished(&mut self) -> Vec<OrientationEvent> {
        let mut events = Vec::new();
        self.iio_present = false;
        self.is_claimed = false;
        self.sync_accelerometer_claimed();
        self.update_has_accel(false, &mut events);
        events
    }

    /// on_get_properties()/iio_properties_changed_idle(): feed a fresh sensor
    /// orientation reading. Emits orientation-changed and, when the sensor is
    /// claimed and active, sensor-active.
    pub fn update_from_sensor(&mut self, reading: &str) -> Vec<OrientationEvent> {
        let mut events = Vec::new();
        if self.has_accel && self.should_claim && self.is_claimed {
            self.sync_state(reading, &mut events);
            events.push(OrientationEvent::SensorActive);
        }
        events
    }

    /// orientation_lock_changed(): apply the "orientation-lock" gsetting.
    pub fn set_locked(&mut self, orientation_locked: bool) {
        if self.orientation_locked == orientation_locked {
            return;
        }
        self.orientation_locked = orientation_locked;
        if self.orientation_locked {
            self.inhibit_tracking();
        } else {
            self.uninhibit_tracking();
        }
    }

    /// meta_orientation_manager_inhibit_tracking()
    pub fn inhibit_tracking(&mut self) {
        self.inhibited_count += 1;
        if self.inhibited_count == 1 {
            self.sync_accelerometer_claimed();
        }
    }

    /// meta_orientation_manager_uninhibit_tracking()
    pub fn uninhibit_tracking(&mut self) {
        self.inhibited_count -= 1;
        if self.inhibited_count == 0 {
            self.sync_accelerometer_claimed();
        }
    }
}
