//! GPollableInputStream matching `gio/gpollableinputstream.h`.
//!
//! Upstream `GPollableInputStream` is an interface for input streams that
//! can be polled for readability. We port it as a Rust trait.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;

/// Trait for pollable input streams (`GPollableInputStream`).
pub trait PollableInputStream {
    /// Checks if the stream is actually pollable.
    ///
    /// Mirrors `g_pollable_input_stream_can_poll`.
    fn can_poll(&self) -> bool {
        true
    }

    /// Checks if the stream is currently readable.
    ///
    /// Mirrors `g_pollable_input_stream_is_readable`.
    fn is_readable(&self) -> bool;

    /// Performs a non-blocking read.
    ///
    /// Mirrors `g_pollable_input_stream_read_nonblocking`.
    /// Returns `Ok(n)` with bytes read, or `Err` with `IOErrorEnum::WouldBlock`.
    fn read_nonblocking(
        &self,
        buffer: &mut [u8],
        cancellable: Option<&GCancellable>,
    ) -> Result<usize, Error>;
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gioerror::IOErrorEnum;
    use alloc::vec::Vec;
    use spin::Mutex;

    struct TestPollableInput {
        data: Mutex<Vec<u8>>,
        pos: Mutex<usize>,
        readable: Mutex<bool>,
    }

    impl TestPollableInput {
        fn new(data: &[u8]) -> Self {
            Self {
                data: Mutex::new(data.to_vec()),
                pos: Mutex::new(0),
                readable: Mutex::new(true),
            }
        }
    }

    impl PollableInputStream for TestPollableInput {
        fn is_readable(&self) -> bool {
            *self.readable.lock()
        }

        fn read_nonblocking(
            &self,
            buffer: &mut [u8],
            _cancellable: Option<&GCancellable>,
        ) -> Result<usize, Error> {
            if !*self.readable.lock() {
                return Err(Error::new(
                    crate::gioerror::io_error_quark(),
                    IOErrorEnum::WouldBlock.to_code(),
                    "Would block",
                ));
            }
            let mut pos = self.pos.lock();
            let data = self.data.lock();
            let available = data.len().saturating_sub(*pos);
            let to_read = buffer.len().min(available);
            if to_read == 0 {
                return Ok(0);
            }
            buffer[..to_read].copy_from_slice(&data[*pos..*pos + to_read]);
            *pos += to_read;
            Ok(to_read)
        }
    }

    #[test]
    fn test_can_poll_default() {
        let stream = TestPollableInput::new(b"hello");
        assert!(stream.can_poll());
    }

    #[test]
    fn test_is_readable() {
        let stream = TestPollableInput::new(b"hello");
        assert!(stream.is_readable());
        *stream.readable.lock() = false;
        assert!(!stream.is_readable());
    }

    #[test]
    fn test_read_nonblocking() {
        let stream = TestPollableInput::new(b"hello world");
        let mut buf = [0u8; 5];
        let n = stream.read_nonblocking(&mut buf, None).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn test_read_nonblocking_would_block() {
        let stream = TestPollableInput::new(b"hello");
        *stream.readable.lock() = false;
        let mut buf = [0u8; 5];
        let result = stream.read_nonblocking(&mut buf, None);
        assert!(result.is_err());
    }

    #[test]
    fn test_read_nonblocking_exhausted() {
        let stream = TestPollableInput::new(b"hi");
        let mut buf = [0u8; 10];
        let n = stream.read_nonblocking(&mut buf, None).unwrap();
        assert_eq!(n, 2);
        let n2 = stream.read_nonblocking(&mut buf, None).unwrap();
        assert_eq!(n2, 0);
    }
}
