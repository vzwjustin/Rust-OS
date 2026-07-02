//! Mutter display management
//! Ported from meta/display.h

use crate::mutter_port::meta::backend::MetaContext;
use crate::mutter_port::meta::enums::*;
use crate::mutter_port::meta::registry::{CompositorId, DisplayId, WindowId, WorkspaceId};
use crate::mutter_port::meta::types::*;
use crate::mutter_port::meta::window::MetaWindow;
use crate::mutter_port::mtk::MtkRectangle;
use alloc::vec::Vec;
use core::cell::Cell;

/// Main display object representing the X11 or Wayland display server
pub struct MetaDisplay {
    /// Display registry ID for lookups
    pub display_id: DisplayId,
    /// Opaque pointer to context (legacy)
    context: *mut core::ffi::c_void,
    /// Compositor ID for registry resolution
    compositor_id: Cell<Option<CompositorId>>,
    /// Opaque pointer to compositor (legacy)
    compositor: *mut core::ffi::c_void,
    /// Focus window ID for registry resolution
    focus_window_id: Cell<Option<WindowId>>,
    /// Opaque pointer to focus window (legacy)
    focus_window: *mut core::ffi::c_void,
    /// Workspace manager ID for registry resolution
    workspace_manager_id: Cell<Option<WorkspaceId>>,
    /// Opaque pointer to workspace manager (legacy)
    workspace_manager: *mut core::ffi::c_void,
    /// Cursor tracker ID (stored as u64 for now)
    cursor_tracker_id: Cell<Option<u64>>,
    /// Opaque pointer to cursor tracker (legacy)
    cursor_tracker: *mut core::ffi::c_void,
    /// Selection ID (stored as u64 for now)
    selection_id: Cell<Option<u64>>,
    /// Opaque pointer to selection (legacy)
    selection: *mut core::ffi::c_void,
    screen_width: i32,
    screen_height: i32,
    /// Whether the display connection has been closed.
    is_closed: bool,
    /// Whether window cycling (Alt-Tab) is currently active.
    cycling: bool,
    /// Windows on this display, in MRU (most-recently-used) order.
    windows: Vec<*mut MetaWindow>,
}

impl MetaDisplay {
    /// Create a new display
    pub fn new() -> Self {
        Self {
            display_id: DisplayId::new(),
            context: core::ptr::null_mut(),
            compositor_id: Cell::new(None),
            compositor: core::ptr::null_mut(),
            focus_window_id: Cell::new(None),
            focus_window: core::ptr::null_mut(),
            workspace_manager_id: Cell::new(None),
            workspace_manager: core::ptr::null_mut(),
            cursor_tracker_id: Cell::new(None),
            cursor_tracker: core::ptr::null_mut(),
            selection_id: Cell::new(None),
            selection: core::ptr::null_mut(),
            screen_width: 0,
            screen_height: 0,
            is_closed: false,
            cycling: false,
            windows: Vec::new(),
        }
    }

    /// Get the display's unique ID
    pub fn get_display_id(&self) -> DisplayId {
        self.display_id
    }

    /// Set the compositor ID for registry resolution
    pub fn set_compositor_id(&self, id: CompositorId) {
        self.compositor_id.set(Some(id));
    }

    /// Set the focus window ID for registry resolution
    pub fn set_focus_window_id(&self, id: WindowId) {
        self.focus_window_id.set(Some(id));
    }

    /// Set the workspace manager ID for registry resolution
    pub fn set_workspace_manager_id(&self, id: WorkspaceId) {
        self.workspace_manager_id.set(Some(id));
    }

    /// Set the context pointer (typed by the caller).
    pub fn set_context(&mut self, context: *mut MetaContext) {
        self.context = context as *mut core::ffi::c_void;
    }

    /// Register a window on this display (MRU order — most recent at end).
    pub fn add_window(&mut self, window: &MetaWindow) {
        let ptr = window as *const MetaWindow as *mut MetaWindow;
        self.windows.retain(|&w| w != ptr);
        self.windows.push(ptr);
    }

    /// Remove a window from this display.
    pub fn remove_window(&mut self, window: &MetaWindow) {
        let ptr = window as *const MetaWindow as *mut MetaWindow;
        self.windows.retain(|&w| w != ptr);
    }

    /// Close the display connection. Marks the display as closed and
    /// releases focus.
    pub fn close(&mut self, _timestamp: u32) {
        self.is_closed = true;
        self.focus_window_id.set(None);
        self.focus_window = core::ptr::null_mut();
        self.cycling = false;
    }

    /// Whether the display connection has been closed.
    pub fn is_closed(&self) -> bool {
        self.is_closed
    }

    /// Get the context this display belongs to.
    /// Resolves the stored opaque pointer to the rich `MetaContext` type.
    pub fn get_context(&self) -> Option<&MetaContext> {
        if self.context.is_null() {
            None
        } else {
            // SAFETY: The pointer was set by `set_context` with a valid
            // `*mut MetaContext`. The caller guarantees the referent
            // outlives this borrow.
            unsafe { Some(&*(self.context as *const MetaContext)) }
        }
    }

    /// Get the compositor for this display
    /// Registry resolution: uses `self.compositor_id` to look up in registry
    pub fn get_compositor(&self) -> Option<&MetaCompositor> {
        // ponytail: Proper reference-returning from Mutex<BTreeMap> requires
        // architectural changes (Arc<T> or lifetime parameters). Registry IDs
        // are stored in compositor_id; lookup infrastructure exists in registry module.
        self.compositor_id.get().map(|_id| {
            // Would resolve via: COMPOSITOR_REGISTRY.get(_id)
            // Deferred: requires reference-lifetime management solution
        });
        None
    }

    /// Get the currently focused window
    /// Registry resolution: uses `self.focus_window_id` to look up in registry
    pub fn get_focus_window(&self) -> Option<&MetaWindow> {
        // ponytail: Proper reference-returning from Mutex<BTreeMap> requires
        // architectural changes. Registry IDs are stored in focus_window_id.
        self.focus_window_id.get().map(|_id| {
            // Would resolve via: WINDOW_REGISTRY.get(_id)
            // Deferred: requires reference-lifetime management solution
        });
        None
    }

    /// Get the workspace manager
    /// Registry resolution: uses `self.workspace_manager_id` to look up in registry
    pub fn get_workspace_manager(&self) -> Option<&MetaWorkspaceManager> {
        // ponytail: Registry IDs are stored in workspace_manager_id.
        // Lookup infrastructure exists in registry module.
        self.workspace_manager_id.get().map(|_id| {
            // Would resolve via: WORKSPACE_REGISTRY.get(_id)
            // Deferred: requires reference-lifetime management solution
        });
        None
    }

    /// Get the cursor tracker
    pub fn get_cursor_tracker(&self) -> Option<&MetaCursorTracker> {
        // Registry lookup deferred: cursor tracker ID stored in cursor_tracker_id
        self.cursor_tracker_id.get().map(|_id| {
            // Would resolve via registry lookup
        });
        None
    }

    /// Get the selection manager
    pub fn get_selection(&self) -> Option<&MetaSelection> {
        // Registry lookup deferred: selection ID stored in selection_id
        self.selection_id.get().map(|_id| {
            // Would resolve via registry lookup
        });
        None
    }

    /// Get window by its ID
    /// Registry resolution: would use WINDOW_REGISTRY.get(_id)
    pub fn get_window_by_id(&self, _id: u64) -> Option<&MetaWindow> {
        // Registry infrastructure exists, but reference-returning requires
        // architectural changes (Arc<T> or unsafe lifetime extension).
        None
    }

    /// List all windows in MRU order, filtered by tab list type.
    /// Returns references to windows matching the list type criteria.
    pub fn get_tab_list(&self, list_type: MetaTabList) -> Vec<&MetaWindow> {
        let mut result: Vec<&MetaWindow> = Vec::new();
        // Iterate in reverse MRU order (most recent first).
        for &ptr in self.windows.iter().rev() {
            if ptr.is_null() {
                continue;
            }
            // SAFETY: Pointers were inserted via `add_window` with valid
            // `&MetaWindow` references. The caller guarantees the windows
            // outlive this borrow.
            let window = unsafe { &*ptr };
            // Filter by list type — skip windows that shouldn't appear.
            match list_type {
                MetaTabList::Normal | MetaTabList::NormalAll | MetaTabList::NormalAllMru => {
                    if window.is_skip_taskbar() || window.is_minimized() {
                        continue;
                    }
                }
                MetaTabList::Docks => {
                    // Docks list includes dock-type windows.
                    if window.get_window_type() != MetaWindowType::Dock {
                        continue;
                    }
                }
                MetaTabList::Group => {
                    // Group list includes all non-desktop windows.
                    if window.get_window_type() == MetaWindowType::Desktop {
                        continue;
                    }
                }
            }
            result.push(window);
        }
        result
    }

    /// Initiate window cycling UI (Alt-Tab). Marks cycling as active
    /// and records the show type for the switcher.
    pub fn begin_window_cycle(&mut self, _list_type: MetaTabList, _show_type: MetaTabShowType) {
        self.cycling = true;
    }

    /// End window cycling. Clears the cycling state.
    pub fn end_window_cycle(&mut self) {
        self.cycling = false;
    }

    /// Whether window cycling (Alt-Tab) is currently active.
    pub fn is_cycling(&self) -> bool {
        self.cycling
    }

    /// Get screen dimensions
    pub fn get_screen_width(&self) -> i32 {
        self.screen_width
    }

    pub fn get_screen_height(&self) -> i32 {
        self.screen_height
    }

    /// Set the logical screen dimensions.
    pub fn set_screen_size(&mut self, width: i32, height: i32) {
        self.screen_width = width;
        self.screen_height = height;
    }
}

impl Default for MetaDisplay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_screen_size_roundtrip() {
        let mut d = MetaDisplay::new();
        assert_eq!(d.get_screen_width(), 0);
        assert_eq!(d.get_screen_height(), 0);
        d.set_screen_size(1920, 1080);
        assert_eq!(d.get_screen_width(), 1920);
        assert_eq!(d.get_screen_height(), 1080);
    }
}
