//! Screen Cast Session — ported from GNOME Mutter
//!
//! Represents a single screen casting session, managing streams and remote access state.
//! Sessions can be normal screen capture or remote desktop sessions. Each session
//! tracks peer name, object path, streams list, and remote desktop linkage.
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

pub struct MetaDbusSessionManager {
    // Opaque session manager type
}

pub struct MetaScreenCastStream {
    // Opaque stream type
}

pub struct MetaRemoteDesktopSession {
    // Opaque remote desktop session type
}

/// Handle to a screen cast session for remote access tracking.
/// Links the session to the remote access controller system.
pub struct MetaScreenCastSessionHandle {
    pub session: *mut MetaScreenCastSession,
}

impl MetaScreenCastSessionHandle {
    pub fn new() -> Self {
        MetaScreenCastSessionHandle {
            session: core::ptr::null_mut(),
        }
    }
}

impl Default for MetaScreenCastSessionHandle {
    fn default() -> Self {
        Self::new()
    }
}

/// A screen cast session managing one or more capture streams.
/// Maintains session identity (peer name, object path, session ID), stream list,
/// animation disable flag, and optional remote desktop linkage.
pub struct MetaScreenCastSession {
    pub session_manager: *mut MetaDbusSessionManager,
    pub peer_name: String,
    pub session_type: MetaScreenCastSessionType,
    pub object_path: String,
    pub session_id: String,
    pub streams: *mut core::ffi::c_void, // GList<MetaScreenCastStream>
    pub handle: *mut MetaScreenCastSessionHandle,
    pub is_active: bool,
    pub disable_animations: bool,
    pub remote_desktop_session: *mut MetaRemoteDesktopSession,
}

impl MetaScreenCastSession {
    pub fn new(session_type: MetaScreenCastSessionType) -> Self {
        MetaScreenCastSession {
            session_manager: core::ptr::null_mut(),
            peer_name: String::new(),
            session_type,
            object_path: String::new(),
            session_id: String::new(),
            streams: core::ptr::null_mut(),
            handle: core::ptr::null_mut(),
            is_active: false,
            disable_animations: false,
            remote_desktop_session: core::ptr::null_mut(),
        }
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