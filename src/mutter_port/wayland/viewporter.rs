//! GNOME src/wayland/meta-wayland-viewporter.c
//!
//! Implements wp_viewporter / wp_viewport. A viewport attaches a source crop
//! rectangle and a destination size to a surface. Validation mirrors the
//! protocol: the source must be all-positive (x/y >= 0, w/h > 0) or all -1 to
//! unset; the destination must be positive or -1 to unset.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-viewporter.c

use alloc::collections::BTreeMap;

/// Error returned when a request violates the protocol constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewportError {
    /// The backing wl_surface no longer exists.
    NoSurface,
    /// Supplied values were out of range.
    BadValue,
}

/// Pending viewport state for a surface (applied on the next surface commit).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub surface_id: u32,
    /// Source crop rect in surface-local coords; width < 0 means unset.
    pub src_x: f32,
    pub src_y: f32,
    pub src_width: f32,
    pub src_height: f32,
    /// Destination size; width < 0 means unset.
    pub dst_width: i32,
    pub dst_height: i32,
    pub has_new_src_rect: bool,
    pub has_new_dst_size: bool,
}

impl Viewport {
    pub fn new(surface_id: u32) -> Self {
        Viewport {
            surface_id,
            src_x: -1.0,
            src_y: -1.0,
            src_width: -1.0,
            src_height: -1.0,
            dst_width: -1,
            dst_height: -1,
            has_new_src_rect: false,
            has_new_dst_size: false,
        }
    }

    pub fn has_src_rect(&self) -> bool {
        self.src_width > 0.0
    }

    pub fn has_dst_size(&self) -> bool {
        self.dst_width > 0
    }
}

/// Manages one viewport per surface (wp_viewporter.get_viewport enforces this).
pub struct ViewportManager {
    viewports: BTreeMap<u32, Viewport>,
}

impl ViewportManager {
    pub fn new() -> Self {
        ViewportManager {
            viewports: BTreeMap::new(),
        }
    }

    /// wp_viewporter.get_viewport. Errors if the surface already has a viewport.
    pub fn get_viewport(&mut self, surface_id: u32) -> Result<(), ViewportError> {
        if self.viewports.contains_key(&surface_id) {
            return Err(ViewportError::BadValue); // VIEWPORT_EXISTS
        }
        self.viewports.insert(surface_id, Viewport::new(surface_id));
        Ok(())
    }

    pub fn get(&self, surface_id: u32) -> Option<&Viewport> {
        self.viewports.get(&surface_id)
    }

    /// wp_viewport.set_source. Valid if all values are positive (x,y >= 0;
    /// w,h > 0) or all equal -1 to unset.
    pub fn set_source(
        &mut self,
        surface_id: u32,
        x: f32,
        y: f32,
        width: f32,
        height: f32,
    ) -> Result<(), ViewportError> {
        let vp = self
            .viewports
            .get_mut(&surface_id)
            .ok_or(ViewportError::NoSurface)?;

        let positive = x >= 0.0 && y >= 0.0 && width > 0.0 && height > 0.0;
        let unset = x == -1.0 && y == -1.0 && width == -1.0 && height == -1.0;
        if !(positive || unset) {
            return Err(ViewportError::BadValue);
        }

        vp.src_x = x;
        vp.src_y = y;
        vp.src_width = width;
        vp.src_height = height;
        vp.has_new_src_rect = true;
        Ok(())
    }

    /// wp_viewport.set_destination. Valid if both positive or both -1 to unset.
    pub fn set_destination(
        &mut self,
        surface_id: u32,
        width: i32,
        height: i32,
    ) -> Result<(), ViewportError> {
        let vp = self
            .viewports
            .get_mut(&surface_id)
            .ok_or(ViewportError::NoSurface)?;

        let positive = width > 0 && height > 0;
        let unset = width == -1 && height == -1;
        if !(positive || unset) {
            return Err(ViewportError::BadValue);
        }

        vp.dst_width = width;
        vp.dst_height = height;
        vp.has_new_dst_size = true;
        Ok(())
    }

    /// wp_viewport destructor: unset the viewport on the pending surface state.
    pub fn destroy(&mut self, surface_id: u32) -> bool {
        self.viewports.remove(&surface_id).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_unique() {
        let mut mgr = ViewportManager::new();
        assert!(mgr.get_viewport(1).is_ok());
        assert!(mgr.get_viewport(1).is_err());
    }

    #[test]
    fn test_source_validation() {
        let mut mgr = ViewportManager::new();
        mgr.get_viewport(1).unwrap();
        assert!(mgr.set_source(1, 0.0, 0.0, 10.0, 10.0).is_ok());
        assert!(mgr.set_source(1, -1.0, -1.0, -1.0, -1.0).is_ok());
        assert_eq!(
            mgr.set_source(1, -5.0, 0.0, 10.0, 10.0),
            Err(ViewportError::BadValue)
        );
        assert_eq!(
            mgr.set_source(9, 0.0, 0.0, 10.0, 10.0),
            Err(ViewportError::NoSurface)
        );
    }

    #[test]
    fn test_destination_validation() {
        let mut mgr = ViewportManager::new();
        mgr.get_viewport(1).unwrap();
        assert!(mgr.set_destination(1, 100, 50).is_ok());
        assert!(mgr.get(1).unwrap().has_dst_size());
        assert!(mgr.set_destination(1, -1, -1).is_ok());
        assert_eq!(mgr.set_destination(1, 0, 10), Err(ViewportError::BadValue));
    }
}
