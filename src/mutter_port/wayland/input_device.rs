//! GNOME src/wayland/meta-wayland-input-device.c
//!
//! MetaWaylandInputDevice is the common GObject base class shared by the
//! pointer, keyboard and touch devices. In Mutter it stores a back-pointer to
//! the owning seat and a per-device monotonically increasing serial counter
//! used to tag protocol events (enter/leave/button/key/down/up).
//!
//! Here we model it as a small value type referenced by id. Serials are the
//! load-bearing piece: every Wayland input event carries a serial, and clients
//! echo serials back to validate grabs/popups, so we reproduce that counter.
//!
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-input-device.c

use core::sync::atomic::{AtomicU32, Ordering};

/// Global serial source. In Mutter serials come from
/// `wl_display_next_serial()`, i.e. they are display-wide, not per-device;
/// `meta_wayland_input_device_next_serial()` just forwards to the display.
/// We keep a single shared counter to mirror that behaviour.
static NEXT_SERIAL: AtomicU32 = AtomicU32::new(1);

/// Allocate the next display-wide event serial.
pub fn next_serial() -> u32 {
    NEXT_SERIAL.fetch_add(1, Ordering::Relaxed)
}

/// Common state for a seat input device (pointer/keyboard/touch).
///
/// The real class is a `GObject`; the only instance state that matters to the
/// model is the owning seat id. Serial allocation is display-global (see
/// [`next_serial`]).
#[derive(Debug, Clone)]
pub struct MetaWaylandInputDevice {
    /// Owning seat id.
    seat: u32,
}

impl MetaWaylandInputDevice {
    pub fn new(seat: u32) -> Self {
        MetaWaylandInputDevice { seat }
    }

    /// meta_wayland_input_device_get_seat()
    pub fn get_seat(&self) -> u32 {
        self.seat
    }

    /// meta_wayland_input_device_next_serial()
    pub fn next_serial(&self) -> u32 {
        next_serial()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seat_backpointer() {
        let dev = MetaWaylandInputDevice::new(7);
        assert_eq!(dev.get_seat(), 7);
    }

    #[test]
    fn test_serials_monotonic() {
        let dev = MetaWaylandInputDevice::new(1);
        let a = dev.next_serial();
        let b = dev.next_serial();
        let c = next_serial();
        assert!(b > a);
        assert!(c > b);
    }
}
