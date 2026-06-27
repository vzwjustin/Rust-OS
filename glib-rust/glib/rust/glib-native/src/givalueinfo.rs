//! `givalueinfo` matching `girepository/givalueinfo.h`.
//!
//! Value info: a single enum/flags value.
//! Re-exports `ValueInfo` from `gienuminfo` for convenience.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

pub use crate::gienuminfo::ValueInfo;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_value_info() {
        let vi = ValueInfo::new("FOO", 0, "foo");
        assert_eq!(vi.name(), "FOO");
        assert_eq!(vi.value(), 0);
    }
}
