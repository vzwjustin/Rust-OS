//! GFileDescriptorBased matching `gio/gfiledescriptorbased.h`.
//!
//! Upstream `GFileDescriptorBased` is an interface for I/O objects that
//! have an underlying file descriptor. We port it as a Rust trait.
//!
//! Fully `no_std` compatible.

/// Trait for file-descriptor-based I/O objects (`GFileDescriptorBased`).
pub trait FileDescriptorBased {
    /// Gets the underlying file descriptor.
    ///
    /// Mirrors `g_file_descriptor_based_get_fd`.
    fn get_fd(&self) -> i32;
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    struct TestFdBased {
        fd: i32,
    }

    impl FileDescriptorBased for TestFdBased {
        fn get_fd(&self) -> i32 {
            self.fd
        }
    }

    #[test]
    fn test_get_fd() {
        let obj = TestFdBased { fd: 42 };
        assert_eq!(obj.get_fd(), 42);
    }

    #[test]
    fn test_get_fd_negative() {
        let obj = TestFdBased { fd: -1 };
        assert_eq!(obj.get_fd(), -1);
    }

    #[test]
    fn test_get_fd_zero() {
        let obj = TestFdBased { fd: 0 };
        assert_eq!(obj.get_fd(), 0);
    }
}
