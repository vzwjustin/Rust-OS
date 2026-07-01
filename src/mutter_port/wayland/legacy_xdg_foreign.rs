//! Wayland Legacy XDG Foreign module
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-legacy-xdg-foreign.h
//!
//! Provides legacy XDG foreign (surface handle export/import) protocol support.
//! Protocol binding and handle management are TODO.

/// Placeholder unit type for legacy XDG foreign support in the compositor.
pub struct MetaWaylandLegacyXdgForeign;

impl MetaWaylandLegacyXdgForeign {
    /// Initialize legacy XDG foreign support for the compositor.
    /// TODO: protocol binding for surface handle exchange.
    pub fn init(_compositor: *mut core::ffi::c_void) -> bool {
        false
    }
}
