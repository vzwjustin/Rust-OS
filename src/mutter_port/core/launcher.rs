//! MetaLauncher ported from GNOME Mutter's src/core/meta-launcher.c
//!
//! MetaLauncher handles session management with logind/ConsoleKit2: it
//! acquires the active session, takes control of DRM devices, and manages
//! VT switching. In Mutter this is a GObject that talks to systemd-logind
//! over D-Bus.
//!
//! In the kernel, there is no logind or D-Bus session manager. The launcher
//! is modeled as a plain struct that tracks session state, VT number, and
//! DRM device control. The kernel itself is the session controller, so
//! most D-Bus round-trips become no-ops.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-launcher.c

use alloc::string::String;
use alloc::vec::Vec;

/// Session type, mirroring the logind session type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionType {
    /// Graphical session (Wayland or X11).
    Graphical,
    /// Text console session.
    Tty,
    /// Unknown / unspecified.
    Other,
}

impl Default for SessionType {
    fn default() -> Self {
        SessionType::Graphical
    }
}

/// Session state, mirroring sd_session_state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionState {
    /// Session is active (foreground).
    Active,
    /// Session is online but not active (background).
    Online,
    /// Session is closing.
    Closing,
}

impl Default for SessionState {
    fn default() -> Self {
        SessionState::Active
    }
}

/// A DRM device managed by the launcher. Mirrors the logind
/// TakeDevice() / ReleaseDevice() lifecycle.
#[derive(Debug, Clone)]
pub struct DrmDevice {
    /// Device path (e.g. "/dev/dri/card0").
    pub path: String,
    /// Whether we have taken control of this device.
    pub acquired: bool,
    /// Whether the device is paused (VT switch away).
    pub paused: bool,
}

impl DrmDevice {
    pub fn new(path: &str) -> Self {
        DrmDevice {
            path: String::from(path),
            acquired: false,
            paused: false,
        }
    }
}

/// The launcher. Mirrors MetaLauncher.
///
/// Stubbed: all D-Bus calls to logind (TakeControl, TakeDevice, ReleaseDevice,
/// SetActive, PauseDevice) are no-ops. The state transitions are tracked
/// faithfully so the compositor can react to VT switches and device
/// pause/resume events.
#[derive(Debug)]
pub struct MetaLauncher {
    /// Whether the launcher has been initialized (logind proxy connected).
    initialized: bool,
    /// Whether we have taken control of the active session.
    session_control: bool,
    /// Current session state.
    session_state: SessionState,
    /// Session type.
    session_type: SessionType,
    /// Current VT number.
    vt: u32,
    /// Seat id (e.g. "seat0").
    seat: String,
    /// Session id (from logind).
    session_id: String,
    /// DRM devices under our control.
    devices: Vec<DrmDevice>,
    /// Whether we're in the process of pausing (VT switch away).
    pausing: bool,
}

impl MetaLauncher {
    /// Create a new launcher. Mirrors meta_launcher_new().
    ///
    /// In Mutter this connects to the logind D-Bus service. Here we just
    /// initialize the state; the kernel is always the session controller.
    pub fn new() -> Self {
        MetaLauncher {
            initialized: false,
            session_control: false,
            session_state: SessionState::Active,
            session_type: SessionType::Graphical,
            vt: 1,
            seat: String::from("seat0"),
            session_id: String::new(),
            devices: Vec::new(),
            pausing: false,
        }
    }

    // ── Lifecycle ─────────────────────────────────────────────────────

    /// Initialize the launcher: connect to logind and query the active
    /// session. Mirrors meta_launcher_connect().
    ///
    /// In the kernel, this is a no-op since we are the session controller.
    pub fn initialize(&mut self) -> Result<(), &'static str> {
        if self.initialized {
            return Err("Launcher already initialized");
        }
        self.initialized = true;
        // The kernel is always in control of the active session.
        self.session_control = true;
        self.session_state = SessionState::Active;
        Ok(())
    }

    /// Take control of the active session. Mirrors
    /// logind_call_take_control().
    pub fn take_control(&mut self) -> Result<(), &'static str> {
        if !self.initialized {
            return Err("Launcher not initialized");
        }
        if self.session_control {
            return Err("Already have session control");
        }
        self.session_control = true;
        Ok(())
    }

    /// Release control of the session. Mirrors logind_call_release_control().
    pub fn release_control(&mut self) {
        if self.session_control {
            // Release all devices first.
            for dev in &mut self.devices {
                dev.acquired = false;
            }
            self.session_control = false;
        }
    }

    // ── Device management ─────────────────────────────────────────────

    /// Take control of a DRM device. Mirrors logind_call_take_device().
    pub fn take_device(&mut self, path: &str) -> Result<(), &'static str> {
        if !self.session_control {
            return Err("No session control");
        }
        // Find or create the device entry.
        let dev = self.devices.iter_mut().find(|d| d.path == path);
        match dev {
            Some(d) => {
                if d.acquired {
                    return Err("Device already acquired");
                }
                d.acquired = true;
                d.paused = false;
            }
            None => {
                let mut d = DrmDevice::new(path);
                d.acquired = true;
                self.devices.push(d);
            }
        }
        Ok(())
    }

    /// Release a DRM device. Mirrors logind_call_release_device().
    pub fn release_device(&mut self, path: &str) -> Result<(), &'static str> {
        let dev = self.devices.iter_mut().find(|d| d.path == path);
        match dev {
            Some(d) => {
                d.acquired = false;
                Ok(())
            }
            None => Err("Device not found"),
        }
    }

    /// Get all managed DRM devices.
    pub fn devices(&self) -> &[DrmDevice] {
        &self.devices
    }

    /// Get acquired devices.
    pub fn acquired_devices(&self) -> impl Iterator<Item = &DrmDevice> {
        self.devices.iter().filter(|d| d.acquired)
    }

    // ── VT switching ──────────────────────────────────────────────────

    /// Get the current VT number.
    pub fn vt(&self) -> u32 {
        self.vt
    }

    /// Set the current VT number (called on VT switch).
    pub fn set_vt(&mut self, vt: u32) {
        self.vt = vt;
    }

    /// Called when the session is about to be paused (VT switch away).
    /// Mirrors the logind "Pause" signal handler.
    pub fn on_pause(&mut self) {
        self.pausing = true;
        self.session_state = SessionState::Online;
        // Pause all devices.
        for dev in &mut self.devices {
            if dev.acquired {
                dev.paused = true;
            }
        }
    }

    /// Called when the session is resumed (VT switch back).
    /// Mirrors the logind "Resume" signal handler.
    pub fn on_resume(&mut self) {
        self.pausing = false;
        self.session_state = SessionState::Active;
        // Resume all devices.
        for dev in &mut self.devices {
            dev.paused = false;
        }
    }

    /// Whether the session is pausing (VT switch in progress).
    pub fn is_pausing(&self) -> bool {
        self.pausing
    }

    // ── Accessors ─────────────────────────────────────────────────────

    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    pub fn has_session_control(&self) -> bool {
        self.session_control
    }

    pub fn session_state(&self) -> SessionState {
        self.session_state
    }

    pub fn session_type(&self) -> SessionType {
        self.session_type
    }

    pub fn set_session_type(&mut self, session_type: SessionType) {
        self.session_type = session_type;
    }

    pub fn seat(&self) -> &str {
        &self.seat
    }

    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn set_session_id(&mut self, id: &str) {
        self.session_id = String::from(id);
    }

    /// Whether the session is active (foreground).
    pub fn is_active(&self) -> bool {
        self.session_state == SessionState::Active
    }

    // ── Shutdown ──────────────────────────────────────────────────────

    /// Shut down the launcher: release all devices and session control.
    /// Mirrors meta_launcher_free().
    pub fn shutdown(&mut self) {
        self.release_control();
        self.devices.clear();
        self.initialized = false;
    }
}

impl Default for MetaLauncher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_state() {
        let launcher = MetaLauncher::new();
        assert!(!launcher.is_initialized());
        assert!(!launcher.has_session_control());
        assert_eq!(launcher.session_state(), SessionState::Active);
        assert_eq!(launcher.vt(), 1);
        assert_eq!(launcher.seat(), "seat0");
    }

    #[test]
    fn test_initialize() {
        let mut launcher = MetaLauncher::new();
        assert!(launcher.initialize().is_ok());
        assert!(launcher.is_initialized());
        assert!(launcher.has_session_control());

        // Double init fails.
        assert!(launcher.initialize().is_err());
    }

    #[test]
    fn test_take_control_without_init_fails() {
        let mut launcher = MetaLauncher::new();
        assert!(launcher.take_control().is_err());
    }

    #[test]
    fn test_device_management() {
        let mut launcher = MetaLauncher::new();
        launcher.initialize().unwrap();

        // Take device.
        assert!(launcher.take_device("/dev/dri/card0").is_ok());
        assert_eq!(launcher.devices().len(), 1);
        assert!(launcher.devices()[0].acquired);

        // Taking same device again fails.
        assert!(launcher.take_device("/dev/dri/card0").is_err());

        // Release device.
        assert!(launcher.release_device("/dev/dri/card0").is_ok());
        assert!(!launcher.devices()[0].acquired);

        // Releasing unknown device fails.
        assert!(launcher.release_device("/dev/dri/card99").is_err());
    }

    #[test]
    fn test_take_device_without_control_fails() {
        let mut launcher = MetaLauncher::new();
        // Not initialized, no session control.
        assert!(launcher.take_device("/dev/dri/card0").is_err());
    }

    #[test]
    fn test_vt_switch() {
        let mut launcher = MetaLauncher::new();
        launcher.initialize().unwrap();
        launcher.take_device("/dev/dri/card0").unwrap();

        // VT switch away.
        launcher.on_pause();
        assert!(launcher.is_pausing());
        assert!(!launcher.is_active());
        assert!(launcher.devices()[0].paused);

        // VT switch back.
        launcher.on_resume();
        assert!(!launcher.is_pausing());
        assert!(launcher.is_active());
        assert!(!launcher.devices()[0].paused);
    }

    #[test]
    fn test_release_control_releases_devices() {
        let mut launcher = MetaLauncher::new();
        launcher.initialize().unwrap();
        launcher.take_device("/dev/dri/card0").unwrap();
        assert!(launcher.devices()[0].acquired);

        launcher.release_control();
        assert!(!launcher.has_session_control());
        assert!(!launcher.devices()[0].acquired);
    }

    #[test]
    fn test_shutdown() {
        let mut launcher = MetaLauncher::new();
        launcher.initialize().unwrap();
        launcher.take_device("/dev/dri/card0").unwrap();

        launcher.shutdown();
        assert!(!launcher.is_initialized());
        assert!(!launcher.has_session_control());
        assert_eq!(launcher.devices().len(), 0);
    }

    #[test]
    fn test_acquired_devices_iter() {
        let mut launcher = MetaLauncher::new();
        launcher.initialize().unwrap();
        launcher.take_device("/dev/dri/card0").unwrap();
        launcher.take_device("/dev/dri/card1").unwrap();
        launcher.release_device("/dev/dri/card0").unwrap();

        let acquired: Vec<_> = launcher.acquired_devices().collect();
        assert_eq!(acquired.len(), 1);
        assert_eq!(acquired[0].path, "/dev/dri/card1");
    }
}
