//! Core Mutter type definitions and forward declarations
//! Ported from meta/types.h

use crate::mutter_port::mtk::MtkRectangle;

/// Forward declarations for all major Mutter types
pub struct MetaBackend;
pub struct MetaContext;
pub struct MetaCompositor;
pub struct MetaDisplay;
pub struct MetaWindow;
pub struct MetaWorkspace;
pub struct MetaLaters;

pub struct MetaKeyBinding;
pub struct MetaCursorTracker;

pub struct MetaDnd;
pub struct MetaSettings;

pub struct MetaWorkspaceManager;
pub struct MetaSelection;
pub struct MetaDebugControl;
pub struct MetaWindowConfig;
pub struct MetaExternalConstraint;
pub struct MetaBacklight;

/// Virtual core pointer ID matching X11 convention
pub const META_VIRTUAL_CORE_POINTER_ID: u32 = 2;

/// Virtual core keyboard ID matching X11 convention
pub const META_VIRTUAL_CORE_KEYBOARD_ID: u32 = 3;

/// Replacement for X11 CurrentTime
pub const META_CURRENT_TIME: u32 = 0;

/// Virtual core touch device ID (XInput2 convention).
pub const META_VIRTUAL_CORE_TOUCH_ID: u32 = 4;

/// Atom value used to represent "None" in X11 atom operations.
pub const META_NONE_ATOM: u64 = 0;

/// Minimum window size constants (from meta/common.h).
pub const META_MIN_WINDOW_SIZE: i32 = 1;
pub const META_MAX_WINDOW_SIZE: i32 = i32::MAX;

/// Default D-Bus bus name for the mutter remote desktop service.
pub const META_REMOTE_DESKTOP_DBUS_BUS_NAME: &str = "org.gnome.Mutter.RemoteDesktop";

/// Default D-Bus object path for the remote desktop service.
pub const META_REMOTE_DESKTOP_DBUS_OBJECT_PATH: &str = "/org/gnome/Mutter/RemoteDesktop";
