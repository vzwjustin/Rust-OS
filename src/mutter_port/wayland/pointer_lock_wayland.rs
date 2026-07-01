//! Wayland Pointer Lock protocol implementation.
//!
//! Ported from: meta-pointer-lock-wayland.c/h
//!
//! Implements the zwp_pointer_lock_unstable_v1 protocol, which allows clients to
//! lock the pointer cursor to a surface and optionally hide it. Works in conjunction
//! with pointer confinement.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-pointer-lock-wayland.h

/// Pointer lock state enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum PointerLockState {
    /// Lock is not active.
    INACTIVE = 0,
    /// Lock is requested but not yet enabled.
    PENDING = 1,
    /// Lock is active on the surface.
    ENABLED = 2,
    /// Lock is being disabled.
    DISABLING = 3,
}

/// Represents a pointer lock for a Wayland surface.
///
/// Holds the state of a locked pointer, including whether it is active,
/// the associated surface, and the confinement region. Protocol I/O is TODO.
#[derive(Debug)]
pub struct MetaPointerLockWayland {
    pub state: PointerLockState,
    pub confinement: Option<*mut core::ffi::c_void>, // MetaPointerConfinementWayland pointer
    pub surface: Option<*mut core::ffi::c_void>,     // MetaWaylandSurface pointer
    pub cursor_visible: bool,
}

impl MetaPointerLockWayland {
    pub fn new(confinement: *mut core::ffi::c_void) -> Self {
        MetaPointerLockWayland {
            state: PointerLockState::PENDING,
            confinement: Some(confinement),
            surface: None,
            cursor_visible: true,
        }
    }

    pub fn get_state(&self) -> PointerLockState {
        self.state
    }

    pub fn set_state(&mut self, state: PointerLockState) {
        self.state = state;
    }

    pub fn is_active(&self) -> bool {
        self.state == PointerLockState::ENABLED
    }
}

impl Default for MetaPointerLockWayland {
    fn default() -> Self {
        MetaPointerLockWayland {
            state: PointerLockState::INACTIVE,
            confinement: None,
            surface: None,
            cursor_visible: true,
        }
    }
}
