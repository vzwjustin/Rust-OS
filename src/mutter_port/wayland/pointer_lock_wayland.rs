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
/// the associated surface, the confinement region, and the cursor hint
/// position. A full implementation would drive the
/// `zwp_locked_pointer_v1` protocol via libwayland: emitting `locked`
/// / `unlocked` events to the client, applying the confinement region
/// to pointer motion, and forwarding `cursor_position_hint` requests.
/// Without libwayland, this struct tracks the lock state so the
/// compositor can query and mutate it.
#[derive(Debug)]
pub struct MetaPointerLockWayland {
    pub state: PointerLockState,
    pub confinement: Option<*mut core::ffi::c_void>, // MetaPointerConfinementWayland pointer
    /// Surface the pointer is locked to (set when a lock is enabled).
    pub surface: Option<*mut core::ffi::c_void>, // MetaWaylandSurface pointer
    /// Whether the cursor is visible while locked.
    pub cursor_visible: bool,
    /// Confinement region (x, y, width, height) in surface-local coords.
    /// When set, pointer motion is clipped to this region.
    pub region: Option<(i32, i32, i32, i32)>,
    /// Cursor position hint (x, y) in surface-local coordinates requested
    /// by the client via `locked_pointer.set_cursor_position_hint`.
    pub cursor_position_hint: Option<(i32, i32)>,
    /// Whether a lock is currently active (convenience flag mirroring
    /// `state == ENABLED`).
    pub locked: bool,
}

impl MetaPointerLockWayland {
    pub fn new(confinement: *mut core::ffi::c_void) -> Self {
        MetaPointerLockWayland {
            state: PointerLockState::PENDING,
            confinement: if confinement.is_null() {
                None
            } else {
                Some(confinement)
            },
            surface: None,
            cursor_visible: true,
            region: None,
            cursor_position_hint: None,
            locked: false,
        }
    }

    pub fn get_state(&self) -> PointerLockState {
        self.state
    }

    pub fn set_state(&mut self, state: PointerLockState) {
        self.locked = state == PointerLockState::ENABLED;
        self.state = state;
    }

    pub fn is_active(&self) -> bool {
        self.state == PointerLockState::ENABLED
    }

    /// Get the surface the pointer is locked to, if any.
    pub fn get_locked_surface(&self) -> Option<*mut core::ffi::c_void> {
        self.surface
    }

    /// Set the surface the pointer is locked to. A full implementation
    /// would also emit the `locked` event to the client's
    /// `zwp_locked_pointer_v1` resource.
    pub fn set_locked_surface(&mut self, surface: *mut core::ffi::c_void) {
        if surface.is_null() {
            self.surface = None;
        } else {
            self.surface = Some(surface);
        }
    }

    /// Get the confinement region in surface-local coordinates.
    pub fn get_region(&self) -> Option<(i32, i32, i32, i32)> {
        self.region
    }

    /// Set the confinement region. Coordinates are surface-local x, y,
    /// width, height. Passing `None` clears the region (lock applies to
    /// the whole surface).
    pub fn set_region(&mut self, region: Option<(i32, i32, i32, i32)>) {
        self.region = region;
    }

    /// Whether the pointer is currently locked.
    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Set the locked flag directly. Also updates `state` to keep the
    /// two in sync.
    pub fn set_locked(&mut self, locked: bool) {
        self.locked = locked;
        self.state = if locked {
            PointerLockState::ENABLED
        } else {
            PointerLockState::INACTIVE
        };
    }

    /// Set the cursor position hint requested by the client.
    pub fn set_cursor_position_hint(&mut self, x: i32, y: i32) {
        self.cursor_position_hint = Some((x, y));
    }

    /// Get the cursor position hint, if any.
    pub fn get_cursor_position_hint(&self) -> Option<(i32, i32)> {
        self.cursor_position_hint
    }

    /// Reset all lock state, clearing the surface, region, and hint.
    /// Used when a lock is destroyed or the surface is unmapped.
    pub fn reset(&mut self) {
        self.state = PointerLockState::INACTIVE;
        self.surface = None;
        self.region = None;
        self.cursor_position_hint = None;
        self.locked = false;
    }
}

impl Default for MetaPointerLockWayland {
    fn default() -> Self {
        MetaPointerLockWayland {
            state: PointerLockState::INACTIVE,
            confinement: None,
            surface: None,
            cursor_visible: true,
            region: None,
            cursor_position_hint: None,
            locked: false,
        }
    }
}
