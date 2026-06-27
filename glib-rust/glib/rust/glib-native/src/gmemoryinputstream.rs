//! GMemoryInputStream matching `gio/gmemoryinputstream.h`.
//!
//! The canonical implementation lives in [`crate::ginputstream::MemoryInputStream`],
//! which also implements the [`crate::ginputstream::InputStream`] wrapper API.
//! This module re-exports that type so callers can use the same header-oriented
//! import path as upstream GIO.

pub use crate::ginputstream::MemoryInputStream;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::Bytes;
    use crate::ginputstream::InputStream;

    #[test]
    fn reexport_read_path() {
        let stream = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_static(
            b"rustos",
        )));
        let mut buf = [0u8; 6];

        assert_eq!(stream.read(&mut buf, None).unwrap(), 6);
        assert_eq!(&buf, b"rustos");
    }
}
