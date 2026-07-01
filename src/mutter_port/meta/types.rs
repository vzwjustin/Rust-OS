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

// TODO: port additional type definitions as needed
