//! Mutter enumeration types
//! Ported from meta/meta-enums.h

/// Window grab operations with flags for direction and modifiers
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaGrabOp {
    None = 0,

    // Window grab operations base
    Moving = 1,
    MovingUnconstrained = 1 | 0x0400,
    ResizingNw = 1 | 0x8000 | 0x1000,
    ResizingN = 1 | 0x8000,
    ResizingNe = 1 | 0x8000 | 0x2000,
    ResizingE = 1 | 0x2000,
    ResizingSwSe = 1 | 0x4000 | 0x1000,
    ResizingS = 1 | 0x4000,
    ResizingSeCorner = 1 | 0x4000 | 0x2000,
    ResizingW = 1 | 0x1000,

    KeyboardMoving = 1 | 0x0100,
    KeyboardResizingUnknown = 1 | 0x0100 | 0x0200,
    KeyboardResizingNw = 1 | 0x0100 | 0x8000 | 0x1000,
    KeyboardResizingN = 1 | 0x0100 | 0x8000,
    KeyboardResizingNe = 1 | 0x0100 | 0x8000 | 0x2000,
    KeyboardResizingE = 1 | 0x0100 | 0x2000,
    KeyboardResizingSw = 1 | 0x0100 | 0x4000 | 0x1000,
    KeyboardResizingS = 1 | 0x0100 | 0x4000,
    KeyboardResizingSe = 1 | 0x0100 | 0x4000 | 0x2000,
    KeyboardResizingW = 1 | 0x0100 | 0x1000,
}

/// Window frame type classifications
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaFrameType {
    Normal = 0,
    Dialog = 1,
    ModalDialog = 2,
    Utility = 3,
    Menu = 4,
    Border = 5,
    Attached = 6,
    Last = 7,
}

/// Directional indicators
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaDirection {
    Left = 1,
    Right = 2,
    Top = 3,
    Bottom = 4,
    Up = 5,
    Down = 6,
    Horizontal = 7,
    Vertical = 8,
}

/// Window types for different window classes
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaWindowType {
    Normal = 0,
    Desktop = 1,
    Dock = 2,
    Dialog = 3,
    ModalDialog = 4,
    Toolbar = 5,
    Menu = 6,
    Utility = 7,
    Splashscreen = 8,
    DropdownMenu = 9,
    PopupMenu = 10,
    Tooltip = 11,
    Notification = 12,
    Combo = 13,
    Dnd = 14,
    OverrideOther = 15,
}

/// Maximize direction flags
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaMaximizeFlags {
    Horizontal = 1 << 0,
    Vertical = 1 << 1,
    Both = (1 << 0) | (1 << 1),
}

/// Window client type (Wayland or X11)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaWindowClientType {
    Wayland = 0,
    X11 = 1,
}

/// Tab listing modes for window switcher
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaTabList {
    Normal = 0,
    Docks = 1,
    Group = 2,
    NormalAll = 3,
    NormalAllMru = 4,
}

/// Tab display modes for window switcher
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaTabShowType {
    Icon = 0,      // Alt-Tab mode
    Instantly = 1, // Alt-Esc mode
}

/// Tablet pad feature types
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaPadFeatureType {
    Ring = 0,
    Strip = 1,
    Dial = 2,
}

/// Tablet pad directional input
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaPadDirection {
    Up = 1,
    Down = 2,
    Clockwise = 3,
    CounterClockwise = 4,
}

/// Button functions in window decorations
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MetaButtonFunction {
    Menu = 0,
    ApplyButton = 1,
    ApplyButtonHelp = 2,
    Help = 3,
    Maximize = 4,
    Restore = 5,
    Minimize = 6,
    Close = 7,
    Shade = 8,
    Unshade = 9,
    AboveTab = 10,
    BelowTab = 11,
    LockButton = 12,
    UnlockButton = 13,
    Last = 14,
}

// TODO: Add remaining enums from meta-enums.h
