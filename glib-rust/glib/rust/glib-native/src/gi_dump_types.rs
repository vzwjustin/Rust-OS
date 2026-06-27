//! `gi_dump_types` matching `girepository/gi-dump-types.c`.
//!
//! Type dumper utility: prints all types in a typelib.
//! Stubbed in no_std.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::prelude::*;
use alloc::string::String;

/// Dumps all types from a typelib namespace (mirrors `gi_dump_types`).
/// No-op in our no_std port.
pub fn dump_types(_namespace: &str) -> String {
    String::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dump_types() {
        assert_eq!(dump_types("Gio"), "");
    }
}
