//! GDBusNameWatching matching `gio/gdbusnamewatching.h`.
//!
//! Utilities for watching D-Bus names. In this no_std port we model
//! name watching state with a simple registry.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use spin::Mutex;

/// Flags for name watching.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BusNameWatcherFlags(pub u32);

impl BusNameWatcherFlags {
    pub const NONE: Self = Self(0);
    pub const AUTO_START: Self = Self(1 << 0);

    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

/// Name watch state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameWatchState {
    Vanished,
    Appeared,
}

/// A name watch entry.
struct NameWatch {
    name: String,
    state: NameWatchState,
    owner: Option<String>,
}

/// A D-Bus name watching tracker (`g_bus_watch_name` family).
pub struct DBusNameWatching {
    watches: Mutex<BTreeMap<u32, NameWatch>>,
    next_id: Mutex<u32>,
}

impl DBusNameWatching {
    /// Creates a new name watching tracker.
    pub fn new() -> Self {
        Self {
            watches: Mutex::new(BTreeMap::new()),
            next_id: Mutex::new(1),
        }
    }

    /// Watches a name.
    ///
    /// Mirrors `g_bus_watch_name` (simplified — returns a watch ID).
    pub fn watch_name(&self, name: &str, _flags: BusNameWatcherFlags) -> u32 {
        let mut id = self.next_id.lock();
        let watch_id = *id;
        *id += 1;

        self.watches.lock().insert(
            watch_id,
            NameWatch {
                name: name.to_string(),
                state: NameWatchState::Vanished,
                owner: None,
            },
        );
        watch_id
    }

    /// Unwatches a name by watch ID.
    ///
    /// Mirrors `g_bus_unwatch_name`.
    pub fn unwatch_name(&self, watch_id: u32) -> bool {
        self.watches.lock().remove(&watch_id).is_some()
    }

    /// Marks a name as appeared with an owner.
    pub fn name_appeared(&self, watch_id: u32, owner: &str) -> bool {
        let mut watches = self.watches.lock();
        if let Some(w) = watches.get_mut(&watch_id) {
            w.state = NameWatchState::Appeared;
            w.owner = Some(owner.to_string());
            true
        } else {
            false
        }
    }

    /// Marks a name as vanished.
    pub fn name_vanished(&self, watch_id: u32) -> bool {
        let mut watches = self.watches.lock();
        if let Some(w) = watches.get_mut(&watch_id) {
            w.state = NameWatchState::Vanished;
            w.owner = None;
            true
        } else {
            false
        }
    }

    /// Gets the watch state.
    pub fn get_watch_state(&self, watch_id: u32) -> Option<NameWatchState> {
        self.watches.lock().get(&watch_id).map(|w| w.state)
    }

    /// Gets the owner of a watched name.
    pub fn get_name_owner(&self, watch_id: u32) -> Option<String> {
        self.watches
            .lock()
            .get(&watch_id)
            .and_then(|w| w.owner.clone())
    }

    /// Returns the number of active watches.
    pub fn watch_count(&self) -> usize {
        self.watches.lock().len()
    }
}

impl Default for DBusNameWatching {
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
        let watching = DBusNameWatching::new();
        assert_eq!(watching.watch_count(), 0);
    }

    #[test]
    fn test_watch_name() {
        let watching = DBusNameWatching::new();
        let id = watching.watch_name("org.test.Name", BusNameWatcherFlags::NONE);
        assert!(id > 0);
        assert_eq!(watching.watch_count(), 1);
        assert_eq!(watching.get_watch_state(id), Some(NameWatchState::Vanished));
    }

    #[test]
    fn test_name_appeared() {
        let watching = DBusNameWatching::new();
        let id = watching.watch_name("org.test.Name", BusNameWatcherFlags::NONE);
        watching.name_appeared(id, ":1.42");
        assert_eq!(watching.get_watch_state(id), Some(NameWatchState::Appeared));
        assert_eq!(watching.get_name_owner(id), Some(":1.42".to_string()));
    }

    #[test]
    fn test_name_vanished() {
        let watching = DBusNameWatching::new();
        let id = watching.watch_name("org.test.Name", BusNameWatcherFlags::NONE);
        watching.name_appeared(id, ":1.42");
        watching.name_vanished(id);
        assert_eq!(watching.get_watch_state(id), Some(NameWatchState::Vanished));
        assert!(watching.get_name_owner(id).is_none());
    }

    #[test]
    fn test_unwatch() {
        let watching = DBusNameWatching::new();
        let id = watching.watch_name("org.test.Name", BusNameWatcherFlags::NONE);
        assert!(watching.unwatch_name(id));
        assert_eq!(watching.watch_count(), 0);
        assert!(!watching.unwatch_name(id));
    }
}
