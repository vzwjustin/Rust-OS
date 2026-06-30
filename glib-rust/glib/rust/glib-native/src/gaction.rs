//! GAction interface matching `gio/gaction.h` / `gio/gaction.c`.
//!
//! Upstream `GAction` is a `GInterface` representing a named action with
//! optional parameter type, state type, state hint, enabled flag, and
//! state. We port the interface as a Rust trait and also port the pure-logic
//! utility functions `g_action_name_is_valid`, `g_action_parse_detailed_name`,
//! and `g_action_print_detailed_name`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::variant::Variant;
use crate::varianttype::VariantType;
use alloc::string::{String, ToString};

/// Trait for actions (`GAction`).
pub trait Action {
    /// Gets the name of the action.
    fn get_name(&self) -> &str;

    /// Gets the parameter type, or `None` if the action takes no parameter.
    fn get_parameter_type(&self) -> Option<&VariantType>;

    /// Gets the state type, or `None` if the action is stateless.
    fn get_state_type(&self) -> Option<&VariantType>;

    /// Gets the state hint as a `Variant`, or `None`.
    fn get_state_hint(&self) -> Option<Variant>;

    /// Gets whether the action is enabled.
    fn get_enabled(&self) -> bool;

    /// Gets the current state, or `None` if stateless.
    fn get_state(&self) -> Option<Variant>;

    /// Requests a state change.
    fn change_state(&self, value: Variant);

    /// Activates the action with an optional parameter.
    fn activate(&self, parameter: Option<Variant>);
}

/// Checks if an action name is valid.
///
/// Valid action names contain only alphanumeric characters, '-', and '.',
/// and must not start with a digit.
///
/// Mirrors `g_action_name_is_valid`.
pub fn action_name_is_valid(action_name: &str) -> bool {
    if action_name.is_empty() {
        return false;
    }
    let bytes = action_name.as_bytes();
    if bytes[0].is_ascii_digit() {
        return false;
    }
    bytes
        .iter()
        .all(|&b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
}

/// Parses a detailed action name into the action name and optional target value.
///
/// Format: `"action_name"` or `"action_name(target)"`.
///
/// Mirrors `g_action_parse_detailed_name`.
pub fn action_parse_detailed_name(detailed_name: &str) -> Result<(String, Option<Variant>), Error> {
    if detailed_name.is_empty() {
        return Err(Error::new(
            crate::gioerror::io_error_quark(),
            crate::gioerror::IOErrorEnum::InvalidArgument.to_code(),
            "Detailed action name is empty",
        ));
    }

    let open_paren = detailed_name.find('(');

    if open_paren.is_none() {
        if !action_name_is_valid(detailed_name) {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                crate::gioerror::IOErrorEnum::InvalidArgument.to_code(),
                "Invalid action name in detailed name",
            ));
        }
        return Ok((detailed_name.to_string(), None));
    }

    let open_idx = open_paren.unwrap();
    let action_name = &detailed_name[..open_idx];

    if !action_name_is_valid(action_name) {
        return Err(Error::new(
            crate::gioerror::io_error_quark(),
            crate::gioerror::IOErrorEnum::InvalidArgument.to_code(),
            "Invalid action name in detailed name",
        ));
    }

    let rest = &detailed_name[open_idx..];
    if !rest.starts_with('(') || !rest.ends_with(')') {
        return Err(Error::new(
            crate::gioerror::io_error_quark(),
            crate::gioerror::IOErrorEnum::InvalidArgument.to_code(),
            "Malformed target in detailed name",
        ));
    }

    let target_str = &rest[1..rest.len() - 1];

    if target_str.is_empty() {
        return Err(Error::new(
            crate::gioerror::io_error_quark(),
            crate::gioerror::IOErrorEnum::InvalidArgument.to_code(),
            "Empty target in detailed name",
        ));
    }

    // Parse the target as a GVariant. For simplicity, we handle basic types.
    let target = parse_target_variant(target_str)?;

    Ok((action_name.to_string(), Some(target)))
}

/// Formats an action name and optional target value into a detailed name.
///
/// Mirrors `g_action_print_detailed_name`.
pub fn action_print_detailed_name(action_name: &str, target: Option<&Variant>) -> String {
    match target {
        None => action_name.to_string(),
        Some(v) => {
            let target_str = format_variant_for_detailed_name(v);
            format!("{}({})", action_name, target_str)
        }
    }
}

fn parse_target_variant(s: &str) -> Result<Variant, Error> {
    // Try to parse as string (quoted with single quotes)
    if s.len() >= 2 && s.starts_with('\'') && s.ends_with('\'') {
        let inner = &s[1..s.len() - 1];
        return Ok(Variant::new_string(inner));
    }

    // Try to parse as integer
    if let Ok(n) = s.parse::<i64>() {
        return Ok(Variant::new_int64(n));
    }

    // Try to parse as unsigned integer
    if let Ok(n) = s.parse::<u32>() {
        return Ok(Variant::new_uint32(n));
    }

    // Try boolean
    if s == "true" {
        return Ok(Variant::new_boolean(true));
    }
    if s == "false" {
        return Ok(Variant::new_boolean(false));
    }

    // Fallback: treat as string without quotes
    Ok(Variant::new_string(s))
}

fn format_variant_for_detailed_name(v: &Variant) -> String {
    match v.type_string() {
        "s" => format!("'{}'", v.get_string()),
        "b" => {
            if v.get_boolean() {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        "x" => v.get_int64().to_string(),
        "u" => v.get_uint32().to_string(),
        "i" => v.get_int32().to_string(),
        "t" => v.get_uint64().to_string(),
        "d" => v.get_double().to_string(),
        _ => v.print(false),
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_name_is_valid_simple() {
        assert!(action_name_is_valid("open"));
        assert!(action_name_is_valid("file.save"));
        assert!(action_name_is_valid("edit-copy"));
        assert!(action_name_is_valid("action123"));
    }

    #[test]
    fn test_action_name_is_valid_invalid() {
        assert!(!action_name_is_valid(""));
        assert!(!action_name_is_valid("1action"));
        assert!(!action_name_is_valid("action name"));
        assert!(!action_name_is_valid("action!"));
        assert!(!action_name_is_valid("action/"));
    }

    #[test]
    fn test_parse_detailed_name_no_target() {
        let (name, target) = action_parse_detailed_name("open").unwrap();
        assert_eq!(name, "open");
        assert!(target.is_none());
    }

    #[test]
    fn test_parse_detailed_name_with_string_target() {
        let (name, target) = action_parse_detailed_name("open('file.txt')").unwrap();
        assert_eq!(name, "open");
        assert!(target.is_some());
    }

    #[test]
    fn test_parse_detailed_name_with_int_target() {
        let (name, target) = action_parse_detailed_name("scroll(5)").unwrap();
        assert_eq!(name, "scroll");
        assert!(target.is_some());
    }

    #[test]
    fn test_parse_detailed_name_with_bool_target() {
        let (name, target) = action_parse_detailed_name("set(true)").unwrap();
        assert_eq!(name, "set");
        assert!(target.is_some());
    }

    #[test]
    fn test_parse_detailed_name_invalid() {
        assert!(action_parse_detailed_name("").is_err());
        assert!(action_parse_detailed_name("1bad(5)").is_err());
    }

    #[test]
    fn test_parse_detailed_name_bad_is_valid() {
        // "bad" contains only alphanumeric chars, so it's valid
        let (name, target) = action_parse_detailed_name("bad").unwrap();
        assert_eq!(name, "bad");
        assert!(target.is_none());
    }

    #[test]
    fn test_print_detailed_name_no_target() {
        let s = action_print_detailed_name("open", None);
        assert_eq!(s, "open");
    }

    #[test]
    fn test_print_detailed_name_with_string_target() {
        let v = Variant::new_string("file.txt");
        let s = action_print_detailed_name("open", Some(&v));
        assert_eq!(s, "open('file.txt')");
    }

    #[test]
    fn test_print_detailed_name_with_int_target() {
        let v = Variant::new_int64(42);
        let s = action_print_detailed_name("scroll", Some(&v));
        assert_eq!(s, "scroll(42)");
    }

    #[test]
    fn test_print_detailed_name_with_bool_target() {
        let v = Variant::new_boolean(true);
        let s = action_print_detailed_name("set", Some(&v));
        assert_eq!(s, "set(true)");
    }

    #[test]
    fn test_round_trip_print_parse() {
        let v = Variant::new_string("test");
        let printed = action_print_detailed_name("action", Some(&v));
        let (name, target) = action_parse_detailed_name(&printed).unwrap();
        assert_eq!(name, "action");
        assert!(target.is_some());
    }
}
