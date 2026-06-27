//! GLocalFileOutputStream matching `gio/glocalfileoutputstream.h`.
//! A `GOutputStream` for local files. In this no_std port we model it
//! with an in-memory buffer.
//! Fully `no_std` compatible using `alloc`.

use alloc::vec::Vec;
use spin::Mutex;

/// A local file output stream (`GLocalFileOutputStream`).
pub struct LocalFileOutputStream {
    data: Mutex<Vec<u8>>,
    closed: Mutex<bool>,
}

impl LocalFileOutputStream {
    pub fn new() -> Self {
        Self {
            data: Mutex::new(Vec::new()),
            closed: Mutex::new(false),
        }
    }

    pub fn write(&self, buf: &[u8]) -> usize {
        if *self.closed.lock() {
            return 0;
        }
        self.data.lock().extend_from_slice(buf);
        buf.len()
    }

    pub fn close(&self) {
        *self.closed.lock() = true;
    }
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
    pub fn get_data(&self) -> Vec<u8> {
        self.data.lock().clone()
    }
    pub fn size(&self) -> usize {
        self.data.lock().len()
    }
}

impl Default for LocalFileOutputStream {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write() {
        let s = LocalFileOutputStream::new();
        assert_eq!(s.write(b"hello"), 5);
        assert_eq!(s.get_data(), b"hello".to_vec());
    }

    #[test]
    fn test_close() {
        let s = LocalFileOutputStream::new();
        s.close();
        assert_eq!(s.write(b"data"), 0);
    }
}
