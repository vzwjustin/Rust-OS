//! GSeekable interface matching `gio/gseekable.h` / `gio/gseekable.c`.
//!
//! Provides the `SeekType` enum and `Seekable` trait.
//!
//! Fully `no_std` compatible.

use crate::error::Error;
use crate::gcancellable::GCancellable;

pub use crate::iochannel::SeekType;

/// A trait for seekable streams (`GSeekable`).
pub trait Seekable {
    /// Checks if the stream supports seeking.
    ///
    /// Mirrors `g_seekable_can_seek`.
    fn can_seek(&self) -> bool;

    /// Seeks the stream.
    ///
    /// Mirrors `g_seekable_seek`.
    fn seek(
        &self,
        offset: i64,
        type_: SeekType,
        cancellable: Option<&GCancellable>,
    ) -> Result<(), Error>;

    /// Checks if the stream supports truncating.
    ///
    /// Mirrors `g_seekable_can_truncate`.
    fn can_truncate(&self) -> bool;

    /// Truncates the stream.
    ///
    /// Mirrors `g_seekable_truncate`.
    fn truncate(&self, offset: i64, cancellable: Option<&GCancellable>) -> Result<(), Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::Bytes;
    use crate::ginputstream::{InputStream, MemoryInputStream};

    #[test]
    fn seek_type_reexport_works_with_memory_input_stream() {
        let stream = InputStream::from(MemoryInputStream::new_from_bytes(Bytes::from_static(
            b"abcdef",
        )));
        stream.seek(2, SeekType::Set, None).unwrap();

        let mut buf = [0u8; 2];
        assert_eq!(stream.read(&mut buf, None).unwrap(), 2);
        assert_eq!(&buf, b"cd");
    }
}
