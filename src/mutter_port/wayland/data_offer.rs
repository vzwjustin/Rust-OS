//! GNOME src/wayland/meta-wayland-data-offer.c
//!
//! MetaWaylandDataOffer represents a `wl_data_offer` handed to a client that
//! is the potential recipient of a selection or drag-and-drop. It tracks which
//! MIME type the client accepted, the preferred/negotiated action, and the
//! finish/receive state machine. The backing source is referenced by id.
//!
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-data-offer.c

use alloc::string::String;

use super::data_source::{DND_ACTION_ASK, DND_ACTION_NONE};

/// MetaWaylandDataOffer — a `wl_data_offer` presented to a receiving client.
#[derive(Debug, Clone)]
pub struct MetaWaylandDataOffer {
    /// Protocol object id.
    pub id: u32,
    /// Backing data source id (`MetaWaylandDataSource`), if still alive.
    pub source_id: Option<u32>,
    /// MIME type the receiver accepted, or `None` (accept with null clears it).
    pub accepted_mime_type: Option<String>,
    /// Whether the receiver has accepted a MIME type.
    pub accepted: bool,
    /// Whether an action event has been sent to the receiver.
    pub action_sent: bool,
    /// Receiver's preferred action (from `set_actions`).
    pub preferred_action: u32,
    /// Receiver's advertised action mask (from `set_actions`).
    pub dnd_actions: u32,
    /// True once `finish` has been processed (drop completed).
    pub finished: bool,
}

impl MetaWaylandDataOffer {
    pub fn new(id: u32, source_id: Option<u32>) -> Self {
        MetaWaylandDataOffer {
            id,
            source_id,
            accepted_mime_type: None,
            accepted: false,
            action_sent: false,
            preferred_action: DND_ACTION_NONE,
            dnd_actions: DND_ACTION_NONE,
            finished: false,
        }
    }

    /// wl_data_offer.accept — the receiver accepts (or clears, with `None`) a
    /// MIME type. The caller should propagate `has_target` to the source.
    pub fn accept(&mut self, mime_type: Option<&str>) {
        self.accepted = mime_type.is_some();
        self.accepted_mime_type = mime_type.map(String::from);
    }

    /// wl_data_offer.receive — requests transfer of `mime_type` to `fd`.
    /// Returns `false` if the MIME type is unknown to `available_mime_types`.
    // STUB: the actual byte transfer is performed by the selection/DRM layer;
    // here we only validate and record intent.
    pub fn receive(&self, mime_type: &str, _fd: i32, available_mime_types: &[String]) -> bool {
        available_mime_types.iter().any(|m| m == mime_type)
    }

    /// wl_data_offer.set_actions — record the receiver's action preferences.
    pub fn set_actions(&mut self, dnd_actions: u32, preferred_action: u32) {
        self.dnd_actions = dnd_actions;
        self.preferred_action = preferred_action;
        self.action_sent = true;
    }

    /// wl_data_offer.finish — validate the drop can be completed. Returns
    /// `false` (protocol error) if premature, or if the negotiated action is
    /// missing/ASK. `current_action` is the source's negotiated action.
    pub fn finish(&mut self, current_action: u32) -> bool {
        if !self.accepted || !self.action_sent {
            return false;
        }
        if current_action == DND_ACTION_NONE || current_action == DND_ACTION_ASK {
            return false;
        }
        self.finished = true;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::super::data_source::{DND_ACTION_COPY, DND_ACTION_MOVE};
    use super::*;
    use alloc::string::String;
    use alloc::vec;

    #[test]
    fn test_accept_and_clear() {
        let mut o = MetaWaylandDataOffer::new(1, Some(5));
        o.accept(Some("text/plain"));
        assert!(o.accepted);
        o.accept(None);
        assert!(!o.accepted);
        assert!(o.accepted_mime_type.is_none());
    }

    #[test]
    fn test_receive_validates_mime() {
        let o = MetaWaylandDataOffer::new(1, Some(5));
        let avail = vec![String::from("text/plain")];
        assert!(o.receive("text/plain", 3, &avail));
        assert!(!o.receive("image/png", 3, &avail));
    }

    #[test]
    fn test_finish_premature_fails() {
        let mut o = MetaWaylandDataOffer::new(1, Some(5));
        // Not accepted, no action sent.
        assert!(!o.finish(DND_ACTION_COPY));
    }

    #[test]
    fn test_finish_rejects_ask_and_none() {
        let mut o = MetaWaylandDataOffer::new(1, Some(5));
        o.accept(Some("text/plain"));
        o.set_actions(DND_ACTION_COPY, DND_ACTION_COPY);
        assert!(!o.finish(DND_ACTION_NONE));
        assert!(!o.finish(DND_ACTION_ASK));
    }

    #[test]
    fn test_finish_success() {
        let mut o = MetaWaylandDataOffer::new(1, Some(5));
        o.accept(Some("text/plain"));
        o.set_actions(DND_ACTION_MOVE, DND_ACTION_MOVE);
        assert!(o.finish(DND_ACTION_MOVE));
        assert!(o.finished);
    }
}
