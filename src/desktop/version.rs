//! GNOME Desktop version information — ported from gnome-desktop-version.c
//!
//! Provides the GNOME platform version constant and accessor, matching the
//! upstream `gnome_get_platform_version()` API.

/// GNOME desktop platform version (major).  Upstream uses the meson-defined
/// `GNOME_PLATFORM` macro; we hard-code 44 (the latest stable at port time).
pub const GNOME_DESKTOP_PLATFORM_VERSION: i32 = 44;

/// Returns the GNOME platform major version.  Dynamic-language equivalent of
/// the `GNOME_DESKTOP_PLATFORM_VERSION` constant.
pub fn gnome_get_platform_version() -> i32 {
    GNOME_DESKTOP_PLATFORM_VERSION
}
