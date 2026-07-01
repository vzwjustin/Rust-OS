//! GNOME src/wayland/meta-wayland-data-source.c
//!
//! MetaWaylandDataSource models a client-offered data source (clipboard or
//! drag-and-drop): the set of MIME types it advertises, the negotiated DnD
//! actions, and the currently selected action. Loose coupling is preserved by
//! referencing other objects by id (`u32`) rather than by reference.
//!
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-data-source.c

use alloc::string::String;
use alloc::vec::Vec;

/// wl_data_device_manager.dnd_action bitmask values.
pub const DND_ACTION_NONE: u32 = 0;
pub const DND_ACTION_COPY: u32 = 1;
pub const DND_ACTION_MOVE: u32 = 2;
pub const DND_ACTION_ASK: u32 = 4;

/// Union of all valid DnD actions.
pub const ALL_ACTIONS: u32 = DND_ACTION_COPY | DND_ACTION_MOVE | DND_ACTION_ASK;

/// MetaWaylandDataSource — a client's offered data (`wl_data_source`).
#[derive(Debug, Clone)]
pub struct MetaWaylandDataSource {
    /// Protocol object id.
    pub id: u32,
    /// Owning client id.
    pub client_id: u32,
    /// Advertised MIME types, in advertisement order.
    pub mime_types: Vec<String>,
    /// True once a matching offer has a target (accept received).
    pub has_target: bool,
    /// Actions advertised by the source (`set_actions`).
    pub dnd_actions: u32,
    /// Action preferred by the user / compositor.
    pub user_dnd_action: u32,
    /// Negotiated current action, or `None` if not yet computed.
    pub current_dnd_action: Option<u32>,
    /// Whether `set_actions` has been called (may only happen once).
    pub actions_set: bool,
    /// True while acting as a drag-and-drop source (vs. plain clipboard).
    pub actions_changed: bool,
    /// True once the source has been used/cancelled and must not be reused.
    pub in_ask: bool,
}

impl MetaWaylandDataSource {
    pub fn new(id: u32, client_id: u32) -> Self {
        MetaWaylandDataSource {
            id,
            client_id,
            mime_types: Vec::new(),
            has_target: false,
            dnd_actions: 0,
            user_dnd_action: 0,
            current_dnd_action: None,
            actions_set: false,
            actions_changed: false,
            in_ask: false,
        }
    }

    /// wl_data_source.offer — advertise a MIME type (deduplicated).
    pub fn add_mime_type(&mut self, mime_type: &str) {
        if !self.mime_types.iter().any(|m| m == mime_type) {
            self.mime_types.push(String::from(mime_type));
        }
    }

    pub fn has_mime_type(&self, mime_type: &str) -> bool {
        self.mime_types.iter().any(|m| m == mime_type)
    }

    /// wl_data_source.set_actions — may only be called once; the mask must be
    /// a subset of `ALL_ACTIONS`. Returns `false` on a protocol violation.
    pub fn set_actions(&mut self, dnd_actions: u32) -> bool {
        if self.actions_set {
            return false;
        }
        if dnd_actions & !ALL_ACTIONS != 0 {
            return false;
        }
        self.dnd_actions = dnd_actions;
        self.actions_set = true;
        self.actions_changed = true;
        true
    }

    /// Set the target's accepted state (from an offer's `accept`).
    pub fn set_has_target(&mut self, has_target: bool) {
        self.has_target = has_target;
    }

    /// Set the user-preferred action (from an offer's `set_actions`).
    pub fn set_user_action(&mut self, action: u32) {
        if self.user_dnd_action != action {
            self.user_dnd_action = action;
            self.actions_changed = true;
        }
    }

    /// Compute the negotiated action from source + user preference.
    ///
    /// Mirrors mutter's negotiation: if both sides intersect on an action,
    /// prefer the user's action when contained, else the highest common bit;
    /// ASK is only chosen when explicitly requested by both.
    pub fn compute_current_action(&mut self) -> u32 {
        let available = self.dnd_actions & self.user_dnd_action;
        let action = if available == 0 {
            DND_ACTION_NONE
        } else if available & self.user_dnd_action & DND_ACTION_ASK != 0 {
            DND_ACTION_ASK
        } else if available & DND_ACTION_COPY != 0 {
            DND_ACTION_COPY
        } else if available & DND_ACTION_MOVE != 0 {
            DND_ACTION_MOVE
        } else {
            DND_ACTION_NONE
        };
        self.current_dnd_action = Some(action);
        self.actions_changed = false;
        action
    }

    /// wl_data_source current action accessor.
    pub fn current_action(&self) -> u32 {
        self.current_dnd_action.unwrap_or(DND_ACTION_NONE)
    }

    /// Whether the negotiated action requires the "ask" user interaction.
    pub fn requires_ask(&self) -> bool {
        self.current_dnd_action == Some(DND_ACTION_ASK)
    }

    // STUB: wl_data_source.send — the compositor must write `mime_type` data
    // into the client-provided fd; wire this to the DRM/IPC transfer layer.
    pub fn send(&self, _mime_type: &str, _fd: i32) {}

    // STUB: wl_data_source.cancelled / .dnd_finished protocol events must be
    // dispatched over the Wayland wire once the transfer completes.
    pub fn notify_finish(&mut self) {
        self.in_ask = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mime_dedup() {
        let mut s = MetaWaylandDataSource::new(1, 10);
        s.add_mime_type("text/plain");
        s.add_mime_type("text/plain");
        s.add_mime_type("image/png");
        assert_eq!(s.mime_types.len(), 2);
        assert!(s.has_mime_type("image/png"));
    }

    #[test]
    fn test_set_actions_once() {
        let mut s = MetaWaylandDataSource::new(1, 10);
        assert!(s.set_actions(DND_ACTION_COPY | DND_ACTION_MOVE));
        // Second call is a protocol error.
        assert!(!s.set_actions(DND_ACTION_COPY));
    }

    #[test]
    fn test_set_actions_invalid_mask() {
        let mut s = MetaWaylandDataSource::new(1, 10);
        assert!(!s.set_actions(0x100));
    }

    #[test]
    fn test_compute_action_prefers_copy() {
        let mut s = MetaWaylandDataSource::new(1, 10);
        s.set_actions(DND_ACTION_COPY | DND_ACTION_MOVE);
        s.set_user_action(DND_ACTION_COPY | DND_ACTION_MOVE);
        assert_eq!(s.compute_current_action(), DND_ACTION_COPY);
    }

    #[test]
    fn test_compute_action_ask() {
        let mut s = MetaWaylandDataSource::new(1, 10);
        s.set_actions(ALL_ACTIONS);
        s.set_user_action(DND_ACTION_ASK);
        assert_eq!(s.compute_current_action(), DND_ACTION_ASK);
        assert!(s.requires_ask());
    }

    #[test]
    fn test_compute_action_none() {
        let mut s = MetaWaylandDataSource::new(1, 10);
        s.set_actions(DND_ACTION_COPY);
        s.set_user_action(DND_ACTION_MOVE);
        assert_eq!(s.compute_current_action(), DND_ACTION_NONE);
    }
}
