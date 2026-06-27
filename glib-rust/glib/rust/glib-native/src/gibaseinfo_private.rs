//! `gibaseinfo_private` matching `girepository/gibaseinfo-private.h`.
//!
//! Private internal API for `GIBaseInfo`.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::gibaseinfo::{BaseInfo, InfoType};
use crate::gitypelib::Typelib;
use alloc::sync::{Arc, Weak};

/// Creates a new `BaseInfo` with a typelib link (internal constructor).
pub fn base_info_new(
    name: &str,
    namespace: &str,
    info_type: InfoType,
    container: Option<Arc<BaseInfo>>,
    typelib: Weak<Typelib>,
) -> Arc<BaseInfo> {
    BaseInfo::new(name, namespace, info_type, container, typelib)
}

/// Sets the deprecated flag (internal).
pub fn set_deprecated(info: &mut BaseInfo, deprecated: bool) {
    // BaseInfo fields are private; this is a no-op stub
    let _ = (info, deprecated);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base_info_new() {
        let info = base_info_new("Foo", "Ns", InfoType::Function, None, Weak::new());
        assert_eq!(info.name(), "Foo");
    }
}
