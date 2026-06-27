//! GPollableOutputStream matching `gio/gpollableoutputstream.h`.
//!
//! Upstream `GPollableOutputStream` is an interface for output streams that
//! can be polled for writability. We port it as a Rust trait.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;

/// Return type for vectored non-blocking writes (`GPollableReturn`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollableReturn {
    Ok = 0,
    WouldBlock = 1,
    Error = 2,
}

/// Trait for pollable output streams (`GPollableOutputStream`).
pub trait PollableOutputStream {
    /// Checks if the stream is actually pollable.
    ///
    /// Mirrors `g_pollable_output_stream_can_poll`.
    fn can_poll(&self) -> bool {
        true
    }

    /// Checks if the stream is currently writable.
    ///
    /// Mirrors `g_pollable_output_stream_is_writable`.
    fn is_writable(&self) -> bool;

    /// Performs a non-blocking write.
    ///
    /// Mirrors `g_pollable_output_stream_write_nonblocking`.
    /// Returns `Ok(n)` with bytes written, or `Err` with `IOErrorEnum::WouldBlock`.
    fn write_nonblocking(
        &self,
        buffer: &[u8],
        cancellable: Option<&GCancellable>,
    ) -> Result<usize, Error>;

    /// Performs a vectored non-blocking write.
    ///
    /// Mirrors `g_pollable_output_stream_writev_nonblocking`.
    /// Default implementation calls `write_nonblocking` for each vector.
    fn writev_nonblocking(
        &self,
        vectors: &[&[u8]],
        cancellable: Option<&GCancellable>,
    ) -> Result<(PollableReturn, usize), Error> {
        let mut total = 0;
        for v in vectors {
            match self.write_nonblocking(v, cancellable) {
                Ok(n) => total += n,
                Err(_) => {
                    if total > 0 {
                        return Ok((PollableReturn::Ok, total));
                    }
                    return Ok((PollableReturn::WouldBlock, 0));
                }
            }
        }
        Ok((PollableReturn::Ok, total))
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gioerror::IOErrorEnum;
    use alloc::vec::Vec;
    use spin::Mutex;

    struct TestPollableOutput {
        buffer: Mutex<Vec<u8>>,
        writable: Mutex<bool>,
    }

    impl TestPollableOutput {
        fn new() -> Self {
            Self {
                buffer: Mutex::new(Vec::new()),
                writable: Mutex::new(true),
            }
        }
    }

    impl PollableOutputStream for TestPollableOutput {
        fn is_writable(&self) -> bool {
            *self.writable.lock()
        }

        fn write_nonblocking(
            &self,
            buffer: &[u8],
            _cancellable: Option<&GCancellable>,
        ) -> Result<usize, Error> {
            if !*self.writable.lock() {
                return Err(Error::new(
                    crate::gioerror::io_error_quark(),
                    IOErrorEnum::WouldBlock.to_code(),
                    "Would block",
                ));
            }
            self.buffer.lock().extend_from_slice(buffer);
            Ok(buffer.len())
        }
    }

    #[test]
    fn test_can_poll_default() {
        let stream = TestPollableOutput::new();
        assert!(stream.can_poll());
    }

    #[test]
    fn test_is_writable() {
        let stream = TestPollableOutput::new();
        assert!(stream.is_writable());
        *stream.writable.lock() = false;
        assert!(!stream.is_writable());
    }

    #[test]
    fn test_write_nonblocking() {
        let stream = TestPollableOutput::new();
        let n = stream.write_nonblocking(b"hello", None).unwrap();
        assert_eq!(n, 5);
        assert_eq!(*stream.buffer.lock(), b"hello");
    }

    #[test]
    fn test_write_nonblocking_would_block() {
        let stream = TestPollableOutput::new();
        *stream.writable.lock() = false;
        let result = stream.write_nonblocking(b"hello", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_writev_nonblocking() {
        let stream = TestPollableOutput::new();
        let v1 = b"hello";
        let v2 = b" world";
        let (ret, total) = stream.writev_nonblocking(&[v1, v2], None).unwrap();
        assert_eq!(ret, PollableReturn::Ok);
        assert_eq!(total, 11);
        assert_eq!(*stream.buffer.lock(), b"hello world");
    }

    #[test]
    fn test_pollable_return_values() {
        assert_eq!(PollableReturn::Ok as u8, 0);
        assert_eq!(PollableReturn::WouldBlock as u8, 1);
    }
}
