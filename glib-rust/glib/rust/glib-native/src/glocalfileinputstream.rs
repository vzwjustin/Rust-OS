//! GLocalFileInputStream matching `gio/glocalfileinputstream.h`.
//! A `GInputStream` for local files. In this no_std port we model it
//! with an in-memory buffer.
//! Fully `no_std` compatible using `alloc`.

use alloc::vec::Vec;
use spin::Mutex;

/// A local file input stream (`GLocalFileInputStream`).
pub struct LocalFileInputStream {
    data: Mutex<Vec<u8>>,
    position: Mutex<usize>,
    closed: Mutex<bool>,
}

impl LocalFileInputStream {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data: Mutex::new(data),
            position: Mutex::new(0),
            closed: Mutex::new(false),
        }
    }

    pub fn read(&self, buf: &mut [u8]) -> usize {
        if *self.closed.lock() {
            return 0;
        }
        let data = self.data.lock();
        let mut pos = self.position.lock();
        let available = data.len().saturating_sub(*pos);
        let count = buf.len().min(available);
        buf[..count].copy_from_slice(&data[*pos..*pos + count]);
        *pos += count;
        count
    }

    pub fn close(&self) {
        *self.closed.lock() = true;
    }
    pub fn is_closed(&self) -> bool {
        *self.closed.lock()
    }
    pub fn get_size(&self) -> usize {
        self.data.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read() {
        let s = LocalFileInputStream::new(b"hello world".to_vec());
        let mut buf = [0u8; 5];
        assert_eq!(s.read(&mut buf), 5);
        assert_eq!(&buf, b"hello");
    }

    #[test]
    fn test_close() {
        let s = LocalFileInputStream::new(b"data".to_vec());
        s.close();
        let mut buf = [0u8; 4];
        assert_eq!(s.read(&mut buf), 0);
    }
}
