//! Launcher — ported from GNOME Mutter
//!
//! Session launcher for multi-seat and VT management via D-Bus login1.
//! Manages seat activation, VT switching, and session control.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-launcher.h

use alloc::string::String;

/// Opaque backend type.
pub struct MetaBackend;

/// D-Bus login1 session proxy.
pub struct MetaDBusLogin1Session;

/// D-Bus login1 seat proxy.
pub struct MetaDBusLogin1Seat;

/// Session launcher managing D-Bus login1 integration.
/// Holds references to backend, session and seat proxies, and session state flags.
pub struct MetaLauncher {
    pub backend: *mut MetaBackend,
    pub session_proxy: *mut MetaDBusLogin1Session,
    pub seat_proxy: *mut MetaDBusLogin1Seat,
    pub session_active: bool,
    pub have_control: bool,
    /// The seat ID (e.g., "seat0").
    pub seat_id: String,
    /// Current virtual terminal number.
    pub current_vt: u32,
}

impl MetaLauncher {
    /// Create new launcher (error if login1 unavailable).
    pub fn new(backend: *mut MetaBackend) -> Result<Self, String> {
        Ok(MetaLauncher {
            backend,
            session_proxy: core::ptr::null_mut(),
            seat_proxy: core::ptr::null_mut(),
            session_active: true,
            have_control: false,
            seat_id: String::from("seat0"),
            current_vt: 0,
        })
    }

    /// Activate a virtual terminal. A full implementation would call
    /// the login1 D-Bus ActivateSession method. Records the VT number.
    pub fn activate_vt(&mut self, vt: u32) -> Result<(), String> {
        if vt == 0 {
            return Err("invalid VT number".into());
        }
        self.current_vt = vt;
        Ok(())
    }

    /// Check if this session is currently active.
    pub fn is_session_active(&self) -> bool {
        self.session_active
    }

    /// Take device control from session manager. A full implementation
    /// would call the login1 D-Bus TakeControl method. Records the
    /// control state.
    pub fn take_control(&mut self) -> Result<(), String> {
        if self.have_control {
            return Err("already have control".into());
        }
        self.have_control = true;
        Ok(())
    }

    /// Release device control.
    pub fn release_control(&mut self) -> Result<(), String> {
        if !self.have_control {
            return Err("do not have control".into());
        }
        self.have_control = false;
        Ok(())
    }

    /// Get the seat ID for this session.
    pub fn get_seat_id(&self) -> &str {
        &self.seat_id
    }

    /// Get the current VT number.
    pub fn get_current_vt(&self) -> u32 {
        self.current_vt
    }

    /// Get D-Bus login1 session proxy.
    pub fn get_session_proxy(&self) -> *mut MetaDBusLogin1Session {
        self.session_proxy
    }

    /// Get backend.
    pub fn get_backend(&self) -> *mut MetaBackend {
        self.backend
    }
}
