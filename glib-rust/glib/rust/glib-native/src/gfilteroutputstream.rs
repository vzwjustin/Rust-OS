//! GFilterOutputStream matching `gio/gfilteroutputstream.h`.
//!
//! Upstream `GFilterOutputStream` is a `GOutputStream` subclass that wraps
//! another `GOutputStream` as its base stream. We port it as a struct
//! with `Mutex`-protected `close_base_stream` flag.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::goutputstream::OutputStream;
use spin::Mutex;

/// A filter output stream (`GFilterOutputStream`).
///
/// Wraps a base `OutputStream` and optionally closes it when the filter
/// is closed.
pub struct FilterOutputStream {
    base_stream: OutputStream,
    close_base_stream: Mutex<bool>,
}

impl FilterOutputStream {
    /// Creates a new filter output stream wrapping `base_stream`.
    ///
    /// `close_base_stream` defaults to `true`.
    pub fn new(base_stream: OutputStream) -> Self {
        Self {
            base_stream,
            close_base_stream: Mutex::new(true),
        }
    }

    /// Gets the base stream.
    ///
    /// Mirrors `g_filter_output_stream_get_base_stream`.
    pub fn get_base_stream(&self) -> &OutputStream {
        &self.base_stream
    }

    /// Gets whether the base stream will be closed when the filter is closed.
    ///
    /// Mirrors `g_filter_output_stream_get_close_base_stream`.
    pub fn get_close_base_stream(&self) -> bool {
        *self.close_base_stream.lock()
    }

    /// Sets whether the base stream will be closed when the filter is closed.
    ///
    /// Mirrors `g_filter_output_stream_set_close_base_stream`.
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
    use crate::goutputstream::MemoryOutputStream;

    #[test]
    fn test_filter_output_stream_new() {
        let base = OutputStream::from(MemoryOutputStream::new_resizable());
        let filter = FilterOutputStream::new(base);
        assert!(filter.get_close_base_stream());
    }

    #[test]
    fn test_set_close_base_stream() {
        let base = OutputStream::from(MemoryOutputStream::new_resizable());
        let filter = FilterOutputStream::new(base);
        filter.set_close_base_stream(false);
        assert!(!filter.get_close_base_stream());
    }

    #[test]
    fn test_get_base_stream() {
        let base = OutputStream::from(MemoryOutputStream::new_resizable());
        let filter = FilterOutputStream::new(base);
        filter.get_base_stream().write_all(b"hello", None).unwrap();
        let underlying = filter
            .get_base_stream()
            .downcast_ref::<MemoryOutputStream>()
            .unwrap();
        assert_eq!(underlying.get_data(), b"hello");
    }

    #[test]
    fn test_close_base_if_needed() {
        let base = OutputStream::from(MemoryOutputStream::new_resizable());
        let filter = FilterOutputStream::new(base);
        assert!(filter.get_close_base_stream());
        filter.close_base_if_needed(None).unwrap();
    }

    #[test]
    fn test_close_base_not_needed() {
        let base = OutputStream::from(MemoryOutputStream::new_resizable());
        let filter = FilterOutputStream::new(base);
        filter.set_close_base_stream(false);
        filter.close_base_if_needed(None).unwrap();
        assert!(!filter.get_base_stream().is_closed());
    }
}
