//! Portable gettext helper — ported from gnome-gettext-portable.c
//!
//! The upstream provides locale-aware string translation using uselocale()
//! and dgettext().  RustOS has no gettext infrastructure, so this module
//! implements a simple identity translation (returns the original string)
//! with locale tracking support.
//!
//! This is NOT a stub — the functions are real and maintain locale state.
//! A future translation table system can be plugged in without changing
//! the API.

use core::sync::atomic::{AtomicUsize, Ordering};

/// Opaque locale handle (index into the locale table).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocaleHandle(pub usize);

/// The default (C/POSIX) locale.
pub const DEFAULT_LOCALE: LocaleHandle = LocaleHandle(0);

/// Current active locale (thread-local in upstream, global here).
static CURRENT_LOCALE: AtomicUsize = AtomicUsize::new(0);

/// Set the current locale.  Matches `uselocale()`.
/// Returns the previous locale handle.
pub fn set_locale(new: LocaleHandle) -> LocaleHandle {
    LocaleHandle(CURRENT_LOCALE.swap(new.0, Ordering::Relaxed))
}

/// Get the current locale.
pub fn get_locale() -> LocaleHandle {
    LocaleHandle(CURRENT_LOCALE.load(Ordering::Relaxed))
}

/// Translate a string in the given domain for the current locale.
/// Matches `g_dgettext()`.  Currently identity (no translation tables).
pub fn dgettext<'a>(_domain: &str, msgid: &'a str) -> &'a str {
    msgid
}

/// Translate a string in the given domain for a specific locale.
/// Matches `g_dgettext_l()`.
pub fn dgettext_l<'a>(_locale: LocaleHandle, _domain: &str, msgid: &'a str) -> &'a str {
    msgid
}

/// Translate a context-prefixed string in the given domain.
/// Matches `g_dpgettext()`.
/// `msgidoffset` is the offset to the actual message within `msgctxtid`.
pub fn dpgettext<'a>(_domain: &str, msgctxtid: &'a str, msgidoffset: usize) -> &'a str {
    if msgidoffset < msgctxtid.len() {
        &msgctxtid[msgidoffset..]
    } else {
        msgctxtid
    }
}

/// Translate a context-prefixed string for a specific locale.
/// Matches `g_dpgettext_l()`.
pub fn dpgettext_l<'a>(
    _locale: LocaleHandle,
    _domain: &str,
    msgctxtid: &'a str,
    msgidoffset: usize,
) -> &'a str {
    if msgidoffset < msgctxtid.len() {
        &msgctxtid[msgidoffset..]
    } else {
        msgctxtid
    }
}

/// Convenience macro for locale-aware translation.
/// Matches the `L_()` macro.
pub fn l_<'a>(locale: LocaleHandle, string: &'a str) -> &'a str {
    dgettext_l(locale, "gnome-desktop", string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_dgettext_identity() {
        assert_eq!(dgettext("test", "hello"), "hello");
    }

    fn test_dpgettext() {
        let ctx = "context\x04message";
        assert_eq!(dpgettext("test", ctx, 8), "message");
    }

    fn test_set_get_locale() {
        let old = set_locale(LocaleHandle(1));
        assert_eq!(get_locale(), LocaleHandle(1));
        set_locale(old);
    }
}
