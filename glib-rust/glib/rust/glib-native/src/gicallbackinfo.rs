//! `gicallbackinfo` matching `girepository/gicallbackinfo.h`.
//!
//! Callback info: describes a callback type.
//! Extends `CallableInfo`.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::gicallableinfo::CallableInfo;

/// Callback info (mirrors `GICallbackInfo`).
#[derive(Debug, Clone, Default)]
pub struct CallbackInfo {
    pub callable: CallableInfo,
}

impl CallbackInfo {
    /// Creates a new callback info.
    pub fn new() -> Self {
        Self {
            callable: CallableInfo::new(),
        }
    }

    /// Delegates to the inner `CallableInfo`.
    pub fn callable(&self) -> &CallableInfo {
        &self.callable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let cb = CallbackInfo::new();
        assert!(!cb.callable().is_method());
        assert_eq!(cb.callable().n_args(), 0);
    }
}
