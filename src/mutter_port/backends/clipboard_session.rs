//! Clipboard Session ported from GNOME Mutter's src/backends/
//!
//! Manages clipboard content transfers with MIME type support and asynchronous I/O.
//! Provides a D-Bus session interface for clipboard read/write operations with configurable timeouts.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-clipboard-session.c

use alloc::string::String;
use alloc::vec::Vec;

/// D-Bus Clipboard Session skeleton base type (opaque, hardware/D-Bus I/O bound).
pub struct DBusClipboardSkeleton;

/// Clipboard session object managing clipboard content transfers.
///
/// Handles asynchronous read/write operations with MIME type negotiation.
/// D-Bus transport is not available; methods track state locally.
pub struct MetaClipboardSession {
    /// Reference to D-Bus skeleton (opaque).
    pub dbus: DBusClipboardSkeleton,
    /// Whether the session is enabled.
    pub enabled: bool,
    /// Current selection MIME types.
    pub selection_mimes: Vec<String>,
    /// Pending transfer MIME type (if any).
    pub pending_transfer_mime: Option<String>,
}

impl MetaClipboardSession {
    /// Create a new clipboard session.
    pub fn new() -> Self {
        MetaClipboardSession {
            dbus: DBusClipboardSkeleton,
            enabled: false,
            selection_mimes: Vec::new(),
            pending_transfer_mime: None,
        }
    }

    /// Enable clipboard session. Marks the session as enabled and
    /// stores the offered MIME types. A full implementation would
    /// register the D-Bus clipboard interface.
    pub fn enable(&mut self, options: &[u8]) -> Result<(), &'static str> {
        // Parse options as null-separated MIME type strings.
        self.selection_mimes = options
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect();
        self.enabled = true;
        Ok(())
    }

    /// Disable clipboard session. Clears the enabled flag and
    /// selection state.
    pub fn disable(&mut self) -> Result<(), &'static str> {
        self.enabled = false;
        self.selection_mimes.clear();
        self.pending_transfer_mime = None;
        Ok(())
    }

    /// Set selection options. Updates the MIME type list.
    pub fn set_selection(&mut self, options: &[u8]) -> Result<(), &'static str> {
        if !self.enabled {
            return Err("clipboard session not enabled");
        }
        self.selection_mimes = options
            .split(|&b| b == 0)
            .filter(|s| !s.is_empty())
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .collect();
        Ok(())
    }

    /// Request clipboard transfer for a MIME type. Records the
    /// pending transfer. A full implementation would initiate an
    /// asynchronous D-Bus transfer.
    pub fn request_transfer(&mut self, mime_type: &str) {
        if self.enabled {
            self.pending_transfer_mime = Some(String::from(mime_type));
        }
    }

    /// Check if the session is enabled.
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// Get the offered MIME types.
    pub fn get_selection_mimes(&self) -> &[String] {
        &self.selection_mimes
    }
}

impl Default for MetaClipboardSession {
    fn default() -> Self {
        Self::new()
    }
}
