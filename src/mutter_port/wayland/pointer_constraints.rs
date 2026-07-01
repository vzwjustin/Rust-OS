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
    /// Whether the constraint is currently active.
    pub active: bool,
    /// The region hint for confinement (opaque region pointer).
    pub region: Option<*mut core::ffi::c_void>,
    /// Whether the constraint was requested with a persistent lifetime.
    pub persistent: bool,
}

impl MetaWaylandPointerConstraint {
    /// Create a new pointer constraint.
    pub fn new(constraint_type: MetaWaylandPointerConstraintType) -> Self {
        MetaWaylandPointerConstraint {
            surface: None,
            compositor: None,
            constraint_type,
            active: false,
            region: None,
            persistent: false,
        }
    }

    /// Initialize pointer constraints for the compositor. A full
    /// implementation would register the pointer_constraints_v1
    /// global with the wl_display.
    pub fn init(_compositor: *mut core::ffi::c_void) {}

    /// Calculate the effective region for this constraint. A full
    /// implementation would intersect the constraint region with the
    /// monitor geometry. Returns the stored region if set.
    pub fn calculate_effective_region(&self) -> Option<*mut core::ffi::c_void> {
        self.region
    }

    /// Get the surface for this constraint.
    pub fn get_surface(&self) -> Option<*mut core::ffi::c_void> {
        self.surface
    }

    /// Get the compositor for this constraint.
    pub fn get_compositor(&self) -> Option<*mut core::ffi::c_void> {
        self.compositor
    }

    /// Activate the constraint.
    pub fn activate(&mut self) {
        self.active = true;
    }

    /// Deactivate the constraint.
    pub fn deactivate(&mut self) {
        self.active = false;
    }

    /// Whether the constraint is currently active.
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Set the confinement region.
    pub fn set_region(&mut self, region: *mut core::ffi::c_void) {
        self.region = Some(region);
    }

    /// Set whether the constraint is persistent (survives focus loss).
    pub fn set_persistent(&mut self, persistent: bool) {
        self.persistent = persistent;
    }
}

impl Default for MetaWaylandPointerConstraint {
    fn default() -> Self {
        Self::new(MetaWaylandPointerConstraintType::Confine)
    }
}
