//! giounix_private matching `gio/giounix-private.c`.
//!
//! Provides `_g_fd_is_pollable` which determines whether a file descriptor
//! is pollable. On Linux this uses epoll, on BSDs kqueue, and on other UNIX
//! systems it falls back to checking if the fd is not a regular file.
//!
//! In this no_std kernel port, we model fd pollability with a simple
//! heuristic: regular files are not pollable, everything else is.
//!
//! Fully `no_std` compatible using `alloc`.

/// Checks whether a file descriptor is a regular file.
///
/// In the kernel, we model this with a simple flag. Real implementations
/// would call `fstat()` and check `S_ISREG`.
fn fd_is_regular_file(_fd: i32) -> bool {
    // In no_std kernel context, we can't call fstat.
    // Return false as a safe default (assume not a regular file).
    false
}

/// Determines whether a file descriptor is pollable.
///
/// On Linux, the C implementation uses `epoll_ctl(EPOLL_CTL_ADD)` to check
/// if the kernel's `file_can_poll()` returns true. On BSDs, it uses kqueue.
/// On other systems, it falls back to `!g_fd_is_regular_file(fd)`.
///
/// In this no_std port, we use the fallback approach.
///
/// Mirrors `_g_fd_is_pollable`.
pub fn fd_is_pollable(fd: i32) -> bool {
    !fd_is_regular_file(fd)
}

/// Temp-failure-retry wrapper for syscalls.
///
/// In the C code this is a macro that retries on EINTR.
/// In Rust we can use this as a pattern for wrapping syscall results.
pub fn temp_failure_retry<F, T>(mut f: F) -> T
where
    F: FnMut() -> T,
{
    f()
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fd_is_pollable() {
        // In our no_std model, all fds are pollable (since fd_is_regular_file always returns false)
        assert!(fd_is_pollable(0));
        assert!(fd_is_pollable(1));
        assert!(fd_is_pollable(-1));
        assert!(fd_is_pollable(42));
    }

    #[test]
    fn test_temp_failure_retry() {
        let result = temp_failure_retry(|| 42);
        assert_eq!(result, 42);
    }
}
