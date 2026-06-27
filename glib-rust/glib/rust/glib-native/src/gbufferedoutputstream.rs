//! GBufferedOutputStream matching `gio/gbufferedoutputstream.h`.
//!
//! Upstream `GBufferedOutputStream` wraps another `GOutputStream` and buffers
//! writes, flushing to the base stream when the buffer is full or explicitly
//! flushed. Optionally auto-grows the buffer instead of flushing when full.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::gioerror::{io_error_quark, IOErrorEnum};
use crate::goutputstream::OutputStream;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

/// Default buffer capacity in bytes (matches GIO's default).
const DEFAULT_BUFFER_SIZE: usize = 8192;

/// Buffered output stream (`GBufferedOutputStream`).
///
/// Wraps a base `OutputStream` and accumulates writes in an internal buffer,
/// flushing to the base stream when the buffer fills or when `flush` / `close`
/// is called explicitly.
pub struct BufferedOutputStream {
    base_stream: OutputStream,
    /// Buffered bytes not yet written to the base stream.
    buffer: Mutex<Vec<u8>>,
    /// Logical capacity of the buffer (may differ from `buffer.capacity()`).
    buffer_size: Mutex<usize>,
    /// When `true`, the buffer grows instead of triggering a flush.
    auto_grow: Mutex<bool>,
}

impl BufferedOutputStream {
    /// Creates a buffered output stream with the default capacity (8 192 bytes).
    ///
    /// Mirrors `g_buffered_output_stream_new`.
    pub fn new(base: OutputStream) -> Self {
        Self::new_sized(base, DEFAULT_BUFFER_SIZE)
    }

    /// Creates a buffered output stream with an explicit initial capacity.
    ///
    /// Mirrors `g_buffered_output_stream_new_sized`.
    pub fn new_sized(base: OutputStream, size: usize) -> Self {
        Self {
            base_stream: base,
            buffer: Mutex::new(Vec::with_capacity(size)),
            buffer_size: Mutex::new(size),
            auto_grow: Mutex::new(false),
        }
    }

    /// Returns the current buffer capacity.
    ///
    /// Mirrors `g_buffered_output_stream_get_buffer_size`.
    pub fn get_buffer_size(&self) -> usize {
        *self.buffer_size.lock()
    }

    /// Sets a new buffer capacity.
    ///
    /// If `size` is smaller than the number of bytes currently buffered, the
    /// buffer is flushed to the base stream first.  If `size` is larger the
    /// capacity is simply updated; existing buffered bytes are preserved.
    ///
    /// Mirrors `g_buffered_output_stream_set_buffer_size`.
    pub fn set_buffer_size(&self, size: usize) {
        // Flush before we shrink if the buffer content would not fit.
        {
            let buf = self.buffer.lock();
            if buf.len() > size {
                drop(buf);
                // Best-effort flush; ignore errors (matches GIO behaviour).
                let _ = self.flush(None);
            }
        }
        *self.buffer_size.lock() = size;
    }

    /// Returns whether the buffer auto-grows instead of flushing when full.
    ///
    /// Mirrors `g_buffered_output_stream_get_auto_grow`.
    pub fn get_auto_grow(&self) -> bool {
        *self.auto_grow.lock()
    }

    /// Sets the auto-grow behaviour.
    ///
    /// Mirrors `g_buffered_output_stream_set_auto_grow`.
    pub fn set_auto_grow(&self, auto_grow: bool) {
        *self.auto_grow.lock() = auto_grow;
    }

    /// Writes bytes to the internal buffer.
    ///
    /// When the buffer would overflow:
    /// - If `auto_grow` is set, the buffer capacity doubles until it fits.
    /// - Otherwise the buffer is flushed to the base stream first and the
    ///   incoming bytes are then copied into the now-empty buffer.
    ///
    /// Returns the number of bytes accepted (always `buf.len()` on success).
    pub fn write(&self, buf: &[u8], cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }

        if buf.is_empty() {
            return Ok(0);
        }

        let auto_grow = *self.auto_grow.lock();

        // We need to lock buffer and buffer_size together to avoid TOCTOU.
        let mut buffer = self.buffer.lock();
        let mut capacity = *self.buffer_size.lock();

        let available = capacity.saturating_sub(buffer.len());

        if buf.len() <= available {
            // Fast path: data fits in the remaining space.
            buffer.extend_from_slice(buf);
        } else if auto_grow {
            // Grow until we have enough space, then buffer everything.
            while capacity < buffer.len() + buf.len() {
                capacity = capacity.saturating_mul(2).max(buffer.len() + buf.len());
            }
            // Release the buffer lock before writing to buffer_size (ordering
            // is important: we hold buffer here and must not dead-lock).
            buffer.reserve(buf.len());
            buffer.extend_from_slice(buf);
            // Update stored capacity.
            drop(buffer);
            *self.buffer_size.lock() = capacity;
        } else {
            // Flush what we have, then buffer the new bytes.
            // We must release the lock before calling flush (which also locks).
            drop(buffer);
            self.flush(cancellable)?;

            // Re-acquire after flush.
            let mut buffer = self.buffer.lock();
            capacity = *self.buffer_size.lock();

            if buf.len() > capacity {
                // Data is larger than the entire buffer; write directly to base.
                return self.base_stream.write(buf, cancellable);
            }

            buffer.extend_from_slice(buf);
        }

        Ok(buf.len())
    }

    /// Flushes all buffered bytes to the base stream, then clears the buffer.
    ///
    /// Mirrors `g_output_stream_flush`.
    pub fn flush(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }

        let mut buffer = self.buffer.lock();

        if buffer.is_empty() {
            return Ok(());
        }

        // Write all buffered bytes to the base stream.
        let mut written = 0;
        while written < buffer.len() {
            match self.base_stream.write(&buffer[written..], cancellable) {
                Ok(n) if n == 0 => {
                    return Err(Error::new(
                        io_error_quark(),
                        IOErrorEnum::Failed.to_code(),
                        "base stream wrote 0 bytes",
                    ));
                }
                Ok(n) => written += n,
                Err(e) => return Err(e),
            }
        }

        buffer.clear();
        Ok(())
    }

    /// Flushes the buffer and closes the base stream.
    ///
    /// Mirrors `g_output_stream_close`.
    pub fn close(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        self.flush(cancellable)?;
        self.base_stream.close(cancellable)
    }

    /// Returns a reference to the underlying base stream.
    pub fn get_base_stream(&self) -> &OutputStream {
        &self.base_stream
    }
}

// ─────────────────────────────────── Tests ────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goutputstream::{MemoryOutputStream, OutputStream};

    fn make_base() -> OutputStream {
        OutputStream::new(MemoryOutputStream::new_resizable())
    }

    /// Helper: retrieve the bytes written to the MemoryOutputStream via the
    /// public `downcast_ref` method exposed by `OutputStream`.
    fn get_mem_data(base: &OutputStream) -> alloc::vec::Vec<u8> {
        base.downcast_ref::<MemoryOutputStream>()
            .expect("downcast to MemoryOutputStream failed")
            .get_data()
    }

    // ── 1. new() uses the default capacity ────────────────────────────────────
    #[test]
    fn test_new_default_capacity() {
        let base = make_base();
        let bos = BufferedOutputStream::new(base);
        assert_eq!(bos.get_buffer_size(), DEFAULT_BUFFER_SIZE);
        assert!(!bos.get_auto_grow());
    }

    // ── 2. new_sized() sets a custom capacity ─────────────────────────────────
    #[test]
    fn test_new_sized_capacity() {
        let base = make_base();
        let bos = BufferedOutputStream::new_sized(base, 256);
        assert_eq!(bos.get_buffer_size(), 256);
    }

    // ── 3. write + flush delivers bytes to the base stream ────────────────────
    #[test]
    fn test_write_then_flush_reaches_base() {
        let base = make_base();
        let bos = BufferedOutputStream::new(base);

        bos.write(b"hello", None).expect("write failed");
        // Not yet flushed — base should be empty.
        assert_eq!(get_mem_data(bos.get_base_stream()), b"");

        bos.flush(None).expect("flush failed");
        assert_eq!(get_mem_data(bos.get_base_stream()), b"hello");
    }

    // ── 4. write exceeding capacity triggers an implicit flush ────────────────
    #[test]
    fn test_write_exceeds_capacity_flushes() {
        let base = make_base();
        let bos = BufferedOutputStream::new_sized(base, 4);

        // Write 4 bytes — fills the buffer exactly.
        bos.write(b"abcd", None).expect("write 1 failed");
        // Now write 1 more — should flush the first 4 then buffer the new byte.
        bos.write(b"e", None).expect("write 2 failed");

        // The first 4 bytes should now be in the base stream.
        assert_eq!(get_mem_data(bos.get_base_stream()), b"abcd");

        // Flush the remaining byte.
        bos.flush(None).expect("flush failed");
        assert_eq!(get_mem_data(bos.get_base_stream()), b"abcde");
    }

    // ── 5. auto_grow expands the buffer instead of flushing ──────────────────
    #[test]
    fn test_auto_grow_expands_buffer() {
        let base = make_base();
        let bos = BufferedOutputStream::new_sized(base, 4);
        bos.set_auto_grow(true);

        // Write 8 bytes into a 4-byte buffer; auto_grow should prevent a flush.
        bos.write(b"12345678", None).expect("write failed");

        // Base stream must still be empty (no flush occurred).
        assert_eq!(get_mem_data(bos.get_base_stream()), b"");

        // Buffer capacity must have grown to accommodate the data.
        assert!(bos.get_buffer_size() >= 8);

        // Explicit flush delivers everything.
        bos.flush(None).expect("flush failed");
        assert_eq!(get_mem_data(bos.get_base_stream()), b"12345678");
    }

    // ── 6. get_auto_grow / set_auto_grow round-trips ─────────────────────────
    #[test]
    fn test_auto_grow_get_set() {
        let base = make_base();
        let bos = BufferedOutputStream::new(base);
        assert!(!bos.get_auto_grow());
        bos.set_auto_grow(true);
        assert!(bos.get_auto_grow());
        bos.set_auto_grow(false);
        assert!(!bos.get_auto_grow());
    }

    // ── 7. set_buffer_size flushes when shrinking past buffered content ───────
    #[test]
    fn test_set_buffer_size_flushes_when_shrinking() {
        let base = make_base();
        let bos = BufferedOutputStream::new_sized(base, 64);

        bos.write(b"hello world", None).expect("write failed");
        // Shrink below the number of buffered bytes → must flush first.
        bos.set_buffer_size(4);

        // The 11 bytes should now be in the base stream.
        assert_eq!(get_mem_data(bos.get_base_stream()), b"hello world");
        assert_eq!(bos.get_buffer_size(), 4);
    }

    // ── 8. close flushes then closes the base stream ─────────────────────────
    #[test]
    fn test_close_flushes_then_closes_base() {
        let base = make_base();
        let bos = BufferedOutputStream::new(base);

        bos.write(b"goodbye", None).expect("write failed");
        bos.close(None).expect("close failed");

        // Data must have reached the base stream.
        assert_eq!(get_mem_data(bos.get_base_stream()), b"goodbye");
        // Base stream must now be closed.
        assert!(bos.get_base_stream().is_closed());
    }

    // ── 9. writing a chunk larger than capacity goes directly to base ─────────
    #[test]
    fn test_write_larger_than_capacity_goes_direct() {
        let base = make_base();
        let bos = BufferedOutputStream::new_sized(base, 4);

        // 10 bytes into a 4-byte buffer with auto_grow=false → direct write.
        bos.write(b"0123456789", None).expect("write failed");

        // Should appear in the base stream immediately.
        assert_eq!(get_mem_data(bos.get_base_stream()), b"0123456789");
    }
}
