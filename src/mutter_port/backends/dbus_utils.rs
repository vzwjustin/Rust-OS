//! dbus_utils - helpers for building escaped D-Bus object paths.
//!
//! Ported from GNOME Mutter's src/backends/meta-dbus-utils.c. This is pure string
//! logic (the same escaping convention used by systemd / tp-glib) with no external
//! dependencies, so it is ported faithfully.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-dbus-utils.c

use alloc::format;
use alloc::string::{String, ToString};

/// Mirrors `_esc_ident_bad`: returns whether `c` must be escaped. Digits are bad
/// only in the first position (D-Bus path components can't start with a digit).
fn esc_ident_bad(c: u8, is_first: bool) -> bool {
    !c.is_ascii_lowercase() && !c.is_ascii_uppercase() && (!c.is_ascii_digit() || is_first)
}

/// Escape a single D-Bus path component. Mirrors `escape_dbus_component`.
///
/// Unsafe characters are replaced with `_XX` (lowercase hex). An empty component
/// becomes `_`.
fn escape_dbus_component(name: &str) -> String {
    // fast path for empty name
    if name.is_empty() {
        return "_".to_string();
    }

    let bytes = name.as_bytes();

    // fast path if it's clean
    let clean = bytes
        .iter()
        .enumerate()
        .all(|(i, &c)| !esc_ident_bad(c, i == 0));
    if clean {
        return name.to_string();
    }

    let mut out = String::new();
    for (i, &c) in bytes.iter().enumerate() {
        if esc_ident_bad(c, i == 0) {
            out.push_str(&format!("_{:02x}", c));
        } else {
            out.push(c as char);
        }
    }
    out
}

/// Build an escaped D-Bus object path from a prefix and a component.
/// Mirrors `get_escaped_dbus_path`.
pub fn get_escaped_dbus_path(prefix: &str, component: &str) -> String {
    let escaped = escape_dbus_component(component);
    format!("{}/{}", prefix, escaped)
}
