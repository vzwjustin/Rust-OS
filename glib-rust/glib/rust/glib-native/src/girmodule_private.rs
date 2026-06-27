//! `girmodule_private` matching `girepository/girmodule-private.h`.
//!
//! Private internal API for `GirModule`.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::girmodule::Module;
use crate::prelude::*;
use alloc::string::String;

/// Sets the shared library (internal).
pub fn set_shared_library(module: &mut Module, lib: &str) {
    module.shared_library = lib.into();
}

/// Sets the C prefix (internal).
pub fn set_c_prefix(module: &mut Module, prefix: &str) {
    module.c_prefix = prefix.into();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_set_shared_library() {
        let mut m = Module::new("Gio", "2.0");
        set_shared_library(&mut m, "libgio-2.0.so");
        assert_eq!(m.shared_library, "libgio-2.0.so");
    }

    #[test]
    fn test_set_c_prefix() {
        let mut m = Module::new("Gio", "2.0");
        set_c_prefix(&mut m, "G");
        assert_eq!(m.c_prefix, "G");
    }
}
