//! Common types used across Mutter backend modules
//! These types are extracted from Mutter C headers and ported to Rust.

/// MetaFixed16 — 16.16 fixed-point number
pub type MetaFixed16 = i32;

/// MtkMonitorTransform — display transform modes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MtkMonitorTransform {
    Normal = 0,
    Rotate90 = 1,
    Rotate180 = 2,
    Rotate270 = 3,
    Flipped = 4,
    FlippedRotate90 = 5,
    FlippedRotate180 = 6,
    FlippedRotate270 = 7,
}

/// MetaLogicalMonitorLayoutMode — layout mode for logical monitors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaLogicalMonitorLayoutMode {
    Logical = 0,
    Physical = 1,
}

/// MetaOutputRGBRange — RGB range options
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaOutputRGBRange {
    Auto = 0,
    Full = 1,
    Limited = 2,
}

/// MetaColorMode — color mode settings
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum MetaColorMode {
    #[default]
    Default = 0,
    SdrNative = 1,
    BT2100 = 2,
    BT2100Pq = 3,
    Bt2100 = 4,
}

/// MetaCrtcRefreshRateMode — refresh rate mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaCrtcRefreshRateMode {
    Exact = 0,
    Approximate = 1,
    Fixed = 2,
    Variable = 3,
}

/// MetaCrtcModeFlag — CRTC mode flags
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MetaCrtcModeFlag(pub u32);

/// MetaKmsFeedbackResult — KMS feedback result
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsFeedbackResult {
    Ok = 0,
    Failed = 1,
}

/// gatomicrefcount — atomic reference count (using u32)
pub type gatomicrefcount = u32;

/// CoglPixelFormat — pixel format codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum CoglPixelFormat {
    Rgba = 0,
    Rgb = 1,
}

/// MetaFixed16Rectangle — rectangle with fixed-point coordinates
#[derive(Debug, Clone, Copy)]
pub struct MetaFixed16Rectangle {
    pub x: MetaFixed16,
    pub y: MetaFixed16,
    pub width: MetaFixed16,
    pub height: MetaFixed16,
}

/// MonitorTransform — alias for MtkMonitorTransform
pub type MonitorTransform = MtkMonitorTransform;

/// MtkRectangle — integer rectangle (mirrors mutter's `MtkRectangle`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MtkRectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}
