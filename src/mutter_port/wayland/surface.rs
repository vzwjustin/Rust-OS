//! GNOME src/wayland/meta-wayland-surface.c
//!
//! MetaWaylandSurface represents a wl_surface resource from a client. It manages
//! the surface state (buffer, damage, transforms), roles (toplevel, popup, subsurface),
//! frame callbacks, and coordination with the compositor's rendering pipeline.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-surface.c

use alloc::{collections::BTreeMap, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};

/// Represents a Wayland surface role (toplevel, popup, subsurface, etc)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceRole {
    None,
    TopLevel,
    Popup,
    SubSurface,
    Cursor,
    XWayland,
    DndSurface,
}

/// Represents damage region (for incremental redraws)
#[derive(Debug, Clone)]
pub struct DamageRegion {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Pending state changes to be applied on commit
pub struct SurfaceState {
    pub buffer: Option<u32>, // buffer_id
    pub offset_x: i32,
    pub offset_y: i32,
    pub damage_regions: Vec<DamageRegion>,
    pub damage_buffer_regions: Vec<DamageRegion>,
    pub frame_callbacks: Vec<u32>,
    pub scale: i32,
    pub transform: u32,
    pub opaque_region: Option<(u32, u32)>, // width, height
    pub input_region: Option<(u32, u32)>,  // width, height
}

impl Default for SurfaceState {
    fn default() -> Self {
        SurfaceState {
            buffer: None,
            offset_x: 0,
            offset_y: 0,
            damage_regions: Vec::new(),
            damage_buffer_regions: Vec::new(),
            frame_callbacks: Vec::new(),
            scale: 1,
            transform: 0,
            opaque_region: None,
            input_region: None,
        }
    }
}

/// A Wayland surface
pub struct WaylandSurface {
    pub id: u32,
    pub client_id: u32,
    pub width: u32,
    pub height: u32,
    pub role: SurfaceRole,
    pub current_state: SurfaceState,
    pub pending_state: SurfaceState,
    pub current_buffer: Option<u32>,
    pub sync_subsurface: bool,
}

impl WaylandSurface {
    pub fn new(id: u32, client_id: u32) -> Self {
        WaylandSurface {
            id,
            client_id,
            width: 0,
            height: 0,
            role: SurfaceRole::None,
            current_state: SurfaceState::default(),
            pending_state: SurfaceState::default(),
            current_buffer: None,
            sync_subsurface: false,
        }
    }

    pub fn get_id(&self) -> u32 {
        self.id
    }

    pub fn get_client_id(&self) -> u32 {
        self.client_id
    }

    pub fn get_width(&self) -> u32 {
        self.width
    }

    pub fn get_height(&self) -> u32 {
        self.height
    }

    pub fn get_role(&self) -> SurfaceRole {
        self.role
    }

    pub fn set_role(&mut self, role: SurfaceRole) {
        self.role = role;
    }

    pub fn get_current_buffer(&self) -> Option<u32> {
        self.current_buffer
    }

    /// Attach a buffer to be committed on next wl_surface.commit
    pub fn pending_attach(&mut self, buffer_id: u32, x: i32, y: i32) {
        self.pending_state.buffer = Some(buffer_id);
        self.pending_state.offset_x = x;
        self.pending_state.offset_y = y;
    }

    /// Add damage region to be included in next commit
    pub fn add_damage(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.pending_state.damage_regions.push(DamageRegion {
            x,
            y,
            width,
            height,
        });
    }

    /// Add damage in buffer coordinates
    pub fn add_damage_buffer(&mut self, x: i32, y: i32, width: u32, height: u32) {
        self.pending_state.damage_buffer_regions.push(DamageRegion {
            x,
            y,
            width,
            height,
        });
    }

    /// Request frame callback (will be sent on next vblank)
    pub fn request_frame(&mut self, callback_id: u32) {
        self.pending_state.frame_callbacks.push(callback_id);
    }

    /// Set surface scale factor
    pub fn set_scale(&mut self, scale: i32) {
        self.pending_state.scale = scale.max(1);
    }

    /// Set surface transform/rotation
    pub fn set_transform(&mut self, transform: u32) {
        self.pending_state.transform = transform % 8; // 0-7 valid transforms
    }

    /// Apply pending state to current state (wl_surface.commit)
    pub fn commit(&mut self) -> bool {
        // Only actually change buffer if it changed
        if let Some(buffer_id) = self.pending_state.buffer {
            self.current_buffer = Some(buffer_id);
        }

        self.current_state = core::mem::take(&mut self.pending_state);
        self.pending_state = SurfaceState::default();

        true
    }

    /// STUB: Role-based surface initialization. Real implementation would
    /// coordinate with specific role handlers (xdg_shell, wl_shell, etc)
    pub fn initialize_role(&mut self, _role: SurfaceRole) {
        self.role = _role;
    }

    /// STUB: Get texture representation of surface. Requires buffer-to-texture
    /// conversion, format handling, and damage-driven updates
    pub fn get_texture(&self) -> Option<u32> {
        None
    }

    /// STUB: Scanout acquisition for direct-to-crtc rendering. Requires DRM
    /// integration and plane assignments
    pub fn try_acquire_scanout(&self) -> Option<u32> {
        None
    }
}

/// Manages multiple Wayland surfaces
pub struct WaylandSurfaceManager {
    surfaces: BTreeMap<u32, WaylandSurface>,
    next_surface_id: AtomicU32,
}

impl WaylandSurfaceManager {
    pub fn new() -> Self {
        WaylandSurfaceManager {
            surfaces: BTreeMap::new(),
            next_surface_id: AtomicU32::new(1),
        }
    }

    pub fn create_surface(&mut self, client_id: u32) -> u32 {
        let id = self.next_surface_id.fetch_add(1, Ordering::Release);
        let surface = WaylandSurface::new(id, client_id);
        self.surfaces.insert(id, surface);
        id
    }

    pub fn get_surface(&self, id: u32) -> Option<&WaylandSurface> {
        self.surfaces.get(&id)
    }

    pub fn get_surface_mut(&mut self, id: u32) -> Option<&mut WaylandSurface> {
        self.surfaces.get_mut(&id)
    }

    pub fn destroy_surface(&mut self, id: u32) -> bool {
        self.surfaces.remove(&id).is_some()
    }

    pub fn surfaces_for_client(&self, client_id: u32) -> Vec<u32> {
        self.surfaces
            .values()
            .filter(|s| s.client_id == client_id)
            .map(|s| s.id)
            .collect()
    }

    pub fn commit_surface(&mut self, id: u32) -> bool {
        if let Some(surface) = self.surfaces.get_mut(&id) {
            surface.commit()
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_surface_creation() {
        let surf = WaylandSurface::new(1, 100);
        assert_eq!(surf.get_id(), 1);
        assert_eq!(surf.get_client_id(), 100);
        assert_eq!(surf.get_role(), SurfaceRole::None);
    }

    #[test]
    fn test_surface_attach_and_commit() {
        let mut surf = WaylandSurface::new(1, 100);

        surf.pending_attach(10, 5, 10);
        assert_eq!(surf.current_buffer, None);

        surf.commit();
        assert_eq!(surf.current_buffer, Some(10));
    }

    #[test]
    fn test_damage_regions() {
        let mut surf = WaylandSurface::new(1, 100);

        surf.add_damage(0, 0, 100, 100);
        surf.add_damage_buffer(10, 10, 50, 50);

        assert_eq!(surf.pending_state.damage_regions.len(), 1);
        assert_eq!(surf.pending_state.damage_buffer_regions.len(), 1);
    }

    #[test]
    fn test_surface_manager() {
        let mut mgr = WaylandSurfaceManager::new();
        let s1 = mgr.create_surface(100);
        let s2 = mgr.create_surface(100);
        let s3 = mgr.create_surface(200);

        assert_eq!(mgr.surfaces_for_client(100).len(), 2);
        assert_eq!(mgr.surfaces_for_client(200).len(), 1);

        assert!(mgr.destroy_surface(s1));
        assert_eq!(mgr.surfaces_for_client(100).len(), 1);
    }
}
