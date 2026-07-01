//! Session manager ported from GNOME Mutter's src/core/meta-session-manager.c
//!
//! Manages window manager sessions, persistence, and state recovery.
//! Uses key-value database for session state storage.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-session-manager.c

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

/// Maximum session file size (10MB)
const MAX_SESSION_SIZE: usize = 10 * 1024 * 1024;

/// Session file name in session directory
const SESSION_FILE_NAME: &str = "session.gvdb";

/// Session state stored for a single managed window/component
#[derive(Debug, Clone)]
pub struct SessionData {
    pub name: String,
    pub properties: BTreeMap<String, String>,
}

impl SessionData {
    pub fn new(name: String) -> Self {
        SessionData {
            name,
            properties: BTreeMap::new(),
        }
    }

    pub fn set_property(&mut self, key: String, value: String) {
        self.properties.insert(key, value);
    }

    pub fn get_property(&self, key: &str) -> Option<&String> {
        self.properties.get(key)
    }
}

/// Main session manager for WM state persistence and recovery
#[derive(Debug)]
pub struct SessionManager {
    pub id: u32,
    /// Sessions indexed by name
    sessions: BTreeMap<String, SessionData>,
    /// Deleted session names (for cleanup)
    deleted_sessions: BTreeMap<String, bool>,
    /// Name/identifier of this session
    name: String,
    /// File descriptor for session file (if open)
    fd: Option<i32>,
    /// Size of session data
    data_size: AtomicU32,
}

impl SessionManager {
    /// Create new session manager
    pub fn new(name: String) -> Self {
        SessionManager {
            id: 0,
            sessions: BTreeMap::new(),
            deleted_sessions: BTreeMap::new(),
            name,
            fd: None,
            data_size: AtomicU32::new(0),
        }
    }

    /// Get session by name
    pub fn get_session(&self, name: &str) -> Option<&SessionData> {
        self.sessions.get(name)
    }

    /// Get or create session
    pub fn get_or_create_session(&mut self, name: String) -> &mut SessionData {
        if !self.sessions.contains_key(&name) {
            self.sessions
                .insert(name.clone(), SessionData::new(name.clone()));
        }
        self.sessions.get_mut(&name).unwrap()
    }

    /// Remove session
    pub fn remove_session(&mut self, name: &str) -> bool {
        if self.sessions.remove(name).is_some() {
            self.deleted_sessions.insert(name.to_string(), true);
            return true;
        }
        false
    }

    /// List all session names
    pub fn list_sessions(&self) -> Vec<&String> {
        self.sessions.keys().collect()
    }

    /// Save session state to file
    /// Stub: requires GVDB (key-value database) bindings
    pub fn save(&self) -> bool {
        let _size = self.data_size.load(Ordering::Relaxed);
        // Would serialize sessions to GVDB format and write to fd
        // Stubbed for no_std kernel
        true
    }

    /// Load session state from file
    /// Stub: requires GVDB parsing
    pub fn load(&mut self) -> bool {
        // Would parse GVDB file and restore sessions
        // Stubbed for no_std kernel
        true
    }

    /// Clear all sessions
    pub fn clear(&mut self) {
        self.sessions.clear();
        self.deleted_sessions.clear();
        self.data_size.store(0, Ordering::Relaxed);
    }

    /// Get current session name
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Set file descriptor for session persistence
    pub fn set_fd(&mut self, fd: i32) {
        self.fd = Some(fd);
    }

    /// Close session file descriptor
    pub fn close_fd(&mut self) {
        self.fd = None;
    }

    /// Get total data size
    pub fn get_data_size(&self) -> u32 {
        self.data_size.load(Ordering::Acquire)
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new("default".to_string())
    }
}
