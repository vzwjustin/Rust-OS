//! GNOME src/wayland/meta-wayland-fixes.c
//!
//! MetaWaylandFixes implements the wl_fixes global. This tiny protocol exposes
//! a `destroy_registry` request that lets clients tear down a wl_registry
//! object cleanly (working around a historical wire-protocol gap).
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-fixes.c

use alloc::{collections::BTreeMap, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};

/// A bound wl_fixes resource for a client.
pub struct MetaWaylandFixes {
    pub id: u32,
    pub client_id: u32,
    pub version: u32,
    /// registry object ids the client has asked us to track for teardown.
    registries: Vec<u32>,
}

impl MetaWaylandFixes {
    pub fn new(id: u32, client_id: u32, version: u32) -> Self {
        MetaWaylandFixes {
            id,
            client_id,
            version,
            registries: Vec::new(),
        }
    }

    pub fn track_registry(&mut self, registry_id: u32) {
        if !self.registries.contains(&registry_id) {
            self.registries.push(registry_id);
        }
    }

    /// wl_fixes.destroy_registry - request destruction of a registry object.
    ///
    /// STUB: on the real wire this calls `wl_resource_destroy(registry)`. Here
    /// we just drop our record of it; the caller is expected to reap the
    /// registry resource.
    pub fn destroy_registry(&mut self, registry_id: u32) -> bool {
        let before = self.registries.len();
        self.registries.retain(|id| *id != registry_id);
        before != self.registries.len()
    }
}

/// The wl_fixes global; binds per-client resources.
pub struct FixesGlobal {
    resources: BTreeMap<u32, MetaWaylandFixes>,
    next_id: AtomicU32,
}

impl FixesGlobal {
    pub fn new() -> Self {
        FixesGlobal {
            resources: BTreeMap::new(),
            next_id: AtomicU32::new(1),
        }
    }

    /// bind_wl_fixes - a client binds the global, creating a resource.
    pub fn bind(&mut self, client_id: u32, version: u32) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::Release);
        self.resources
            .insert(id, MetaWaylandFixes::new(id, client_id, version));
        id
    }

    pub fn get(&self, id: u32) -> Option<&MetaWaylandFixes> {
        self.resources.get(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut MetaWaylandFixes> {
        self.resources.get_mut(&id)
    }

    /// wl_fixes.destroy - client destroys its wl_fixes resource.
    pub fn destroy(&mut self, id: u32) -> bool {
        self.resources.remove(&id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bind_and_destroy() {
        let mut g = FixesGlobal::new();
        let id = g.bind(7, 1);
        assert!(g.get(id).is_some());
        assert!(g.destroy(id));
        assert!(g.get(id).is_none());
    }

    #[test]
    fn test_destroy_registry() {
        let mut fixes = MetaWaylandFixes::new(1, 7, 1);
        fixes.track_registry(42);
        assert!(fixes.destroy_registry(42));
        assert!(!fixes.destroy_registry(42));
    }
}
