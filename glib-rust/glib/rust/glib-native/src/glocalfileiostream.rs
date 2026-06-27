//! GLocalFileIOStream matching `gio/glocalfileiostream.h`.
//! A `GIOStream` for local files combining read and write. In this
//! no_std port we model it with in-memory buffers.
//! Fully `no_std` compatible using `alloc`.

use alloc::vec::Vec;
use spin::Mutex;

/// A local file IO stream (`GLocalFileIOStream`).
pub struct LocalFileIOStream {
    data: Mutex<Vec<u8>>,
    read_pos: Mutex<usize>,
    closed: Mutex<bool>,
}

impl LocalFileIOStream {
    pub fn new(data: Vec<u8>) -> Self {
        Self {
            data: Mutex::new(data),
            read_pos: Mutex::new(0),
            closed: Mutex::new(false),
        }
    }

    pub fn read(&self, buf: &mut [u8]) -> usize {
        if *self.closed.lock() {
            return 0;
        }
        let mut data = self.data.lock();
        let mut pos = self.read_pos.lock();
        let available = data.len().saturating_sub(*pos);
        let count = buf.len().min(available);
        buf[..count].copy_from_slice(&data[*pos..*pos + count]);
        *pos += count;
        count
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_read_write() {
        let s = LocalFileIOStream::new(b"initial".to_vec());
        let mut buf = [0u8; 3];
        assert_eq!(s.read(&mut buf), 3);
        assert_eq!(&buf, b"ini");
        assert_eq!(s.write(b"_appended"), 9);
        assert_eq!(s.get_data(), b"initial_appended".to_vec());
    }
}
