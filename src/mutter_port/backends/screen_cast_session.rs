//! Screen Cast Session — ported from GNOME Mutter
//!
//! Represents a single screen casting session, managing streams and remote access state.
//! Sessions can be normal screen capture or remote desktop sessions.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-screen-cast-session.h

use alloc::string::String;
use alloc::vec::Vec;

/// Type of screen cast session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaScreenCastSessionType {
    /// Normal screen capture session.
    META_SCREEN_CAST_SESSION_TYPE_NORMAL,
    /// Session linked to remote desktop control.
    META_SCREEN_CAST_SESSION_TYPE_REMOTE_DESKTOP,
}

/// Handle to a screen cast session for remote access tracking.
pub struct MetaScreenCastSessionHandle {
    // TODO: Remote access handle binding from C implementation
}

impl MetaScreenCastSessionHandle {
    pub fn new() -> Self {
        MetaScreenCastSessionHandle {}
    }
}

impl Default for MetaScreenCastSessionHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// A screen cast session managing one or more capture streams.
pub struct MetaScreenCastSession {
    session_type: MetaScreenCastSessionType,
    // TODO: Session state, streams, peer info from C implementation
}

impl MetaScreenCastSession {
    pub fn new(session_type: MetaScreenCastSessionType) -> Self {
        MetaScreenCastSession { session_type }
    }

    pub fn get_session_type(&self) -> MetaScreenCastSessionType {
        self.session_type
    }
}

impl Default for MetaScreenCastSession {
    fn default() -> Self {
        Self::new(MetaScreenCastSessionType::META_SCREEN_CAST_SESSION_TYPE_NORMAL)
    }
}