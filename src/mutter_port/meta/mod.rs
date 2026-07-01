//! Mutter meta/ subsystem port
//!
//! This module provides Rust bindings and types for GNOME Mutter's public API.
//! Ported from /home/justin/Downloads/mutter-main/src/meta/ (59 C header files)
//!
//! The Mutter meta/ API covers:
//! - Core display and window management
//! - Workspace/virtual desktop management
//! - Graphics composition and rendering
//! - Input handling (keyboard, cursor, selection)
//! - Monitor/output configuration
//! - Wayland and X11 protocol support
//! - Plugin and extension interfaces

pub mod backend;
pub mod common;
pub mod compositor;
pub mod cursor;
pub mod display;
pub mod enums;
pub mod keybindings;
pub mod managers;
pub mod misc;
pub mod monitor;
pub mod other;
pub mod plugin;
pub mod selection;
pub mod types;
pub mod util;
pub mod wayland;
pub mod window;
pub mod workspace;
pub mod x11;

// Re-export key types for convenient access
pub use backend::{MetaBackend, MetaContext};
pub use common::{MetaButtonLayout, MetaFrameBorder, MetaFrameBorders};
pub use compositor::{MetaBackground, MetaCompositor, MetaShapedTexture, MetaWindowActor};
pub use cursor::MetaCursorTracker;
pub use display::MetaDisplay;
pub use enums::*;
pub use keybindings::{MetaKeyBinding, MetaKeybindingAction, MetaKeymapDescription};
pub use managers::{
    MetaDebugControl, MetaIdleMonitor, MetaOrientation, MetaOrientationManager,
    MetaWorkspaceManager,
};
pub use misc::{
    MetaBacklight, MetaCloseDialog, MetaDnd, MetaExternalConstraint, MetaLaunchContext,
    MetaSettings, MetaSoundPlayer, MetaWindowConfig,
};
pub use monitor::{MetaLogicalMonitor, MetaMonitor, MetaMonitorManager};
pub use other::{
    MetaBackgroundActor, MetaBackgroundContent, MetaInhibitShortcutsDialog, MetaLaters,
    MetaMultiTexture, MetaMultiTextureFormat, MetaRemoteAccessController, MetaStartupNotification,
    MetaWindowGroup,
};
pub use plugin::MetaPlugin;
pub use selection::{
    MetaSelection, MetaSelectionSource, MetaSelectionSourceMemory, MetaSelectionType,
};
pub use types::*;
pub use wayland::{MetaWaylandClient, MetaWaylandCompositor, MetaWaylandSurface};
pub use window::MetaWindow;
pub use workspace::MetaWorkspace;
pub use x11::{MetaX11Display, MetaX11Group, MetaX11WindowType};
