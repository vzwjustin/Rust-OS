//! GNOME src/wayland/meta-wayland-system-bell.c
//!
//! MetaWaylandSystemBell implements the xdg_system_bell_v1 global. Clients call
//! `ring`, optionally targeting a surface; the compositor forwards this to the
//! display's bell (visual/audible feedback).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-system-bell.c

use alloc::{collections::BTreeMap, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};

/// A single bell ring event. `surface_id == None` means a bell not associated
/// with any particular surface (system-wide).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BellEvent {
    pub surface_id: Option<u32>,
    pub client_id: u32,
}

/// The xdg_system_bell_v1 global.
pub struct MetaWaylandSystemBell {
    /// Bound resource ids -> client id.
    resources: BTreeMap<u32, u32>,
    /// Pending rings to be drained and delivered to the display bell.
    pending: Vec<BellEvent>,
    next_id: AtomicU32,
}

impl MetaWaylandSystemBell {
    pub fn new() -> Self {
        MetaWaylandSystemBell {
            resources: BTreeMap::new(),
            pending: Vec::new(),
            next_id: AtomicU32::new(1),
        }
    }

    /// system_bell_bind - a client binds the global.
    pub fn bind(&mut self, client_id: u32) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::Release);
        self.resources.insert(id, client_id);
        id
    }

    /// system_bell.ring - queue a bell for `surface_id` (or system-wide).
    ///
    /// STUB: the real handler calls `meta_bell_notify(display, window)`; here we
    /// record the event for the compositor to dispatch.
    pub fn ring(&mut self, client_id: u32, surface_id: Option<u32>) {
        self.pending.push(BellEvent {
            surface_id,
            client_id,
        });
    }

    /// Drain queued bell events.
    pub fn take_pending(&mut self) -> Vec<BellEvent> {
        core::mem::take(&mut self.pending)
    }

    pub fn destroy(&mut self, id: u32) -> bool {
        self.resources.remove(&id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ring_targeted_and_global() {
        let mut bell = MetaWaylandSystemBell::new();
        let _r = bell.bind(3);
        bell.ring(3, Some(99));
        bell.ring(3, None);
        let events = bell.take_pending();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].surface_id, Some(99));
        assert_eq!(events[1].surface_id, None);
        assert!(bell.take_pending().is_empty());
    }

    #[test]
    fn test_bind_destroy() {
        let mut bell = MetaWaylandSystemBell::new();
        let id = bell.bind(1);
        assert!(bell.destroy(id));
        assert!(!bell.destroy(id));
    }
}
