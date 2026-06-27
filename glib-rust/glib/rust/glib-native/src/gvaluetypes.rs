//! `gvaluetypes.c` compatibility facade.

pub use crate::gvalue::GValue;

/// Create a zeroed `GValue`.
#[must_use]
pub fn value_init() -> GValue {
    GValue::new()
}

#[cfg(test)]
mod tests {
    use super::value_init;

    #[test]
    fn exposes_gvalue_accessors() {
        let mut value = value_init();
        value.set_int(42);

        assert_eq!(value.get_int(), 42);
    }
}
