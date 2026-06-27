//! Process spawning matching `gspawn.h` / `gspawn.c`.
//!
//! Defines error codes, flags, and types for process spawning.
//! Actual process creation requires OS support (fork/exec) and is
//! deferred to a platform abstraction layer.
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;

/// Spawn error codes (`GSpawnError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SpawnError {
    Fork,
    Read,
    Chdir,
    Acces,
    Perm,
    TooBig,
    Noexec,
    Nametoolong,
    Noent,
    Nomem,
    Notdir,
    Loop,
    Txtbusy,
    Io,
    Nfile,
    Mfile,
    Inval,
    Isdir,
    Libbad,
    Failed,
}

impl SpawnError {
    /// Get the numeric error code.
    pub fn to_code(self) -> i32 {
        match self {
            SpawnError::Fork => 0,
            SpawnError::Read => 1,
            SpawnError::Chdir => 2,
            SpawnError::Acces => 3,
            SpawnError::Perm => 4,
            SpawnError::TooBig => 5,
            SpawnError::Noexec => 6,
            SpawnError::Nametoolong => 7,
            SpawnError::Noent => 8,
            SpawnError::Nomem => 9,
            SpawnError::Notdir => 10,
            SpawnError::Loop => 11,
            SpawnError::Txtbusy => 12,
            SpawnError::Io => 13,
            SpawnError::Nfile => 14,
            SpawnError::Mfile => 15,
            SpawnError::Inval => 16,
            SpawnError::Isdir => 17,
            SpawnError::Libbad => 18,
            SpawnError::Failed => 19,
        }
    }

    /// Get the errno equivalent.
    pub fn to_errno(self) -> i32 {
        match self {
            SpawnError::Acces => 13,      // EACCES
            SpawnError::Perm => 1,        // EPERM
            SpawnError::TooBig => 7,      // E2BIG
            SpawnError::Noexec => 8,      // ENOEXEC
            SpawnError::Nametoolong => 36, // ENAMETOOLONG
            SpawnError::Noent => 2,       // ENOENT
            SpawnError::Nomem => 12,      // ENOMEM
            SpawnError::Notdir => 20,     // ENOTDIR
            SpawnError::Loop => 40,       // ELOOP
            SpawnError::Txtbusy => 26,    // ETXTBSY
            SpawnError::Io => 5,          // EIO
            SpawnError::Nfile => 23,      // ENFILE
            SpawnError::Mfile => 24,      // EMFILE
            SpawnError::Inval => 22,      // EINVAL
            SpawnError::Isdir => 21,      // EISDIR
            SpawnError::Libbad => 80,     // ELIBBAD
            _ => -1,
        }
    }
}

/// Spawn flags (`GSpawnFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct SpawnFlags(pub u32);

impl SpawnFlags {
    pub const DEFAULT: SpawnFlags = SpawnFlags(0);
    pub const LEAVE_DESCRIPTORS_OPEN: SpawnFlags = SpawnFlags(1 << 0);
    pub const DO_NOT_REAP_CHILD: SpawnFlags = SpawnFlags(1 << 1);
    pub const SEARCH_PATH: SpawnFlags = SpawnFlags(1 << 2);
    pub const STDOUT_TO_DEV_NULL: SpawnFlags = SpawnFlags(1 << 3);
    pub const STDERR_TO_DEV_NULL: SpawnFlags = SpawnFlags(1 << 4);
    pub const CHILD_INHERITS_STDIN: SpawnFlags = SpawnFlags(1 << 5);
    pub const FILE_AND_ARGV_ZERO: SpawnFlags = SpawnFlags(1 << 6);
    pub const SEARCH_PATH_FROM_ENVP: SpawnFlags = SpawnFlags(1 << 7);
    pub const CLOEXEC_PIPES: SpawnFlags = SpawnFlags(1 << 8);
    pub const CHILD_INHERITS_STDOUT: SpawnFlags = SpawnFlags(1 << 9);
    pub const CHILD_INHERITS_STDERR: SpawnFlags = SpawnFlags(1 << 10);
    pub const STDIN_FROM_DEV_NULL: SpawnFlags = SpawnFlags(1 << 11);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl core::ops::BitOr for SpawnFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        SpawnFlags(self.0 | rhs.0)
    }
}

/// Child setup function (`GSpawnChildSetupFunc`).
pub type SpawnChildSetupFunc = fn();

/// Process ID type (`GPid`).
pub type Pid = i32;

/// Error quark for spawn errors (`g_spawn_error_quark`).
pub fn spawn_error_quark() -> u32 {
    crate::quark::quark_from_string(Some("g-spawn-error-quark"))
}

/// Error quark for spawn exit errors (`g_spawn_exit_error_quark`).
pub fn spawn_exit_error_quark() -> u32 {
    crate::quark::quark_from_string(Some("g-spawn-exit-error-quark"))
}

/// Result of a spawn operation.
pub struct SpawnResult {
    pub pid: Pid,
    pub stdout: Option<Vec<u8>>,
    pub stderr: Option<Vec<u8>>,
    pub exit_status: i32,
}

/// Platform trait for spawning processes.
pub trait SpawnPlatform {
    /// Spawn a process asynchronously.
    fn spawn_async(
        working_directory: Option<&str>,
        argv: &[&str],
        envp: Option<&[&str]>,
        flags: SpawnFlags,
        child_setup: Option<SpawnChildSetupFunc>,
    ) -> Result<Pid, SpawnError>;

    /// Spawn a process synchronously, capturing stdout/stderr.
    fn spawn_sync(
        working_directory: Option<&str>,
        argv: &[&str],
        envp: Option<&[&str]>,
        flags: SpawnFlags,
        child_setup: Option<SpawnChildSetupFunc>,
    ) -> Result<SpawnResult, SpawnError>;

    /// Check wait status (`g_spawn_check_wait_status`).
    fn check_wait_status(wait_status: i32) -> Result<(), SpawnError>;
}

/// A no-op platform implementation.
pub struct NoSpawnPlatform;

impl SpawnPlatform for NoSpawnPlatform {
    fn spawn_async(
        _working_directory: Option<&str>,
        _argv: &[&str],
        _envp: Option<&[&str]>,
        _flags: SpawnFlags,
        _child_setup: Option<SpawnChildSetupFunc>,
    ) -> Result<Pid, SpawnError> {
        Err(SpawnError::Failed)
    }

    fn spawn_sync(
        _working_directory: Option<&str>,
        _argv: &[&str],
        _envp: Option<&[&str]>,
        _flags: SpawnFlags,
        _child_setup: Option<SpawnChildSetupFunc>,
    ) -> Result<SpawnResult, SpawnError> {
        Err(SpawnError::Failed)
    }

    fn check_wait_status(_wait_status: i32) -> Result<(), SpawnError> {
        Err(SpawnError::Failed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spawn_error_codes() {
        assert_eq!(SpawnError::Fork.to_code(), 0);
        assert_eq!(SpawnError::Failed.to_code(), 19);
    }

    #[test]
    fn spawn_error_errno() {
        assert_eq!(SpawnError::Acces.to_errno(), 13);
        assert_eq!(SpawnError::Noent.to_errno(), 2);
    }

    #[test]
    fn spawn_flags() {
        let flags = SpawnFlags::SEARCH_PATH | SpawnFlags::STDOUT_TO_DEV_NULL;
        assert!(flags.contains(SpawnFlags::SEARCH_PATH));
        assert!(flags.contains(SpawnFlags::STDOUT_TO_DEV_NULL));
        assert!(!flags.contains(SpawnFlags::STDERR_TO_DEV_NULL));
    }

    #[test]
    fn spawn_error_quark() {
        let q = spawn_error_quark();
        assert!(q > 0);
    }

    #[test]
    fn no_spawn_platform_fails() {
        let result = NoSpawnPlatform::spawn_async(None, &["ls"], None, SpawnFlags::DEFAULT, None);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), SpawnError::Failed);
    }
}
