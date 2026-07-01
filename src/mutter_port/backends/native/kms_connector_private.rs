//! Kms Connector Private — ported from GNOME Mutter
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-kms-connector-private.h

use alloc::string::String;

/// MetaKmsConnectorProp
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsConnectorProp {
    META_KMS_CONNECTOR_PROP_CRTC_ID = 0,
    META_KMS_CONNECTOR_PROP_DPMS,
    META_KMS_CONNECTOR_PROP_UNDERSCAN,
    META_KMS_CONNECTOR_PROP_UNDERSCAN_HBORDER,
    META_KMS_CONNECTOR_PROP_UNDERSCAN_VBORDER,
    META_KMS_CONNECTOR_PROP_PRIVACY_SCREEN_SW_STATE,
    META_KMS_CONNECTOR_PROP_PRIVACY_SCREEN_HW_STATE,
    META_KMS_CONNECTOR_PROP_EDID,
    META_KMS_CONNECTOR_PROP_TILE,
    META_KMS_CONNECTOR_PROP_SUGGESTED_X,
    META_KMS_CONNECTOR_PROP_SUGGESTED_Y,
    META_KMS_CONNECTOR_PROP_HOTPLUG_MODE_UPDATE,
    META_KMS_CONNECTOR_PROP_SCALING_MODE,
    META_KMS_CONNECTOR_PROP_PANEL_ORIENTATION,
    META_KMS_CONNECTOR_PROP_NON_DESKTOP,
    META_KMS_CONNECTOR_PROP_MAX_BPC,
}

/// MetaKmsConnectorDpms
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsConnectorDpms {
    META_KMS_CONNECTOR_DPMS_ON = 0,
    META_KMS_CONNECTOR_DPMS_STANDBY,
    META_KMS_CONNECTOR_DPMS_SUSPEND,
    META_KMS_CONNECTOR_DPMS_OFF,
    META_KMS_CONNECTOR_DPMS_N_PROPS,
    META_KMS_CONNECTOR_DPMS_UNKNOWN,
}

// TODO: Extract struct definitions from C header
// TODO: Add type definitions and implementations