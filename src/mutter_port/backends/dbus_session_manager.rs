//! Dbus Session Manager ported from GNOME Mutter
//!
//! MetaDbusSessionManager is a GObject base class for managing D-Bus sessions.
//! It creates and manages screen cast sessions and coordinates with session watchers.
//! Tracks backend, service endpoints, D-Bus registration, and inhibit count for lifecycle.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-dbus-session-manager.h

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// A single D-Bus session managed by the session manager.
///
/// Corresponds to `MetaDbusSession` in `meta-dbus-session.h`. Each session
/// tracks its D-Bus object path, the peer bus name that owns it, an
/// application-provided session ID, and whether the session is currently
/// active. The session manager uses the `id` as the key into its sessions
/// map.
pub struct MetaDbusSession {
    /// Unique session identifier assigned by the manager at registration.
    pub id: usize,
    /// D-Bus object path exported for this session (e.g.
    /// `/org/gnome/Mutter/ScreenCast/Session0`).
    pub object_path: String,
    /// Bus name of the peer that created the session.
    pub peer_bus_name: String,
    /// Whether the session is currently active (started and not closed).
    pub is_active: bool,
}

impl MetaDbusSession {
    /// Create a new session record with the given id, object path, and peer.
    pub fn new(id: usize, object_path: String, peer_bus_name: String) -> Self {
        MetaDbusSession {
            id,
            object_path,
            peer_bus_name,
            is_active: false,
        }
    }
}

pub struct GDBusMethodInvocation {
    // Opaque GIO type
}

pub struct GDBusInterfaceSkeleton {
    // Opaque GIO type
}

pub struct MetaBackend {
    // Opaque backend type
}

pub struct GDBusConnection {
    // Opaque GIO type
}

/// MetaDbusSessionManager: Base class for managing D-Bus sessions.
/// Manages D-Bus service registration, tracks active sessions, and coordinates
/// lifecycle with inhibit count for safe session teardown.
pub struct MetaDbusSessionManager {
    pub backend: *mut MetaBackend,
    pub service_name: String,
    pub service_path: String,
    pub session_gtype: usize,
    pub dbus_name_id: u32,
    pub dbus_connection: *mut GDBusConnection,
    pub interface_skeleton: *mut GDBusInterfaceSkeleton,
    pub is_enabled: bool,
    pub inhibit_count: i32,
    /// Active sessions keyed by session id. Each entry is a boxed
    /// `MetaDbusSession` owned by the manager.
    pub sessions: BTreeMap<usize, Box<MetaDbusSession>>,
    /// Monotonically increasing counter used to assign session ids.
    session_count: usize,
}

impl MetaDbusSessionManager {
    pub fn new() -> Self {
        MetaDbusSessionManager {
            backend: core::ptr::null_mut(),
            service_name: String::new(),
            service_path: String::new(),
            session_gtype: 0,
            dbus_name_id: 0,
            dbus_connection: core::ptr::null_mut(),
            interface_skeleton: core::ptr::null_mut(),
            is_enabled: false,
            inhibit_count: 0,
            sessions: BTreeMap::new(),
            session_count: 0,
        }
    }

    /// Returns the number of currently registered sessions.
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    /// Register a new session. Assigns a unique id, stores the session, and
    /// returns the assigned id. A full implementation would also export the
    /// session's D-Bus interface skeleton on `dbus_connection` at
    /// `object_path`.
    pub fn register_session(&mut self, object_path: String, peer_bus_name: String) -> usize {
        let id = self.session_count;
        self.session_count += 1;
        let session = Box::new(MetaDbusSession::new(id, object_path, peer_bus_name));
        self.sessions.insert(id, session);
        id
    }

    /// Unregister (close) the session with the given id. Removes it from the
    /// sessions map and returns `true` if it existed. A full implementation
    /// would also unexport the D-Bus interface skeleton and emit the
    /// `session-closed` signal.
    pub fn unregister_session(&mut self, id: usize) -> bool {
        self.sessions.remove(&id).is_some()
    }

    /// Look up a session by id.
    pub fn lookup_session(&self, id: usize) -> Option<&MetaDbusSession> {
        self.sessions.get(&id).map(|b| b.as_ref())
    }

    /// Look up a session by id for mutable access.
    pub fn lookup_session_mut(&mut self, id: usize) -> Option<&mut MetaDbusSession> {
        self.sessions.get_mut(&id).map(|b| b.as_mut())
    }

    /// Increment the inhibit count, preventing session teardown. Returns the
    /// new count.
    pub fn inhibit(&mut self) -> i32 {
        self.inhibit_count += 1;
        self.inhibit_count
    }

    /// Decrement the inhibit count (clamped at zero). When the count reaches
    /// zero, pending session teardowns may proceed. Returns the new count.
    pub fn uninhibit(&mut self) -> i32 {
        if self.inhibit_count > 0 {
            self.inhibit_count -= 1;
        }
        self.inhibit_count
    }
}

impl Default for MetaDbusSessionManager {
    fn default() -> Self {
        Self::new()
    }
}
