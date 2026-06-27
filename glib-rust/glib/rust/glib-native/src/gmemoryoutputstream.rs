//! GMemoryOutputStream matching `gio/gmemoryoutputstream.h`.
//!
//! The canonical implementation lives in [`crate::goutputstream::MemoryOutputStream`],
//! which also implements the [`crate::goutputstream::OutputStream`] wrapper API.
//! This module re-exports that type so callers can use the same header-oriented
//! import path as upstream GIO.

pub use crate::goutputstream::MemoryOutputStream;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::goutputstream::OutputStream;

    #[test]
    fn reexport_write_path() {
        let mem = MemoryOutputStream::new_resizable();
        let stream = OutputStream::from(mem);

        assert_eq!(stream.write(b"rust", None).unwrap(), 4);
        assert_eq!(stream.write(b"os", None).unwrap(), 2);
        assert_eq!(
            stream
                .downcast_ref::<MemoryOutputStream>()
                .unwrap()
                .get_data(),
            b"rustos"
        );
    }
}
