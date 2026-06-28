//! Mutter compositor foundation staging.
//!
//! GNOME on Wayland expects Mutter to own the display socket and compositor
//! role. Until a real Mutter binary ships in the rootfs, the kernel provides
//! the pre-bound Wayland socket and wire-protocol handshake via
//! [`crate::wayland::server`].

/// Returns true when the GNOME overlay, Wayland compositor, and wire handshake
/// path are all ready for Mutter-style clients.
pub fn is_ready() -> bool {
    crate::gnome_overlay::is_ready()
        && crate::wayland::is_ready()
        && crate::wayland::server::is_handshake_ready()
}

/// Verify Mutter foundation prerequisites and the in-kernel Wayland handshake.
pub fn smoke_check() -> Result<(), &'static str> {
    if !crate::gnome_overlay::is_ready() {
        return Err("GNOME overlay required for Mutter foundation");
    }
    if !crate::wayland::is_ready() {
        return Err("Wayland compositor is not initialized");
    }
    crate::wayland::server::smoke_check()
}
