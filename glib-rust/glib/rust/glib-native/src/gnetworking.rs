//! GNetworking matching `gio/gnetworking.h` / `gio/gnetworking.c`.
//!
//! Platform networking initialization. On Windows the upstream calls
//! `WSAStartup()`; on Linux/Unix it is a no-op. In this no_std port
//! `networking_init` is always a no-op since the RustOS kernel handles
//! network stack initialization at the driver layer.
//!
//! Also provides `getservbyname_ntohs` — service-by-name lookup with
//! network-to-host byte order conversion. In the kernel we return
//! `false` (not found) since there is no `/etc/services` file; callers
//! that need well-known ports should use numeric constants directly.
//!
//! Fully `no_std` compatible.

use core::sync::atomic::{AtomicBool, Ordering};

static NETWORKING_INITED: AtomicBool = AtomicBool::new(false);

/// Initializes the platform networking libraries.
///
/// On Windows this calls `WSAStartup()`. On Linux/Unix and in this
/// no_std port it is a no-op. GLib will call this itself if needed,
/// so you only need to call it if you directly call system networking
/// functions.
///
/// Mirrors `g_networking_init`.
pub fn networking_init() {
    NETWORKING_INITED.store(true, Ordering::SeqCst);
}

/// Returns whether `networking_init` has been called.
pub fn is_networking_inited() -> bool {
    NETWORKING_INITED.load(Ordering::SeqCst)
}

/// Looks up a service by name and protocol, returning the port in
/// host byte order.
///
/// Mirrors `g_getservbyname_ntohs`. In this no_std port we always
/// return `false` since there is no `/etc/services` database.
/// Callers that need well-known ports (e.g. HTTP = 80, HTTPS = 443)
/// should use numeric constants directly.
///
/// # Parameters
/// - `name`: Service name (e.g. `"http"`).
/// - `proto`: Protocol name (e.g. `"tcp"`).
/// - `out_port`: Receives the port number in host byte order on success.
///
/// # Returns
/// `true` if the service was found, `false` otherwise.
pub fn getservbyname_ntohs(_name: &str, _proto: &str, _out_port: &mut u16) -> bool {
    false
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_networking_init() {
        networking_init();
        assert!(is_networking_inited());
    }

    #[test]
    fn test_getservbyname_returns_false() {
        let mut port: u16 = 0;
        assert!(!getservbyname_ntohs("http", "tcp", &mut port));
    }
}
