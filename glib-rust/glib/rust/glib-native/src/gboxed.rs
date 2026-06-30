//! Boxed type registration (`gboxed.c`).
//!
//! Boxed types are value-like `GType`s whose payload is copied and freed by
//! callbacks in C. The Rust port stores the registration metadata and uses the
//! existing `GType` registry for type identity; payload ownership is handled by
//! Rust values such as `Arc<T>` in [`crate::gvalue`].

use crate::gtype::{
    type_from_name, type_register_static, GType, GTypeFlags, GTypeInfo, G_TYPE_BOXED,
};
use alloc::collections::BTreeMap;
use alloc::string::String;
use spin::{Mutex, Once};

/// Type-erased boxed copy callback marker.
pub type BoxedCopyFunc = fn();

/// Type-erased boxed free callback marker.
pub type BoxedFreeFunc = fn();

/// Metadata recorded for a boxed type registration.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoxedTypeInfo {
    /// Registered `GType`.
    pub type_id: GType,
    /// Registered type name.
    pub name: String,
    /// Whether a copy callback was supplied.
    pub has_copy_func: bool,
    /// Whether a free callback was supplied.
    pub has_free_func: bool,
}

static BOXED_TYPES: Once<Mutex<BTreeMap<GType, BoxedTypeInfo>>> = Once::new();

fn boxed_types() -> &'static Mutex<BTreeMap<GType, BoxedTypeInfo>> {
    BOXED_TYPES.call_once(|| Mutex::new(BTreeMap::new()))
}

/// Register a static boxed type (`g_boxed_type_register_static`).
///
/// GLib stores raw copy/free callbacks for later `GValue` operations. This
/// no_std Rust layer stores callback presence as metadata; `GValue` boxed
/// payloads are owned by Rust smart pointers.
pub fn boxed_type_register_static(
    name: &str,
    copy_func: Option<BoxedCopyFunc>,
    free_func: Option<BoxedFreeFunc>,
) -> GType {
    if name.is_empty() {
        return 0;
    }

    let existing = type_from_name(name);
    if existing != 0 {
        return existing;
    }

    let type_id = type_register_static(G_TYPE_BOXED, name, &GTypeInfo::default(), GTypeFlags::NONE);
    if type_id == 0 {
        return 0;
    }

    boxed_types().lock().insert(
        type_id,
        BoxedTypeInfo {
            type_id,
            name: String::from(name),
            has_copy_func: copy_func.is_some(),
            has_free_func: free_func.is_some(),
        },
    );
    type_id
}

/// Return recorded metadata for a boxed type.
#[must_use]
pub fn boxed_type_info(type_id: GType) -> Option<BoxedTypeInfo> {
    boxed_types().lock().get(&type_id).cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gtype::{type_is_a, type_name};

    fn copy_marker() {}
    fn free_marker() {}

    #[test]
    fn registers_boxed_type() {
        let type_id =
            boxed_type_register_static("RustNativeBoxed", Some(copy_marker), Some(free_marker));

        assert_ne!(type_id, 0);
        assert!(type_is_a(type_id, G_TYPE_BOXED));
        assert_eq!(type_name(type_id).as_deref(), Some("RustNativeBoxed"));

        let info = boxed_type_info(type_id).unwrap();
        assert!(info.has_copy_func);
        assert!(info.has_free_func);
    }

    #[test]
    fn duplicate_name_returns_existing_type() {
        let first = boxed_type_register_static("RustNativeBoxedDup", None, None);
        let second = boxed_type_register_static("RustNativeBoxedDup", None, None);

        assert_eq!(first, second);
    }
}
