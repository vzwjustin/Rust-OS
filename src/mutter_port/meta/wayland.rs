//! Mutter Wayland support
//! Ported from meta/meta-wayland*.h

use crate::mutter_port::meta::display::MetaDisplay;
use crate::mutter_port::meta::registry::{DisplayId, WaylandSurfaceId, WindowId};
use crate::mutter_port::meta::window::MetaWindow;
use alloc::string::String;
use core::cell::Cell;

/// Wayland compositor (manages Wayland protocol and clients)
pub struct MetaWaylandCompositor {
    /// Wayland compositor registry ID
    pub compositor_id: DisplayId,
    /// Display ID for registry resolution
    display_id: Cell<Option<DisplayId>>,
    pub display: Option<*mut core::ffi::c_void>, // opaque Wayland display pointer
    initialized: bool,
}

impl MetaWaylandCompositor {
    pub fn new() -> Self {
        Self {
            compositor_id: DisplayId::new(),
            display_id: Cell::new(None),
            display: None,
            initialized: false,
        }
    }

    /// Set the MetaDisplay ID for registry resolution
    pub fn set_display_id(&self, id: DisplayId) {
        self.display_id.set(Some(id));
    }

    /// Initialize Wayland support. Marks the compositor as initialized
    /// and sets up the display pointer placeholder.
    pub fn init(&mut self) {
        self.initialized = true;
    }

    /// Shutdown Wayland. Marks the compositor as not initialized and
    /// releases the display pointer.
    pub fn shutdown(&mut self) {
        self.initialized = false;
        self.display = None;
        self.display_id.set(None);
    }

    /// Whether the Wayland compositor has been initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// Get the underlying display
    pub fn get_display(&self) -> Option<&MetaDisplay> {
        // Registry infrastructure in place (display_id stored).
        // Full reference-returning requires Arc<T> or lifetime architecture.
        self.display_id.get().map(|_id| {
            // Would resolve via: DISPLAY_REGISTRY.get(_id)
        });
        None
    }
}

impl Default for MetaWaylandCompositor {
    fn default() -> Self {
        Self::new()
    }
}

/// Wayland surface representation (drawing surface with role and window association)
pub struct MetaWaylandSurface {
    pub window: Option<*mut MetaWindow>,
    pub role: Option<alloc::string::String>,
}

impl MetaWaylandSurface {
    pub fn new() -> Self {
        Self {
            window: None,
            role: None,
        }
    }

    /// Set the window associated with this surface.
    pub fn set_window(&mut self, window: *mut MetaWindow) {
        self.window = Some(window);
    }

    /// Get the window associated with this surface.
    /// Resolves the stored typed pointer.
    pub fn get_window(&self) -> Option<&MetaWindow> {
        self.window.and_then(|ptr| {
            if ptr.is_null() {
                None
            } else {
                // SAFETY: The pointer was set by `set_window` with a valid
                // `*mut MetaWindow`. The caller guarantees the referent
                // outlives this borrow.
                unsafe { Some(&*ptr) }
            }
        })
    }

    /// Check if surface has role
    pub fn has_role(&self, role: &str) -> bool {
        self.role.as_ref().map_or(false, |r| r.as_str() == role)
    }
}

impl Default for MetaWaylandSurface {
    fn default() -> Self {
        Self::new()
    }
}

/// Wayland client connection (client process with PID and UID)
pub struct MetaWaylandClient {
    pub pid: u32,
    pub uid: u32,
    killed: Cell<bool>,
}

impl MetaWaylandClient {
    pub fn new(pid: u32, uid: u32) -> Self {
        Self {
            pid,
            uid,
            killed: Cell::new(false),
        }
    }

    /// Get client PID
    pub fn get_pid(&self) -> u32 {
        self.pid
    }

    /// Get client UID
    pub fn get_uid(&self) -> u32 {
        self.uid
    }

    /// Kill client. Marks the client as killed. In a full implementation
    /// this would send SIGKILL to the client process via the kernel
    /// process manager.
    pub fn kill(&self) {
        self.killed.set(true);
    }

    /// Whether this client has been killed.
    pub fn is_killed(&self) -> bool {
        self.killed.get()
    }
}

impl Default for MetaWaylandClient {
    fn default() -> Self {
        Self::new(0, 0)
    }
}
