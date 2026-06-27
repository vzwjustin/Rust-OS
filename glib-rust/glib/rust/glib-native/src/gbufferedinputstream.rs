//! GBufferedInputStream matching `gio/gbufferedinputstream.h`.
//!
//! Wraps an `InputStream` with an internal read-ahead buffer, reducing the
//! number of underlying read calls. Mirrors the GIO `GBufferedInputStream`
//! API. Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginputstream::InputStream;
use crate::gioerror::{io_error_quark, IOErrorEnum};
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

/// Default buffer size used by `BufferedInputStream::new`.
const DEFAULT_BUFFER_SIZE: usize = 8192;

/// A buffered input stream (`GBufferedInputStream`).
///
/// Reads from the base stream in chunks, serving subsequent reads from the
/// in-memory buffer to reduce syscall overhead.
pub struct BufferedInputStream {
    base_stream: InputStream,
    /// Internal read-ahead buffer.
    buffer: Mutex<Vec<u8>>,
    /// Read position within `buffer` — bytes before this offset are consumed.
    pos: Mutex<usize>,
    /// Capacity hint stored so `get_buffer_size` works after a resize.
    buffer_size: Mutex<usize>,
}

impl BufferedInputStream {
    /// Creates a new buffered input stream with the default buffer size (8192).
    ///
    /// Mirrors `g_buffered_input_stream_new`.
    pub fn new(base_stream: InputStream) -> Self {
        Self::new_sized(base_stream, DEFAULT_BUFFER_SIZE)
    }

    /// Creates a new buffered input stream with the given buffer size.
    ///
    /// Mirrors `g_buffered_input_stream_new_sized`.
    pub fn new_sized(base_stream: InputStream, buffer_size: usize) -> Self {
        let cap = if buffer_size == 0 {
            DEFAULT_BUFFER_SIZE
        } else {
            buffer_size
        };
        Self {
            base_stream,
            buffer: Mutex::new(Vec::with_capacity(cap)),
            pos: Mutex::new(0),
            buffer_size: Mutex::new(cap),
        }
    }

    /// Returns the current buffer size (capacity hint).
    ///
    /// Mirrors `g_buffered_input_stream_get_buffer_size`.
    pub fn get_buffer_size(&self) -> usize {
        *self.buffer_size.lock()
    }

    /// Sets a new buffer size, discarding any unconsumed buffered data.
    ///
    /// Mirrors `g_buffered_input_stream_set_buffer_size`.
    pub fn set_buffer_size(&self, size: usize) {
        let new_cap = if size == 0 { DEFAULT_BUFFER_SIZE } else { size };
        let mut buf = self.buffer.lock();
        let mut pos = self.pos.lock();
        let mut bsz = self.buffer_size.lock();
        buf.clear();
        *pos = 0;
        *bsz = new_cap;
        buf.reserve(new_cap);
    }

    /// Returns the number of bytes currently available in the buffer.
    ///
    /// Mirrors `g_buffered_input_stream_get_available`.
    pub fn get_available(&self) -> usize {
        let buf = self.buffer.lock();
        let pos = self.pos.lock();
        buf.len().saturating_sub(*pos)
    }

    /// Returns a copy of the unconsumed bytes in the buffer.
    ///
    /// Mirrors `g_buffered_input_stream_peek_buffer`.
    pub fn peek_buffer(&self) -> Vec<u8> {
        let buf = self.buffer.lock();
        let pos = *self.pos.lock();
        buf[pos..].to_vec()
    }

    /// Fills the buffer from the base stream.
    ///
    /// `count == -1` fills up to the full buffer capacity. `count >= 0`
    /// attempts to make at least `count` bytes available.
    ///
    /// Returns the number of bytes read from the base stream.
    ///
    /// Mirrors `g_buffered_input_stream_fill`.
    pub fn fill(&self, count: i64, cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }

        let mut buf = self.buffer.lock();
        let mut pos = self.pos.lock();
        let cap = *self.buffer_size.lock();

        // Compact: move unconsumed bytes to front.
        if *pos > 0 {
            buf.drain(..*pos);
            *pos = 0;
        }

        let already = buf.len();
        let want: usize = if count < 0 {
            cap.saturating_sub(already)
        } else {
            let need = count as usize;
            if already >= need {
                return Ok(0);
            }
            need.saturating_sub(already)
        };

        if want == 0 {
            return Ok(0);
        }

        // Extend buffer to receive new bytes.
        let old_len = buf.len();
        buf.resize(old_len + want, 0u8);

        // Release the Mutex while doing I/O so other accessors are not blocked.
        // We hold mutable state here so we must use a staging buffer instead.
        // (spin::Mutex does not allow re-entrant use on the same thread.)
        drop(buf);
        drop(pos);

        let mut staging = vec![0u8; want];
        let n = self.base_stream.read(&mut staging, cancellable)?;

        // Re-acquire and apply.
        let mut buf = self.buffer.lock();
        let mut pos = self.pos.lock();
        // Trim the reservation to the actual bytes read.
        buf.truncate(old_len + n);
        // Write the received bytes at the position we reserved.
        buf[old_len..old_len + n].copy_from_slice(&staging[..n]);
        drop(pos); // keep borrow checker happy

        Ok(n)
    }

    /// Reads a single byte from the buffer, filling if necessary.
    ///
    /// Mirrors `g_buffered_input_stream_read_byte`.
    pub fn read_byte(&self, cancellable: Option<&GCancellable>) -> Result<u8, Error> {
        if self.get_available() == 0 {
            let filled = self.fill(-1, cancellable)?;
            if filled == 0 {
                return Err(Error::new(
                    io_error_quark(),
                    IOErrorEnum::Closed.to_code(),
                    "End of stream reached",
                ));
            }
        }
        let mut buf = self.buffer.lock();
        let mut pos = self.pos.lock();
        let byte = buf[*pos];
        *pos += 1;
        Ok(byte)
    }

    /// Reads up to `buffer.len()` bytes, serving from the internal buffer
    /// first and falling through to the base stream when needed.
    ///
    /// Mirrors `g_input_stream_read` on a `GBufferedInputStream`.
    pub fn read(&self, out: &mut [u8], cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }

        let avail = self.get_available();
        if avail > 0 {
            let n = avail.min(out.len());
            let mut buf = self.buffer.lock();
            let mut pos = self.pos.lock();
            out[..n].copy_from_slice(&buf[*pos..*pos + n]);
            *pos += n;
            return Ok(n);
        }

        // Buffer empty — read directly from base stream.
        self.base_stream.read(out, cancellable)
    }

    /// Skips up to `count` bytes, consuming from the internal buffer first.
    ///
    /// Mirrors `g_input_stream_skip` on a `GBufferedInputStream`.
    pub fn skip(&self, count: usize, cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }

        let avail = self.get_available();
        if avail >= count {
            let mut pos = self.pos.lock();
            *pos += count;
            return Ok(count);
        }

        // Consume everything in buffer, then skip the rest from base stream.
        let consumed = avail;
        {
            let mut pos = self.pos.lock();
            *pos += consumed;
        }

        let remaining = count - consumed;
        if remaining == 0 {
            return Ok(consumed);
        }

        let skipped = self.base_stream.skip(remaining, cancellable)?;
        Ok(consumed + skipped)
    }

    /// Closes the stream.
    ///
    /// Mirrors `g_input_stream_close` on a `GBufferedInputStream`.
    pub fn close(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        self.base_stream.close(cancellable)
    }

    /// Returns whether the base stream is closed.
    pub fn is_closed(&self) -> bool {
        self.base_stream.is_closed()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::Bytes;
    use crate::ginputstream::MemoryInputStream;

    fn make_stream(data: &'static [u8]) -> BufferedInputStream {
        let base = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_static(data)));
        BufferedInputStream::new(base)
    }

    fn make_stream_sized(data: &'static [u8], cap: usize) -> BufferedInputStream {
        let base = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_static(data)));
        BufferedInputStream::new_sized(base, cap)
    }

    #[test]
    fn test_new_default_buffer_size() {
        let s = make_stream(b"hello");
        assert_eq!(s.get_buffer_size(), DEFAULT_BUFFER_SIZE);
    }

    #[test]
    fn test_new_sized_buffer_size() {
        let s = make_stream_sized(b"hello", 64);
        assert_eq!(s.get_buffer_size(), 64);
    }

    #[test]
    fn test_fill_and_get_available() {
        let s = make_stream(b"hello world");
        assert_eq!(s.get_available(), 0);
        let n = s.fill(-1, None).unwrap();
        assert_eq!(n, 11);
        assert_eq!(s.get_available(), 11);
    }

    #[test]
    fn test_peek_buffer() {
        let s = make_stream(b"abcde");
        s.fill(-1, None).unwrap();
        let peeked = s.peek_buffer();
        assert_eq!(peeked, b"abcde");
        // Peek does not advance position.
        assert_eq!(s.get_available(), 5);
    }

    #[test]
    fn test_read_byte() {
        let s = make_stream(b"xyz");
        assert_eq!(s.read_byte(None).unwrap(), b'x');
        assert_eq!(s.read_byte(None).unwrap(), b'y');
        assert_eq!(s.read_byte(None).unwrap(), b'z');
        // Next read_byte should return EOF error.
        assert!(s.read_byte(None).is_err());
    }

    #[test]
    fn test_read_through_buffer() {
        let s = make_stream(b"hello world");
        s.fill(-1, None).unwrap();
        let mut out = [0u8; 5];
        let n = s.read(&mut out, None).unwrap();
        assert_eq!(n, 5);
        assert_eq!(&out[..n], b"hello");
        assert_eq!(s.get_available(), 6);
    }

    #[test]
    fn test_read_drains_buffer_then_base() {
        // Buffer size 3 — first read fills 3 bytes; second dips into base stream.
        let s = make_stream_sized(b"abcdef", 3);
        s.fill(-1, None).unwrap(); // fills "abc"
        let mut out = [0u8; 3];
        s.read(&mut out, None).unwrap();
        assert_eq!(&out, b"abc");
        // Buffer exhausted — next read comes from base stream directly.
        s.read(&mut out, None).unwrap();
        assert_eq!(&out, b"def");
    }

    #[test]
    fn test_set_buffer_size_clears_buffer() {
        let s = make_stream(b"hello");
        s.fill(-1, None).unwrap();
        assert_eq!(s.get_available(), 5);
        s.set_buffer_size(16);
        assert_eq!(s.get_buffer_size(), 16);
        // Buffered data was discarded.
        assert_eq!(s.get_available(), 0);
    }

    #[test]
    fn test_skip_within_buffer() {
        let s = make_stream(b"abcde");
        s.fill(-1, None).unwrap();
        let skipped = s.skip(3, None).unwrap();
        assert_eq!(skipped, 3);
        assert_eq!(s.get_available(), 2);
        let b = s.read_byte(None).unwrap();
        assert_eq!(b, b'd');
    }

    #[test]
    fn test_skip_across_buffer_and_base() {
        // 4-byte buffer, 8-byte source; skip 6 bytes total.
        let s = make_stream_sized(b"abcdefgh", 4);
        s.fill(-1, None).unwrap(); // fills "abcd"
        let skipped = s.skip(6, None).unwrap();
        assert_eq!(skipped, 6); // 4 from buffer + 2 from base
    }
}
