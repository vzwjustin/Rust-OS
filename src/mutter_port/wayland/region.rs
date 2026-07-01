//! GNOME src/wayland/meta-wayland-region.c
//!
//! MetaWaylandRegion wraps a wl_region resource. A region is a set of
//! rectangles accumulated by `add` (union) and `subtract` operations. Surfaces
//! peek at regions to define their opaque and input areas.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-region.c

use alloc::{collections::BTreeMap, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};

/// A rectangle in region-local coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rectangle {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Rectangle {
            x,
            y,
            width,
            height,
        }
    }

    pub fn contains_point(&self, px: i32, py: i32) -> bool {
        px >= self.x
            && py >= self.y
            && px < self.x.saturating_add(self.width)
            && py < self.y.saturating_add(self.height)
    }
}

/// A MetaWaylandRegion: an accumulated set of rectangles.
///
/// STUB: the real implementation delegates to MtkRegion, which performs proper
/// geometric union/subtraction and coalescing. Here we keep an additive rect
/// list plus a record of subtracted rects for membership queries.
pub struct MetaWaylandRegion {
    pub id: u32,
    pub client_id: u32,
    rects: Vec<Rectangle>,
    subtracted: Vec<Rectangle>,
}

impl MetaWaylandRegion {
    pub fn new(id: u32, client_id: u32) -> Self {
        MetaWaylandRegion {
            id,
            client_id,
            rects: Vec::new(),
            subtracted: Vec::new(),
        }
    }

    /// wl_region.add - union a rectangle into the region.
    pub fn add_rectangle(&mut self, x: i32, y: i32, width: i32, height: i32) {
        self.rects.push(Rectangle::new(x, y, width, height));
    }

    /// wl_region.subtract - subtract a rectangle from the region.
    pub fn subtract_rectangle(&mut self, x: i32, y: i32, width: i32, height: i32) {
        self.subtracted.push(Rectangle::new(x, y, width, height));
    }

    /// meta_wayland_region_peek_region - the additive rectangles.
    pub fn peek_region(&self) -> &[Rectangle] {
        &self.rects
    }

    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// Whether a point is in the region (added and not later subtracted).
    pub fn contains_point(&self, px: i32, py: i32) -> bool {
        let inside = self.rects.iter().any(|r| r.contains_point(px, py));
        let removed = self.subtracted.iter().any(|r| r.contains_point(px, py));
        inside && !removed
    }
}

/// Tracks all live regions keyed by id.
pub struct RegionManager {
    regions: BTreeMap<u32, MetaWaylandRegion>,
    next_id: AtomicU32,
}

impl RegionManager {
    pub fn new() -> Self {
        RegionManager {
            regions: BTreeMap::new(),
            next_id: AtomicU32::new(1),
        }
    }

    pub fn create_region(&mut self, client_id: u32) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::Release);
        self.regions
            .insert(id, MetaWaylandRegion::new(id, client_id));
        id
    }

    pub fn get_region(&self, id: u32) -> Option<&MetaWaylandRegion> {
        self.regions.get(&id)
    }

    pub fn get_region_mut(&mut self, id: u32) -> Option<&mut MetaWaylandRegion> {
        self.regions.get_mut(&id)
    }

    pub fn destroy_region(&mut self, id: u32) -> bool {
        self.regions.remove(&id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_and_peek() {
        let mut r = MetaWaylandRegion::new(1, 10);
        assert!(r.is_empty());
        r.add_rectangle(0, 0, 100, 100);
        assert_eq!(r.peek_region().len(), 1);
        assert!(r.contains_point(50, 50));
        assert!(!r.contains_point(200, 200));
    }

    #[test]
    fn test_subtract() {
        let mut r = MetaWaylandRegion::new(1, 10);
        r.add_rectangle(0, 0, 100, 100);
        r.subtract_rectangle(0, 0, 50, 50);
        assert!(!r.contains_point(10, 10));
        assert!(r.contains_point(75, 75));
    }

    #[test]
    fn test_manager() {
        let mut mgr = RegionManager::new();
        let id = mgr.create_region(5);
        mgr.get_region_mut(id).unwrap().add_rectangle(1, 2, 3, 4);
        assert_eq!(mgr.get_region(id).unwrap().peek_region().len(), 1);
        assert!(mgr.destroy_region(id));
        assert!(mgr.get_region(id).is_none());
    }
}
