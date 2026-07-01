//! GNOME src/wayland/meta-cursor-wayland.c
//!
//! MetaCursorWayland is a ClutterCursor backed by a client wl_surface. It wraps
//! the surface's buffer as the cursor texture (with a hotspot), and on each
//! `prepare_at` recomputes texture scale / transform / viewport for the monitor
//! the pointer is over.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-cursor-wayland.c

use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU32, Ordering};

/// A cursor sourced from a Wayland surface.
///
/// STUB: the real object is a ClutterCursor subclass that realizes a CoglTexture
/// and coordinates with MetaCursorTracker. Here we model the id-level links and
/// the derived scale/transform values.
pub struct MetaCursorWayland {
    pub id: u32,
    /// Backing wl_surface id (weak link; may become None when destroyed).
    pub surface_id: Option<u32>,
    /// Cached texture id from the surface buffer.
    pub texture_id: Option<u32>,
    pub hot_x: i32,
    pub hot_y: i32,
    /// Set when the backing buffer changed and the texture must be re-realized.
    invalidated: bool,
    /// Last computed texture scale for the monitor under the pointer.
    pub texture_scale: f32,
    /// wl_output transform (0..7) applied to the buffer.
    pub buffer_transform: u32,
}

impl MetaCursorWayland {
    pub fn new(id: u32, surface_id: u32) -> Self {
        MetaCursorWayland {
            id,
            surface_id: Some(surface_id),
            texture_id: None,
            hot_x: 0,
            hot_y: 0,
            invalidated: false,
            texture_scale: 1.0,
            buffer_transform: 0,
        }
    }

    /// meta_cursor_wayland_set_texture.
    pub fn set_texture(&mut self, texture_id: Option<u32>, hot_x: i32, hot_y: i32) {
        self.texture_id = texture_id;
        self.hot_x = hot_x;
        self.hot_y = hot_y;
        self.invalidated = true;
    }

    /// meta_cursor_wayland_get_texture -> (texture, hot_x, hot_y).
    pub fn get_texture(&self) -> (Option<u32>, i32, i32) {
        (self.texture_id, self.hot_x, self.hot_y)
    }

    /// meta_cursor_wayland_invalidate.
    pub fn invalidate(&mut self) {
        self.invalidated = true;
    }

    /// meta_cursor_wayland_realize_texture: consumes the invalidation flag,
    /// returning true if the texture was (re)realized.
    pub fn realize_texture(&mut self) -> bool {
        if self.invalidated {
            self.invalidated = false;
            true
        } else {
            false
        }
    }

    /// Wayland cursors are never animated (unlike sprite/xcursor cursors).
    pub fn is_animated(&self) -> bool {
        false
    }

    /// meta_cursor_wayland_prepare_at: derive the texture scale for the monitor
    /// under the pointer.
    ///
    /// `has_dst_size` mirrors the surface viewport having an explicit
    /// destination size (forces scale 1.0). `stage_views_scaled` reflects the
    /// backend rendering scaled stage views.
    ///
    /// STUB: monitor lookup, viewport src/dst plumbing and output notification
    /// are compositor/backend responsibilities.
    pub fn prepare_at(
        &mut self,
        surface_scale: i32,
        monitor_scale: f32,
        has_dst_size: bool,
        stage_views_scaled: bool,
    ) {
        let surface_scale = surface_scale.max(1) as f32;
        self.texture_scale = if has_dst_size {
            1.0
        } else if stage_views_scaled {
            1.0 / surface_scale
        } else {
            monitor_scale / surface_scale
        };
    }
}

/// Owns cursor objects keyed by id.
pub struct CursorWaylandManager {
    cursors: BTreeMap<u32, MetaCursorWayland>,
    next_id: AtomicU32,
}

impl CursorWaylandManager {
    pub fn new() -> Self {
        CursorWaylandManager {
            cursors: BTreeMap::new(),
            next_id: AtomicU32::new(1),
        }
    }

    /// meta_cursor_wayland_new.
    pub fn create(&mut self, surface_id: u32) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::Release);
        self.cursors
            .insert(id, MetaCursorWayland::new(id, surface_id));
        id
    }

    pub fn get(&self, id: u32) -> Option<&MetaCursorWayland> {
        self.cursors.get(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut MetaCursorWayland> {
        self.cursors.get_mut(&id)
    }

    /// The backing surface was destroyed: drop the weak link (finalize path).
    pub fn on_surface_destroyed(&mut self, surface_id: u32) {
        for c in self.cursors.values_mut() {
            if c.surface_id == Some(surface_id) {
                c.surface_id = None;
                c.texture_id = None;
            }
        }
    }

    pub fn destroy(&mut self, id: u32) -> bool {
        self.cursors.remove(&id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_texture_roundtrip() {
        let mut c = MetaCursorWayland::new(1, 10);
        c.set_texture(Some(55), 4, 8);
        assert_eq!(c.get_texture(), (Some(55), 4, 8));
        assert!(!c.is_animated());
    }

    #[test]
    fn test_realize_consumes_invalidation() {
        let mut c = MetaCursorWayland::new(1, 10);
        c.invalidate();
        assert!(c.realize_texture());
        assert!(!c.realize_texture());
    }

    #[test]
    fn test_prepare_at_scale() {
        let mut c = MetaCursorWayland::new(1, 10);
        // Explicit dst size forces 1.0.
        c.prepare_at(2, 2.0, true, false);
        assert_eq!(c.texture_scale, 1.0);
        // Scaled stage views -> 1/surface_scale.
        c.prepare_at(2, 2.0, false, true);
        assert_eq!(c.texture_scale, 0.5);
        // Otherwise monitor/surface.
        c.prepare_at(2, 3.0, false, false);
        assert_eq!(c.texture_scale, 1.5);
    }

    #[test]
    fn test_surface_destroyed() {
        let mut mgr = CursorWaylandManager::new();
        let id = mgr.create(10);
        mgr.get_mut(id).unwrap().set_texture(Some(7), 0, 0);
        mgr.on_surface_destroyed(10);
        assert_eq!(mgr.get(id).unwrap().surface_id, None);
        assert_eq!(mgr.get(id).unwrap().texture_id, None);
    }
}
