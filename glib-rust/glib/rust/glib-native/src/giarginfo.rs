//! `giarginfo` matching `girepository/giarginfo.h`.
//!
//! Argument info: describes a single callable argument.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::gitypes::{GIDirection, GIScopeType, GITransfer};

/// Argument info (mirrors `GIArgInfo`).
#[derive(Debug, Clone)]
pub struct ArgInfo {
    pub direction: GIDirection,
    pub return_value: bool,
    pub optional: bool,
    pub caller_allocates: bool,
    pub may_be_null: bool,
    pub skip: bool,
    pub ownership_transfer: GITransfer,
    pub scope: GIScopeType,
    pub closure_index: Option<u32>,
    pub destroy_index: Option<u32>,
}

impl ArgInfo {
    /// Creates a new argument info with defaults.
    pub fn new() -> Self {
        Self {
            direction: GIDirection::In,
            return_value: false,
            optional: false,
            caller_allocates: false,
            may_be_null: false,
            skip: false,
            ownership_transfer: GITransfer::Nothing,
            scope: GIScopeType::Invalid,
            closure_index: None,
            destroy_index: None,
        }
    }

    /// Returns the direction (mirrors `gi_arg_info_get_direction`).
    pub fn direction(&self) -> GIDirection {
        self.direction
    }

    /// Returns whether it's a return value (mirrors `gi_arg_info_is_return_value`).
    pub fn is_return_value(&self) -> bool {
        self.return_value
    }

    /// Returns whether optional (mirrors `gi_arg_info_is_optional`).
    pub fn is_optional(&self) -> bool {
        self.optional
    }

    /// Returns whether caller allocates (mirrors `gi_arg_info_is_caller_allocates`).
    pub fn is_caller_allocates(&self) -> bool {
        self.caller_allocates
    }

    /// Returns whether may be null (mirrors `gi_arg_info_may_be_null`).
    pub fn may_be_null(&self) -> bool {
        self.may_be_null
    }

    /// Returns whether to skip (mirrors `gi_arg_info_is_skip`).
    pub fn is_skip(&self) -> bool {
        self.skip
    }

    /// Returns ownership transfer (mirrors `gi_arg_info_get_ownership_transfer`).
    pub fn ownership_transfer(&self) -> GITransfer {
        self.ownership_transfer
    }

    /// Returns the scope (mirrors `gi_arg_info_get_scope`).
    pub fn scope(&self) -> GIScopeType {
        self.scope
    }

    /// Returns the closure index (mirrors `gi_arg_info_get_closure_index`).
    pub fn closure_index(&self) -> Option<u32> {
        self.closure_index
    }

    /// Returns the destroy index (mirrors `gi_arg_info_get_destroy_index`).
    pub fn destroy_index(&self) -> Option<u32> {
        self.destroy_index
    }
}

impl Default for ArgInfo {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let arg = ArgInfo::new();
        assert_eq!(arg.direction(), GIDirection::In);
        assert!(!arg.is_return_value());
        assert!(!arg.is_optional());
        assert_eq!(arg.ownership_transfer(), GITransfer::Nothing);
        assert_eq!(arg.scope(), GIScopeType::Invalid);
    }

    #[test]
    fn test_custom() {
        let mut arg = ArgInfo::new();
        arg.direction = GIDirection::Out;
        arg.optional = true;
        arg.may_be_null = true;
        assert_eq!(arg.direction(), GIDirection::Out);
        assert!(arg.is_optional());
        assert!(arg.may_be_null());
    }
}
