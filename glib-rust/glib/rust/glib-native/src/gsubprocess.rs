//! GSubprocess matching `gio/gsubprocess.h`.
//!
//! Represents a child process. In this `no_std` port the process is modelled
//! as a stored command-line with a simulated exit status so the API surface
//! is fully testable without spawning real processes.
//!
//! No_std compatible using `alloc`.

use crate::error::Error;
use crate::quark::quark_from_string;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

fn spawn_error_quark() -> u32 {
    quark_from_string(Some("g-spawn-error-quark"))
}

/// Flags controlling how a subprocess is spawned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SubprocessFlags(u32);

impl SubprocessFlags {
    pub const NONE: Self = Self(0);
    pub const STDIN_PIPE: Self = Self(1 << 0);
    pub const STDIN_INHERIT: Self = Self(1 << 1);
    pub const STDOUT_PIPE: Self = Self(1 << 2);
    pub const STDOUT_SILENCE: Self = Self(1 << 3);
    pub const STDERR_PIPE: Self = Self(1 << 4);
    pub const STDERR_SILENCE: Self = Self(1 << 5);
    pub const STDERR_MERGE: Self = Self(1 << 6);
    pub const INHERIT_FDS: Self = Self(1 << 7);

    pub fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn bits(self) -> u32 {
        self.0
    }
}

impl core::ops::BitOr for SubprocessFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// The simulated exit state of a subprocess.
#[derive(Debug, Clone)]
enum ExitState {
    Running,
    Exited(i32),
}

/// A child process (`GSubprocess`).
pub struct Subprocess {
    argv: Vec<String>,
    flags: SubprocessFlags,
    state: Mutex<ExitState>,
}

impl Subprocess {
    /// Creates a subprocess from an argument vector.
    ///
    /// Mirrors `g_subprocess_newv`.
    pub fn new(argv: Vec<String>, flags: SubprocessFlags) -> Result<Self, Error> {
        if argv.is_empty() {
            return Err(Error::new(spawn_error_quark(), 1, "argv must not be empty"));
        }
        Ok(Self {
            argv,
            flags,
            state: Mutex::new(ExitState::Running),
        })
    }

    /// Returns the program name (argv[0]).
    pub fn get_identifier(&self) -> &str {
        &self.argv[0]
    }

    /// Returns the full argv slice.
    pub fn get_argv(&self) -> &[String] {
        &self.argv
    }

    /// Returns the flags this subprocess was spawned with.
    pub fn get_flags(&self) -> SubprocessFlags {
        self.flags
    }

    /// Returns true if the process is still running.
    pub fn is_running(&self) -> bool {
        matches!(*self.state.lock(), ExitState::Running)
    }

    /// Simulates the process exiting with a given status code.
    pub fn simulate_exit(&self, status: i32) {
        *self.state.lock() = ExitState::Exited(status);
    }

    /// Waits for the process to finish, returning the exit status.
    ///
    /// Mirrors `g_subprocess_wait`.
    pub fn wait(&self) -> Result<i32, Error> {
        match *self.state.lock() {
            ExitState::Running => Err(Error::new(spawn_error_quark(), 2, "process still running")),
            ExitState::Exited(code) => Ok(code),
        }
    }

    /// Returns true if the process exited normally (exit code 0).
    ///
    /// Mirrors `g_subprocess_get_successful`.
    pub fn get_successful(&self) -> bool {
        matches!(*self.state.lock(), ExitState::Exited(0))
    }

    /// Returns the exit status, or None if still running.
    ///
    /// Mirrors `g_subprocess_get_exit_status`.
    pub fn get_exit_status(&self) -> Option<i32> {
        match *self.state.lock() {
            ExitState::Running => None,
            ExitState::Exited(code) => Some(code),
        }
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
    fn test_new_empty_argv_fails() {
        assert!(Subprocess::new(vec![], SubprocessFlags::NONE).is_err());
    }

    #[test]
    fn test_new_ok() {
        let p = Subprocess::new(argv(&["ls", "-la"]), SubprocessFlags::NONE).unwrap();
        assert_eq!(p.get_identifier(), "ls");
        assert!(p.is_running());
    }

    #[test]
    fn test_get_argv() {
        let p = Subprocess::new(argv(&["echo", "hi"]), SubprocessFlags::NONE).unwrap();
        assert_eq!(p.get_argv(), &[String::from("echo"), String::from("hi")]);
    }

    #[test]
    fn test_simulate_exit_success() {
        let p = Subprocess::new(argv(&["true"]), SubprocessFlags::NONE).unwrap();
        p.simulate_exit(0);
        assert!(!p.is_running());
        assert!(p.get_successful());
        assert_eq!(p.get_exit_status(), Some(0));
        assert_eq!(p.wait().unwrap(), 0);
    }

    #[test]
    fn test_simulate_exit_failure() {
        let p = Subprocess::new(argv(&["false"]), SubprocessFlags::NONE).unwrap();
        p.simulate_exit(1);
        assert!(!p.get_successful());
        assert_eq!(p.get_exit_status(), Some(1));
    }

    #[test]
    fn test_wait_running_fails() {
        let p = Subprocess::new(argv(&["sleep"]), SubprocessFlags::NONE).unwrap();
        assert!(p.wait().is_err());
    }

    #[test]
    fn test_flags_bits() {
        let f = SubprocessFlags::STDOUT_PIPE | SubprocessFlags::STDERR_PIPE;
        assert!(f.contains(SubprocessFlags::STDOUT_PIPE));
        assert!(f.contains(SubprocessFlags::STDERR_PIPE));
        assert!(!f.contains(SubprocessFlags::STDIN_PIPE));
    }

    #[test]
    fn test_get_flags() {
        let p = Subprocess::new(argv(&["cat"]), SubprocessFlags::STDOUT_PIPE).unwrap();
        assert_eq!(p.get_flags(), SubprocessFlags::STDOUT_PIPE);
    }
}
