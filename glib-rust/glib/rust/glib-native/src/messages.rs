//! Logging and message output matching `gmessages.h` / `gmessages.c`.
//!
//! Phase 5 covers the classic `g_log()` API, default and per-domain handlers,
//! and stdout/stderr print hooks. Structured logging (`g_log_structured*`) and
//! test-framework fatal hooks are deferred.

use crate::prelude::*;
use alloc::collections::BTreeMap;
use core::ffi::c_void;
use core::fmt::{self, Write as FmtWrite};
use spin::mutex::Mutex;
use spin::Once;

/// Bit shift for user-defined log levels (0–7 are reserved by GLib).
pub const LOG_LEVEL_USER_SHIFT: u32 = 8;

/// Log level and flag bitmask (`GLogLevelFlags`).
#[repr(transparent)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct LogLevelFlags(u32);

impl LogLevelFlags {
    /// Recursion guard flag (`G_LOG_FLAG_RECURSION`).
    pub const FLAG_RECURSION: Self = Self(1 << 0);
    /// Fatal flag (`G_LOG_FLAG_FATAL`).
    pub const FLAG_FATAL: Self = Self(1 << 1);
    /// Error level (`G_LOG_LEVEL_ERROR`); always fatal.
    pub const LEVEL_ERROR: Self = Self(1 << 2);
    /// Critical level (`G_LOG_LEVEL_CRITICAL`).
    pub const LEVEL_CRITICAL: Self = Self(1 << 3);
    /// Warning level (`G_LOG_LEVEL_WARNING`).
    pub const LEVEL_WARNING: Self = Self(1 << 4);
    /// Message level (`G_LOG_LEVEL_MESSAGE`).
    pub const LEVEL_MESSAGE: Self = Self(1 << 5);
    /// Info level (`G_LOG_LEVEL_INFO`).
    pub const LEVEL_INFO: Self = Self(1 << 6);
    /// Debug level (`G_LOG_LEVEL_DEBUG`).
    pub const LEVEL_DEBUG: Self = Self(1 << 7);
    /// Mask of level bits, excluding recursion/fatal flags (`G_LOG_LEVEL_MASK`).
    pub const LEVEL_MASK: Self = Self(!(Self::FLAG_RECURSION.0 | Self::FLAG_FATAL.0));
    /// Default per-domain fatal mask (`G_LOG_FATAL_MASK`).
    pub const FATAL_MASK: Self = Self(Self::FLAG_RECURSION.0 | Self::LEVEL_ERROR.0);

    /// Raw bitmask value.
    #[inline]
    #[must_use]
    pub const fn bits(self) -> u32 {
        self.0
    }

    /// Whether `self` contains all bits in `other`.
    #[inline]
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    /// Whether `self` shares any bit with `other`.
    #[inline]
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        (self.0 & other.0) != 0
    }

    /// Bitwise union.
    #[inline]
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// Whether all bits in `log_level` are set in `self` (handler mask matching).
    #[inline]
    #[must_use]
    pub const fn covers(self, log_level: Self) -> bool {
        (self.0 & log_level.0) == log_level.0
    }

    /// Strip recursion/fatal flags, leaving level bits only.
    #[inline]
    #[must_use]
    pub const fn level_mask(self) -> Self {
        Self(self.0 & Self::LEVEL_MASK.0)
    }
}

/// Log handler callback (`GLogFunc`).
pub type LogFunc =
    fn(log_domain: Option<&str>, log_level: LogLevelFlags, message: &str, user_data: *mut c_void);

/// Print handler callback (`GPrintFunc`).
pub type PrintFunc = fn(string: &str);

const ALERT_LEVELS: LogLevelFlags = LogLevelFlags(
    LogLevelFlags::LEVEL_ERROR.0 | LogLevelFlags::LEVEL_CRITICAL.0 | LogLevelFlags::LEVEL_WARNING.0,
);

struct LogHandlerEntry {
    id: u32,
    log_levels: LogLevelFlags,
    func: LogFunc,
    user_data: usize,
}

struct LogDomain {
    handlers: Vec<LogHandlerEntry>,
    fatal_mask: LogLevelFlags,
}

struct MessagesState {
    domains: BTreeMap<String, LogDomain>,
    default_handler: (LogFunc, usize),
    print_handler: PrintFunc,
    printerr_handler: PrintFunc,
    next_handler_id: u32,
    log_depth: u32,
    always_fatal: LogLevelFlags,
}

impl MessagesState {
    fn new() -> Self {
        Self {
            domains: BTreeMap::new(),
            default_handler: (log_default_handler, 0),
            print_handler: default_print_handler,
            printerr_handler: default_printerr_handler,
            next_handler_id: 0,
            log_depth: 0,
            always_fatal: LogLevelFlags::empty(),
        }
    }

    fn domain_key(log_domain: Option<&str>) -> String {
        log_domain.unwrap_or("").to_owned()
    }

    fn domain_mut(&mut self, log_domain: Option<&str>) -> &mut LogDomain {
        let key = Self::domain_key(log_domain);
        self.domains.entry(key).or_insert(LogDomain {
            handlers: Vec::new(),
            fatal_mask: LogLevelFlags::FATAL_MASK,
        })
    }

    fn find_handler(&self, log_domain: Option<&str>, log_level: LogLevelFlags) -> (LogFunc, usize) {
        let key = Self::domain_key(log_domain);
        if let Some(domain) = self.domains.get(&key) {
            for handler in &domain.handlers {
                if handler.log_levels.covers(log_level) {
                    return (handler.func, handler.user_data);
                }
            }
        }
        self.default_handler
    }

    fn domain_fatal_mask(&self, log_domain: Option<&str>) -> LogLevelFlags {
        let key = Self::domain_key(log_domain);
        self.domains
            .get(&key)
            .map_or(LogLevelFlags::FATAL_MASK, |d| d.fatal_mask)
    }

    fn prune_empty_domain(&mut self, log_domain: Option<&str>) {
        let key = Self::domain_key(log_domain);
        if let Some(domain) = self.domains.get(&key) {
            if domain.handlers.is_empty() && domain.fatal_mask == LogLevelFlags::FATAL_MASK {
                self.domains.remove(&key);
            }
        }
    }
}

impl LogLevelFlags {
    const fn empty() -> Self {
        Self(0)
    }
}

fn state() -> &'static Mutex<MessagesState> {
    static STATE: Once<Mutex<MessagesState>> = Once::new();
    STATE.call_once(|| Mutex::new(MessagesState::new()))
}

fn with_state<F, R>(f: F) -> R
where
    F: FnOnce(&mut MessagesState) -> R,
{
    let mut guard = state().lock();
    f(&mut guard)
}

fn default_print_handler(string: &str) {
    let _ = string;
}

fn default_printerr_handler(string: &str) {
    let _ = string;
}

fn log_level_to_stream(log_level: LogLevelFlags) -> LogStream {
    if log_level.intersects(
        LogLevelFlags::LEVEL_ERROR
            .union(LogLevelFlags::LEVEL_CRITICAL)
            .union(LogLevelFlags::LEVEL_WARNING)
            .union(LogLevelFlags::LEVEL_MESSAGE),
    ) {
        LogStream::Stderr
    } else {
        LogStream::Stdout
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LogStream {
    Stderr,
    Stdout,
}

impl LogStream {
    fn write_all(self, bytes: &[u8]) {
        let _ = bytes;
        let _ = self;
    }
}

fn level_prefix(log_level: LogLevelFlags) -> String {
    let mut prefix = String::new();
    match log_level.level_mask() {
        LogLevelFlags::LEVEL_ERROR => prefix.push_str("ERROR"),
        LogLevelFlags::LEVEL_CRITICAL => prefix.push_str("CRITICAL"),
        LogLevelFlags::LEVEL_WARNING => prefix.push_str("WARNING"),
        LogLevelFlags::LEVEL_MESSAGE => prefix.push_str("Message"),
        LogLevelFlags::LEVEL_INFO => prefix.push_str("INFO"),
        LogLevelFlags::LEVEL_DEBUG => prefix.push_str("DEBUG"),
        other if other.bits() != 0 => {
            prefix.push_str("LOG-");
            let _ = write!(prefix, "{:x}", other.bits());
        }
        _ => prefix.push_str("LOG"),
    }

    if log_level.contains(LogLevelFlags::FLAG_RECURSION) {
        prefix.push_str(" (recursed)");
    }
    if log_level.intersects(ALERT_LEVELS) {
        prefix.push_str(" **");
    }
    prefix
}

/// Format a log line for the default/fallback handlers.
#[must_use]
pub fn format_log_line(
    log_domain: Option<&str>,
    log_level: LogLevelFlags,
    message: &str,
) -> String {
    let message = if message.is_empty() {
        "(NULL) message"
    } else {
        message
    };
    let prefix = level_prefix(log_level);
    let mut line = String::new();

    if log_level.intersects(ALERT_LEVELS) {
        line.push('\n');
    }
    if log_domain.is_none() {
        line.push_str("** ");
    }
    if let Some(domain) = log_domain {
        if !domain.is_empty() {
            line.push_str(domain);
            line.push('-');
        }
    }
    line.push_str(&prefix);
    line.push_str(": ");
    line.push_str(message);
    if !message.ends_with('\n') {
        line.push('\n');
    }
    line
}

fn log_fallback_handler(
    log_domain: Option<&str>,
    log_level: LogLevelFlags,
    message: &str,
    _unused_data: *mut c_void,
) {
    let line = format_log_line(log_domain, log_level, message);
    log_level_to_stream(log_level).write_all(line.as_bytes());
}

/// Default log handler (`g_log_default_handler`).
pub fn log_default_handler(
    log_domain: Option<&str>,
    log_level: LogLevelFlags,
    message: &str,
    _unused_data: *mut c_void,
) {
    if log_level.contains(LogLevelFlags::FLAG_RECURSION) {
        log_fallback_handler(log_domain, log_level, message, core::ptr::null_mut());
        return;
    }

    let line = format_log_line(log_domain, log_level, message);
    log_level_to_stream(log_level).write_all(line.as_bytes());
}

/// Install the process-wide default log handler (`g_log_set_default_handler`).
pub fn log_set_default_handler(log_func: LogFunc, user_data: *mut c_void) -> LogFunc {
    with_state(|state| {
        let old = state.default_handler.0;
        state.default_handler = (log_func, user_data as usize);
        old
    })
}

/// Register a log handler for a domain and level mask (`g_log_set_handler`).
///
/// Returns `0` when `log_levels` contains no level bits or `log_func` is invalid.
pub fn log_set_handler(
    log_domain: Option<&str>,
    log_levels: LogLevelFlags,
    log_func: LogFunc,
    user_data: *mut c_void,
) -> u32 {
    if !log_levels.intersects(LogLevelFlags::LEVEL_MASK) {
        return 0;
    }

    with_state(|state| {
        state.next_handler_id = state.next_handler_id.saturating_add(1);
        let id = state.next_handler_id;
        let domain = state.domain_mut(log_domain);
        domain.handlers.insert(
            0,
            LogHandlerEntry {
                id,
                log_levels,
                func: log_func,
                user_data: user_data as usize,
            },
        );
        id
    })
}

/// Remove a previously registered log handler (`g_log_remove_handler`).
pub fn log_remove_handler(log_domain: Option<&str>, handler_id: u32) {
    if handler_id == 0 {
        return;
    }

    let removed = with_state(|state| {
        let domain = state.domain_mut(log_domain);
        if let Some(pos) = domain.handlers.iter().position(|h| h.id == handler_id) {
            domain.handlers.remove(pos);
            state.prune_empty_domain(log_domain);
            true
        } else {
            false
        }
    });

    if !removed {
        let domain = log_domain.unwrap_or("");
        let msg = format!(
            "log_remove_handler: could not find handler with id '{handler_id}' for domain \"{domain}\""
        );
        log_default_handler(
            None,
            LogLevelFlags::LEVEL_WARNING,
            &msg,
            core::ptr::null_mut(),
        );
    }
}

fn iter_level_bits(level: LogLevelFlags) -> impl Iterator<Item = LogLevelFlags> {
    (0..32).rev().filter_map(move |bit| {
        let mask = 1_u32 << bit;
        if level.bits() & mask != 0 {
            Some(LogLevelFlags(mask))
        } else {
            None
        }
    })
}

fn maybe_abort(log_level: LogLevelFlags) {
    if log_level.contains(LogLevelFlags::FLAG_FATAL) {
        #[cfg(not(test))]
        {
            panic!("GLib fatal log level triggered");
        }
    }
}

/// Log a message at the given level (`g_log` / `g_logv`).
pub fn log(log_domain: Option<&str>, log_level: LogLevelFlags, message: &str) {
    let was_fatal = log_level.contains(LogLevelFlags::FLAG_FATAL);
    let was_recursion = log_level.contains(LogLevelFlags::FLAG_RECURSION);

    let level = log_level.level_mask();
    if level.bits() == 0 {
        return;
    }

    for single_level in iter_level_bits(level) {
        let (log_func, user_data, test_level) = with_state(|state| {
            let mut test_level = single_level;
            if was_fatal {
                test_level = test_level.union(LogLevelFlags::FLAG_FATAL);
            }
            if was_recursion {
                test_level = test_level.union(LogLevelFlags::FLAG_RECURSION);
            }
            if state.log_depth > 0 {
                test_level = test_level.union(LogLevelFlags::FLAG_RECURSION);
            }

            let domain_fatal_mask = state.domain_fatal_mask(log_domain);
            if domain_fatal_mask
                .union(state.always_fatal)
                .intersects(test_level)
            {
                test_level = test_level.union(LogLevelFlags::FLAG_FATAL);
            }

            let (log_func, user_data) = if test_level.contains(LogLevelFlags::FLAG_RECURSION) {
                (log_fallback_handler as LogFunc, 0)
            } else {
                state.find_handler(log_domain, test_level)
            };

            state.log_depth = state.log_depth.saturating_add(1);
            (log_func, user_data, test_level)
        });

        log_func(log_domain, test_level, message, user_data as *mut c_void);

        with_state(|state| {
            state.log_depth = state.log_depth.saturating_sub(1);
        });

        maybe_abort(test_level);
    }
}

/// Log helper accepting formatted arguments.
pub fn log_fmt(log_domain: Option<&str>, log_level: LogLevelFlags, args: fmt::Arguments<'_>) {
    log(log_domain, log_level, &args.to_string());
}

/// Log a `G_LOG_LEVEL_MESSAGE` message in the default domain (`g_message`).
pub fn message(message: &str) {
    log(None, LogLevelFlags::LEVEL_MESSAGE, message);
}

/// Log a `G_LOG_LEVEL_WARNING` message in the default domain (`g_warning`).
pub fn warning(message: &str) {
    log(None, LogLevelFlags::LEVEL_WARNING, message);
}

/// Log a `G_LOG_LEVEL_CRITICAL` message in the default domain (`g_critical`).
pub fn critical(message: &str) {
    log(None, LogLevelFlags::LEVEL_CRITICAL, message);
}

/// Log a `G_LOG_LEVEL_INFO` message in the default domain (`g_info`).
pub fn info(message: &str) {
    log(None, LogLevelFlags::LEVEL_INFO, message);
}

/// Log a `G_LOG_LEVEL_DEBUG` message in the default domain (`g_debug`).
pub fn debug(message: &str) {
    log(None, LogLevelFlags::LEVEL_DEBUG, message);
}

/// Replace the stdout print handler (`g_set_print_handler`).
///
/// Pass `None` to restore the default handler.
pub fn set_print_handler(func: Option<PrintFunc>) -> PrintFunc {
    with_state(|state| {
        let old = state.print_handler;
        state.print_handler = func.unwrap_or(default_print_handler);
        old
    })
}

/// Replace the stderr print handler (`g_set_printerr_handler`).
///
/// Pass `None` to restore the default handler.
pub fn set_printerr_handler(func: Option<PrintFunc>) -> PrintFunc {
    with_state(|state| {
        let old = state.printerr_handler;
        state.printerr_handler = func.unwrap_or(default_printerr_handler);
        old
    })
}

/// Output a string via the current print handler (`g_print`).
pub fn print(string: &str) {
    let handler = with_state(|state| state.print_handler);
    handler(string);
}

/// Output a string via the current printerr handler (`g_printerr`).
pub fn printerr(string: &str) {
    let handler = with_state(|state| state.printerr_handler);
    handler(string);
}

#[cfg(test)]
pub(crate) fn messages_test_lock() -> std::sync::MutexGuard<'static, ()> {
    use std::sync::{Mutex, OnceLock};

    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;
    use std::sync::atomic::{AtomicU32, Ordering};

    thread_local! {
        static CAPTURED_LOGS: RefCell<Vec<(Option<String>, LogLevelFlags, String)>> =
            RefCell::new(Vec::new());
        static CAPTURED_PRINTS: RefCell<Vec<String>> = RefCell::new(Vec::new());
        static CAPTURED_PRINTERRS: RefCell<Vec<String>> = RefCell::new(Vec::new());
    }

    struct TestGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl TestGuard {
        fn new() -> Self {
            let _lock = super::messages_test_lock();
            with_state(|state| {
                *state = MessagesState::new();
            });
            log_set_default_handler(log_default_handler, std::ptr::null_mut());
            CAPTURED_LOGS.with(|logs| logs.borrow_mut().clear());
            CAPTURED_PRINTS.with(|prints| prints.borrow_mut().clear());
            CAPTURED_PRINTERRS.with(|prints| prints.borrow_mut().clear());
            Self { _lock }
        }
    }

    fn reset_state() -> TestGuard {
        TestGuard::new()
    }

    fn capture_log_handler(
        log_domain: Option<&str>,
        log_level: LogLevelFlags,
        message: &str,
        _user_data: *mut c_void,
    ) {
        CAPTURED_LOGS.with(|logs| {
            logs.borrow_mut()
                .push((log_domain.map(str::to_owned), log_level, message.to_owned()));
        });
    }

    fn capture_print_handler(string: &str) {
        CAPTURED_PRINTS.with(|prints| prints.borrow_mut().push(string.to_owned()));
    }

    fn capture_printerr_handler(string: &str) {
        CAPTURED_PRINTERRS.with(|prints| prints.borrow_mut().push(string.to_owned()));
    }

    static USER_DATA_VALUE: AtomicU32 = AtomicU32::new(0);

    fn user_data_log_handler(
        _log_domain: Option<&str>,
        _log_level: LogLevelFlags,
        message: &str,
        user_data: *mut c_void,
    ) {
        let expected = USER_DATA_VALUE.load(Ordering::SeqCst);
        let got = user_data as usize as u32;
        CAPTURED_LOGS.with(|logs| {
            logs.borrow_mut()
                .push((None, LogLevelFlags::empty(), format!("{got}:{message}")));
        });
        assert_eq!(got, expected);
    }

    #[test]
    fn log_level_constants_match_glib() {
        assert_eq!(LogLevelFlags::FLAG_RECURSION.bits(), 1 << 0);
        assert_eq!(LogLevelFlags::FLAG_FATAL.bits(), 1 << 1);
        assert_eq!(LogLevelFlags::LEVEL_ERROR.bits(), 1 << 2);
        assert_eq!(LogLevelFlags::LEVEL_CRITICAL.bits(), 1 << 3);
        assert_eq!(LogLevelFlags::LEVEL_WARNING.bits(), 1 << 4);
        assert_eq!(LogLevelFlags::LEVEL_MESSAGE.bits(), 1 << 5);
        assert_eq!(LogLevelFlags::LEVEL_INFO.bits(), 1 << 6);
        assert_eq!(LogLevelFlags::LEVEL_DEBUG.bits(), 1 << 7);
        assert_eq!(LOG_LEVEL_USER_SHIFT, 8);
        assert_eq!(
            LogLevelFlags::LEVEL_MASK.bits(),
            !(LogLevelFlags::FLAG_RECURSION.bits() | LogLevelFlags::FLAG_FATAL.bits())
        );
    }

    #[test]
    fn default_handler_formats_warning_with_domain() {
        let line = format_log_line(
            Some("Test"),
            LogLevelFlags::LEVEL_WARNING,
            "something happened",
        );
        assert!(line.starts_with('\n'));
        assert!(line.contains("Test-WARNING **:"));
        assert!(line.contains("something happened"));
        assert!(line.ends_with('\n'));
    }

    #[test]
    fn default_handler_formats_info_without_domain_prefix() {
        let line = format_log_line(None, LogLevelFlags::LEVEL_INFO, "details");
        assert!(line.starts_with("** INFO:"));
        assert!(line.contains("details"));
    }

    #[test]
    fn log_routes_to_custom_domain_handler() {
        let _guard = reset_state();
        let id = log_set_handler(
            Some("Widget"),
            LogLevelFlags::LEVEL_WARNING,
            capture_log_handler,
            std::ptr::null_mut(),
        );
        assert_ne!(id, 0);

        log(
            Some("Widget"),
            LogLevelFlags::LEVEL_WARNING,
            "layout changed",
        );

        let entries = CAPTURED_LOGS.with(|logs| logs.borrow().clone());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].0.as_deref(), Some("Widget"));
        assert_eq!(entries[0].1, LogLevelFlags::LEVEL_WARNING);
        assert_eq!(entries[0].2, "layout changed");
    }

    #[test]
    fn log_respects_handler_level_mask() {
        let _guard = reset_state();
        log_set_handler(
            None,
            LogLevelFlags::LEVEL_WARNING,
            capture_log_handler,
            std::ptr::null_mut(),
        );

        log(None, LogLevelFlags::LEVEL_INFO, "hidden");
        assert!(CAPTURED_LOGS.with(|logs| logs.borrow().is_empty()));

        log(None, LogLevelFlags::LEVEL_WARNING, "shown");
        assert_eq!(CAPTURED_LOGS.with(|logs| logs.borrow().len()), 1);
    }

    #[test]
    fn newest_handler_wins_when_multiple_match() {
        let _guard = reset_state();
        fn first_handler(_: Option<&str>, _: LogLevelFlags, message: &str, _: *mut c_void) {
            CAPTURED_LOGS.with(|logs| {
                logs.borrow_mut()
                    .push((None, LogLevelFlags::empty(), format!("first:{message}")));
            });
        }

        fn second_handler(_: Option<&str>, _: LogLevelFlags, message: &str, _: *mut c_void) {
            CAPTURED_LOGS.with(|logs| {
                logs.borrow_mut()
                    .push((None, LogLevelFlags::empty(), format!("second:{message}")));
            });
        }

        log_set_handler(
            None,
            LogLevelFlags::LEVEL_MESSAGE,
            first_handler,
            std::ptr::null_mut(),
        );
        log_set_handler(
            None,
            LogLevelFlags::LEVEL_MESSAGE,
            second_handler,
            std::ptr::null_mut(),
        );

        log(None, LogLevelFlags::LEVEL_MESSAGE, "pick me");
        let entries = CAPTURED_LOGS.with(|logs| logs.borrow().clone());
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].2, "second:pick me");
    }

    #[test]
    fn log_remove_handler_stops_delivery() {
        let _guard = reset_state();
        let id = log_set_handler(
            None,
            LogLevelFlags::LEVEL_CRITICAL,
            capture_log_handler,
            std::ptr::null_mut(),
        );
        log_remove_handler(None, id);
        log(None, LogLevelFlags::LEVEL_CRITICAL, "after removal");
        assert!(CAPTURED_LOGS.with(|logs| logs.borrow().is_empty()));
    }

    #[test]
    fn log_set_default_handler_replaces_fallback() {
        let _guard = reset_state();
        let previous = log_set_default_handler(capture_log_handler, std::ptr::null_mut());
        #[allow(unpredictable_function_pointer_comparisons)]
        {
            assert!(previous == log_default_handler);
        }

        log(None, LogLevelFlags::LEVEL_MESSAGE, "via default");
        assert_eq!(CAPTURED_LOGS.with(|logs| logs.borrow().len()), 1);
    }

    #[test]
    fn convenience_functions_use_default_domain() {
        let _guard = reset_state();
        log_set_default_handler(capture_log_handler, std::ptr::null_mut());

        message("msg");
        warning("warn");
        critical("crit");
        info("info");
        debug("dbg");

        let entries = CAPTURED_LOGS.with(|logs| logs.borrow().clone());
        assert_eq!(entries.len(), 5);
        assert_eq!(entries[0].1, LogLevelFlags::LEVEL_MESSAGE);
        assert_eq!(entries[1].1, LogLevelFlags::LEVEL_WARNING);
        assert_eq!(entries[2].1, LogLevelFlags::LEVEL_CRITICAL);
        assert_eq!(entries[3].1, LogLevelFlags::LEVEL_INFO);
        assert_eq!(entries[4].1, LogLevelFlags::LEVEL_DEBUG);
        assert!(entries.iter().all(|entry| entry.0.is_none()));
    }

    #[test]
    fn error_level_is_marked_fatal_for_handlers() {
        let _guard = reset_state();
        log_set_default_handler(capture_log_handler, std::ptr::null_mut());
        log(None, LogLevelFlags::LEVEL_ERROR, "fatal path");
        let level = CAPTURED_LOGS.with(|logs| logs.borrow()[0].1);
        assert!(level.contains(LogLevelFlags::FLAG_FATAL));
        assert!(level.contains(LogLevelFlags::LEVEL_ERROR));
    }

    #[test]
    fn set_print_handler_replaces_output() {
        let _guard = reset_state();
        let old = set_print_handler(Some(capture_print_handler));
        #[allow(unpredictable_function_pointer_comparisons)]
        {
            assert!(old == default_print_handler);
        }

        print("hello stdout");
        assert_eq!(
            CAPTURED_PRINTS.with(|prints| prints.borrow().clone()),
            vec!["hello stdout".to_owned()]
        );

        let restored = set_print_handler(None);
        #[allow(unpredictable_function_pointer_comparisons)]
        {
            assert!(restored == capture_print_handler);
        }
    }

    #[test]
    fn set_printerr_handler_replaces_output() {
        let _guard = reset_state();
        let old = set_printerr_handler(Some(capture_printerr_handler));
        #[allow(unpredictable_function_pointer_comparisons)]
        {
            assert!(old == default_printerr_handler);
        }

        printerr("hello stderr");
        assert_eq!(
            CAPTURED_PRINTERRS.with(|prints| prints.borrow().clone()),
            vec!["hello stderr".to_owned()]
        );
    }

    #[test]
    fn log_set_handler_passes_user_data() {
        let _guard = reset_state();
        USER_DATA_VALUE.store(42, Ordering::SeqCst);
        let user_data = 42_usize as *mut c_void;
        log_set_handler(
            None,
            LogLevelFlags::LEVEL_INFO,
            user_data_log_handler,
            user_data,
        );
        log(None, LogLevelFlags::LEVEL_INFO, "payload");
        let entries = CAPTURED_LOGS.with(|logs| logs.borrow().clone());
        assert_eq!(entries[0].2, "42:payload");
    }

    #[test]
    fn combined_levels_invoke_handler_per_bit() {
        let _guard = reset_state();
        log_set_default_handler(capture_log_handler, std::ptr::null_mut());
        log(
            None,
            LogLevelFlags::LEVEL_WARNING.union(LogLevelFlags::LEVEL_INFO),
            "multi",
        );
        assert_eq!(CAPTURED_LOGS.with(|logs| logs.borrow().len()), 2);
    }
}
