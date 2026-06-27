//! Error reporting matching `gerror.h` / `gerror.c`.
//!
//! Phase 5 covers the core [`Error`] type and propagation helpers. Extended
//! error domains (`G_DEFINE_EXTENDED_ERROR`) are deferred.

use crate::messages::warning;
use crate::prelude::*;
use crate::quark::Quark;
use core::fmt;

const ERROR_OVERWRITTEN_WARNING: &str =
    "GError set over the top of a previous GError or uninitialized memory.\n\
     This indicates a bug in someone's code. You must ensure an error is NULL before it's set.\n\
     The overwriting error message was: ";

/// Structured error with domain, code, and message (`GError`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Error {
    domain: Quark,
    code: i32,
    message: String,
}

impl Error {
    /// Creates a new error with a formatted message (`g_error_new`).
    #[must_use]
    pub fn new(domain: Quark, code: i32, message: impl fmt::Display) -> Self {
        debug_assert_ne!(domain, 0, "error domain must be non-zero");
        Self {
            domain,
            code,
            message: message.to_string(),
        }
    }

    /// Creates a new error with a literal message (`g_error_new_literal`).
    #[must_use]
    pub fn new_literal(domain: Quark, code: i32, message: impl Into<String>) -> Self {
        debug_assert_ne!(domain, 0, "error domain must be non-zero");
        Self {
            domain,
            code,
            message: message.into(),
        }
    }

    /// Error domain quark.
    #[inline]
    #[must_use]
    pub fn domain(&self) -> Quark {
        self.domain
    }

    /// Error code within the domain.
    #[inline]
    #[must_use]
    pub fn code(&self) -> i32 {
        self.code
    }

    /// Human-readable error message.
    #[inline]
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Replace the error message in place.
    ///
    /// Used by D-Bus error stripping (`g_dbus_error_strip_remote_error`)
    /// to remove the `"GDBus.Error:NAME: "` prefix from the message.
    /// Not part of the upstream public `GError` API (upstream pokes
    /// `error->message` directly) but exposed here so D-Bus error
    /// handling can be implemented outside the `error` module.
    pub fn set_message(&mut self, message: impl Into<String>) {
        self.message = message.into();
    }

    /// Whether this error matches `domain` and `code` (`g_error_matches`).
    #[inline]
    #[must_use]
    pub fn matches(&self, domain: Quark, code: i32) -> bool {
        error_matches(self, domain, code)
    }
}

/// Creates a new error with a formatted message (`g_error_new`).
#[must_use]
pub fn error_new(domain: Quark, code: i32, message: impl fmt::Display) -> Error {
    Error::new(domain, code, message)
}

/// Creates a new error with a literal message (`g_error_new_literal`).
#[must_use]
pub fn error_new_literal(domain: Quark, code: i32, message: impl Into<String>) -> Error {
    Error::new_literal(domain, code, message)
}

/// Frees an error (`g_error_free`).
pub fn error_free(error: Error) {
    drop(error);
}

/// Copies an error (`g_error_copy`).
#[must_use]
pub fn error_copy(error: &Error) -> Error {
    error.clone()
}

/// Returns whether `error` matches `domain` and `code` (`g_error_matches`).
#[must_use]
pub fn error_matches(error: &Error, domain: Quark, code: i32) -> bool {
    error.domain == domain && error.code == code
}

/// Sets `*err` when it is `None`; warns when already set (`g_set_error`).
pub fn set_error(
    err: Option<&mut Option<Error>>,
    domain: Quark,
    code: i32,
    message: impl fmt::Display,
) {
    let Some(slot) = err else {
        return;
    };

    if slot.is_none() {
        *slot = Some(Error::new(domain, code, message));
    } else {
        warning(&format!("{ERROR_OVERWRITTEN_WARNING}{}", message));
    }
}

/// Sets `*err` with a literal message (`g_set_error_literal`).
pub fn set_error_literal(
    err: Option<&mut Option<Error>>,
    domain: Quark,
    code: i32,
    message: impl Into<String>,
) {
    let Some(slot) = err else {
        return;
    };

    let message = message.into();
    if slot.is_none() {
        *slot = Some(Error::new_literal(domain, code, message));
    } else {
        warning(&format!("{ERROR_OVERWRITTEN_WARNING}{message}"));
    }
}

/// Moves `src` into `*dest`, or drops `src` when `dest` is `None` (`g_propagate_error`).
pub fn propagate_error(dest: Option<&mut Option<Error>>, src: Error) {
    let Some(slot) = dest else {
        drop(src);
        return;
    };

    if slot.is_none() {
        *slot = Some(src);
    } else {
        warning(&format!("{ERROR_OVERWRITTEN_WARNING}{}", src.message()));
        drop(src);
    }
}

/// Clears `*err` when present (`g_clear_error`).
pub fn clear_error(err: Option<&mut Option<Error>>) {
    if let Some(slot) = err {
        slot.take();
    }
}

/// Prefixes the formatted string to an existing error message (`g_prefix_error`).
pub fn prefix_error(err: Option<&mut Option<Error>>, prefix: impl fmt::Display) {
    if let Some(error) = err.and_then(|slot| slot.as_mut()) {
        let mut message = prefix.to_string();
        message.push_str(error.message());
        error.message = message;
    }
}

/// Prefixes a literal string to an existing error message (`g_prefix_error_literal`).
pub fn prefix_error_literal(err: Option<&mut Option<Error>>, prefix: &str) {
    if let Some(error) = err.and_then(|slot| slot.as_mut()) {
        let mut message = String::with_capacity(prefix.len() + error.message.len());
        message.push_str(prefix);
        message.push_str(error.message());
        error.message = message;
    }
}

/// Propagates `src` into `dest` and prefixes the message (`g_propagate_prefixed_error`).
pub fn propagate_prefixed_error(
    dest: Option<&mut Option<Error>>,
    src: Error,
    prefix: impl fmt::Display,
) {
    match dest {
        None => drop(src),
        Some(slot) => {
            if slot.is_none() {
                *slot = Some(src);
                prefix_error(Some(slot), prefix);
            } else {
                warning(&format!("{ERROR_OVERWRITTEN_WARNING}{}", src.message()));
                drop(src);
            }
        }
    }
}

/// Takes ownership of the error in `err`, leaving `None` (`Rust helper`).
#[must_use]
pub fn steal_error(err: &mut Option<Error>) -> Option<Error> {
    err.take()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages::{
        log_default_handler, log_set_default_handler, messages_test_lock, LogLevelFlags,
    };
    use crate::quark::quark_from_static_string;
    use std::cell::RefCell;

    fn markup_error_quark() -> Quark {
        quark_from_static_string(Some("g-markup-error-quark"))
    }

    thread_local! {
        static CAPTURED_WARNINGS: RefCell<Vec<String>> = RefCell::new(Vec::new());
    }

    fn capture_warning_handler(
        _domain: Option<&str>,
        _level: LogLevelFlags,
        message: &str,
        _user_data: *mut std::ffi::c_void,
    ) {
        CAPTURED_WARNINGS.with(|warnings| warnings.borrow_mut().push(message.to_owned()));
    }

    struct LogHandlerGuard {
        _lock: std::sync::MutexGuard<'static, ()>,
    }

    impl LogHandlerGuard {
        fn new() -> Self {
            let _lock = messages_test_lock();
            CAPTURED_WARNINGS.with(|warnings| warnings.borrow_mut().clear());
            log_set_default_handler(capture_warning_handler, std::ptr::null_mut());
            Self { _lock }
        }
    }

    impl Drop for LogHandlerGuard {
        fn drop(&mut self) {
            log_set_default_handler(log_default_handler, std::ptr::null_mut());
            CAPTURED_WARNINGS.with(|warnings| warnings.borrow_mut().clear());
        }
    }

    fn reset_warnings() -> LogHandlerGuard {
        LogHandlerGuard::new()
    }

    #[test]
    fn new_literal_stores_message_verbatim() {
        let err = error_new_literal(markup_error_quark(), 1, "%s %d %x");
        assert_eq!(err.domain(), markup_error_quark());
        assert_eq!(err.code(), 1);
        assert_eq!(err.message(), "%s %d %x");
    }

    #[test]
    fn new_formats_message() {
        let err = error_new(markup_error_quark(), 2, format!("Oh no! {}", 42));
        assert_eq!(err.message(), "Oh no! 42");
    }

    #[test]
    fn matches_domain_and_code() {
        let err = error_new_literal(markup_error_quark(), 3, "bla");
        assert!(error_matches(&err, markup_error_quark(), 3));
        assert!(!error_matches(&err, markup_error_quark(), 4));
        assert!(!error_matches(&err, 0, 3));
    }

    #[test]
    fn copy_duplicates_error() {
        let err = error_new_literal(markup_error_quark(), 1, "%s %d %x");
        let copy = error_copy(&err);
        assert_eq!(copy, err);
        assert!(!std::ptr::eq(
            copy.message().as_ptr(),
            err.message().as_ptr()
        ));
    }

    #[test]
    fn set_error_literal_assigns_when_empty() {
        let mut err: Option<Error> = None;
        set_error_literal(Some(&mut err), markup_error_quark(), 1, "%s %d %x");
        let err = err.unwrap();
        assert!(err.matches(markup_error_quark(), 1));
        assert_eq!(err.message(), "%s %d %x");
    }

    #[test]
    fn set_error_warns_on_overwrite() {
        let _guard = reset_warnings();
        let mut err = Some(error_new_literal(markup_error_quark(), 1, "bla"));
        set_error_literal(Some(&mut err), markup_error_quark(), 2, "new");
        assert!(err.unwrap().matches(markup_error_quark(), 1));
        let warnings = CAPTURED_WARNINGS.with(|w| w.borrow().clone());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("set over the top"));
    }

    #[test]
    fn propagate_error_moves_into_empty_slot() {
        let mut dest: Option<Error> = None;
        let src = error_new_literal(markup_error_quark(), 2, "src");
        propagate_error(Some(&mut dest), src);
        assert_eq!(dest.unwrap().message(), "src");
    }

    #[test]
    fn propagate_error_warns_on_overwrite() {
        let _guard = reset_warnings();
        let mut dest = Some(error_new_literal(markup_error_quark(), 1, "dest"));
        let src = error_new_literal(markup_error_quark(), 2, "src");
        propagate_error(Some(&mut dest), src);
        assert_eq!(dest.unwrap().message(), "dest");
        let warnings = CAPTURED_WARNINGS.with(|w| w.borrow().clone());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("src"));
    }

    #[test]
    fn propagate_error_drops_src_when_dest_is_none() {
        let src = error_new_literal(markup_error_quark(), 1, "gone");
        propagate_error(None, src);
    }

    #[test]
    fn clear_error_clears_present_value() {
        let mut err = Some(error_new_literal(markup_error_quark(), 1, "bla"));
        clear_error(Some(&mut err));
        assert!(err.is_none());
        clear_error(Some(&mut err));
    }

    #[test]
    fn prefix_error_noop_on_null() {
        prefix_error(None, "foo: ");
        let mut err: Option<Error> = None;
        prefix_error(Some(&mut err), "foo: ");
        assert!(err.is_none());
    }

    #[test]
    fn prefix_error_formats_prefix() {
        let mut err = Some(error_new_literal(markup_error_quark(), 1, "bla"));
        prefix_error(Some(&mut err), format!("foo {} {}: ", 1, "two"));
        assert_eq!(err.unwrap().message(), "foo 1 two: bla");
    }

    #[test]
    fn prefix_error_literal_prefixes_message() {
        let mut err = Some(error_new_literal(markup_error_quark(), 1, "bla"));
        prefix_error_literal(Some(&mut err), "foo: ");
        assert_eq!(err.unwrap().message(), "foo: bla");
    }

    #[test]
    fn propagate_prefixed_error_combines_operations() {
        let mut dest: Option<Error> = None;
        let src = error_new_literal(markup_error_quark(), 1, "bla");
        propagate_prefixed_error(Some(&mut dest), src, format!("foo {} {}: ", 1, "two"));
        assert_eq!(dest.unwrap().message(), "foo 1 two: bla");
    }

    #[test]
    fn propagate_prefixed_error_frees_src_when_dest_null() {
        let src = error_new_literal(markup_error_quark(), 1, "bla");
        propagate_prefixed_error(None, src, "foo: ");
    }

    #[test]
    fn steal_error_takes_ownership() {
        let mut err = Some(error_new_literal(markup_error_quark(), 1, "take"));
        let stolen = steal_error(&mut err).unwrap();
        assert!(err.is_none());
        assert_eq!(stolen.message(), "take");
    }

    #[test]
    fn steal_error_returns_none_for_empty_slot() {
        let mut err: Option<Error> = None;
        assert!(steal_error(&mut err).is_none());
    }

    #[test]
    fn error_free_drops_error() {
        error_free(error_new_literal(markup_error_quark(), 1, "free"));
    }

    #[test]
    fn set_error_formats_message() {
        let mut err: Option<Error> = None;
        set_error(
            Some(&mut err),
            markup_error_quark(),
            1,
            format!("code {}", 7),
        );
        assert_eq!(err.unwrap().message(), "code 7");
    }
}
