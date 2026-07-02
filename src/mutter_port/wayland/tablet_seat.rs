//! Wayland Tablet Seat module
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-tablet-seat.h
//!
//! Manages all tablet input devices for a given Wayland seat.
//! Holds collections of tablets, tools, and pads with insert/lookup/remove
//! operations mirroring the GHashTable-based C implementation.

use alloc::vec::Vec;

/// Manages tablet devices (stylus pens) for a Wayland seat.
/// The C implementation uses `GHashTable` for device collections and `struct wl_list` for resources.
pub struct MetaWaylandTabletSeat {
    /// Parent tablet manager.
    pub manager: *mut core::ffi::c_void,
    /// Wayland seat this tablet seat belongs to.
    pub seat: *mut core::ffi::c_void,
    /// Clutter seat providing input device enumeration.
    pub clutter_seat: *mut core::ffi::c_void,
    /// Linked list of Wayland resources (opaque). Modeled as a Vec of
    /// wl_resource pointers; the C code uses `struct wl_list`.
    pub resource_list: Vec<*mut core::ffi::c_void>,
    /// Hash table mapping ClutterInputDevice* to MetaWaylandTablet*.
    /// Stored as a flat Vec of (device, tablet) pointer pairs since we
    /// have no GHashTable; lookup is linear.
    pub tablets: Vec<(*mut core::ffi::c_void, *mut core::ffi::c_void)>,
    /// Hash table mapping ClutterInputDevice* to MetaWaylandTabletTool*.
    pub tools: Vec<(*mut core::ffi::c_void, *mut core::ffi::c_void)>,
    /// Hash table mapping ClutterInputDevice* to MetaWaylandTabletPad*.
    pub pads: Vec<(*mut core::ffi::c_void, *mut core::ffi::c_void)>,
}

impl MetaWaylandTabletSeat {
    /// Create a new tablet seat for the given manager and Wayland seat.
    /// Initializes empty resource, tablet, tool, and pad collections. A
    /// full implementation would also allocate the wl_list for
    /// `resource_list` and create GHashTables with destroy callbacks for
    /// `tablets`, `tools`, and `pads`.
    pub fn new() -> Self {
        Self {
            manager: core::ptr::null_mut(),
            seat: core::ptr::null_mut(),
            clutter_seat: core::ptr::null_mut(),
            resource_list: Vec::new(),
            tablets: Vec::new(),
            tools: Vec::new(),
            pads: Vec::new(),
        }
    }

    /// Register a tablet resource bound to a ClutterInputDevice. Replaces
    /// any existing entry for the same device, mirroring g_hash_table_replace.
    pub fn insert_tablet(
        &mut self,
        device: *mut core::ffi::c_void,
        tablet: *mut core::ffi::c_void,
    ) {
        if let Some(entry) = self
            .tablets
            .iter_mut()
            .find(|(d, _)| core::ptr::eq(*d, device))
        {
            entry.1 = tablet;
        } else {
            self.tablets.push((device, tablet));
        }
    }

    /// Look up the tablet associated with a device, if any.
    pub fn lookup_tablet(&self, device: *mut core::ffi::c_void) -> Option<*mut core::ffi::c_void> {
        self.tablets
            .iter()
            .find(|(d, _)| core::ptr::eq(*d, device))
            .map(|(_, t)| *t)
    }

    /// Remove the tablet entry for a device. Returns the removed tablet
    /// pointer so the caller can destroy it.
    pub fn remove_tablet(
        &mut self,
        device: *mut core::ffi::c_void,
    ) -> Option<*mut core::ffi::c_void> {
        let pos = self
            .tablets
            .iter()
            .position(|(d, _)| core::ptr::eq(*d, device));
        pos.map(|i| self.tablets.remove(i).1)
    }

    /// Register a tablet tool bound to a ClutterInputDevice.
    pub fn insert_tool(&mut self, device: *mut core::ffi::c_void, tool: *mut core::ffi::c_void) {
        if let Some(entry) = self
            .tools
            .iter_mut()
            .find(|(d, _)| core::ptr::eq(*d, device))
        {
            entry.1 = tool;
        } else {
            self.tools.push((device, tool));
        }
    }

    /// Look up the tablet tool associated with a device, if any.
    pub fn lookup_tool(&self, device: *mut core::ffi::c_void) -> Option<*mut core::ffi::c_void> {
        self.tools
            .iter()
            .find(|(d, _)| core::ptr::eq(*d, device))
            .map(|(_, t)| *t)
    }

    /// Remove the tablet tool entry for a device.
    pub fn remove_tool(
        &mut self,
        device: *mut core::ffi::c_void,
    ) -> Option<*mut core::ffi::c_void> {
        let pos = self
            .tools
            .iter()
            .position(|(d, _)| core::ptr::eq(*d, device));
        pos.map(|i| self.tools.remove(i).1)
    }

    /// Register a tablet pad bound to a ClutterInputDevice.
    pub fn insert_pad(&mut self, device: *mut core::ffi::c_void, pad: *mut core::ffi::c_void) {
        if let Some(entry) = self
            .pads
            .iter_mut()
            .find(|(d, _)| core::ptr::eq(*d, device))
        {
            entry.1 = pad;
        } else {
            self.pads.push((device, pad));
        }
    }

    /// Look up the tablet pad associated with a device, if any.
    pub fn lookup_pad(&self, device: *mut core::ffi::c_void) -> Option<*mut core::ffi::c_void> {
        self.pads
            .iter()
            .find(|(d, _)| core::ptr::eq(*d, device))
            .map(|(_, p)| *p)
    }

    /// Remove the tablet pad entry for a device.
    pub fn remove_pad(&mut self, device: *mut core::ffi::c_void) -> Option<*mut core::ffi::c_void> {
        let pos = self
            .pads
            .iter()
            .position(|(d, _)| core::ptr::eq(*d, device));
        pos.map(|i| self.pads.remove(i).1)
    }

    /// Add a Wayland resource to this tablet seat's resource list.
    pub fn add_resource(&mut self, resource: *mut core::ffi::c_void) {
        self.resource_list.push(resource);
    }

    /// Number of tablets currently tracked.
    pub fn tablet_count(&self) -> usize {
        self.tablets.len()
    }

    /// Number of tablet tools currently tracked.
    pub fn tool_count(&self) -> usize {
        self.tools.len()
    }

    /// Number of tablet pads currently tracked.
    pub fn pad_count(&self) -> usize {
        self.pads.len()
    }
}

impl Default for MetaWaylandTabletSeat {
    fn default() -> Self {
        Self::new()
    }
}
