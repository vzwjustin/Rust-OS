//! Backlight Ref White Private ported from GNOME Mutter's src/backends/
//!
//! Private interface for the reference white backlight implementation.
//! Provides the constructor and type definition for use within the backend system.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-backlight-ref-white-private.h

use super::backlight_ref_white::BacklightRefWhite;

/// Type alias for the base Backlight class (opaque, used by private implementation).
pub type Backlight = ();

/// Constructor for creating a new BacklightRefWhite instance.
///
/// In the upstream C code, this takes a MetaBackend pointer and MetaMonitor pointer.
/// Those are hardware-specific, so this Rust stub leaves them as TODO I/O.
pub fn meta_backlight_ref_white_new() -> BacklightRefWhite {
    // TODO: Initialize from MetaBackend and MetaMonitor
    BacklightRefWhite::new()
}
