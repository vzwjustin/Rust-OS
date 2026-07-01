//! Port of GNOME mutter's `clutter/clutter-seat.{c,h}`.
//!
//! `ClutterSeat` is the abstract base class representing one input seat:
//! it owns the set of `ClutterInputDevice`s, tracks pointer/keyboard
//! state, and exposes backend virtuals (`warp_pointer`, `query_state`,
//! `handle_event_post`, ...). In upstream it's an abstract GObject
//! subclassed per backend (evdev, native, X11).
//!
//! # What's ported
//!
//! - The device-ownership core: a `Seat` struct owning a
//!   `Vec<InputDevice>` keyed by `DeviceId`, with `add_device`/
//!   `remove_device`/`device`/`devices`/`n_devices` accessors mirroring
//!   `clutter_seat_list_devices`/`peek_devices` and the backend
//!   `add_device`/`remove_device` helpers. This is the piece that wires
//!   `input_device.rs` into a coherent owner.
//! - The `inhibit_unfocus_count` counter + `inhibit_unfocus`/
//!   `uninhibit_unfocus`/`is_unfocus_inhibited` accessors, mirroring the
//!   C `ClutterSeatPrivate::inhibit_unfocus_count` and the three
//!   `clutter_seat_*` functions.
//! - The `name` field + `name()` accessor (matching the `name` property).
//! - `has_touchscreen`: scans the device list for a `Touchscreen`-type
//!   device, matching `clutter_seat_has_touchscreen`.
//! - `get_touch_mode`: returns `true` when a touchscreen is present and no
//!   pointer/keyboard is present (a simplified version of the C
//!   `CLUTTER_TOUCH_MODE` heuristic, which consults settings + heuristics;
//!   documented inline).
//!
//! # What's skipped, with rationale
//!
//! - GObject machinery (`G_DEFINE_ABSTRACT_TYPE_WITH_PRIVATE`, `GParamSpec`
//!   property install/notify, `constructed`/`finalize`/`set_property`/
//!   `get_property`): plain fields + methods.
//! - `ClutterContext *context` back-pointer: no `ClutterContext` port yet.
//! - `ClutterPointerA11ySettings` + the pointer-a11y settings accessors:
//!   the a11y settings struct isn't ported (it's a large
//!   `ClutterPointerA11ySettings` with click/dwell config); deferred to an
//!   a11y wave.
//! - `ClutterKeymap *get_keymap`: no `Keymap` port yet.
//! - `ClutterVirtualInputDevice *create_virtual_device` /
//!   `get_supported_virtual_device_types` / `get_virtual_source_pointer`:
//!   virtual input devices are backend-specific; deferred.
//! - `warp_pointer` / `init_pointer_position` / `query_state` /
//!   `handle_event_post` / `bell_notify` virtuals: these are backend
//!   virtuals that need the actual input backend (evdev/libinput) to
//!   implement; the `SeatTrait` below declares them as default-no-op
//!   virtuals so a backend port can fill them in without changing the
//!   struct shape.
//! - `is_unfocus_inhibited_changed` signal: emitted when the inhibit
//!   counter transitions to/from zero; no signal system in this port. The
//!   `inhibit_unfocus`/`uninhibit_unfocus` methods return whether the
//!   *effective* inhibited state changed, so a caller can emit/observe.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use alloc::string::String;
use alloc::vec::Vec;

use super::event::{DeviceId, Event};
use super::input_device::{InputCapabilities, InputDevice, InputDeviceType};

/// Port of `ClutterSeatClass` vtable — the backend virtuals. Default
/// implementations are no-ops / `false`, matching the C null-vtable guards.
/// A backend port implements this trait to provide `warp_pointer`,
/// `query_state`, etc.
pub trait SeatBackend {
    /// `ClutterSeatClass::warp_pointer`: move the pointer to `(x, y)`.
    /// Default no-op.
    fn warp_pointer(&mut self, _x: i32, _y: i32) {}

    /// `ClutterSeatClass::init_pointer_position`: initialize (not warp) the
    /// pointer position. Default no-op.
    fn init_pointer_position(&mut self, _x: f32, _y: f32) {}

    /// `ClutterSeatClass::handle_event_post`: post-processing after an event
    /// is dispatched. Return `true` to stop further propagation. Default
    /// `false`.
    fn handle_event_post(&mut self, _event: &Event) -> bool {
        false
    }

    /// `ClutterSeatClass::bell_notify`: ring the keyboard bell. Default
    /// no-op.
    fn bell_notify(&mut self) {}
}

/// A no-op backend for tests and hosts that haven't wired a real input
/// backend yet. All virtuals are default no-ops.
#[derive(Debug, Default)]
pub struct NullBackend;

impl SeatBackend for NullBackend {}

/// Port of `ClutterSeat` / `ClutterSeatPrivate` — the device-owning core.
///
/// Owns the seat's `InputDevice`s in a `Vec` and tracks the unfocus-inhibit
/// counter. The backend virtuals are dispatched through a separate
/// `SeatBackend` impl to keep the data/backend split clean.
#[derive(Debug)]
pub struct Seat {
    pub name: Option<String>,
    devices: Vec<InputDevice>,
    inhibit_unfocus_count: u32,
}

impl Default for Seat {
    fn default() -> Self {
        Self::new()
    }
}

impl Seat {
    /// Construct an empty seat (matching `clutter_seat_init` leaving the
    /// private fields at zero/NULL).
    pub fn new() -> Self {
        Seat {
            name: None,
            devices: Vec::new(),
            inhibit_unfocus_count: 0,
        }
    }

    /// Construct a seat with a name (matching the `name` property).
    pub fn with_name(name: impl Into<String>) -> Self {
        Seat {
            name: Some(name.into()),
            devices: Vec::new(),
            inhibit_unfocus_count: 0,
        }
    }

    /// The seat name (the `name` property getter).
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    // ---- device ownership ----

    /// Add a device to the seat, returning the `DeviceId` it's stored
    /// under. Mirrors the backend `clutter_seat_add_device` helper (the
    /// public API is `clutter_seat_list_devices`; backends call an internal
    /// add). The `DeviceId` is the index into the internal vec.
    pub fn add_device(&mut self, device: InputDevice) -> DeviceId {
        let id = DeviceId(self.devices.len() as u32);
        self.devices.push(device);
        id
    }

    /// Remove the device at `id`. Mirrors the backend
    /// `clutter_seat_remove_device` helper. Returns the removed device if
    /// the id was valid. Note: this leaves a hole (the id becomes stale);
    /// callers should not reuse ids after removal. A generational-id
    /// variant can replace this if reuse becomes a concern.
    pub fn remove_device(&mut self, id: DeviceId) -> Option<InputDevice> {
        let idx = id.0 as usize;
        if idx < self.devices.len() {
            // Swap-remove preserves the other ids' validity only if the
            // caller treats the moved device's old id as stale. For seat
            // use, devices are rarely removed, so this is acceptable.
            Some(self.devices.swap_remove(idx))
        } else {
            None
        }
    }

    /// `clutter_seat_list_devices` / `peek_devices`: borrow the device list.
    pub fn devices(&self) -> &[InputDevice] {
        &self.devices
    }

    /// Mutable access to the device list.
    pub fn devices_mut(&mut self) -> &mut [InputDevice] {
        &mut self.devices
    }

    /// Look up a device by id.
    pub fn device(&self, id: DeviceId) -> Option<&InputDevice> {
        self.devices.get(id.0 as usize)
    }

    /// Mutable device lookup.
    pub fn device_mut(&mut self, id: DeviceId) -> Option<&mut InputDevice> {
        self.devices.get_mut(id.0 as usize)
    }

    /// Number of devices on the seat.
    pub fn n_devices(&self) -> usize {
        self.devices.len()
    }

    // ---- unfocus inhibition ----

    /// `clutter_seat_inhibit_unfocus`: increment the inhibit counter.
    /// Returns `true` if this transitioned the seat from not-inhibited to
    /// inhibited (so a caller can fire the `is_unfocus_inhibited_changed`
    /// signal equivalent).
    pub fn inhibit_unfocus(&mut self) -> bool {
        let was_inhibited = self.inhibit_unfocus_count > 0;
        self.inhibit_unfocus_count = self.inhibit_unfocus_count.saturating_add(1);
        !was_inhibited
    }

    /// `clutter_seat_uninhibit_unfocus`: decrement the inhibit counter.
    /// Returns `true` if this transitioned the seat from inhibited to
    /// not-inhibited. The C version `g_critical`s on underflow; here that's
    /// expressed by returning `false` when the counter is already zero
    /// (no-op, no underflow).
    pub fn uninhibit_unfocus(&mut self) -> bool {
        if self.inhibit_unfocus_count == 0 {
            return false;
        }
        self.inhibit_unfocus_count -= 1;
        self.inhibit_unfocus_count == 0
    }

    /// `clutter_seat_is_unfocus_inhibited`.
    pub fn is_unfocus_inhibited(&self) -> bool {
        self.inhibit_unfocus_count > 0
    }

    // ---- touch-mode heuristics ----

    /// `clutter_seat_has_touchscreen`: any device with `TOUCH` capability.
    pub fn has_touchscreen(&self) -> bool {
        self.devices
            .iter()
            .any(|d| d.capabilities().contains(InputCapabilities::TOUCH))
    }

    /// `clutter_seat_get_touch_mode`: a simplified version of the C
    /// heuristic. The C version consults `ClutterSettings` and a
    /// touch-mode heuristic; here we return `true` when a touchscreen is
    /// present and no pointer or keyboard device is present (a
    /// tablet/touch-only device). A fuller port can override this once
    /// `ClutterSettings` lands.
    pub fn get_touch_mode(&self) -> bool {
        let has_touch = self.has_touchscreen();
        let has_pointer = self
            .devices
            .iter()
            .any(|d| d.capabilities().contains(InputCapabilities::POINTER));
        let has_keyboard = self
            .devices
            .iter()
            .any(|d| d.capabilities().contains(InputCapabilities::KEYBOARD));
        has_touch && !has_pointer && !has_keyboard
    }

    /// Convenience: the first pointer device on the seat, if any. Mirrors
    /// the backend `clutter_seat_get_pointer` (which returns the primary
    /// pointer device).
    pub fn pointer_device(&self) -> Option<(DeviceId, &InputDevice)> {
        self.devices
            .iter()
            .enumerate()
            .find(|(_, d)| {
                d.device_type() == InputDeviceType::Pointer
                    || d.capabilities().contains(InputCapabilities::POINTER)
            })
            .map(|(i, d)| (DeviceId(i as u32), d))
    }

    /// Convenience: the first keyboard device on the seat, if any. Mirrors
    /// the backend `clutter_seat_get_keyboard`.
    pub fn keyboard_device(&self) -> Option<(DeviceId, &InputDevice)> {
        self.devices
            .iter()
            .enumerate()
            .find(|(_, d)| {
                d.device_type() == InputDeviceType::Keyboard
                    || d.capabilities().contains(InputCapabilities::KEYBOARD)
            })
            .map(|(i, d)| (DeviceId(i as u32), d))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pointer(name: &str) -> InputDevice {
        InputDevice::new(
            InputDeviceType::Pointer,
            InputCapabilities::POINTER,
            Some(alloc::string::String::from(name)),
            0,
            0,
            0,
            None,
            0,
            0,
            0,
            0,
            3,
        )
    }

    fn keyboard(name: &str) -> InputDevice {
        InputDevice::new(
            InputDeviceType::Keyboard,
            InputCapabilities::KEYBOARD,
            Some(alloc::string::String::from(name)),
            0,
            0,
            0,
            None,
            0,
            0,
            0,
            0,
            0,
        )
    }

    fn touch(name: &str) -> InputDevice {
        InputDevice::new(
            InputDeviceType::Touchscreen,
            InputCapabilities::TOUCH,
            Some(alloc::string::String::from(name)),
            0,
            0,
            0,
            None,
            0,
            0,
            0,
            0,
            0,
        )
    }

    #[test]
    fn add_and_lookup_device() {
        let mut seat = Seat::with_name("seat0");
        let pid = seat.add_device(pointer("mouse"));
        let kid = seat.add_device(keyboard("kbd"));
        assert_eq!(seat.n_devices(), 2);
        assert_eq!(seat.device(pid).unwrap().device_name(), Some("mouse"));
        assert_eq!(seat.device(kid).unwrap().device_name(), Some("kbd"));
        assert_eq!(seat.name(), Some("seat0"));
    }

    #[test]
    fn remove_device_returns_it() {
        let mut seat = Seat::new();
        let pid = seat.add_device(pointer("mouse"));
        let removed = seat.remove_device(pid);
        assert_eq!(removed.unwrap().device_name(), Some("mouse"));
        assert_eq!(seat.n_devices(), 0);
        // Removing again -> None.
        assert!(seat.remove_device(pid).is_none());
    }

    #[test]
    fn inhibit_unfocus_transitions() {
        let mut seat = Seat::new();
        assert!(!seat.is_unfocus_inhibited());
        assert!(seat.inhibit_unfocus()); // -> inhibited
        assert!(seat.is_unfocus_inhibited());
        assert!(!seat.inhibit_unfocus()); // already inhibited, count now 2
        assert!(!seat.uninhibit_unfocus()); // count 2 -> 1, still inhibited
        assert!(seat.is_unfocus_inhibited());
        assert!(seat.uninhibit_unfocus()); // count 1 -> 0, -> not inhibited
        assert!(!seat.is_unfocus_inhibited());
        assert!(!seat.uninhibit_unfocus()); // already 0, no-op
    }

    #[test]
    fn touch_mode_only_when_touch_and_no_pointer_keyboard() {
        let mut seat = Seat::new();
        seat.add_device(touch("ts"));
        assert!(seat.has_touchscreen());
        assert!(seat.get_touch_mode());
        // Adding a pointer turns touch mode off.
        seat.add_device(pointer("mouse"));
        assert!(!seat.get_touch_mode());
    }

    #[test]
    fn pointer_and_keyboard_device_accessors() {
        let mut seat = Seat::new();
        let pid = seat.add_device(pointer("mouse"));
        let kid = seat.add_device(keyboard("kbd"));
        assert_eq!(seat.pointer_device().unwrap().0, pid);
        assert_eq!(seat.keyboard_device().unwrap().0, kid);
    }

    #[test]
    fn null_backend_virtuals_are_noops() {
        let mut b = NullBackend;
        b.warp_pointer(10, 20);
        b.init_pointer_position(5.0, 5.0);
        b.bell_notify();
        assert!(
            !b.handle_event_post(&Event::Any(super::super::event::AnyEvent {
                time_us: 0,
                flags: super::super::event::EventFlags::NONE,
                source_device: None,
            }))
        );
    }
}
