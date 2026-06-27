//! GLoadableIcon interface matching `gio/gloadableicon.h`.
//!
//! Upstream `GLoadableIcon` is a `GInterface` for icons that can be
//! loaded as an input stream. We port it as a Rust trait.
//!
//! Fully `no_std` compatible.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::ginputstream::InputStream;
use alloc::string::String;

/// Trait for loadable icons (`GLoadableIcon`).
pub trait LoadableIcon {
    /// Loads the icon as an input stream.
    ///
    /// Mirrors `g_loadable_icon_load`.
    fn load(
        &self,
        size: i32,
        cancellable: Option<&GCancellable>,
    ) -> Result<(InputStream, Option<String>), Error>;
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bytes::Bytes;
    use crate::ginputstream::MemoryInputStream;
    use alloc::string::ToString;

    struct TestLoadableIcon {
        data: &'static [u8],
        icon_type: &'static str,
    }

    impl LoadableIcon for TestLoadableIcon {
        fn load(
            &self,
            _size: i32,
            _cancellable: Option<&GCancellable>,
        ) -> Result<(InputStream, Option<String>), Error> {
            let bytes = Bytes::from_static(self.data);
            let stream = InputStream::new(MemoryInputStream::new_from_bytes(bytes));
            Ok((stream, Some(self.icon_type.to_string())))
        }
    }

    #[test]
    fn test_loadable_icon_load() {
        let icon = TestLoadableIcon {
            data: b"icon data",
            icon_type: "png",
        };
        let (stream, icon_type) = icon.load(48, None).unwrap();
        assert_eq!(icon_type.unwrap(), "png");
        let mut buf = [0u8; 9];
        let (n, _) = stream.read_all(&mut buf, None).unwrap();
        assert_eq!(n, 9);
        assert_eq!(&buf, b"icon data");
    }

    #[test]
    fn test_loadable_icon_load_no_type() {
        struct NoTypeIcon;
        impl LoadableIcon for NoTypeIcon {
            fn load(
                &self,
                _size: i32,
                _c: Option<&GCancellable>,
            ) -> Result<(InputStream, Option<String>), Error> {
                let bytes = Bytes::from_static(b"data");
                Ok((
                    InputStream::new(MemoryInputStream::new_from_bytes(bytes)),
                    None,
                ))
            }
        }
        let icon = NoTypeIcon;
        let (_, icon_type) = icon.load(0, None).unwrap();
        assert!(icon_type.is_none());
    }
}
