//! Base MetaBackend ported from GNOME Mutter's src/core/meta-backend.c
//!
//! MetaBackend is the top-level object that ties together the monitor manager,
//! cursor tracker, renderer, input settings, input mapper, idle manager,
//! settings, and orientation manager. It drives the compositor lifecycle
//! (prepare → start → running → shutdown) and emits signals when monitor
//! configuration changes.
//!
//! In Mutter this is an abstract GObject class with virtual methods implemented
//! by MetaBackendNative, MetaBackendX11, etc. Here it is a concrete struct with
//! a `BackendKind` discriminator; the native backend (`backends_native`) wraps
//! this and supplies KMS/DRM-specific behavior.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-backend.c

use alloc::string::String;
use alloc::vec::Vec;

use super::cursor_tracker::MetaCursorTracker;
use super::input_mapper::MetaInputMapper;
use super::input_settings::MetaInputSettings;
use super::monitor_manager::MetaMonitorManager;
use crate::mutter_port::backends::idle_manager::IdleManager;
use crate::mutter_port::backends::orientation_manager::OrientationManager;
use crate::mutter_port::backends::settings::Settings;

/// Which backend implementation is active. Mirrors the GObject subclass
/// hierarchy: MetaBackendNative (DRM/KMS), MetaBackendX11 (X11), MetaBackendX11Nested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// Native DRM/KMS backend (bare metal or QEMU).
    Native,
    /// X11 backend (not used in kernel mode).
    X11,
    /// Headless backend (no hardware output).
    Headless,
}

/// Lifecycle state of the backend, mirroring the Mutter init sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendState {
    /// Just created, not yet prepared.
    Created,
    /// prepare() has been called.
    Prepared,
    /// start() has been called.
    Started,
    /// Running the main loop.
    Running,
    /// prepare_shutdown() has been called.
    ShuttingDown,
    /// finish() has been called.
    Finished,
}

/// Power save mode, mirrors MetaPowerSave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerSave {
    Unknown,
    On,
    Suspend,
    Off,
}

impl Default for PowerSave {
    fn default() -> Self {
        PowerSave::Unknown
    }
}

/// A monitor configuration change event. Mirrors the "monitors-changed"
/// GObject signal on MetaBackend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendSignal {
    /// Monitors-changed: the monitor topology was reconfigured.
    MonitorsChanged,
    /// Lid-is-closed-changed: laptop lid state changed.
    LidIsClosedChanged,
    /// Power-save-changed: power save mode changed.
    PowerSaveChanged,
}

/// The base MetaBackend. Owns the core compositor subsystems and drives
/// the lifecycle.
///
/// Stubbed: GObject signals, D-Bus interfaces, Clutter stage integration,
/// and the GSettings key watcher are not present in the kernel. Signal
/// listeners are a simple Vec of function-like enum values; callers poll
/// `take_pending_signals()` to drain them.
#[derive(Debug)]
pub struct MetaBackend {
    /// Which backend implementation is active.
    kind: BackendKind,
    /// Current lifecycle state.
    state: BackendState,
    /// Whether the backend is headless (no physical display).
    is_headless: bool,
    /// Whether the laptop lid is closed (from the orientation/input manager).
    lid_is_closed: bool,
    /// Current power save mode.
    power_save: PowerSave,

    // ── Owned subsystems ──────────────────────────────────────────────
    monitor_manager: MetaMonitorManager,
    cursor_tracker: MetaCursorTracker,
    input_mapper: MetaInputMapper,
    input_settings: MetaInputSettings,
    idle_manager: IdleManager,
    settings: Settings,
    orientation_manager: OrientationManager,

    // ── Pending signals (replaces GObject signal emission) ────────────
    pending_signals: Vec<BackendSignal>,

    // ── Context name (for debugging / D-Bus) ──────────────────────────
    name: String,
}

impl MetaBackend {
    /// Create a new backend of the given kind. Mirrors meta_backend_new()
    /// which dispatches to the appropriate subclass constructor.
    pub fn new(kind: BackendKind, name: &str) -> Self {
        let is_headless = kind == BackendKind::Headless;
        MetaBackend {
            kind,
            state: BackendState::Created,
            is_headless,
            lid_is_closed: false,
            power_save: PowerSave::default(),
            monitor_manager: MetaMonitorManager::new(),
            cursor_tracker: MetaCursorTracker::new(),
            input_mapper: MetaInputMapper::new(),
            input_settings: MetaInputSettings::new(),
            idle_manager: IdleManager::new(),
            settings: Settings::new(),
            orientation_manager: OrientationManager::new(),
            pending_signals: Vec::new(),
            name: String::from(name),
        }
    }

    // ── Lifecycle ─────────────────────────────────────────────────────

    /// meta_backend_prepare(): initialize subsystems before the main loop.
    ///
    /// In Mutter this calls:
    ///   - meta_monitor_manager_setup()
    ///   - meta_input_settings_init()
    ///   - meta_input_mapper_init()
    ///   - clutter_stage_show()
    ///
    /// The Clutter/GObject parts are stubbed; the subsystem structs are
    /// already constructed in `new()`.
    pub fn prepare(&mut self) -> Result<(), &'static str> {
        if self.state != BackendState::Created {
            return Err("Backend already prepared");
        }
        self.state = BackendState::Prepared;
        Ok(())
    }

    /// meta_backend_start(): begin running.
    ///
    /// In Mutter this starts the Clutter master clock, connects to D-Bus,
    /// and begins processing events.
    pub fn start(&mut self) -> Result<(), &'static str> {
        if self.state != BackendState::Prepared {
            return Err("Backend not prepared");
        }
        self.state = BackendState::Started;
        // Transition to Running immediately (no async Clutter init in kernel).
        self.state = BackendState::Running;
        Ok(())
    }

    /// meta_backend_prepare_shutdown(): begin graceful shutdown.
    pub fn prepare_shutdown(&mut self) {
        if self.state == BackendState::Running {
            self.state = BackendState::ShuttingDown;
        }
    }

    /// meta_backend_finish(): finalize shutdown.
    pub fn finish(&mut self) {
        self.state = BackendState::Finished;
    }

    // ── Accessors ─────────────────────────────────────────────────────

    pub fn kind(&self) -> BackendKind {
        self.kind
    }

    pub fn state(&self) -> BackendState {
        self.state
    }

    pub fn is_headless(&self) -> bool {
        self.is_headless
    }

    pub fn is_running(&self) -> bool {
        self.state == BackendState::Running
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn lid_is_closed(&self) -> bool {
        self.lid_is_closed
    }

    pub fn set_lid_is_closed(&mut self, closed: bool) {
        if self.lid_is_closed != closed {
            self.lid_is_closed = closed;
            self.pending_signals.push(BackendSignal::LidIsClosedChanged);
        }
    }

    pub fn power_save(&self) -> PowerSave {
        self.power_save
    }

    pub fn set_power_save(&mut self, mode: PowerSave) {
        if self.power_save != mode {
            self.power_save = mode;
            self.pending_signals.push(BackendSignal::PowerSaveChanged);
        }
    }

    // ── Subsystem accessors ───────────────────────────────────────────

    pub fn monitor_manager(&self) -> &MetaMonitorManager {
        &self.monitor_manager
    }

    pub fn monitor_manager_mut(&mut self) -> &mut MetaMonitorManager {
        &mut self.monitor_manager
    }

    pub fn cursor_tracker(&self) -> &MetaCursorTracker {
        &self.cursor_tracker
    }

    pub fn cursor_tracker_mut(&mut self) -> &mut MetaCursorTracker {
        &mut self.cursor_tracker
    }

    pub fn input_mapper(&self) -> &MetaInputMapper {
        &self.input_mapper
    }

    pub fn input_mapper_mut(&mut self) -> &mut MetaInputMapper {
        &mut self.input_mapper
    }

    pub fn input_settings(&self) -> &MetaInputSettings {
        &self.input_settings
    }

    pub fn input_settings_mut(&mut self) -> &mut MetaInputSettings {
        &mut self.input_settings
    }

    pub fn idle_manager(&self) -> &IdleManager {
        &self.idle_manager
    }

    pub fn idlemanager_mut(&mut self) -> &mut IdleManager {
        &mut self.idle_manager
    }

    pub fn settings(&self) -> &Settings {
        &self.settings
    }

    pub fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    pub fn orientation_manager(&self) -> &OrientationManager {
        &self.orientation_manager
    }

    pub fn orientation_manager_mut(&mut self) -> &mut OrientationManager {
        &mut self.orientation_manager
    }

    // ── Signal handling (replaces GObject signals) ────────────────────

    /// Emit a signal (queue it for the caller to drain).
    pub fn emit_signal(&mut self, signal: BackendSignal) {
        self.pending_signals.push(signal);
    }

    /// Drain pending signals. Callers should process these after each
    /// event loop iteration.
    pub fn take_pending_signals(&mut self) -> Vec<BackendSignal> {
        core::mem::take(&mut self.pending_signals)
    }

    /// Notify that the monitor configuration has changed. Mirrors
    /// meta_backend_monitors_changed().
    pub fn monitors_changed(&mut self) {
        self.monitor_manager.on_monitors_changed();
        self.pending_signals.push(BackendSignal::MonitorsChanged);
    }

    // ── Input event forwarding ────────────────────────────────────────

    /// Notify the backend that user input occurred (resets idle timers).
    /// Mirrors the input event → idle_monitor_reset_idletime() path.
    pub fn on_user_activity(&mut self, now_usec: i64) {
        // In Mutter, meta_idle_monitor_reset_idletime() is called on the
        // core idle monitor. The IdleManager's core monitor is stubbed but
        // we still reset it.
        self.idle_manager.reset_idle_time();
        let _ = now_usec;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_creation() {
        let backend = MetaBackend::new(BackendKind::Native, "test");
        assert_eq!(backend.kind(), BackendKind::Native);
        assert_eq!(backend.state(), BackendState::Created);
        assert!(!backend.is_headless());
        assert!(!backend.is_running());
    }

    #[test]
    fn test_headless_backend() {
        let backend = MetaBackend::new(BackendKind::Headless, "test");
        assert!(backend.is_headless());
    }

    #[test]
    fn test_lifecycle() {
        let mut backend = MetaBackend::new(BackendKind::Native, "test");
        assert!(backend.prepare().is_ok());
        assert_eq!(backend.state(), BackendState::Prepared);

        assert!(backend.start().is_ok());
        assert!(backend.is_running());

        backend.prepare_shutdown();
        assert_eq!(backend.state(), BackendState::ShuttingDown);

        backend.finish();
        assert_eq!(backend.state(), BackendState::Finished);
    }

    #[test]
    fn test_double_prepare_fails() {
        let mut backend = MetaBackend::new(BackendKind::Native, "test");
        backend.prepare().unwrap();
        assert!(backend.prepare().is_err());
    }

    #[test]
    fn test_start_without_prepare_fails() {
        let mut backend = MetaBackend::new(BackendKind::Native, "test");
        assert!(backend.start().is_err());
    }

    #[test]
    fn test_lid_closed_signal() {
        let mut backend = MetaBackend::new(BackendKind::Native, "test");
        backend.set_lid_is_closed(true);
        let signals = backend.take_pending_signals();
        assert!(signals.contains(&BackendSignal::LidIsClosedChanged));
        assert!(backend.lid_is_closed());
    }

    #[test]
    fn test_power_save_signal() {
        let mut backend = MetaBackend::new(BackendKind::Native, "test");
        backend.set_power_save(PowerSave::Suspend);
        let signals = backend.take_pending_signals();
        assert!(signals.contains(&BackendSignal::PowerSaveChanged));
        assert_eq!(backend.power_save(), PowerSave::Suspend);
    }

    #[test]
    fn test_monitors_changed() {
        let mut backend = MetaBackend::new(BackendKind::Native, "test");
        backend.monitors_changed();
        let signals = backend.take_pending_signals();
        assert!(signals.contains(&BackendSignal::MonitorsChanged));
    }

    #[test]
    fn test_subsystem_access() {
        let backend = MetaBackend::new(BackendKind::Native, "test");
        // Just verify we can get references without panicking.
        let _ = backend.monitor_manager();
        let _ = backend.cursor_tracker();
        let _ = backend.input_mapper();
        let _ = backend.input_settings();
        let _ = backend.idle_manager();
        let _ = backend.settings();
        let _ = backend.orientation_manager();
    }
}
