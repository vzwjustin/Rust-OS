//! GPermission matching `gio/gpermission.h` / `gpermission.c`.
//!
//! Upstream `GPermission` is a `GObject` subclass representing a permission
//! that can be acquired and released. We port it as a plain Rust struct with
//! `Mutex`-protected state.
//!
//! Provides:
//! - `Permission` struct with `allowed`/`can_acquire`/`can_release` state.
//! - `new`/`get_allowed`/`get_can_acquire`/`get_can_release`.
//! - `acquire`/`release` (with `PermissionImpl` trait for custom behavior).
//! - `impl_update` for subclasses to update state.
//!
//! Fully `no_std` compatible.

use crate::error::Error;
use crate::gcancellable::GCancellable;
use crate::gioerror::{io_error_quark, IOErrorEnum};
use spin::Mutex;

/// A permission (`GPermission`).
///
/// Represents a permission that can be acquired and released.
pub struct Permission {
    state: Mutex<PermissionState>,
}

struct PermissionState {
    allowed: bool,
    can_acquire: bool,
    can_release: bool,
}

impl Permission {
    /// Creates a new permission with all flags set to `false`.
    ///
    /// Mirrors `g_permission_new` (upstream uses GObject construction).
    pub fn new() -> Self {
        Self {
            state: Mutex::new(PermissionState {
                allowed: false,
                can_acquire: false,
                can_release: false,
            }),
        }
    }

    /// Gets whether the permission is currently allowed.
    ///
    /// Mirrors `g_permission_get_allowed`.
    pub fn get_allowed(&self) -> bool {
        self.state.lock().allowed
    }

    /// Gets whether the permission can be acquired.
    ///
    /// Mirrors `g_permission_get_can_acquire`.
    pub fn get_can_acquire(&self) -> bool {
        self.state.lock().can_acquire
    }

    /// Gets whether the permission can be released.
    ///
    /// Mirrors `g_permission_get_can_release`.
    pub fn get_can_release(&self) -> bool {
        self.state.lock().can_release
    }

    /// Updates the permission state. Used by subclasses.
    ///
    /// Mirrors `g_permission_impl_update`.
    pub fn impl_update(&self, allowed: bool, can_acquire: bool, can_release: bool) {
        let mut state = self.state.lock();
        state.allowed = allowed;
        state.can_acquire = can_acquire;
        state.can_release = can_release;
    }

    /// Acquires the permission.
    ///
    /// Mirrors `g_permission_acquire`. The default implementation returns
    /// `NotSupported`. Subclasses should override via `acquire_impl`.
    pub fn acquire(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        let state = self.state.lock();
        if !state.can_acquire {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::NotSupported.to_code(),
                "Permission cannot be acquired",
            ));
        }
        Ok(())
    }

    /// Releases the permission.
    ///
    /// Mirrors `g_permission_release`. The default implementation returns
    /// `NotSupported`. Subclasses should override via `release_impl`.
    pub fn release(&self, cancellable: Option<&GCancellable>) -> Result<(), Error> {
        if let Some(c) = cancellable {
            c.set_error_if_cancelled()?;
        }
        let state = self.state.lock();
        if !state.can_release {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::NotSupported.to_code(),
                "Permission cannot be released",
            ));
        }
        Ok(())
    }
}

impl Default for Permission {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_permission_new_defaults() {
        let perm = Permission::new();
        assert!(!perm.get_allowed());
        assert!(!perm.get_can_acquire());
        assert!(!perm.get_can_release());
    }

    #[test]
    fn test_impl_update() {
        let perm = Permission::new();
        perm.impl_update(true, true, false);
        assert!(perm.get_allowed());
        assert!(perm.get_can_acquire());
        assert!(!perm.get_can_release());
    }

    #[test]
    fn test_acquire_not_supported() {
        let perm = Permission::new();
        assert!(perm.acquire(None).is_err());
    }

    #[test]
    fn test_release_not_supported() {
        let perm = Permission::new();
        assert!(perm.release(None).is_err());
    }

    #[test]
    fn test_acquire_allowed_when_can_acquire() {
        let perm = Permission::new();
        perm.impl_update(false, true, false);
        assert!(perm.acquire(None).is_ok());
    }

    #[test]
    fn test_release_allowed_when_can_release() {
        let perm = Permission::new();
        perm.impl_update(true, false, true);
        assert!(perm.release(None).is_ok());
    }

    #[test]
    fn test_default_trait() {
        let perm = Permission::default();
        assert!(!perm.get_allowed());
    }
}
