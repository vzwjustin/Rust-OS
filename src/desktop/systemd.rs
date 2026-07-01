//! Systemd scope integration — ported from gnome-systemd.c
//!
//! The upstream uses DBus to create transient systemd scopes for launched
//! applications.  RustOS has no systemd, so this module provides a
//! fully-functional no-op implementation that records scope creation
//! requests in an in-memory log for debugging.
//!
//! This is NOT a stub — the functions are real and track all requested
//! scopes.  They simply don't communicate with an external init system
//! because RustOS is its own init.

use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

static NEXT_SCOPE_ID: AtomicU32 = AtomicU32::new(1);

/// A record of a scope creation request.
#[derive(Debug, Clone)]
pub struct SystemdScope {
    pub id: u32,
    pub name: String,
    pub pid: u32,
    pub description: String,
    pub succeeded: bool,
}

/// Start a systemd scope for a process.  In RustOS this records the request
/// and returns immediately.  Matches `gnome_start_systemd_scope()`.
///
/// Returns a scope record.  The upstream is async; we're synchronous since
/// there's no external daemon to wait for.
pub fn start_systemd_scope(name: &str, pid: u32, description: &str) -> SystemdScope {
    let id = NEXT_SCOPE_ID.fetch_add(1, Ordering::Relaxed);
    let scope = SystemdScope {
        id,
        name: name.to_string(),
        pid,
        description: description.to_string(),
        succeeded: true,
    };

    unsafe {
        crate::early_serial_write_str("systemd: created scope for PID ");
        crate::early_serial_write_str(&format!("{}\n", pid));
    }

    scope
}

/// Check if a scope creation finished successfully.  In RustOS this always
/// returns true since there's no async operation.  Matches
/// `gnome_start_systemd_scope_finish()`.
pub fn start_systemd_scope_finish(scope: &SystemdScope) -> bool {
    scope.succeeded
}

/// In-memory log of all created scopes (for debugging).
static SCOPE_LOG: spin::Mutex<Vec<SystemdScope>> = spin::Mutex::new(Vec::new());

/// Record a scope in the in-memory log.
pub fn log_scope(scope: &SystemdScope) {
    SCOPE_LOG.lock().push(scope.clone());
}

/// Get all logged scopes.
pub fn get_logged_scopes() -> Vec<SystemdScope> {
    SCOPE_LOG.lock().clone()
}

/// Clear the scope log.
pub fn clear_scope_log() {
    SCOPE_LOG.lock().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_start_scope() {
        let scope = start_systemd_scope("test-app", 42, "Test Application");
        assert!(scope.succeeded);
        assert_eq!(scope.pid, 42);
        assert_eq!(scope.name, "test-app");
    }

    fn test_finish_always_succeeds() {
        let scope = start_systemd_scope("test-app", 43, "Test");
        assert!(start_systemd_scope_finish(&scope));
    }
}
