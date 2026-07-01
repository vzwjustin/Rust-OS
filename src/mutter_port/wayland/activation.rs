//! Wayland Activation module
//!
//! Handles xdg-activation protocol for application startup tokens.
//! Clients can request tokens to launch windows and get visual feedback.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-activation.h

use alloc::{string::String, vec::Vec};

/// Represents an XDG activation token for application startup.
/// Associates a token string with a surface, seat, and optional startup sequence.
pub struct MetaXdgActivationToken {
    pub surface: Option<*mut core::ffi::c_void>, // MetaWaylandSurface pointer
    pub seat: Option<*mut core::ffi::c_void>,    // MetaWaylandSeat pointer
    pub activation: Option<*mut core::ffi::c_void>, // MetaWaylandActivation pointer
    pub sequence: Option<*mut core::ffi::c_void>, // MetaStartupSequence pointer
    pub app_id: Option<String>,
    pub token: Option<String>,
    pub serial: u32,
    pub committed: bool,
}

/// Manages XDG activation tokens for the Wayland compositor.
/// Tracks token lifecycle, pending activations, and resource allocation.
pub struct MetaWaylandActivation {
    pub compositor: Option<*mut core::ffi::c_void>, // MetaWaylandCompositor pointer
    pub resource_list: Vec<*mut core::ffi::c_void>,
    pub token_list: Vec<*mut core::ffi::c_void>,
    /// Token registry mapping token strings to their owning resource
    /// pointers. The C code uses a GHashTable keyed by token string;
    /// here we keep a flat Vec of (token, resource) pairs with linear
    /// lookup since there is no GHashTable available in no_std.
    pub tokens: Vec<(String, *mut core::ffi::c_void)>,
    /// Pending activations keyed by token string, holding the surface
    /// pointer to activate once the token is committed.
    pub pending_activations: Vec<(String, *mut core::ffi::c_void)>,
}

impl MetaWaylandActivation {
    /// Create a new activation handler with empty registries.
    pub fn new() -> Self {
        MetaWaylandActivation {
            compositor: None,
            resource_list: Vec::new(),
            token_list: Vec::new(),
            tokens: Vec::new(),
            pending_activations: Vec::new(),
        }
    }

    /// Initialize wayland activation support for the compositor. Records
    /// the compositor pointer so token handlers can reach back into the
    /// compositor. A full implementation would also register the
    /// xdg_activation_v1 global via wl_global_create.
    pub fn init(&mut self, compositor: *mut core::ffi::c_void) {
        if !compositor.is_null() {
            self.compositor = Some(compositor);
        }
    }

    /// Register a token with its owning resource. If the token already
    /// exists, the resource pointer is replaced (mirroring
    /// g_hash_table_replace). Returns true if a new entry was added.
    pub fn add_token(&mut self, token: String, resource: *mut core::ffi::c_void) -> bool {
        if let Some(entry) = self.tokens.iter_mut().find(|(t, _)| *t == token) {
            entry.1 = resource;
            false
        } else {
            self.tokens.push((token, resource));
            true
        }
    }

    /// Look up the resource associated with a token string, if any.
    pub fn lookup_token(&self, token: &str) -> Option<*mut core::ffi::c_void> {
        self.tokens
            .iter()
            .find(|(t, _)| t.as_str() == token)
            .map(|(_, r)| *r)
    }

    /// Remove a token from the registry. Returns the removed resource
    /// pointer so the caller can destroy the underlying wl_resource.
    pub fn remove_token(&mut self, token: &str) -> Option<*mut core::ffi::c_void> {
        let pos = self.tokens.iter().position(|(t, _)| t.as_str() == token);
        pos.map(|i| self.tokens.remove(i).1)
    }

    /// Queue a pending activation for a token against a surface. The
    /// activation is applied when the token is later committed by the
    /// client. Replaces any existing pending activation for the token.
    pub fn add_pending_activation(&mut self, token: String, surface: *mut core::ffi::c_void) {
        if let Some(entry) = self
            .pending_activations
            .iter_mut()
            .find(|(t, _)| *t == token)
        {
            entry.1 = surface;
        } else {
            self.pending_activations.push((token, surface));
        }
    }

    /// Look up the pending activation surface for a token, if any.
    pub fn lookup_pending_activation(&self, token: &str) -> Option<*mut core::ffi::c_void> {
        self.pending_activations
            .iter()
            .find(|(t, _)| t.as_str() == token)
            .map(|(_, s)| *s)
    }

    /// Remove a pending activation for a token. Returns the surface
    /// pointer that was pending activation, if any.
    pub fn remove_pending_activation(&mut self, token: &str) -> Option<*mut core::ffi::c_void> {
        let pos = self
            .pending_activations
            .iter()
            .position(|(t, _)| t.as_str() == token);
        pos.map(|i| self.pending_activations.remove(i).1)
    }

    /// Finalize wayland activation support for the compositor. Drops all
    /// token and pending-activation entries and clears the resource and
    /// token lists so no stale pointers remain. A full implementation
    /// would also destroy each wl_resource via wl_resource_destroy and
    /// tear down the xdg_activation_v1 global.
    pub fn finalize(&mut self) {
        self.tokens.clear();
        self.pending_activations.clear();
        self.token_list.clear();
        self.resource_list.clear();
        self.compositor = None;
    }

    /// Number of registered tokens.
    pub fn token_count(&self) -> usize {
        self.tokens.len()
    }

    /// Number of pending activations.
    pub fn pending_count(&self) -> usize {
        self.pending_activations.len()
    }
}

impl Default for MetaWaylandActivation {
    fn default() -> Self {
        Self::new()
    }
}
