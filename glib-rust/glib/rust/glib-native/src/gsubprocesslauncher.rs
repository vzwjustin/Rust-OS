//! GSubprocessLauncher matching `gio/gsubprocesslauncher.h`.
//!
//! Builder for `GSubprocess`; configures environment, working directory, and
//! stream redirection flags before spawning.  In this `no_std` port no real
//! process is spawned — `spawn` returns a `Subprocess` in the Running state.
//!
//! No_std compatible using `alloc`.

use crate::error::Error;
use crate::gsubprocess::{Subprocess, SubprocessFlags};
use alloc::string::String;
use alloc::vec::Vec;

/// Builder for `Subprocess` (`GSubprocessLauncher`).
pub struct SubprocessLauncher {
    flags: SubprocessFlags,
    env: Vec<(String, String)>,
    cwd: Option<String>,
}

impl SubprocessLauncher {
    /// Creates a new launcher with the given flags.
    ///
    /// Mirrors `g_subprocess_launcher_new`.
    pub fn new(flags: SubprocessFlags) -> Self {
        Self {
            flags,
            env: Vec::new(),
            cwd: None,
        }
    }

    /// Sets the working directory for spawned processes.
    ///
    /// Mirrors `g_subprocess_launcher_set_cwd`.
    pub fn set_cwd(&mut self, cwd: &str) {
        self.cwd = Some(cwd.into());
    }

    /// Gets the working directory override (if any).
    pub fn get_cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    /// Adds or overrides an environment variable.
    ///
    /// Mirrors `g_subprocess_launcher_setenv`.
    pub fn setenv(&mut self, key: &str, value: &str, overwrite: bool) {
        if !overwrite {
            if self.env.iter().any(|(k, _)| k == key) {
                return;
            }
        } else {
            self.env.retain(|(k, _)| k != key);
        }
        self.env.push((key.into(), value.into()));
    }

    /// Removes an environment variable.
    ///
    /// Mirrors `g_subprocess_launcher_unsetenv`.
    pub fn unsetenv(&mut self, key: &str) {
        self.env.retain(|(k, _)| k != key);
    }

    /// Gets the value of an env var set on this launcher (if any).
    pub fn getenv(&self, key: &str) -> Option<&str> {
        self.env
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// Sets the subprocess flags.
    ///
    /// Mirrors `g_subprocess_launcher_set_flags`.
    pub fn set_flags(&mut self, flags: SubprocessFlags) {
        self.flags = flags;
    }

    /// Gets the current flags.
    pub fn get_flags(&self) -> SubprocessFlags {
        self.flags
    }

    /// Spawns a subprocess with the given argument vector.
    ///
    /// Mirrors `g_subprocess_launcher_spawnv`.
    pub fn spawn(&self, argv: Vec<String>) -> Result<Subprocess, Error> {
        Subprocess::new(argv, self.flags)
    }
}

impl Default for SubprocessLauncher {
    fn default() -> Self {
        Self::new(SubprocessFlags::NONE)
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn argv(args: &[&str]) -> Vec<String> {
        args.iter().map(|s| String::from(*s)).collect()
    }

    #[test]
    fn test_new_defaults() {
        let l = SubprocessLauncher::new(SubprocessFlags::NONE);
        assert_eq!(l.get_flags(), SubprocessFlags::NONE);
        assert!(l.get_cwd().is_none());
    }

    #[test]
    fn test_set_cwd() {
        let mut l = SubprocessLauncher::new(SubprocessFlags::NONE);
        l.set_cwd("/tmp");
        assert_eq!(l.get_cwd(), Some("/tmp"));
    }

    #[test]
    fn test_setenv_getenv() {
        let mut l = SubprocessLauncher::new(SubprocessFlags::NONE);
        l.setenv("HOME", "/root", true);
        assert_eq!(l.getenv("HOME"), Some("/root"));
    }

    #[test]
    fn test_setenv_no_overwrite() {
        let mut l = SubprocessLauncher::new(SubprocessFlags::NONE);
        l.setenv("KEY", "first", true);
        l.setenv("KEY", "second", false);
        assert_eq!(l.getenv("KEY"), Some("first"));
    }

    #[test]
    fn test_setenv_overwrite() {
        let mut l = SubprocessLauncher::new(SubprocessFlags::NONE);
        l.setenv("KEY", "first", true);
        l.setenv("KEY", "second", true);
        assert_eq!(l.getenv("KEY"), Some("second"));
    }

    #[test]
    fn test_unsetenv() {
        let mut l = SubprocessLauncher::new(SubprocessFlags::NONE);
        l.setenv("KEY", "value", true);
        l.unsetenv("KEY");
        assert!(l.getenv("KEY").is_none());
    }

    #[test]
    fn test_spawn_ok() {
        let l = SubprocessLauncher::new(SubprocessFlags::NONE);
        let p = l.spawn(argv(&["echo", "hello"])).unwrap();
        assert!(p.is_running());
        assert_eq!(p.get_identifier(), "echo");
    }

    #[test]
    fn test_spawn_empty_argv_fails() {
        let l = SubprocessLauncher::new(SubprocessFlags::NONE);
        assert!(l.spawn(vec![]).is_err());
    }

    #[test]
    fn test_default() {
        let l = SubprocessLauncher::default();
        assert_eq!(l.get_flags(), SubprocessFlags::NONE);
    }
}
