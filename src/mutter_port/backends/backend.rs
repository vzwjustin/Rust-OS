//! Backend ported from GNOME Mutter's src/backends/
//!
//! The MetaBackend abstraction handles monitor configuration, modesetting,
//! cursor management, input device handling, renderer setup, D-Bus interaction,
//! and remote desktop/screencasting infrastructure.
//! This is the core entry point for all display server functionality.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-backend.c

/// Represents a remote desktop session (D-Bus/RDP interface).
/// Hardware-specific; left opaque for D-Bus I/O.
pub struct RemoteDesktop;

impl RemoteDesktop {
    /// Create a new remote desktop instance.
    pub fn new() -> Self {
        RemoteDesktop
    }
}

impl Default for RemoteDesktop {
    fn default() -> Self {
        Self::new()
    }
}

/// A pointer constraint applied by client (e.g., via Wayland).
/// Limits pointer movement to a defined region; opaque for input handling.
pub struct PointerConstraint;

impl PointerConstraint {
    /// Create a new pointer constraint.
    pub fn new() -> Self {
        PointerConstraint
    }
}

impl Default for PointerConstraint {
    fn default() -> Self {
        Self::new()
    }
}

/// GLib event source wrapper for MetaBackend I/O.
/// Integrates backend operations into the main event loop.
pub struct BackendSource;

impl BackendSource {
    /// Create a new backend event source.
    pub fn new() -> Self {
        BackendSource
    }
}

impl Default for BackendSource {
    fn default() -> Self {
        Self::new()
    }
}

// Key signal indices from upstream (for GObject signal emission)
pub const SIGNAL_KEYMAP_CHANGED: u32 = 0;
pub const SIGNAL_KEYMAP_LAYOUT_GROUP_CHANGED: u32 = 1;
pub const SIGNAL_LAST_DEVICE_CHANGED: u32 = 2;
pub const SIGNAL_LID_IS_CLOSED_CHANGED: u32 = 3;
pub const SIGNAL_GPU_ADDED: u32 = 4;
pub const SIGNAL_PREPARE_SHUTDOWN: u32 = 5;
pub const SIGNAL_OVERRIDE_CURSOR: u32 = 6;
pub const SIGNAL_RESET_KEYMAP_DESCRIPTION: u32 = 7;
pub const SIGNAL_RESET_KEYMAP_LAYOUT_INDEX: u32 = 8;

/// Hidden pointer timeout in milliseconds (for auto-hiding cursors).
pub const HIDDEN_POINTER_TIMEOUT_MS: u32 = 300;

// Input event button codes (when not available from system headers)
pub const BTN_LEFT: u32 = 0x110;
pub const BTN_RIGHT: u32 = 0x111;
pub const BTN_MIDDLE: u32 = 0x112;
pub const BTN_STYLUS3: u32 = 0x149;
pub const BTN_TOUCH: u32 = 0x14a;
pub const BTN_STYLUS: u32 = 0x14b;
pub const BTN_STYLUS2: u32 = 0x14c;
pub const BTN_JOYSTICK: u32 = 0x120;
