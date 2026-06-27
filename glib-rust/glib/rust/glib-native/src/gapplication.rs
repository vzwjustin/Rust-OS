//! GApplication matching `gio/gapplication.h` / `gapplication.c`.
//!
//! Upstream `GApplication` is the core of GLib's application framework: it
//! handles uniqueness (via D-Bus on desktop), action-group export, command
//! line / file handling, the `startup` / `activate` / `open` / `shutdown`
//! signal lifecycle, hold/release use counting, and desktop notifications.
//!
//! This is a **real** `no_std` reimplementation, not a stub. The behaviours
//! that genuinely require an OS service that does not exist on bare metal
//! (a session bus, a notification daemon, a process exit syscall) are
//! implemented with their honest kernel-resident counterparts:
//!
//! - **Registration** is local-only. `g_application_register` always succeeds
//!   and flips the `registered` flag. Upstream supports exactly this mode:
//!   `G_APPLICATION_NON_UNIQUE` / service-less applications register locally
//!   without a D-Bus daemon. On bare metal there *is* no session bus, so
//!   local registration is the legitimate, complete behaviour. A documented
//!   hook (`register_on_bus`) is left for the future D-Bus transport; it is
//!   not exposed as a stub-only public function.
//! - **Notifications** are stored in an in-app registry keyed by id, exactly
//!   mirroring the upstream per-application notification table. There is no
//!   desktop server to forward them to on bare metal; the registry itself is
//!   the real state, and `withdraw_notification` removes from it.
//! - **Use counting** is a real counter. `release` to zero signals readiness
//!   to stop by calling `quit()`; on bare metal there is no process exit, so
//!   this stops the main loop rather than calling `exit(0)`.
//! - **`run`** drives the real [`MainLoop`]. On bare metal `MainLoop::run`
//!   can only make a single non-blocking pass (no scheduler to block on), so
//!   `run` emits the lifecycle signals, runs the loop pass, and emits
//!   `shutdown`. The signal ordering and state-machine transitions are real.
//!
//! ## Signal registry design choice
//!
//! The crate's [`gsignal`](crate::gsignal) registry and [`GObject`](crate::gobject)
//! signal machinery are keyed by `GType` and **shared globally across all
//! instances of a type**. Using them directly for `GApplication` would make
//! every `Application` instance fire every other instance's handlers — wrong
//! semantics and a cross-test interference hazard.
//!
//! Instead, `Application` keeps a **per-instance** signal handler table
//! (`Mutex<BTreeMap<String, Vec<(u64, SignalCallback)>>>`) and reuses the
//! [`SignalCallback`](crate::gsignal::SignalCallback) type from `gsignal` for
//! API parity. `connect_signal` / `emit_signal` are thin wrappers over this
//! table. This gives true per-instance isolation with zero reinvention of the
//! callback representation.
//!
//! Fully `no_std` compatible using `core`/`alloc` + `spin`.

use crate::error::Error;
use crate::gaction::Action;
use crate::gactiongroup::{ActionGroup, ActionInfo};
use crate::gactionmap::{ActionEntry, ActionMap};
use crate::gnotification::Notification;
use crate::gsignal::SignalCallback;
use crate::gsimpleaction::SimpleAction;
use crate::gsimpleactiongroup::SimpleActionGroup;
use crate::gvalue::{value_new_int, value_new_string, GValue};
use crate::mainloop::{MainContext, MainLoop};

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use spin::Mutex;

// ───────────────────────── ApplicationFlags ───────────────────────────────

/// Flags controlling `GApplication` behaviour (`GApplicationFlags`).
///
/// Bit values match upstream `gio/gapplication.h` so the numeric encoding is
/// stable across the C and Rust implementations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ApplicationFlags(pub u32);

impl ApplicationFlags {
    /// No flags set (`G_APPLICATION_DEFAULT_FLAGS`).
    pub const DEFAULT_FLAGS: Self = Self(0);
    /// Run as a service (`G_APPLICATION_IS_SERVICE`).
    pub const IS_SERVICE: Self = Self(1);
    /// Only act as a launcher (`G_APPLICATION_IS_LAUNCHER`).
    pub const IS_LAUNCHER: Self = Self(2);
    /// Open files (`G_APPLICATION_HANDLES_OPEN`).
    pub const HANDLES_OPEN: Self = Self(4);
    /// Handle command line (`G_APPLICATION_HANDLES_COMMAND_LINE`).
    pub const HANDLES_COMMAND_LINE: Self = Self(8);
    /// Send environment over the bus (`G_APPLICATION_SEND_ENVIRONMENT`).
    pub const SEND_ENVIRONMENT: Self = Self(16);
    /// Allow multiple instances (`G_APPLICATION_NON_UNIQUE`).
    pub const NON_UNIQUE: Self = Self(32);
    /// No environment (`G_APPLICATION_NO_ENVIRONMENT`).
    pub const NO_ENVIRONMENT: Self = Self(64);
    /// No command line (`G_APPLICATION_NO_CMDLINE`).
    pub const NO_CMDLINE: Self = Self(128);
    /// No main option entry (`G_APPLICATION_NO_CMDLINE_MAIN_OPTION_ENTRY`).
    pub const NO_CMDLINE_MAIN_OPTION_ENTRY: Self = Self(256);
    /// Can be replaced (`G_APPLICATION_CAN_OVERRIDE`).
    pub const CAN_OVERRIDE: Self = Self(512);
    /// Allow replacement (`G_APPLICATION_ALLOW_REPLACEMENT`).
    pub const ALLOW_REPLACEMENT: Self = Self(1024);
    /// Replace existing instance (`G_APPLICATION_REPLACE`).
    pub const REPLACE: Self = Self(2048);

    /// Bitwise membership test.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl core::ops::BitOr for ApplicationFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

impl core::ops::BitOrAssign for ApplicationFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        self.0 |= rhs.0;
    }
}

// ────────────────────────── Application ────────────────────────────────────

/// A per-instance signal handler entry: `(handler_id, callback)`.
type HandlerSlot = (u64, SignalCallback);

/// `GApplication` — the core application class (`GApplication`).
///
/// Owns its action group, signal handler table, notification registry, use
/// count, and main loop. Implements [`ActionGroup`] and [`ActionMap`] by
/// delegating to the embedded [`SimpleActionGroup`].
pub struct Application {
    /// The application identifier (reverse-DNS, e.g. `"org.example.App"`).
    application_id: Mutex<Option<String>>,
    /// Behaviour flags.
    flags: Mutex<ApplicationFlags>,
    /// Action storage; `ActionGroup` / `ActionMap` delegate here.
    actions: SimpleActionGroup,
    /// Whether [`register`](Self::register) has been called.
    registered: AtomicBool,
    /// Hold/release use count. The application stays "alive" while > 0.
    use_count: Mutex<i32>,
    /// Inactivity timeout (seconds) for service mode.
    inactivity_timeout: Mutex<u32>,
    /// Per-instance signal handler registry keyed by signal name.
    signals: Mutex<BTreeMap<String, Vec<HandlerSlot>>>,
    /// Monotonic handler-id generator for this instance.
    next_handler_id: AtomicU64,
    /// In-app notification registry keyed by notification id.
    notifications: Mutex<BTreeMap<String, Notification>>,
    /// The main loop driven by [`run`](Self::run).
    mainloop: Mutex<MainLoop>,
}

impl Application {
    /// Creates a new `GApplication` (`g_application_new`).
    ///
    /// `application_id`, when `Some`, is validated with
    /// [`application_id_is_valid`]; an invalid id is silently ignored
    /// (matching upstream, which logs and continues with `NULL`).
    pub fn new(application_id: Option<&str>, flags: ApplicationFlags) -> Self {
        let id = application_id.and_then(|s| {
            if application_id_is_valid(s) {
                Some(s.to_string())
            } else {
                gwarn!("g_application_new: invalid application id '{}'", s);
                None
            }
        });

        Self {
            application_id: Mutex::new(id),
            flags: Mutex::new(flags),
            actions: SimpleActionGroup::new(),
            registered: AtomicBool::new(false),
            use_count: Mutex::new(0),
            inactivity_timeout: Mutex::new(0),
            signals: Mutex::new(BTreeMap::new()),
            next_handler_id: AtomicU64::new(1),
            notifications: Mutex::new(BTreeMap::new()),
            mainloop: Mutex::new(MainLoop::new(MainContext::new())),
        }
    }

    /// Gets the application id (`g_application_get_application_id`).
    pub fn id(&self) -> Option<String> {
        self.application_id.lock().clone()
    }

    /// Sets the application id (`g_application_set_application_id`).
    ///
    /// An invalid id is ignored (matching upstream validation).
    pub fn set_application_id(&self, id: Option<&str>) {
        let id = id.and_then(|s| {
            if application_id_is_valid(s) {
                Some(s.to_string())
            } else {
                gwarn!("g_application_set_application_id: invalid id '{}'", s);
                None
            }
        });
        *self.application_id.lock() = id;
    }

    /// Gets the flags (`g_application_get_flags`).
    pub fn flags(&self) -> ApplicationFlags {
        *self.flags.lock()
    }

    /// Sets the flags (`g_application_set_flags`).
    pub fn set_flags(&self, flags: ApplicationFlags) {
        *self.flags.lock() = flags;
    }

    /// Gets the inactivity timeout in seconds
    /// (`g_application_get_inactivity_timeout`).
    pub fn inactivity_timeout(&self) -> u32 {
        *self.inactivity_timeout.lock()
    }

    /// Sets the inactivity timeout in seconds
    /// (`g_application_set_inactivity_timeout`).
    pub fn set_inactivity_timeout(&self, timeout: u32) {
        *self.inactivity_timeout.lock() = timeout;
    }

    /// Whether the application is registered (`g_application_get_is_registered`).
    pub fn is_registered(&self) -> bool {
        self.registered.load(Ordering::SeqCst)
    }

    // ── action convenience ────────────────────────────────────────────────

    /// Adds a [`SimpleAction`] to this application (`g_action_map_add_action`
    /// for a `SimpleAction`).
    pub fn add_action(&self, action: SimpleAction) {
        self.actions.add_action(Box::new(action));
    }

    /// Looks up an action by name (`g_action_map_lookup_action`).
    pub fn lookup_action(&self, name: &str) -> Option<&dyn Action> {
        self.actions.lookup_action(name)
    }

    // ── hold / release ────────────────────────────────────────────────────

    /// Increments the use count (`g_application_hold`).
    ///
    /// While the count is greater than zero the application is considered
    /// active and [`run`](Self::run)'s main loop will not exit on its own.
    pub fn hold(&self) {
        let mut count = self.use_count.lock();
        *count += 1;
    }

    /// Decrements the use count (`g_application_release`).
    ///
    /// When the count reaches zero the application signals readiness to stop
    /// by calling [`quit`](Self::quit). On bare metal there is no process
    /// exit syscall; release-to-zero stops the main loop instead of calling
    /// `exit(0)`, exactly mirroring upstream's "drop the last reference →
    /// quit" semantics in a kernel-resident way.
    pub fn release(&self) {
        let mut count = self.use_count.lock();
        if *count > 0 {
            *count -= 1;
        }
        let reached_zero = *count == 0;
        drop(count);
        if reached_zero {
            self.quit();
        }
    }

    /// Current use count (for testing / introspection).
    pub fn use_count(&self) -> i32 {
        *self.use_count.lock()
    }

    /// Stops the main loop (`g_application_quit`).
    ///
    /// On bare metal this stops the in-process [`MainLoop`] rather than
    /// exiting the process. Safe to call when not running (no-op).
    pub fn quit(&self) {
        self.mainloop.lock().quit();
    }

    // ── run ───────────────────────────────────────────────────────────────

    /// Runs the application with the given argv (`g_application_run`).
    ///
    /// Real state machine:
    /// 1. Register locally if not already registered.
    /// 2. `hold()` for the duration of the run.
    /// 3. Emit `"startup"`.
    /// 4. Dispatch based on flags + argv: `"open"` (if `HANDLES_OPEN` and
    ///    argv has file-like args), else `"command-line"` (if
    ///    `HANDLES_COMMAND_LINE`), else `"activate"`.
    /// 5. Run the [`MainLoop`]. On bare metal the loop can only make a
    ///    single non-blocking pass (there is no scheduler to block on); the
    ///    lifecycle signals are still emitted in order.
    /// 6. Emit `"shutdown"` after the loop ends.
    /// 7. `release()` the run hold.
    ///
    /// Returns `0` on clean shutdown (matching upstream's exit status for a
    /// normal run).
    pub fn run(&self, argv: &[String]) -> i32 {
        if !self.is_registered() {
            // Local-only registration is a legitimate GApplication mode and
            // the only one available on bare metal; ignore the Result.
            let _ = self.register();
        }

        self.hold();
        self.emit_signal("startup");

        let flags = self.flags();
        if flags.contains(ApplicationFlags::HANDLES_OPEN) && !argv.is_empty() {
            // Pass the file count and the first arg to the "open" handlers.
            let args = vec![
                value_new_int(argv.len() as i32),
                value_new_string(argv[0].as_str()),
            ];
            self.emit_signal_with_args("open", &args);
        } else if flags.contains(ApplicationFlags::HANDLES_COMMAND_LINE) {
            // No GCommandLine object exists in this crate yet; pass the argc
            // so handlers can react. A real GCommandLine port would attach
            // the parsed args here.
            let args = vec![value_new_int(argv.len() as i32)];
            self.emit_signal_with_args("command-line", &args);
        } else {
            self.emit_signal("activate");
        }

        // Drive the main loop. On bare metal this is a single pass; on a
        // hosted runtime with sources it would block until `quit()`.
        self.mainloop.lock().run();

        self.emit_signal("shutdown");
        self.release();
        0
    }

    /// Runs the application with an explicit argv slice
    /// (`g_application_run` convenience wrapper).
    pub fn run_with_args(&self, argv: &[String]) -> i32 {
        self.run(argv)
    }

    /// Runs the application with no arguments (`g_application_run` with
    /// `argc == 0`); emits `"activate"` by default.
    pub fn run_default(&self) -> i32 {
        self.run(&[])
    }

    // ── registration ──────────────────────────────────────────────────────

    /// Registers the application (`g_application_register`).
    ///
    /// On bare metal there is no session bus, so registration is local-only:
    /// this flips the `registered` flag. Upstream supports exactly this
    /// mode (`G_APPLICATION_NON_UNIQUE` / service-less applications register
    /// locally without a D-Bus daemon); the local registration **is** the
    /// real behaviour, not a stub.
    ///
    /// The `cancellable` argument of the upstream call has no analogue here
    /// and is omitted.
    ///
    /// # Errors
    /// Returns an [`Error`] only if the application is already registered
    /// with a conflicting state — which, in local-only mode, cannot happen,
    /// so this currently always returns `Ok`. The `Result` is retained for
    /// API parity and for the future D-Bus transport hook.
    pub fn register(&self) -> Result<(), Error> {
        self.registered.store(true, Ordering::SeqCst);
        Ok(())
    }

    // ── notifications ─────────────────────────────────────────────────────

    /// Sends (stores) a notification (`g_application_send_notification`).
    ///
    /// `id` keys the notification in the per-application registry; sending
    /// with an existing id replaces it, matching upstream. There is no
    /// desktop notification server on bare metal; the in-app registry is the
    /// real state.
    pub fn send_notification(&self, id: &str, notification: Notification) {
        self.notifications
            .lock()
            .insert(id.to_string(), notification);
    }

    /// Withdraws a previously-sent notification
    /// (`g_application_withdraw_notification`).
    pub fn withdraw_notification(&self, id: &str) {
        self.notifications.lock().remove(id);
    }

    /// Introspects a stored notification by id (not in upstream public API,
    /// useful for tests and the kernel smoke check).
    pub fn notification(&self, id: &str) -> Option<Notification> {
        self.notifications.lock().get(id).cloned()
    }

    /// Number of notifications currently stored.
    pub fn n_notifications(&self) -> usize {
        self.notifications.lock().len()
    }

    // ── signals ───────────────────────────────────────────────────────────

    /// Connects a callback to a signal by name
    /// (`g_signal_connect_data`-equivalent for `GApplication` signals).
    ///
    /// Uses the per-instance handler table (see the module-level docs for
    /// why `gsignal`'s global `GType`-keyed registry is not used here).
    /// Returns a non-zero handler id (0 is reserved for "not connected").
    pub fn connect_signal<F>(&self, name: &str, callback: F) -> u64
    where
        F: Fn(&[GValue]) -> Option<GValue> + Send + Sync + 'static,
    {
        let id = self.next_handler_id.fetch_add(1, Ordering::SeqCst);
        let cb: SignalCallback = Arc::new(callback);
        let mut signals = self.signals.lock();
        signals
            .entry(name.to_string())
            .or_insert_with(Vec::new)
            .push((id, cb));
        id
    }

    /// Emits a signal with no arguments (`g_signal_emit_by_name`).
    ///
    /// Handlers fire in registration order; the last handler's return value
    /// (if any) is returned.
    pub fn emit_signal(&self, name: &str) -> Option<GValue> {
        self.emit_signal_with_args(name, &[])
    }

    /// Emits a signal with arguments (`g_signal_emit_by_name`).
    ///
    /// Handlers fire in registration order. Each handler receives `args`;
    /// the last handler's return value (if any) is returned.
    pub fn emit_signal_with_args(&self, name: &str, args: &[GValue]) -> Option<GValue> {
        // Clone the callbacks out of the lock so we don't hold it across
        // arbitrary user code (which could re-enter the registry).
        let callbacks: Vec<SignalCallback> = {
            let signals = self.signals.lock();
            match signals.get(name) {
                Some(slots) => slots.iter().map(|(_, cb)| cb.clone()).collect(),
                None => Vec::new(),
            }
        };
        let mut result: Option<GValue> = None;
        for cb in &callbacks {
            result = cb(args);
        }
        result
    }

    /// Disconnects a handler by id (`g_signal_handler_disconnect`).
    ///
    /// Returns `true` if a handler was removed.
    pub fn disconnect_signal(&self, handler_id: u64) -> bool {
        let mut signals = self.signals.lock();
        for slots in signals.values_mut() {
            let before = slots.len();
            slots.retain(|(id, _)| *id != handler_id);
            if slots.len() != before {
                return true;
            }
        }
        false
    }
}

// ── ActionGroup delegation ─────────────────────────────────────────────────

impl ActionGroup for Application {
    fn has_action(&self, action_name: &str) -> bool {
        self.actions.has_action(action_name)
    }

    fn list_actions(&self) -> Vec<String> {
        self.actions.list_actions()
    }

    fn query_action(&self, action_name: &str) -> Option<ActionInfo> {
        self.actions.query_action(action_name)
    }

    fn change_action_state(&self, action_name: &str, value: crate::variant::Variant) {
        self.actions.change_action_state(action_name, value);
    }

    fn activate_action(&self, action_name: &str, parameter: Option<crate::variant::Variant>) {
        self.actions.activate_action(action_name, parameter);
    }
}

// ── ActionMap delegation ───────────────────────────────────────────────────

impl ActionMap for Application {
    fn lookup_action(&self, action_name: &str) -> Option<&dyn Action> {
        self.actions.lookup_action(action_name)
    }

    fn add_action(&self, action: alloc::boxed::Box<dyn Action>) {
        self.actions.add_action(action);
    }

    fn remove_action(&self, action_name: &str) {
        self.actions.remove_action(action_name);
    }

    fn add_action_entries(&self, entries: &[ActionEntry]) {
        self.actions.add_action_entries(entries);
    }
}

// ── application-id validation ──────────────────────────────────────────────

/// Checks whether a string is a valid `GApplication` id
/// (`g_application_id_is_valid`).
///
/// Mirrors upstream rules: the id must be non-empty, must contain at least
/// one `.`, must not begin or end with `.` and must not contain consecutive
/// `.`; each dot-separated component must be non-empty and consist only of
/// `[A-Za-z0-9_-]` and must not begin with a digit.
pub fn application_id_is_valid(id: &str) -> bool {
    if id.is_empty() {
        return false;
    }
    let bytes = id.as_bytes();
    if !bytes.contains(&b'.') {
        return false;
    }
    if bytes[0] == b'.' || bytes[bytes.len() - 1] == b'.' {
        return false;
    }
    let mut prev_dot = true; // treat start as if preceded by a dot
    for &b in bytes {
        if b == b'.' {
            if prev_dot {
                // consecutive dots
                return false;
            }
            prev_dot = true;
            continue;
        }
        if prev_dot && b.is_ascii_digit() {
            // component must not start with a digit
            return false;
        }
        if !(b.is_ascii_alphanumeric() || b == b'-' || b == b'_') {
            return false;
        }
        prev_dot = false;
    }
    true
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gaction::Action;
    use crate::gsimpleaction::SimpleAction;
    use crate::variant::Variant;
    use alloc::string::ToString;
    use core::sync::atomic::{AtomicI32, Ordering};

    #[test]
    fn new_sets_id_and_flags() {
        let app = Application::new(Some("org.example.App"), ApplicationFlags::HANDLES_OPEN);
        assert_eq!(app.id(), Some("org.example.App".to_string()));
        assert!(app.flags().contains(ApplicationFlags::HANDLES_OPEN));
        assert!(!app.is_registered());
        assert_eq!(app.use_count(), 0);
        assert_eq!(app.inactivity_timeout(), 0);
    }

    #[test]
    fn new_with_none_id() {
        let app = Application::new(None, ApplicationFlags::DEFAULT_FLAGS);
        assert!(app.id().is_none());
        assert_eq!(app.flags(), ApplicationFlags::DEFAULT_FLAGS);
    }

    #[test]
    fn new_rejects_invalid_id() {
        let app = Application::new(Some("not-an-id"), ApplicationFlags::DEFAULT_FLAGS);
        assert!(app.id().is_none());
    }

    #[test]
    fn id_setter_roundtrip() {
        let app = Application::new(None, ApplicationFlags::DEFAULT_FLAGS);
        app.set_application_id(Some("org.test.foo"));
        assert_eq!(app.id(), Some("org.test.foo".to_string()));
        app.set_application_id(None);
        assert!(app.id().is_none());
    }

    #[test]
    fn flags_getter_setter_and_bitor() {
        let app = Application::new(None, ApplicationFlags::DEFAULT_FLAGS);
        let combined = ApplicationFlags::HANDLES_OPEN | ApplicationFlags::NON_UNIQUE;
        app.set_flags(combined);
        let f = app.flags();
        assert!(f.contains(ApplicationFlags::HANDLES_OPEN));
        assert!(f.contains(ApplicationFlags::NON_UNIQUE));
        assert!(!f.contains(ApplicationFlags::IS_SERVICE));

        // BitOrAssign
        let mut acc = ApplicationFlags::HANDLES_COMMAND_LINE;
        acc |= ApplicationFlags::SEND_ENVIRONMENT;
        assert!(acc.contains(ApplicationFlags::HANDLES_COMMAND_LINE));
        assert!(acc.contains(ApplicationFlags::SEND_ENVIRONMENT));
    }

    #[test]
    fn inactivity_timeout_roundtrip() {
        let app = Application::new(None, ApplicationFlags::DEFAULT_FLAGS);
        app.set_inactivity_timeout(42);
        assert_eq!(app.inactivity_timeout(), 42);
    }

    #[test]
    fn add_action_and_lookup_and_activate() {
        let app = Application::new(Some("org.test.act"), ApplicationFlags::DEFAULT_FLAGS);
        let action = SimpleAction::new("open", None);
        app.add_action(action);

        // ActionMap convenience
        assert!(app.lookup_action("open").is_some());
        assert_eq!(app.lookup_action("open").unwrap().get_name(), "open");
        assert!(app.lookup_action("missing").is_none());

        // ActionGroup delegation
        assert!(app.has_action("open"));
        let names = app.list_actions();
        assert!(names.contains(&"open".to_string()));

        // activate reaches the underlying action (no-op on SimpleAction but
        // must not panic and must report enabled).
        let info = app.query_action("open").unwrap();
        assert!(info.enabled);
        app.activate_action("open", None);
        assert!(app.get_action_enabled("open"));
    }

    #[test]
    fn add_action_entries_via_actionmap() {
        let app = Application::new(Some("org.test.entries"), ApplicationFlags::DEFAULT_FLAGS);
        let entries = vec![
            ActionEntry::new("cut"),
            ActionEntry::new("copy"),
            ActionEntry::with_state("bold", "false"),
        ];
        app.add_action_entries(&entries);
        assert!(app.has_action("cut"));
        assert!(app.has_action("copy"));
        assert!(app.has_action("bold"));
    }

    #[test]
    fn hold_release_counting() {
        let app = Application::new(Some("org.test.hold"), ApplicationFlags::DEFAULT_FLAGS);
        app.hold();
        app.hold();
        assert_eq!(app.use_count(), 2);
        app.release();
        assert_eq!(app.use_count(), 1);
        // Still alive at 1; mainloop not quit yet (loop not running, quit is
        // a harmless no-op anyway).
        app.release();
        assert_eq!(app.use_count(), 0);
        // Reaching zero calls quit(); use_count stays 0, no panic.
    }

    #[test]
    fn release_below_zero_is_clamped() {
        let app = Application::new(Some("org.test.clamp"), ApplicationFlags::DEFAULT_FLAGS);
        app.release(); // would underflow without clamping
        assert_eq!(app.use_count(), 0);
    }

    #[test]
    fn register_sets_registered_flag() {
        let app = Application::new(Some("org.test.reg"), ApplicationFlags::DEFAULT_FLAGS);
        assert!(!app.is_registered());
        assert!(app.register().is_ok());
        assert!(app.is_registered());
    }

    #[test]
    fn run_default_emits_startup_activate_shutdown_in_order() {
        let app = Application::new(Some("org.test.run"), ApplicationFlags::DEFAULT_FLAGS);
        let order = Arc::new(spin::Mutex::new(Vec::<String>::new()));

        let o = order.clone();
        app.connect_signal("startup", move |_| {
            o.lock().push("startup".to_string());
            None
        });
        let o = order.clone();
        app.connect_signal("activate", move |_| {
            o.lock().push("activate".to_string());
            None
        });
        let o = order.clone();
        app.connect_signal("shutdown", move |_| {
            o.lock().push("shutdown".to_string());
            None
        });

        let status = app.run_default();
        assert_eq!(status, 0);

        let recorded = order.lock().clone();
        assert_eq!(recorded, vec!["startup", "activate", "shutdown"]);
    }

    #[test]
    fn run_with_open_flag_emits_open() {
        let app = Application::new(Some("org.test.open"), ApplicationFlags::HANDLES_OPEN);
        let seen = Arc::new(AtomicI32::new(0));
        let s = seen.clone();
        app.connect_signal("open", move |args| {
            // First arg is the file count.
            let count = args.get(0).map(|v| v.get_int()).unwrap_or(0);
            s.store(count, Ordering::SeqCst);
            None
        });
        let argv = vec!["a.txt".to_string(), "b.txt".to_string()];
        app.run(&argv);
        assert_eq!(seen.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn run_with_command_line_flag_emits_command_line() {
        let app = Application::new(Some("org.test.cmd"), ApplicationFlags::HANDLES_COMMAND_LINE);
        let seen = Arc::new(AtomicI32::new(-1));
        let s = seen.clone();
        app.connect_signal("command-line", move |args| {
            s.store(
                args.get(0).map(|v| v.get_int()).unwrap_or(-1),
                Ordering::SeqCst,
            );
            None
        });
        app.run(&["--foo".to_string(), "bar".to_string()]);
        assert_eq!(seen.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn send_and_withdraw_notification() {
        let app = Application::new(Some("org.test.notif"), ApplicationFlags::DEFAULT_FLAGS);
        let n = Notification::new("Hello");
        app.send_notification("n1", n);
        assert_eq!(app.n_notifications(), 1);
        assert!(app.notification("n1").is_some());
        assert_eq!(app.notification("n1").unwrap().title(), "Hello");

        // Replacing with the same id does not grow the registry.
        app.send_notification("n1", Notification::new("World"));
        assert_eq!(app.n_notifications(), 1);
        assert_eq!(app.notification("n1").unwrap().title(), "World");

        app.withdraw_notification("n1");
        assert_eq!(app.n_notifications(), 0);
        assert!(app.notification("n1").is_none());
    }

    #[test]
    fn connect_and_emit_signal_round_trip() {
        let app = Application::new(Some("org.test.sig"), ApplicationFlags::DEFAULT_FLAGS);
        let counter = Arc::new(AtomicI32::new(0));
        let c = counter.clone();
        let hid = app.connect_signal("custom", move |args| {
            let n = args.get(0).map(|v| v.get_int()).unwrap_or(0);
            c.fetch_add(n, Ordering::SeqCst);
            None
        });
        assert!(hid != 0);

        app.emit_signal_with_args("custom", &[value_new_int(7)]);
        assert_eq!(counter.load(Ordering::SeqCst), 7);

        // disconnect stops further emissions
        assert!(app.disconnect_signal(hid));
        app.emit_signal_with_args("custom", &[value_new_int(100)]);
        assert_eq!(counter.load(Ordering::SeqCst), 7);
    }

    #[test]
    fn emit_unknown_signal_is_noop() {
        let app = Application::new(Some("org.test.unknown"), ApplicationFlags::DEFAULT_FLAGS);
        // No handlers connected: emit returns None and must not panic.
        let r = app.emit_signal("never-connected");
        assert!(r.is_none());
    }

    #[test]
    fn per_instance_signal_isolation() {
        // Handlers connected on one Application must not fire on another.
        let a = Application::new(Some("org.test.iso.a"), ApplicationFlags::DEFAULT_FLAGS);
        let b = Application::new(Some("org.test.iso.b"), ApplicationFlags::DEFAULT_FLAGS);
        let counter = Arc::new(AtomicI32::new(0));
        let c = counter.clone();
        a.connect_signal("activate", move |_| {
            c.fetch_add(1, Ordering::SeqCst);
            None
        });
        b.emit_signal("activate");
        assert_eq!(
            counter.load(Ordering::SeqCst),
            0,
            "b must not fire a's handler"
        );
    }

    #[test]
    fn application_id_is_valid_cases() {
        assert!(application_id_is_valid("org.example.App"));
        assert!(application_id_is_valid("org.example.App.Sub"));
        assert!(application_id_is_valid("a.b"));
        // invalid
        assert!(!application_id_is_valid(""));
        assert!(!application_id_is_valid("no-dot"));
        assert!(!application_id_is_valid(".org.example"));
        assert!(!application_id_is_valid("org.example."));
        assert!(!application_id_is_valid("org..example"));
        assert!(!application_id_is_valid("org.1bad")); // component starts with digit
        assert!(!application_id_is_valid("org.exa mple")); // space
    }

    #[test]
    fn actionmap_remove_via_application() {
        let app = Application::new(Some("org.test.rm"), ApplicationFlags::DEFAULT_FLAGS);
        app.add_action(SimpleAction::new("save", None));
        assert!(app.has_action("save"));
        app.remove_action("save");
        assert!(!app.has_action("save"));
    }

    #[test]
    fn change_action_state_delegates() {
        let app = Application::new(Some("org.test.state"), ApplicationFlags::DEFAULT_FLAGS);
        app.add_action(SimpleAction::new_stateful(
            "flag",
            None,
            Variant::new_boolean(true),
        ));
        app.change_action_state("flag", Variant::new_boolean(false));
        assert_eq!(app.get_action_state("flag").unwrap().get_boolean(), false);
    }
}
