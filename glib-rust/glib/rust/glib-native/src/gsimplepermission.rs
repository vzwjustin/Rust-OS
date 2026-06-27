//! GSimplePermission matching `gio/gsimplepermission.h` / `gsimplepermission.c`.
//!
//! Upstream `GSimplePermission` is a `GPermission` subclass that represents
//! a permission that is either allowed or not, with no way to acquire or
//! release it. We port it as a thin wrapper around `Permission`.
//!
//! Fully `no_std` compatible.

use crate::gpermission::Permission;

/// A simple permission (`GSimplePermission`).
///
/// Represents a permission that is either allowed or not.
/// Cannot be acquired or released.
pub struct SimplePermission {
    perm: Permission,
}

impl SimplePermission {
    /// Creates a new simple permission with the given `allowed` state.
    ///
    /// Mirrors `g_simple_permission_new`.
    pub fn new(allowed: bool) -> Self {
        let perm = Permission::new();
        perm.impl_update(allowed, false, false);
        Self { perm }
    }

    /// Gets whether the permission is allowed.
    pub fn get_allowed(&self) -> bool {
        self.perm.get_allowed()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_permission_allowed() {
        let perm = SimplePermission::new(true);
        assert!(perm.get_allowed());
    }

    #[test]
    fn test_simple_permission_not_allowed() {
        let perm = SimplePermission::new(false);
        assert!(!perm.get_allowed());
    }

    #[test]
    fn test_simple_permission_cannot_acquire_or_release() {
        let perm = SimplePermission::new(true);
        assert!(perm.perm.acquire(None).is_err());
        assert!(perm.perm.release(None).is_err());
    }
}
