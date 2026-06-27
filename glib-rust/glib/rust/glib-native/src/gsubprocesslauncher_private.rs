//! `gsubprocesslauncher-private` matching `gio/gsubprocesslauncher-private.h`.
//!
//! Private `GSubprocessLauncher` struct fields and `g_subprocess_set_launcher`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gsubprocess::SubprocessFlags;
use crate::gsubprocesslauncher::SubprocessLauncher;
use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Private launcher state (mirrors `struct _GSubprocessLauncher`).
#[derive(Debug)]
pub struct SubprocessLauncherPrivate {
    pub flags: SubprocessFlags,
    pub envp: Vec<String>,
    pub cwd: Option<String>,
    pub stdin_fd: i32,
    pub stdin_path: Option<String>,
    pub stdout_fd: i32,
    pub stdout_path: Option<String>,
    pub stderr_fd: i32,
    pub stderr_path: Option<String>,
    pub source_fds: Vec<i32>,
    pub target_fds: Vec<i32>,
    pub closed_fd: bool,
}

impl SubprocessLauncherPrivate {
    pub fn new() -> Self {
        Self {
            flags: SubprocessFlags::NONE,
            envp: Vec::new(),
            cwd: None,
            stdin_fd: -1,
            stdin_path: None,
            stdout_fd: -1,
            stdout_path: None,
            stderr_fd: -1,
            stderr_path: None,
            source_fds: Vec::new(),
            target_fds: Vec::new(),
            closed_fd: false,
        }
    }
}

impl Default for SubprocessLauncherPrivate {
    fn default() -> Self {
        Self::new()
    }
}

/// Sets the launcher on a subprocess (mirrors `g_subprocess_set_launcher`).
pub fn set_launcher(subprocess_launcher: &SubprocessLauncher, _launcher: &SubprocessLauncher) {
    // In the C code, this sets the launcher pointer on the subprocess.
    // In our no_std port, this is a no-op since we don't have actual subprocess spawning.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_private() {
        let priv_ = SubprocessLauncherPrivate::new();
        assert_eq!(priv_.stdin_fd, -1);
        assert_eq!(priv_.stdout_fd, -1);
        assert_eq!(priv_.stderr_fd, -1);
        assert!(!priv_.closed_fd);
        assert!(priv_.source_fds.is_empty());
        assert!(priv_.target_fds.is_empty());
    }

    #[test]
    fn test_default() {
        let priv_ = SubprocessLauncherPrivate::default();
        assert!(priv_.cwd.is_none());
        assert!(priv_.stdin_path.is_none());
    }
}
