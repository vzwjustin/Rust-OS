//! Environment variable management matching `genviron.h` / `genviron.c`.
//!
//! In a `no_std` environment there is no OS-provided environ. This module
//! provides an in-memory environment table backed by `BTreeMap`, plus
//! functions for working with `envp`-style string arrays.

use crate::prelude::*;
use alloc::collections::BTreeMap;
use alloc::string::String;
use spin::mutex::Mutex;
use spin::Once;

/// Global environment table for `g_getenv` / `g_setenv` / `g_unsetenv`.
struct Environ {
    vars: BTreeMap<String, String>,
}

impl Environ {
    fn new() -> Self {
        Self {
            vars: BTreeMap::new(),
        }
    }
}

fn global_environ() -> &'static Mutex<Environ> {
    static ENV: Once<Mutex<Environ>> = Once::new();
    ENV.call_once(|| Mutex::new(Environ::new()))
}

/// Returns the value of `variable` from the global environment (`g_getenv`).
pub fn getenv(variable: &str) -> Option<String> {
    let env = global_environ().lock();
    env.vars.get(variable).cloned()
}

/// Sets `variable` to `value` in the global environment (`g_setenv`).
///
/// If `overwrite` is `false` and the variable already exists, it is not modified.
/// Returns `true` if the variable was set.
pub fn setenv(variable: &str, value: &str, overwrite: bool) -> bool {
    let mut env = global_environ().lock();
    if !overwrite && env.vars.contains_key(variable) {
        return false;
    }
    env.vars.insert(variable.to_owned(), value.to_owned());
    true
}

/// Removes `variable` from the global environment (`g_unsetenv`).
pub fn unsetenv(variable: &str) {
    let mut env = global_environ().lock();
    env.vars.remove(variable);
}

/// Lists all variable names in the global environment (`g_listenv`).
pub fn listenv() -> Vec<String> {
    let env = global_environ().lock();
    env.vars.keys().cloned().collect()
}

/// Returns all environment variables as `KEY=VALUE` strings (`g_get_environ`).
pub fn get_environ() -> Vec<String> {
    let env = global_environ().lock();
    env.vars
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect()
}

/// Looks up `variable` in `envp` (`g_environ_getenv`).
///
/// `envp` is a list of `KEY=VALUE` strings.
pub fn environ_getenv(envp: &[String], variable: &str) -> Option<String> {
    let prefix = format!("{variable}=");
    for entry in envp {
        if let Some(value) = entry.strip_prefix(&prefix) {
            return Some(value.to_owned());
        }
    }
    None
}

/// Sets `variable` to `value` in `envp` (`g_environ_setenv`).
///
/// Returns a new envp with the variable set. If `overwrite` is `false` and
/// the variable already exists, the original envp is returned unchanged.
pub fn environ_setenv(envp: Vec<String>, variable: &str, value: &str, overwrite: bool) -> Vec<String> {
    let prefix = format!("{variable}=");
    let mut result = Vec::new();
    let mut found = false;

    for entry in envp {
        if entry.starts_with(&prefix) {
            if overwrite {
                result.push(format!("{variable}={value}"));
                found = true;
            } else {
                result.push(entry);
                found = true;
            }
        } else {
            result.push(entry);
        }
    }

    if !found {
        result.push(format!("{variable}={value}"));
    }

    result
}

/// Removes `variable` from `envp` (`g_environ_unsetenv`).
///
/// Returns a new envp with the variable removed.
pub fn environ_unsetenv(envp: Vec<String>, variable: &str) -> Vec<String> {
    let prefix = format!("{variable}=");
    envp.into_iter().filter(|e| !e.starts_with(&prefix)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_and_get() {
        // Use unique variable name to avoid interference
        let var = "_RUSTOS_TEST_ENV_VAR_42";
        assert!(setenv(var, "hello", true));
        assert_eq!(getenv(var), Some("hello".to_owned()));
        unsetenv(var);
        assert_eq!(getenv(var), None);
    }

    #[test]
    fn setenv_no_overwrite() {
        let var = "_RUSTOS_TEST_ENV_NO_OVERWRITE";
        assert!(setenv(var, "first", true));
        assert!(!setenv(var, "second", false));
        assert_eq!(getenv(var), Some("first".to_owned()));
        assert!(setenv(var, "third", true));
        assert_eq!(getenv(var), Some("third".to_owned()));
        unsetenv(var);
    }

    #[test]
    fn listenv_and_get_environ() {
        let var1 = "_RUSTOS_TEST_LISTENV_A";
        let var2 = "_RUSTOS_TEST_LISTENV_B";
        setenv(var1, "1", true);
        setenv(var2, "2", true);

        let list = listenv();
        assert!(list.contains(&var1.to_owned()));
        assert!(list.contains(&var2.to_owned()));

        let envp = get_environ();
        assert!(envp.contains(&format!("{var1}=1")));
        assert!(envp.contains(&format!("{var2}=2")));

        unsetenv(var1);
        unsetenv(var2);
    }

    #[test]
    fn environ_getenv_from_array() {
        let envp = vec![
            "PATH=/usr/bin".to_owned(),
            "HOME=/root".to_owned(),
            "SHELL=/bin/sh".to_owned(),
        ];
        assert_eq!(environ_getenv(&envp, "HOME"), Some("/root".to_owned()));
        assert_eq!(environ_getenv(&envp, "PATH"), Some("/usr/bin".to_owned()));
        assert_eq!(environ_getenv(&envp, "NOPE"), None);
    }

    #[test]
    fn environ_setenv_in_array() {
        let envp = vec![
            "PATH=/usr/bin".to_owned(),
            "HOME=/root".to_owned(),
        ];
        let result = environ_setenv(envp, "PATH", "/usr/local/bin", true);
        assert_eq!(environ_getenv(&result, "PATH"), Some("/usr/local/bin".to_owned()));
        assert_eq!(result.len(), 2);

        let envp2 = vec!["PATH=/usr/bin".to_owned()];
        let result2 = environ_setenv(envp2, "PATH", "/new", false);
        assert_eq!(environ_getenv(&result2, "PATH"), Some("/usr/bin".to_owned()));

        let envp3 = vec!["PATH=/usr/bin".to_owned()];
        let result3 = environ_setenv(envp3, "NEW", "value", true);
        assert_eq!(environ_getenv(&result3, "NEW"), Some("value".to_owned()));
        assert_eq!(result3.len(), 2);
    }

    #[test]
    fn environ_unsetenv_in_array() {
        let envp = vec![
            "PATH=/usr/bin".to_owned(),
            "HOME=/root".to_owned(),
            "SHELL=/bin/sh".to_owned(),
        ];
        let result = environ_unsetenv(envp, "HOME");
        assert_eq!(result.len(), 2);
        assert_eq!(environ_getenv(&result, "HOME"), None);
        assert_eq!(environ_getenv(&result, "PATH"), Some("/usr/bin".to_owned()));
    }
}
