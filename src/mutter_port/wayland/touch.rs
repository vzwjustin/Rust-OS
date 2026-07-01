//! GNOME src/wayland/meta-wayland-touch.c
//!
//! MetaWaylandTouch implements the wl_touch half of a seat. Touch is
//! multi-point: each active contact is a "touch info" keyed by its Clutter
//! event sequence, tracking the slot, the focus surface it was delivered to,
//! the down serial, and the current/start coordinates. A per-surface refcount
//! (MetaWaylandTouchSurface) keeps a surface "touched" while any contact
//! targets it. We model the touch/slot state machine; wl_touch resource
//! delivery and frame batching are stubbed.
//!
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-touch.c

use super::input_device::{next_serial, MetaWaylandInputDevice};
use alloc::collections::BTreeMap;

/// Per-active-contact state (mirrors MetaWaylandTouchInfo).
#[derive(Debug, Clone)]
pub struct TouchInfo {
    /// Surface this contact was delivered to on touch-down.
    pub focus_surface: Option<u32>,
    /// Surface currently under the contact.
    pub current_surface: Option<u32>,
    /// wl_touch slot id.
    pub slot: i32,
    /// Serial of the touch-down that started this contact.
    pub slot_serial: u32,
    pub start_x: f32,
    pub start_y: f32,
    pub x: f32,
    pub y: f32,
    /// Motion accumulated since the last frame.
    pub updated: bool,
    /// Whether the down event was actually delivered to a client.
    pub begin_delivered: bool,
}

/// MetaWaylandTouch
pub struct MetaWaylandTouch {
    parent: MetaWaylandInputDevice,

    /// Serial of the most recent touch-down (used to authorise grabs/popups).
    latest_touch_down_serial: u32,

    /// Active contacts keyed by Clutter event sequence id.
    touches: BTreeMap<u32, TouchInfo>,
    /// surface id -> number of active contacts on it (MetaWaylandTouchSurface
    /// refcount). A surface stays "touched" while its count is > 0.
    touch_surfaces: BTreeMap<u32, i32>,
}

impl MetaWaylandTouch {
    pub fn new(seat: u32) -> Self {
        MetaWaylandTouch {
            parent: MetaWaylandInputDevice::new(seat),
            latest_touch_down_serial: 0,
            touches: BTreeMap::new(),
            touch_surfaces: BTreeMap::new(),
        }
    }

    pub fn seat(&self) -> u32 {
        self.parent.get_seat()
    }

    pub fn active_touches(&self) -> usize {
        self.touches.len()
    }

    pub fn latest_down_serial(&self) -> u32 {
        self.latest_touch_down_serial
    }

    pub fn touch_info(&self, sequence: u32) -> Option<&TouchInfo> {
        self.touches.get(&sequence)
    }

    fn ref_surface(&mut self, surface: u32) {
        *self.touch_surfaces.entry(surface).or_insert(0) += 1;
    }

    fn unref_surface(&mut self, surface: u32) {
        if let Some(count) = self.touch_surfaces.get_mut(&surface) {
            *count -= 1;
            if *count <= 0 {
                self.touch_surfaces.remove(&surface);
            }
        }
    }

    /// True while `surface` has at least one active contact.
    pub fn is_touched(&self, surface: u32) -> bool {
        self.touch_surfaces.contains_key(&surface)
    }

    /// touch_down handler: begin a new contact. Returns the down serial.
    pub fn down(&mut self, sequence: u32, slot: i32, surface: Option<u32>, x: f32, y: f32) -> u32 {
        let serial = next_serial();
        self.latest_touch_down_serial = serial;
        if let Some(s) = surface {
            self.ref_surface(s);
        }
        let info = TouchInfo {
            focus_surface: surface,
            current_surface: surface,
            slot,
            slot_serial: serial,
            start_x: x,
            start_y: y,
            x,
            y,
            updated: false,
            begin_delivered: surface.is_some(),
        };
        self.touches.insert(sequence, info);
        // STUB: wl_touch.down to focus resources.
        serial
    }

    /// touch_motion handler: update coordinates for an active contact.
    /// Returns true if the contact exists.
    pub fn motion(&mut self, sequence: u32, x: f32, y: f32) -> bool {
        if let Some(info) = self.touches.get_mut(&sequence) {
            info.x = x;
            info.y = y;
            info.updated = true;
            // STUB: wl_touch.motion to focus resources.
            true
        } else {
            false
        }
    }

    /// touch_up handler: end a contact, releasing its surface reference.
    /// Returns the up serial if the contact existed.
    pub fn up(&mut self, sequence: u32) -> Option<u32> {
        let info = self.touches.remove(&sequence)?;
        if let Some(s) = info.focus_surface {
            self.unref_surface(s);
        }
        let serial = next_serial();
        // STUB: wl_touch.up + wl_touch.frame to focus resources.
        Some(serial)
    }

    /// touch_cancel handler: abort all active contacts.
    pub fn cancel(&mut self) {
        self.touches.clear();
        self.touch_surfaces.clear();
        // STUB: wl_touch.cancel to all resources.
    }

    /// meta_wayland_touch_can_popup(): a serial matching the latest down
    /// authorises a popup/grab.
    pub fn can_grab(&self, serial: u32) -> bool {
        serial != 0 && serial == self.latest_touch_down_serial
    }

    /// Called when a surface is destroyed: drop contacts targeting it.
    pub fn surface_destroyed(&mut self, surface: u32) {
        let seqs: alloc::vec::Vec<u32> = self
            .touches
            .iter()
            .filter(|(_, i)| i.focus_surface == Some(surface))
            .map(|(&seq, _)| seq)
            .collect();
        for seq in seqs {
            self.touches.remove(&seq);
        }
        self.touch_surfaces.remove(&surface);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_down_motion_up() {
        let mut t = MetaWaylandTouch::new(1);
        let s = t.down(100, 0, Some(5), 10.0, 20.0);
        assert!(s > 0);
        assert_eq!(t.active_touches(), 1);
        assert!(t.is_touched(5));

        assert!(t.motion(100, 11.0, 21.0));
        assert_eq!(t.touch_info(100).unwrap().x, 11.0);

        assert!(t.up(100).is_some());
        assert_eq!(t.active_touches(), 0);
        assert!(!t.is_touched(5));
    }

    #[test]
    fn test_surface_refcount_multitouch() {
        let mut t = MetaWaylandTouch::new(1);
        t.down(1, 0, Some(9), 0.0, 0.0);
        t.down(2, 1, Some(9), 5.0, 5.0);
        assert!(t.is_touched(9));
        t.up(1);
        // Still one contact left on surface 9.
        assert!(t.is_touched(9));
        t.up(2);
        assert!(!t.is_touched(9));
    }

    #[test]
    fn test_cancel_clears_all() {
        let mut t = MetaWaylandTouch::new(1);
        t.down(1, 0, Some(2), 0.0, 0.0);
        t.down(2, 1, Some(3), 0.0, 0.0);
        t.cancel();
        assert_eq!(t.active_touches(), 0);
        assert!(!t.is_touched(2));
    }

    #[test]
    fn test_down_serial_authorises_grab() {
        let mut t = MetaWaylandTouch::new(1);
        let s = t.down(1, 0, Some(2), 0.0, 0.0);
        assert!(t.can_grab(s));
        assert!(!t.can_grab(0));
    }

    #[test]
    fn test_surface_destroyed_drops_contacts() {
        let mut t = MetaWaylandTouch::new(1);
        t.down(1, 0, Some(4), 0.0, 0.0);
        t.surface_destroyed(4);
        assert_eq!(t.active_touches(), 0);
        assert!(!t.is_touched(4));
    }
}
