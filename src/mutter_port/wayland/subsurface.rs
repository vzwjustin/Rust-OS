//! GNOME src/wayland/meta-wayland-subsurface.c
//!
//! MetaWaylandSubsurface is the wl_subsurface role. A subsurface is a child
//! surface positioned at an (x, y) offset relative to its parent, stacked
//! above/below siblings, and committed either synchronously (with the parent)
//! or desynchronously.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-subsurface.c

use alloc::{collections::BTreeMap, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};

/// wl_subsurface.place_above / place_below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubsurfacePlacement {
    Above,
    Below,
}

/// A queued stacking operation relative to a sibling (or the parent itself).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlacementOp {
    pub surface_id: u32,
    pub sibling_id: Option<u32>,
    pub placement: SubsurfacePlacement,
}

/// The wl_subsurface role attached to a surface.
///
/// STUB: mutter models stacking with a GNode tree of actors and applies moves
/// via transactions; here we track the parent/sibling links by id and keep an
/// ordered child list per parent.
pub struct MetaWaylandSubsurface {
    pub surface_id: u32,
    pub parent_id: u32,
    pub x: i32,
    pub y: i32,
    /// Synchronous mode: commits are held until the parent commits.
    pub synchronous: bool,
}

impl MetaWaylandSubsurface {
    pub fn new(surface_id: u32, parent_id: u32) -> Self {
        MetaWaylandSubsurface {
            surface_id,
            parent_id,
            x: 0,
            y: 0,
            synchronous: true,
        }
    }

    /// wl_subsurface.set_position (applied on the parent's next commit).
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    pub fn position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    /// wl_subsurface.set_sync.
    pub fn set_sync(&mut self) {
        self.synchronous = true;
    }

    /// wl_subsurface.set_desync.
    pub fn set_desync(&mut self) {
        self.synchronous = false;
    }

    /// meta_wayland_subsurface_is_synchronized: sync if this surface is sync OR
    /// (STUB) any ancestor is sync. Ancestor walk is resolved by the manager.
    pub fn is_synchronized(&self) -> bool {
        self.synchronous
    }
}

/// Tracks subsurfaces and the ordered child list of each parent.
pub struct SubsurfaceManager {
    subsurfaces: BTreeMap<u32, MetaWaylandSubsurface>,
    /// parent surface id -> ordered child surface ids (bottom to top).
    children: BTreeMap<u32, Vec<u32>>,
    /// Queued placement ops applied on parent commit.
    pending_ops: Vec<PlacementOp>,
    next_id: AtomicU32,
}

impl SubsurfaceManager {
    pub fn new() -> Self {
        SubsurfaceManager {
            subsurfaces: BTreeMap::new(),
            children: BTreeMap::new(),
            pending_ops: Vec::new(),
            next_id: AtomicU32::new(1),
        }
    }

    /// wl_subcompositor.get_subsurface. Returns an allocated role id and links
    /// the surface under its parent (initially placed on top).
    pub fn create_subsurface(&mut self, surface_id: u32, parent_id: u32) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::Release);
        self.subsurfaces.insert(
            surface_id,
            MetaWaylandSubsurface::new(surface_id, parent_id),
        );
        let siblings = self.children.entry(parent_id).or_insert_with(Vec::new);
        if !siblings.contains(&surface_id) {
            siblings.push(surface_id);
        }
        id
    }

    pub fn get(&self, surface_id: u32) -> Option<&MetaWaylandSubsurface> {
        self.subsurfaces.get(&surface_id)
    }

    pub fn get_mut(&mut self, surface_id: u32) -> Option<&mut MetaWaylandSubsurface> {
        self.subsurfaces.get_mut(&surface_id)
    }

    pub fn children_of(&self, parent_id: u32) -> Vec<u32> {
        self.children.get(&parent_id).cloned().unwrap_or_default()
    }

    /// A valid sibling shares this surface's parent (or is the parent itself).
    pub fn is_valid_sibling(&self, surface_id: u32, sibling_id: u32) -> bool {
        let parent = match self.subsurfaces.get(&surface_id) {
            Some(s) => s.parent_id,
            None => return false,
        };
        if sibling_id == parent {
            return true;
        }
        self.subsurfaces
            .get(&sibling_id)
            .map(|s| s.parent_id == parent)
            .unwrap_or(false)
    }

    /// wl_subsurface.place_above / place_below: queue and immediately reorder
    /// the child list. Returns false for an invalid sibling.
    pub fn place(
        &mut self,
        surface_id: u32,
        sibling_id: u32,
        placement: SubsurfacePlacement,
    ) -> bool {
        if !self.is_valid_sibling(surface_id, sibling_id) {
            return false;
        }
        self.pending_ops.push(PlacementOp {
            surface_id,
            sibling_id: Some(sibling_id),
            placement,
        });
        self.apply_placement(surface_id, sibling_id, placement);
        true
    }

    fn apply_placement(
        &mut self,
        surface_id: u32,
        sibling_id: u32,
        placement: SubsurfacePlacement,
    ) {
        let parent = match self.subsurfaces.get(&surface_id) {
            Some(s) => s.parent_id,
            None => return,
        };
        let list = match self.children.get_mut(&parent) {
            Some(l) => l,
            None => return,
        };
        list.retain(|id| *id != surface_id);
        // A sibling equal to the parent anchors at the bottom.
        let anchor = list.iter().position(|id| *id == sibling_id);
        match (anchor, placement) {
            (Some(i), SubsurfacePlacement::Above) => list.insert(i + 1, surface_id),
            (Some(i), SubsurfacePlacement::Below) => list.insert(i, surface_id),
            (None, SubsurfacePlacement::Below) => list.insert(0, surface_id),
            (None, SubsurfacePlacement::Above) => list.push(surface_id),
        }
    }

    /// Drain placement ops accumulated for the parent's commit.
    pub fn take_pending_ops(&mut self) -> Vec<PlacementOp> {
        core::mem::take(&mut self.pending_ops)
    }

    /// wl_subsurface destructor: unlink from parent and drop the role.
    pub fn destroy(&mut self, surface_id: u32) -> bool {
        if let Some(sub) = self.subsurfaces.remove(&surface_id) {
            if let Some(list) = self.children.get_mut(&sub.parent_id) {
                list.retain(|id| *id != surface_id);
            }
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_and_position() {
        let mut mgr = SubsurfaceManager::new();
        mgr.create_subsurface(10, 5);
        mgr.get_mut(10).unwrap().set_position(20, 30);
        assert_eq!(mgr.get(10).unwrap().position(), (20, 30));
        assert!(mgr.get(10).unwrap().is_synchronized());
    }

    #[test]
    fn test_sync_desync() {
        let mut mgr = SubsurfaceManager::new();
        mgr.create_subsurface(10, 5);
        mgr.get_mut(10).unwrap().set_desync();
        assert!(!mgr.get(10).unwrap().is_synchronized());
        mgr.get_mut(10).unwrap().set_sync();
        assert!(mgr.get(10).unwrap().is_synchronized());
    }

    #[test]
    fn test_placement_ordering() {
        let mut mgr = SubsurfaceManager::new();
        mgr.create_subsurface(10, 5);
        mgr.create_subsurface(11, 5);
        mgr.create_subsurface(12, 5);
        // Initial order: [10, 11, 12].
        assert_eq!(mgr.children_of(5), alloc::vec![10, 11, 12]);
        // Place 10 above 12 -> [11, 12, 10].
        assert!(mgr.place(10, 12, SubsurfacePlacement::Above));
        assert_eq!(mgr.children_of(5), alloc::vec![11, 12, 10]);
        // Place 10 below 11 -> [10, 11, 12].
        assert!(mgr.place(10, 11, SubsurfacePlacement::Below));
        assert_eq!(mgr.children_of(5), alloc::vec![10, 11, 12]);
        assert_eq!(mgr.take_pending_ops().len(), 2);
    }

    #[test]
    fn test_invalid_sibling_and_destroy() {
        let mut mgr = SubsurfaceManager::new();
        mgr.create_subsurface(10, 5);
        assert!(!mgr.place(10, 999, SubsurfacePlacement::Above));
        assert!(mgr.destroy(10));
        assert!(mgr.children_of(5).is_empty());
    }
}
