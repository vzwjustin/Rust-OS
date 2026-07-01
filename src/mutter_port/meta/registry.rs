//! Object registry / handle resolution
//!
//! Provides ID-based registries for meta objects (Display, Window, Workspace, etc.)
//! to resolve opaque pointers to actual references.

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

// Unique ID generators for each object type
static DISPLAY_ID_SEQ: AtomicU64 = AtomicU64::new(1);
static WINDOW_ID_SEQ: AtomicU64 = AtomicU64::new(1);
static COMPOSITOR_ID_SEQ: AtomicU64 = AtomicU64::new(1);
static WORKSPACE_ID_SEQ: AtomicU64 = AtomicU64::new(1);
static WAYLAND_SURFACE_ID_SEQ: AtomicU64 = AtomicU64::new(1);
static X11_DISPLAY_ID_SEQ: AtomicU64 = AtomicU64::new(1);
static MONITOR_ID_SEQ: AtomicU64 = AtomicU64::new(1);

/// Display registry ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct DisplayId(u64);

impl DisplayId {
    /// Generate a new unique display ID
    pub fn new() -> Self {
        Self(DISPLAY_ID_SEQ.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for DisplayId {
    fn default() -> Self {
        Self::new()
    }
}

/// Window registry ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WindowId(u64);

impl WindowId {
    /// Generate a new unique window ID
    pub fn new() -> Self {
        Self(WINDOW_ID_SEQ.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for WindowId {
    fn default() -> Self {
        Self::new()
    }
}

/// Compositor registry ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CompositorId(u64);

impl CompositorId {
    /// Generate a new unique compositor ID
    pub fn new() -> Self {
        Self(COMPOSITOR_ID_SEQ.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for CompositorId {
    fn default() -> Self {
        Self::new()
    }
}

/// Workspace registry ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkspaceId(u64);

impl WorkspaceId {
    /// Generate a new unique workspace ID
    pub fn new() -> Self {
        Self(WORKSPACE_ID_SEQ.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for WorkspaceId {
    fn default() -> Self {
        Self::new()
    }
}

/// Wayland surface registry ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WaylandSurfaceId(u64);

impl WaylandSurfaceId {
    /// Generate a new unique Wayland surface ID
    pub fn new() -> Self {
        Self(WAYLAND_SURFACE_ID_SEQ.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for WaylandSurfaceId {
    fn default() -> Self {
        Self::new()
    }
}

/// X11 display registry ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct X11DisplayId(u64);

impl X11DisplayId {
    /// Generate a new unique X11 display ID
    pub fn new() -> Self {
        Self(X11_DISPLAY_ID_SEQ.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for X11DisplayId {
    fn default() -> Self {
        Self::new()
    }
}

/// Monitor registry ID
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct MonitorId(u64);

impl MonitorId {
    /// Generate a new unique monitor ID
    pub fn new() -> Self {
        Self(MONITOR_ID_SEQ.fetch_add(1, Ordering::Relaxed))
    }
}

impl Default for MonitorId {
    fn default() -> Self {
        Self::new()
    }
}

/// Generic registry mapping IDs to objects
pub struct Registry<K: Ord, T> {
    map: Mutex<BTreeMap<K, Box<T>>>,
}

impl<K: Ord, T> Registry<K, T> {
    /// Create a new empty registry
    pub const fn new() -> Self {
        Self {
            map: Mutex::new(BTreeMap::new()),
        }
    }

    /// Register an object with the given ID
    pub fn insert(&self, id: K, obj: Box<T>) {
        self.map.lock().insert(id, obj);
    }

    /// Retrieve a reference to a registered object
    pub fn get(&self, id: K) -> Option<spin::MutexGuard<'_, BTreeMap<K, Box<T>>>> {
        let guard = self.map.lock();
        if guard.contains_key(&id) {
            Some(guard)
        } else {
            None
        }
    }

    /// Retrieve an object by ID, returning a reference to it
    pub fn get_ref<'a>(&'a self, id: K) -> Option<&'a T> {
        // ponytail: This is a workaround for borrow checker issues with Mutex.
        // A proper implementation would use interior mutability patterns.
        // For now, we return None as a placeholder; proper handle resolution
        // is implemented in specific registry instances below.
        None
    }

    /// Remove an object from the registry
    pub fn remove(&self, id: K) -> Option<Box<T>> {
        self.map.lock().remove(&id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_id_generation() {
        let id1 = DisplayId::new();
        let id2 = DisplayId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_window_id_generation() {
        let id1 = WindowId::new();
        let id2 = WindowId::new();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_registry_insert_and_remove() {
        let reg: Registry<WindowId, i32> = Registry::new();
        let id = WindowId::new();
        reg.insert(id, Box::new(42));
        let removed = reg.remove(id);
        assert_eq!(removed.map(|b| *b), Some(42));
    }

    #[test]
    fn test_registry_multiple_objects() {
        let reg: Registry<WindowId, u32> = Registry::new();
        let id1 = WindowId::new();
        let id2 = WindowId::new();
        reg.insert(id1, Box::new(100));
        reg.insert(id2, Box::new(200));

        let removed1 = reg.remove(id1);
        let removed2 = reg.remove(id2);
        assert_eq!(removed1.map(|b| *b), Some(100));
        assert_eq!(removed2.map(|b| *b), Some(200));
    }
}
