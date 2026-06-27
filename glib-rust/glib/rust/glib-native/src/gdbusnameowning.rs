//! GDBusNameOwning matching `gio/gdbusnameowning.h`.
//!
//! Utilities for owning D-Bus names. In this no_std port we model
//! name ownership state with a simple registry.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use spin::Mutex;

/// Flags for name ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusNameOwnerFlags(pub u32);

impl BusNameOwnerFlags {
    pub const NONE: Self = Self(0);
    pub const ALLOW_REPLACEMENT: Self = Self(1 << 0);
    pub const REPLACE: Self = Self(1 << 1);
    pub const DO_NOT_QUEUE: Self = Self(1 << 2);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Name ownership state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameOwnerState {
    Unowned,
    Queued,
    Owned,
    Lost,
}

/// A name ownership entry.
struct NameOwner {
    state: NameOwnerState,
    flags: BusNameOwnerFlags,
}

/// A D-Bus name ownership tracker (`g_bus_own_name` family).
pub struct DBusNameOwning {
    owners: Mutex<BTreeMap<String, NameOwner>>,
    next_id: Mutex<u32>,
    id_to_name: Mutex<BTreeMap<u32, String>>,
}

impl DBusNameOwning {
    /// Creates a new name ownership tracker.
    pub fn new() -> Self {
        Self {
            owners: Mutex::new(BTreeMap::new()),
            next_id: Mutex::new(1),
            id_to_name: Mutex::new(BTreeMap::new()),
        }
    }

    /// Owns a name.
    ///
    /// Mirrors `g_bus_own_name` (simplified — returns a watch ID).
    pub fn own_name(&self, name: &str, flags: BusNameOwnerFlags) -> u32 {
        let mut id = self.next_id.lock();
        let watch_id = *id;
        *id += 1;

        self.owners.lock().insert(
            name.to_string(),
            NameOwner {
                state: NameOwnerState::Owned,
                flags,
            },
        );
        self.id_to_name.lock().insert(watch_id, name.to_string());
        watch_id
    }

    /// Unowns a name by watch ID.
    ///
    /// Mirrors `g_bus_unown_name`.
    pub fn unown_name(&self, watch_id: u32) -> bool {
        let name = self.id_to_name.lock().remove(&watch_id);
        if let Some(n) = name {
            self.owners.lock().remove(&n);
            true
        } else {
            false
        }
    }

    /// Gets the state of a name.
    pub fn get_name_state(&self, name: &str) -> NameOwnerState {
        self.owners
            .lock()
            .get(name)
            .map(|o| o.state)
            .unwrap_or(NameOwnerState::Unowned)
    }

    /// Returns the number of owned names.
    pub fn owned_count(&self) -> usize {
        self.owners.lock().len()
    }
}

impl Default for DBusNameOwning {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let owning = DBusNameOwning::new();
        assert_eq!(owning.owned_count(), 0);
    }

    #[test]
    fn test_own_name() {
        let owning = DBusNameOwning::new();
        let id = owning.own_name("org.test.Name", BusNameOwnerFlags::NONE);
        assert!(id > 0);
        assert_eq!(
            owning.get_name_state("org.test.Name"),
            NameOwnerState::Owned
        );
        assert_eq!(owning.owned_count(), 1);
    }

    #[test]
    fn test_unown_name() {
        let owning = DBusNameOwning::new();
        let id = owning.own_name("org.test.Name", BusNameOwnerFlags::NONE);
        assert!(owning.unown_name(id));
        assert_eq!(
            owning.get_name_state("org.test.Name"),
            NameOwnerState::Unowned
        );
    }

    #[test]
    fn test_unown_invalid_id() {
        let owning = DBusNameOwning::new();
        assert!(!owning.unown_name(999));
    }

    #[test]
    fn test_multiple_owners() {
        let owning = DBusNameOwning::new();
        owning.own_name("org.test.A", BusNameOwnerFlags::NONE);
        owning.own_name("org.test.B", BusNameOwnerFlags::REPLACE);
        assert_eq!(owning.owned_count(), 2);
    }
}
