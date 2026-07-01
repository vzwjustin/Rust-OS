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
/// Tracks session state, capabilities, and lifecycle events. Protocol I/O is TODO.
#[derive(Debug)]
pub struct MetaWaylandXdgSession {
    pub session_state: Option<*mut core::ffi::c_void>, // MetaWaylandXdgSessionState pointer
    pub wl_client: Option<*mut core::ffi::c_void>,     // wl_client pointer
    pub id: Option<String>,
    pub state: XdgSessionState,
    pub capabilities: u32,
}

impl MetaWaylandXdgSession {
    pub fn new(
        session_state: *mut core::ffi::c_void,
        wl_client: *mut core::ffi::c_void,
        _version: u32,
        id: u32,
    ) -> Self {
        MetaWaylandXdgSession {
            session_state: Some(session_state),
            wl_client: Some(wl_client),
            id: Some(id.to_string()),
            state: XdgSessionState::INACTIVE,
            capabilities: 0,
        }
    }

    pub fn get_id(&self) -> Option<&str> {
        self.id.as_deref()
    }

    pub fn get_state(&self) -> XdgSessionState {
        self.state
    }

    pub fn set_state(&mut self, state: XdgSessionState) {
        self.state = state;
    }

    pub fn add_capability(&mut self, capability: u32) {
        self.capabilities |= capability;
    }

    pub fn remove_capability(&mut self, capability: u32) {
        self.capabilities &= !capability;
    }

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
        }
    }
}
