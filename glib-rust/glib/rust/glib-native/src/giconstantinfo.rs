//! `giconstantinfo` matching `girepository/giconstantinfo.h`.
//!
//! Constant info: describes a constant value.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::gitypes::GIArgument;

/// Constant info (mirrors `GIConstantInfo`).
#[derive(Debug, Clone, Default)]
pub struct ConstantInfo {
    pub value: GIArgument,
}

impl ConstantInfo {
    /// Creates a new constant info.
    pub fn new() -> Self {
        Self::default()
    }

    /// Gets the value (mirrors `gi_constant_info_get_value`).
    pub fn get_value(&self) -> &GIArgument {
        &self.value
    }

    /// Frees the value (mirrors `gi_constant_info_free_value`).
    /// No-op in Rust since `GIArgument` owns its data.
    pub fn free_value(&self, _value: GIArgument) {}

    /// Sets the value.
    pub fn set_value(&mut self, value: GIArgument) {
        self.value = value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let ci = ConstantInfo::new();
        let _ = ci.get_value();
    }

    #[test]
    fn test_set_value() {
        let mut ci = ConstantInfo::new();
        ci.set_value(GIArgument::new_int32(42));
        assert_eq!(ci.get_value().v_int32, 42);
    }
}
