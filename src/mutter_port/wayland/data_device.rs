//! GNOME src/wayland/meta-wayland-data-device.c
//!
//! MetaWaylandDataDevice implements `wl_data_device`: the per-seat entry point
//! for clipboard selection and drag-and-drop. This is a compact model of the
//! two selection channels (clipboard + DnD) and the drag grab state machine
//! (origin surface, drag icon, focus surface, negotiated action). All objects
//! (sources, offers, surfaces) are referenced by id (`u32`) for loose coupling.
//!
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-data-device.c

use alloc::vec::Vec;

use super::data_source::DND_ACTION_NONE;

/// Which selection channel a source occupies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionType {
    Clipboard,
    Primary,
    Dnd,
}

/// State of the drag-and-drop grab.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragState {
    /// No drag in progress.
    Idle,
    /// Drag started, pointer/touch grab active, awaiting motion/drop.
    Dragging,
    /// Drop occurred, awaiting the receiving client's `finish`.
    Dropped,
    /// Drag cancelled or completed.
    Finished,
}

/// The active drag grab, mirroring MetaWaylandDragGrab.
#[derive(Debug, Clone)]
pub struct DragGrab {
    /// Client that initiated the drag.
    pub drag_client_id: u32,
    /// Surface the drag originated from (`drag_origin`).
    pub origin_surface_id: u32,
    /// Optional drag icon surface (`drag_surface`).
    pub icon_surface_id: Option<u32>,
    /// Data source backing the drag, if any (NULL for intra-client drags).
    pub data_source_id: Option<u32>,
    /// Surface currently under the pointer (`drag_focus`).
    pub focus_surface_id: Option<u32>,
    /// Data offer sent to the focus surface's client.
    pub focus_offer_id: Option<u32>,
    /// Grab start position in stage coordinates.
    pub start_x: i32,
    pub start_y: i32,
    /// State machine.
    pub state: DragState,
}

impl DragGrab {
    pub fn new(drag_client_id: u32, origin_surface_id: u32, data_source_id: Option<u32>) -> Self {
        DragGrab {
            drag_client_id,
            origin_surface_id,
            icon_surface_id: None,
            data_source_id,
            focus_surface_id: None,
            focus_offer_id: None,
            start_x: 0,
            start_y: 0,
            state: DragState::Dragging,
        }
    }

    /// Set the surface under the pointer, clearing any stale offer.
    pub fn set_focus(&mut self, surface_id: Option<u32>) {
        if self.focus_surface_id != surface_id {
            self.focus_surface_id = surface_id;
            self.focus_offer_id = None;
        }
    }
}

/// A single selection channel's current source.
#[derive(Debug, Clone, Copy, Default)]
struct SelectionSlot {
    source_id: Option<u32>,
    serial: u32,
}

/// Back-compat alias for the module's earlier `DataDevice` name, kept so the
/// parent `mod.rs` re-export (`pub use data_device::DataDevice;`) resolves.
pub type DataDevice = MetaWaylandDataDevice;

/// MetaWaylandDataDevice — per-seat clipboard + DnD coordinator.
pub struct MetaWaylandDataDevice {
    /// Owning seat id.
    pub seat_id: u32,
    clipboard: SelectionSlot,
    primary: SelectionSlot,
    dnd: SelectionSlot,
    /// Active drag grab, if a drag is in progress.
    pub drag_grab: Option<DragGrab>,
    /// Bound `wl_data_device` resource ids (one per client focus).
    pub resources: Vec<u32>,
}

impl MetaWaylandDataDevice {
    pub fn new(seat_id: u32) -> Self {
        MetaWaylandDataDevice {
            seat_id,
            clipboard: SelectionSlot::default(),
            primary: SelectionSlot::default(),
            dnd: SelectionSlot::default(),
            drag_grab: None,
            resources: Vec::new(),
        }
    }

    fn slot(&mut self, ty: SelectionType) -> &mut SelectionSlot {
        match ty {
            SelectionType::Clipboard => &mut self.clipboard,
            SelectionType::Primary => &mut self.primary,
            SelectionType::Dnd => &mut self.dnd,
        }
    }

    fn slot_ref(&self, ty: SelectionType) -> &SelectionSlot {
        match ty {
            SelectionType::Clipboard => &self.clipboard,
            SelectionType::Primary => &self.primary,
            SelectionType::Dnd => &self.dnd,
        }
    }

    /// The source id currently owning a channel.
    pub fn selection_source(&self, ty: SelectionType) -> Option<u32> {
        self.slot_ref(ty).source_id
    }

    /// wl_data_device.set_selection — install a clipboard source. Requests
    /// with a stale serial are ignored (returns `false`).
    pub fn set_selection(&mut self, source_id: Option<u32>, serial: u32) -> bool {
        let slot = self.slot(SelectionType::Clipboard);
        if serial < slot.serial {
            return false;
        }
        slot.source_id = source_id;
        slot.serial = serial;
        // STUB: emit the owner-changed signal and send new offers to the
        // currently focused client over the wire.
        true
    }

    /// Install/replace a channel source directly (used for DnD/primary).
    pub fn set_selection_source(&mut self, ty: SelectionType, source_id: Option<u32>, serial: u32) {
        let slot = self.slot(ty);
        slot.source_id = source_id;
        slot.serial = serial;
    }

    /// Clear a channel's source (`unset_selection_source`).
    pub fn unset_selection_source(&mut self, ty: SelectionType) {
        self.slot(ty).source_id = None;
    }

    /// Set the DnD data source used by the active grab.
    pub fn set_dnd_source(&mut self, source_id: Option<u32>) {
        self.dnd.source_id = source_id;
    }

    /// wl_data_device.start_drag — begin a drag grab. Returns `false` if a
    /// drag is already in progress on this seat.
    pub fn start_drag(
        &mut self,
        client_id: u32,
        origin_surface_id: u32,
        data_source_id: Option<u32>,
        icon_surface_id: Option<u32>,
        serial: u32,
    ) -> bool {
        if self.drag_grab.is_some() {
            return false;
        }
        let mut grab = DragGrab::new(client_id, origin_surface_id, data_source_id);
        grab.icon_surface_id = icon_surface_id;
        self.drag_grab = Some(grab);
        self.set_selection_source(SelectionType::Dnd, data_source_id, serial);
        true
    }

    /// The negotiated DnD action, resolved through the source lookup callback.
    pub fn drag_action(&self, source_action: impl Fn(u32) -> u32) -> u32 {
        match &self.drag_grab {
            Some(g) => g
                .data_source_id
                .map(source_action)
                .unwrap_or(DND_ACTION_NONE),
            None => DND_ACTION_NONE,
        }
    }

    /// Handle the drop: transition to `Dropped` awaiting the receiver's finish.
    pub fn drop(&mut self) -> bool {
        if let Some(g) = &mut self.drag_grab {
            if g.state == DragState::Dragging {
                g.state = DragState::Dropped;
                return true;
            }
        }
        false
    }

    /// End the drag grab and clear the DnD selection channel.
    pub fn end_drag(&mut self) {
        if let Some(g) = &mut self.drag_grab {
            g.state = DragState::Finished;
        }
        self.drag_grab = None;
        self.unset_selection_source(SelectionType::Dnd);
    }
}

#[cfg(test)]
mod tests {
    use super::super::data_source::DND_ACTION_COPY;
    use super::*;

    #[test]
    fn test_set_selection_serial_gate() {
        let mut d = MetaWaylandDataDevice::new(1);
        assert!(d.set_selection(Some(10), 5));
        assert_eq!(d.selection_source(SelectionType::Clipboard), Some(10));
        // Stale serial ignored.
        assert!(!d.set_selection(Some(20), 3));
        assert_eq!(d.selection_source(SelectionType::Clipboard), Some(10));
    }

    #[test]
    fn test_start_and_end_drag() {
        let mut d = MetaWaylandDataDevice::new(1);
        assert!(d.start_drag(7, 100, Some(30), Some(101), 8));
        assert!(d.drag_grab.is_some());
        assert_eq!(d.selection_source(SelectionType::Dnd), Some(30));
        // Second drag rejected while one is active.
        assert!(!d.start_drag(7, 100, None, None, 9));
        d.end_drag();
        assert!(d.drag_grab.is_none());
        assert_eq!(d.selection_source(SelectionType::Dnd), None);
    }

    #[test]
    fn test_drop_state_machine() {
        let mut d = MetaWaylandDataDevice::new(1);
        d.start_drag(7, 100, Some(30), None, 8);
        assert!(d.drop());
        assert_eq!(d.drag_grab.as_ref().unwrap().state, DragState::Dropped);
        // Cannot drop twice.
        assert!(!d.drop());
    }

    #[test]
    fn test_drag_focus_clears_offer() {
        let mut grab = DragGrab::new(1, 100, None);
        grab.focus_offer_id = Some(55);
        grab.set_focus(Some(200));
        assert_eq!(grab.focus_offer_id, None);
    }

    #[test]
    fn test_drag_action_lookup() {
        let mut d = MetaWaylandDataDevice::new(1);
        d.start_drag(7, 100, Some(30), None, 8);
        let action = d.drag_action(|src| {
            assert_eq!(src, 30);
            DND_ACTION_COPY
        });
        assert_eq!(action, DND_ACTION_COPY);
    }
}
