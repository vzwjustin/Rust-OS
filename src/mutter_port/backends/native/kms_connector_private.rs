//! Kms Connector Private — ported from GNOME Mutter
//!
//! KMS connector (output/display) properties and state management.
//! Maps DRM connector properties to high-level display configuration.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-kms-connector-private.h

use alloc::string::String;

/// KMS connector property identifiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsConnectorProp {
    CRTC_ID = 0,
    DPMS = 1,
    UNDERSCAN = 2,
    UNDERSCAN_HBORDER = 3,
    UNDERSCAN_VBORDER = 4,
    PRIVACY_SCREEN_SW_STATE = 5,
    PRIVACY_SCREEN_HW_STATE = 6,
    EDID = 7,
    TILE = 8,
    SUGGESTED_X = 9,
    SUGGESTED_Y = 10,
    HOTPLUG_MODE_UPDATE = 11,
    SCALING_MODE = 12,
    PANEL_ORIENTATION = 13,
    NON_DESKTOP = 14,
    MAX_BPC = 15,
    COLORSPACE = 16,
    HDR_OUTPUT_METADATA = 17,
    BROADCAST_RGB = 18,
    VRR_CAPABLE = 19,
    N_PROPS = 20,
}

/// DPMS (Display Power Management Signaling) states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsConnectorDpms {
    ON = 0,
    STANDBY = 1,
    SUSPEND = 2,
    OFF = 3,
    N_PROPS = 4,
    UNKNOWN = 5,
}

/// Underscan mode for legacy display support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsConnectorUnderscan {
    OFF = 0,
    ON = 1,
    AUTO = 2,
    N_PROPS = 3,
    UNKNOWN = 4,
}

/// Privacy screen state (physical shutter over display).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsConnectorPrivacyScreen {
    DISABLED = 0,
    ENABLED = 1,
    DISABLED_LOCKED = 2,
    ENABLED_LOCKED = 3,
    N_PROPS = 4,
    UNKNOWN = 5,
}

/// Scaling mode for resolution adaptation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsConnectorScalingMode {
    NONE = 0,
    FULL = 1,
    CENTER = 2,
    FULL_ASPECT = 3,
    N_PROPS = 4,
    UNKNOWN = 5,
}

/// Physical panel orientation correction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsConnectorPanelOrientation {
    NORMAL = 0,
    UPSIDE_DOWN = 1,
    LEFT_SIDE_UP = 2,
    RIGHT_SIDE_UP = 3,
    N_PROPS = 4,
    UNKNOWN = 5,
}

/// HDR color space support.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsConnectorColorspace {
    DEFAULT = 0,
    RGB_WIDE_GAMUT_FIXED_POINT = 1,
    RGB_WIDE_GAMUT_FLOATING_POINT = 2,
    RGB_OPRGB = 3,
    RGB_DCI_P3_RGB_D65 = 4,
    BT2020_RGB = 5,
    BT601_YCC = 6,
    BT709_YCC = 7,
    XVYCC_601 = 8,
    XVYCC_709 = 9,
    SYCC_601 = 10,
    OPYCC_601 = 11,
    BT2020_CYCC = 12,
    BT2020_YCC = 13,
    SMPTE_170M_YCC = 14,
    DCI_P3_RGB_THEATER = 15,
    N_PROPS = 16,
    UNKNOWN = 17,
}

/// Broadcast RGB range (full vs. limited).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsConnectorBroadcastRGB {
    AUTOMATIC = 0,
    FULL = 1,
    LIMITED_16_235 = 2,
    N_PROPS = 3,
    UNKNOWN = 4,
}

/// Number of connector properties.
pub const META_KMS_CONNECTOR_N_PROPS: u32 = 20;