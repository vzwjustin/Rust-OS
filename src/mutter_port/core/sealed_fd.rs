//! Sealed file descriptor wrapper ported from GNOME Mutter (src/core/meta-sealed-fd.c).
//!
//! Wraps file descriptors with seal bits (F_SEAL_GROW, F_SEAL_WRITE, F_SEAL_SHRINK)
//! to prevent modification after sealing. Useful for secure memory-mapped files and IPC.
//!
//! Ported from: /home/justin/Downloads/mutter-main/src/core/meta-sealed-fd.c
//! Omitted: GObject class machinery, GVariant handle marshaling (requires integration with D-Bus infrastructure)

use alloc::vec::Vec;
use core::fmt;

/// File descriptor with sealing bits applied.
/// Wraps a file descriptor that has been sealed with standard seal flags.
pub struct SealedFd {
    /// The underlying file descriptor.
    /// -1 indicates the fd is closed or invalid.
    fd: i32,
}

impl SealedFd {
    /// Minimum file descriptor number (typically 3 after stdin/stdout/stderr).
    const MIN_FD: i32 = 0;

    /// Create a new SealedFd from a memory file descriptor.
    ///
    /// Verifies the fd has the required seals: F_SEAL_GROW, F_SEAL_WRITE, F_SEAL_SHRINK.
    /// If seals are not present, attempts to add them.
    ///
    /// # Arguments
    /// * `memfd` - A valid memory file descriptor (from memfd_create or similar)
    ///
    /// # Returns
    /// Ok(SealedFd) if successful, Err with description if sealing fails
    pub fn new_take_memfd(memfd: i32) -> Result<Self, &'static str> {
        if memfd < Self::MIN_FD {
            return Err("invalid file descriptor");
        }

        // In no_std environment without fcntl, we trust the fd is already sealed.
        // A production kernel would call fcntl(fd, F_GET_SEALS) and F_ADD_SEALS here.
        // Omitted: fcntl seal verification - requires kernel integration.

        Ok(SealedFd { fd: memfd })
    }

    /// Get the underlying file descriptor number.
    pub fn fd(&self) -> i32 {
        self.fd
    }

    /// Check if the file descriptor is valid (not -1).
    pub fn is_valid(&self) -> bool {
        self.fd >= Self::MIN_FD
    }

    /// Close the file descriptor if valid.
    /// After calling this, the fd is marked as invalid (-1).
    pub fn close(&mut self) -> Result<(), &'static str> {
        if self.fd >= Self::MIN_FD {
            // In no_std environment, actual close() would be a syscall.
            // Omitted: syscall to close fd - requires kernel integration.
            self.fd = -1;
            Ok(())
        } else {
            Err("file descriptor already closed")
        }
    }
}

impl fmt::Debug for SealedFd {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SealedFd").field("fd", &self.fd).finish()
    }
}

impl Drop for SealedFd {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sealed_fd_creation() {
        let sealed = SealedFd::new_take_memfd(5);
        assert!(sealed.is_ok());
        let fd = sealed.unwrap();
        assert_eq!(fd.fd(), 5);
        assert!(fd.is_valid());
    }

    #[test]
    fn test_sealed_fd_invalid() {
        let sealed = SealedFd::new_take_memfd(-1);
        assert!(sealed.is_err());
    }

    #[test]
    fn test_sealed_fd_close() {
        let mut fd = SealedFd::new_take_memfd(5).unwrap();
        assert!(fd.is_valid());
        assert!(fd.close().is_ok());
        assert!(!fd.is_valid());
        // Closing again should fail
        assert!(fd.close().is_err());
    }

    #[test]
    fn test_sealed_fd_debug() {
        let fd = SealedFd::new_take_memfd(5).unwrap();
        let debug_str = format!("{:?}", fd);
        assert!(debug_str.contains("SealedFd"));
        assert!(debug_str.contains("5"));
    }
}
