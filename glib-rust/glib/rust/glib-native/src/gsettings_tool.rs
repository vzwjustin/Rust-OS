//! gsettings-tool matching `gio/gsettings-tool.c`.
//!
//! Query and modify GSettings keys from the command line.

use crate::gsettings::Settings;
use crate::gsettingsschema::SettingsSchemaSource;
use crate::prelude::*;

/// Validate a GSettings path (must start and end with `/`).
pub fn check_path(path: &str) -> Result<(), String> {
    if path.is_empty() {
        return Err("empty path".into());
    }
    if !path.starts_with('/') {
        return Err("path must begin with /".into());
    }
    if !path.ends_with('/') {
        return Err("path must end with /".into());
    }
    if path.contains("//") {
        return Err("path must not contain //".into());
    }
    Ok(())
}

/// Get a setting value as printable text.
pub fn get_value(settings: &Settings, key: &str) -> String {
    if settings.get_string(key).len() > 0 || settings.list_keys().contains(&key.to_string()) {
        return settings.get_string(key);
    }
    if settings.get_boolean(key) || settings.list_keys().iter().any(|k| k == key) {
        return settings.get_boolean(key).to_string();
    }
    settings.get_int(key).to_string()
}

/// Set a setting from a string value (best-effort type coercion).
pub fn set_value(settings: &Settings, key: &str, value: &str) -> Result<(), String> {
    if value.eq_ignore_ascii_case("true") || value.eq_ignore_ascii_case("false") {
        settings.set_boolean(key, value.eq_ignore_ascii_case("true"));
    } else if let Ok(i) = value.parse::<i32>() {
        settings.set_int(key, i);
    } else {
        settings.set_string(key, value);
    }
    Ok(())
}

/// List keys in a schema via [`SettingsSchemaSource`].
pub fn list_schema_keys(
    source: &SettingsSchemaSource,
    schema_id: &str,
) -> Result<Vec<String>, String> {
    let schema = source
        .lookup(schema_id, true)
        .ok_or_else(|| format!("no such schema {schema_id}"))?;
    Ok(schema.list_keys())
}

/// Entry point for `gsettings`.
pub fn run(args: &[&str]) -> i32 {
    if args.is_empty() || args[0] == "help" || args.contains(&"--help") {
        gwarn!("Usage: gsettings COMMAND SCHEMA [KEY] [VALUE]");
        return if args.is_empty() { 1 } else { 0 };
    }
    match args[0] {
        "get" => {
            if args.len() < 3 {
                return 1;
            }
            let _settings = Settings::new(args[1]);
            gwarn!("{}", get_value(&settings, args[2]));
            0
        }
        "set" => {
            if args.len() < 4 {
                return 1;
            }
            let settings = Settings::new(args[1]);
            match set_value(&settings, args[2], args[3]) {
                Ok(()) => 0,
                Err(_msg) => {
                    gwarn!("{msg}");
                    1
                }
            }
        }
        "list-keys" => {
            if args.len() < 2 {
                return 1;
            }
            let source = SettingsSchemaSource::new();
            match list_schema_keys(&source, args[1]) {
                Ok(keys) => {
                    for _k in keys {
                        gwarn!("{k}");
                    }
                    0
                }
                Err(_msg) => {
                    gwarn!("{msg}");
                    1
                }
            }
        }
        "list-schemas" => {
            let source = SettingsSchemaSource::new();
            for _id in source.list_schemas() {
                gwarn!("{id}");
            }
            0
        }
        "reset" => {
            if args.len() < 3 {
                return 1;
            }
            let settings = Settings::new(args[1]);
            settings.reset(args[2]);
            0
        }
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_validation() {
        assert!(check_path("/org/gnome/").is_ok());
        assert!(check_path("bad").is_err());
        assert!(check_path("/org//gnome/").is_err());
    }

    #[test]
    fn set_and_get_string() {
        let s = Settings::new("org.test");
        set_value(&s, "name", "hello").unwrap();
        assert_eq!(get_value(&s, "name"), "hello");
    }

    #[test]
    fn list_keys_missing_schema() {
        let source = SettingsSchemaSource::new();
        assert!(list_schema_keys(&source, "missing").is_err());
    }

    #[test]
    fn run_get_missing_args() {
        assert_eq!(run(&["get"]), 1);
    }
}
