//! Wayland XDG Session protocol implementation.
//!
//! Ported from: meta-wayland-xdg-session.c/h
//!
//! Implements the xdg_session_v1 protocol for session management,
//! allowing clients to coordinate session state with the compositor.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-xdg-session.h

use alloc::string::String;
use alloc::string::ToString;

/// Session state enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum XdgSessionState {
    INACTIVE = 0,
    ACTIVE = 1,
    SUSPENDED = 2,
}

/// Session capability flags.
pub const XDG_SESSION_CAPABILITY_LOCK_SCREEN: u32 = 1 << 0;
pub const XDG_SESSION_CAPABILITY_IDLE: u32 = 1 << 1;
pub const XDG_SESSION_CAPABILITY_SCREENSAVER: u32 = 1 << 2;

/// Represents a single client XDG session.
///
/// Tracks session state, capabilities, and lifecycle events. A full
/// implementation would emit xdg_session_v1 state events to the client
/// when the session state or capabilities change.
#[derive(Debug)]
pub struct MetaWaylandXdgSession {
    pub session_state: Option<*mut core::ffi::c_void>, // MetaWaylandXdgSessionState pointer
    pub wl_client: Option<*mut core::ffi::c_void>,     // wl_client pointer
    pub id: Option<String>,
    pub state: XdgSessionState,
    pub capabilities: u32,
    /// Whether the session has been committed by the client.
    pub committed: bool,
}

impl MetaWaylandXdgSession {
    pub fn new(
        session_state: *mut core::ffi::c_void,
        wl_client: *mut core::ffi::c_void,
        _version: u32,
        id: u32,
    ) -> Self {
        MetaWaylandXdgSession {
            session_state: if session_state.is_null() {
                None
            } else {
                Some(session_state)
            },
            wl_client: if wl_client.is_null() {
                None
            } else {
                Some(wl_client)
            },
            id: Some(id.to_string()),
            state: XdgSessionState::INACTIVE,
            capabilities: 0,
            committed: false,
        }
    }

    /// Get the session ID string.
    pub fn get_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    /// Set the session ID string.
    pub fn set_id(&mut self, id: String) {
        self.id = Some(id);
    }

    /// Check whether a session ID has been assigned.
    pub fn has_id(&self) -> bool {
        self.id.is_some()
    }

    /// Get the wl_client pointer, if any.
    pub fn get_wl_client(&self) -> Option<*mut core::ffi::c_void> {
        self.wl_client
    }

    /// Get the session state pointer, if any.
    pub fn get_session_state_ptr(&self) -> Option<*mut core::ffi::c_void> {
        self.session_state
    }

    /// Get the current session state.
    pub fn get_state(&self) -> XdgSessionState {
        self.state
    }

    /// Set the session state. A full implementation would emit the
    /// xdg_session_v1.state event to the client.
    pub fn set_state(&mut self, state: XdgSessionState) {
        self.state = state;
    }

    /// Mark the session as committed by the client.
    pub fn commit(&mut self) {
        self.committed = true;
    }

    /// Check whether the session has been committed.
    pub fn is_committed(&self) -> bool {
        self.committed
    }

    /// Add a capability flag to the session.
    pub fn add_capability(&mut self, capability: u32) {
        self.capabilities |= capability;
    }

    /// Remove a capability flag from the session.
    pub fn remove_capability(&mut self, capability: u32) {
        self.capabilities &= !capability;
    }

    /// Check whether the session has a specific capability.
    pub fn has_capability(&self, capability: u32) -> bool {
        (self.capabilities & capability) != 0
    }
}

impl Default for MetaWaylandXdgSession {
    fn default() -> Self {
        MetaWaylandXdgSession {
            session_state: None,
            wl_client: None,
            id: None,
            state: XdgSessionState::INACTIVE,
            capabilities: 0,
            committed: false,
        }
    }
}
