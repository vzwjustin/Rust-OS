//! Monitor Manager — ported from GNOME Mutter
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-monitor-manager-private.h

use alloc::string::String;

/// MetaPrivacyScreenChangeState
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaPrivacyScreenChangeState {
    META_PRIVACY_SCREEN_CHANGE_STATE_NONE,
    META_PRIVACY_SCREEN_CHANGE_STATE_INIT,
    META_PRIVACY_SCREEN_CHANGE_STATE_PENDING_HOTKEY,
    META_PRIVACY_SCREEN_CHANGE_STATE_PENDING_SETTING,
}

// TODO: Extract struct definitions from C header
// TODO: Add type definitions and implementations