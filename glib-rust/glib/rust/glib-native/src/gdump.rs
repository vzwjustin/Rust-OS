//! `gdump` matching `girepository/gdump.c`.
//!
//! Type dumper: generates introspection data from a loaded module.
//! Stubbed in no_std.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::prelude::*;
use alloc::string::String;

/// Dumps introspection data to a file (mirrors `gi_repository_dump`).
/// No-op in our no_std port.
pub fn dump(_input_filename: &str, _output_filename: &str) -> Result<(), String> {
    Err("Dump not supported in no_std".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dump_fails() {
        assert!(dump("input", "output").is_err());
    }
}
