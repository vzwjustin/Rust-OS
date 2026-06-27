//! Charset helpers matching `gcharset.h` / `gcharset.c`.
//!
//! In a `no_std` environment there is no locale system, so these functions
//! return sensible defaults. `g_get_locale_variants` is a pure string
//! operation and is fully implemented.

use crate::prelude::*;

/// Returns the current character set.
///
/// In `no_std`, always returns `"UTF-8"`.
pub fn get_charset() -> &'static str {
    "UTF-8"
}

/// Returns the codeset for the current locale (`g_get_codeset`).
///
/// In `no_std`, always returns `"UTF-8"`.
pub fn get_codeset() -> String {
    "UTF-8".to_owned()
}

/// Returns the charset used by the console (`g_get_console_charset`).
///
/// In `no_std`, always returns `"UTF-8"`.
pub fn get_console_charset() -> &'static str {
    "UTF-8"
}

/// Returns language names for the current locale (`g_get_language_names`).
///
/// In `no_std`, returns a single entry `"C"` (the POSIX default locale).
pub fn get_language_names() -> Vec<&'static str> {
    vec!["C"]
}

/// Returns locale variants for `locale` (`g_get_locale_variants`).
///
/// Given a locale like `"en_US.UTF-8"`, returns `["en_US.UTF-8", "en_US", "en"]`.
pub fn get_locale_variants(locale: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = locale.to_owned();

    result.push(current.clone());

    // Strip encoding suffix (everything after `.`)
    if let Some(dot) = current.find('.') {
        current.truncate(dot);
        result.push(current.clone());
    }

    // Strip variant (everything after `@`)
    if let Some(at) = current.find('@') {
        current.truncate(at);
        result.push(current.clone());
    }

    // Strip territory (everything after `_`)
    if let Some(underscore) = current.find('_') {
        current.truncate(underscore);
        result.push(current.clone());
    }

    // Remove duplicates while preserving order
    let mut seen = Vec::new();
    result.retain(|item| {
        if seen.contains(item) {
            false
        } else {
            seen.push(item.clone());
            true
        }
    });

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn charset_defaults() {
        assert_eq!(get_charset(), "UTF-8");
        assert_eq!(get_codeset(), "UTF-8");
        assert_eq!(get_console_charset(), "UTF-8");
    }

    #[test]
    fn language_names_default() {
        assert_eq!(get_language_names(), vec!["C"]);
    }

    #[test]
    fn locale_variants_full() {
        let variants = get_locale_variants("en_US.UTF-8");
        assert_eq!(variants, vec!["en_US.UTF-8", "en_US", "en"]);
    }

    #[test]
    fn locale_variants_no_encoding() {
        let variants = get_locale_variants("en_US");
        assert_eq!(variants, vec!["en_US", "en"]);
    }

    #[test]
    fn locale_variants_language_only() {
        let variants = get_locale_variants("en");
        assert_eq!(variants, vec!["en"]);
    }

    #[test]
    fn locale_variants_with_variant() {
        let variants = get_locale_variants("en_US@latin");
        assert_eq!(variants, vec!["en_US@latin", "en_US", "en"]);
    }
}
