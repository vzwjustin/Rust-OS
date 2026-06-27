//! GFilterInputStream matching `gio/gfilterinputstream.h`.
//!
//! Upstream `GFilterInputStream` is a `GInputStream` subclass that wraps
//! another `GInputStream` as its base stream. We port it as a struct
//! with `Mutex`-protected `close_base_stream` flag.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginputstream::InputStream;
use spin::Mutex;

/// A filter input stream (`GFilterInputStream`).
///
/// Wraps a base `InputStream` and optionally closes it when the filter
/// is closed.
pub struct FilterInputStream {
    base_stream: InputStream,
    close_base_stream: Mutex<bool>,
}

impl FilterInputStream {
    /// Creates a new filter input stream wrapping `base_stream`.
    ///
    /// `close_base_stream` defaults to `true`.
    pub fn new(base_stream: InputStream) -> Self {
        Self {
            base_stream,
            close_base_stream: Mutex::new(true),
        }
    }

    /// Gets the base stream.
    ///
    /// Mirrors `g_filter_input_stream_get_base_stream`.
    pub fn get_base_stream(&self) -> &InputStream {
        &self.base_stream
    }

    /// Gets whether the base stream will be closed when the filter is closed.
    ///
    /// Mirrors `g_filter_input_stream_get_close_base_stream`.
    pub fn get_close_base_stream(&self) -> bool {
        *self.close_base_stream.lock()
    }

    /// Sets whether the base stream will be closed when the filter is closed.
    ///
    /// Mirrors `g_filter_input_stream_set_close_base_stream`.
    pub fn set_close_base_stream(&self, close_base: bool) {
        *self.close_base_stream.lock() = close_base;
    }

    /// Closes the base stream if `close_base_stream` is set.
    pub fn close_base_if_needed(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        if self.get_close_base_stream() {
            self.base_stream.close(cancellable)
        } else {
            Ok(())
        }
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::Bytes;
    use crate::ginputstream::MemoryInputStream;

    #[test]
    fn test_filter_input_stream_new() {
        let base = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_static(
            b"hello",
        )));
        let filter = FilterInputStream::new(base);
        assert!(filter.get_close_base_stream());
    }

    #[test]
    fn test_set_close_base_stream() {
        let base = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_static(
            b"hello",
        )));
        let filter = FilterInputStream::new(base);
        filter.set_close_base_stream(false);
        assert!(!filter.get_close_base_stream());
    }

    #[test]
    fn test_get_base_stream() {
        let base = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_static(
            b"hello",
        )));
        let filter = FilterInputStream::new(base);
        let mut buf = [0u8; 5];
        filter.get_base_stream().read_all(&mut buf, None).unwrap();
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn test_close_base_if_needed() {
        let base = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_static(
            b"hello",
        )));
        let filter = FilterInputStream::new(base);
        assert!(filter.get_close_base_stream());
        filter.close_base_if_needed(None).unwrap();
    }

    #[test]
    fn test_close_base_not_needed() {
        let base = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_static(
            b"hello",
        )));
        let filter = FilterInputStream::new(base);
        filter.set_close_base_stream(false);
        filter.close_base_if_needed(None).unwrap();
        // Base stream should not be closed
        assert!(!filter.get_base_stream().is_closed());
    }
}
