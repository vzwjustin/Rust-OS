//! Wayland Pointer Constraints module
//!
//! Implements pointer_constraints_v1 protocol for games and applications
//! that need to lock or confine the pointer to a surface region.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-pointer-constraints.h

/// Pointer constraint type (lock or confine).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaWaylandPointerConstraintType {
    /// Pointer is locked (invisible, position fixed).
    Lock = 1,
    /// Pointer is confined to a region but remains visible.
    Confine = 2,
}

/// Represents a pointer lock or confinement constraint.
/// Restricts pointer movement to a specific surface region or locks it.
pub struct MetaWaylandPointerConstraint {
    pub surface: Option<*mut core::ffi::c_void>, // MetaWaylandSurface pointer
    pub compositor: Option<*mut core::ffi::c_void>, // MetaWaylandCompositor pointer
    pub constraint_type: MetaWaylandPointerConstraintType,
}

impl MetaWaylandPointerConstraint {
    /// Create a new pointer constraint (stub).
    pub fn new(constraint_type: MetaWaylandPointerConstraintType) -> Self {
        MetaWaylandPointerConstraint {
            surface: None,
            compositor: None,
            constraint_type,
        }
    }

    /// Initialize pointer constraints for the compositor.
    /// TODO: register pointer_constraints_v1 protocol
    pub fn init(_compositor: *mut core::ffi::c_void) {
    }

    /// Calculate the effective region for this constraint.
    /// TODO: intersect constraint region with monitor geometry
    pub fn calculate_effective_region(&self) -> Option<*mut core::ffi::c_void> {
        // TODO: implement - returns MtkRegion
        None
    }

    /// Get the surface for this constraint.
    pub fn get_surface(&self) -> Option<*mut core::ffi::c_void> {
        self.surface
    }

    /// Get the compositor for this constraint.
    pub fn get_compositor(&self) -> Option<*mut core::ffi::c_void> {
        self.compositor
    }
}

impl Default for MetaWaylandPointerConstraint {
    fn default() -> Self {
        Self::new(MetaWaylandPointerConstraintType::Confine)
    }
}
