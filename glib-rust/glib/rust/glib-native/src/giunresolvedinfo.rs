//! `giunresolvedinfo` matching `girepository/giunresolvedinfo.h`.
//!
//! Unresolved info: placeholder for a type that could not be resolved.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use alloc::string::String;

/// Unresolved info (mirrors `GIUnresolvedInfo`).
#[derive(Debug, Clone, Default)]
pub struct UnresolvedInfo {
    pub name: String,
    pub namespace: String,
}

impl UnresolvedInfo {
    /// Creates a new unresolved info.
    pub fn new(name: &str, namespace: &str) -> Self {
        Self {
            name: name.into(),
            namespace: namespace.into(),
        }
    }

    /// Returns the name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the namespace.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let ui = UnresolvedInfo::new("Missing", "Ns");
        assert_eq!(ui.name(), "Missing");
        assert_eq!(ui.namespace(), "Ns");
    }
}
