//! Debug control interface ported from GNOME Mutter (src/core/meta-debug-control.c).
//!
//! Provides a D-Bus interface for debugging and controlling Mutter behavior at runtime.
//! Allows enabling/disabling debug features without recompilation.
//!
//! Ported from: /home/justin/Downloads/mutter-main/src/core/meta-debug-control.c
//! Omitted: GObject property machinery, D-Bus service registration (g_dbus_interface_skeleton_export),
//! D-Bus connection handling, GBusNameOwnedCallback integration

use alloc::string::String;

/// Debug features that can be controlled at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebugFlags {
    /// Force HDR (High Dynamic Range) rendering.
    pub force_hdr: bool,
    /// Force linear blending in rendering.
    pub force_linear_blending: bool,
    /// Enable session management protocol debug.
    pub session_management_protocol: bool,
    /// Inhibit hardware cursor.
    pub inhibit_hw_cursor: bool,
    /// Run accessibility manager without access control.
    pub a11y_manager_without_access_control: bool,
}

impl Default for DebugFlags {
    fn default() -> Self {
        DebugFlags {
            force_hdr: false,
            force_linear_blending: false,
            session_management_protocol: false,
            inhibit_hw_cursor: false,
            a11y_manager_without_access_control: false,
        }
    }
}

/// Debug control service for runtime configuration.
pub struct DebugControl {
    /// Current debug flags.
    flags: DebugFlags,
    /// Whether the service is exported on D-Bus.
    exported: bool,
}

impl DebugControl {
    /// Create a new debug control service.
    pub fn new() -> Self {
        DebugControl {
            flags: DebugFlags::default(),
            exported: false,
        }
    }

    /// Create debug control with environment variables (as in original C code).
    pub fn from_env() -> Self {
        let mut ctrl = DebugControl::new();

        // In a full implementation, these would use getenv()
        // Omitted: environment variable reading - requires stdlib integration

        ctrl
    }

    /// Set a debug flag.
    pub fn set_flag(&mut self, flag: &str, value: bool) {
        match flag {
            "force-hdr" => self.flags.force_hdr = value,
            "force-linear-blending" => self.flags.force_linear_blending = value,
            "session-management-protocol" => self.flags.session_management_protocol = value,
            "inhibit-hw-cursor" => self.flags.inhibit_hw_cursor = value,
            "a11y-manager-without-access-control" => {
                self.flags.a11y_manager_without_access_control = value
            }
            _ => {}
        }
    }

    /// Get the current value of a debug flag.
    pub fn get_flag(&self, flag: &str) -> Option<bool> {
        match flag {
            "force-hdr" => Some(self.flags.force_hdr),
            "force-linear-blending" => Some(self.flags.force_linear_blending),
            "session-management-protocol" => Some(self.flags.session_management_protocol),
            "inhibit-hw-cursor" => Some(self.flags.inhibit_hw_cursor),
            "a11y-manager-without-access-control" => {
                Some(self.flags.a11y_manager_without_access_control)
            }
            _ => None,
        }
    }

    /// Get all debug flags.
    pub fn flags(&self) -> DebugFlags {
        self.flags
    }

    /// Set all debug flags at once.
    pub fn set_flags(&mut self, flags: DebugFlags) {
        self.flags = flags;
    }

    /// Mark the service as exported on D-Bus.
    pub fn set_exported(&mut self, exported: bool) {
        self.exported = exported;
    }

    /// Check if the service is exported on D-Bus.
    pub fn is_exported(&self) -> bool {
        self.exported
    }

    /// Export the service on D-Bus (placeholder).
    ///
    /// In a full implementation, this would call g_dbus_interface_skeleton_export
    /// to register the D-Bus object.
    /// Omitted: D-Bus registration - requires D-Bus daemon integration
    pub fn export(&mut self) -> Result<(), &'static str> {
        self.exported = true;
        Ok(())
    }

    /// Unexport the service from D-Bus (placeholder).
    pub fn unexport(&mut self) {
        self.exported = false;
    }
}

impl Default for DebugControl {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for debug control configuration.
pub struct DebugControlBuilder {
    flags: DebugFlags,
}

impl DebugControlBuilder {
    /// Create a new builder with default flags.
    pub fn new() -> Self {
        DebugControlBuilder {
            flags: DebugFlags::default(),
        }
    }

    /// Set force_hdr flag.
    pub fn force_hdr(mut self, value: bool) -> Self {
        self.flags.force_hdr = value;
        self
    }

    /// Set force_linear_blending flag.
    pub fn force_linear_blending(mut self, value: bool) -> Self {
        self.flags.force_linear_blending = value;
        self
    }

    /// Set session_management_protocol flag.
    pub fn session_management_protocol(mut self, value: bool) -> Self {
        self.flags.session_management_protocol = value;
        self
    }

    /// Set inhibit_hw_cursor flag.
    pub fn inhibit_hw_cursor(mut self, value: bool) -> Self {
        self.flags.inhibit_hw_cursor = value;
        self
    }

    /// Set a11y_manager_without_access_control flag.
    pub fn a11y_manager_without_access_control(mut self, value: bool) -> Self {
        self.flags.a11y_manager_without_access_control = value;
        self
    }

    /// Build the debug control service.
    pub fn build(self) -> DebugControl {
        DebugControl {
            flags: self.flags,
            exported: false,
        }
    }
}

impl Default for DebugControlBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_debug_flags_default() {
        let flags = DebugFlags::default();
        assert!(!flags.force_hdr);
        assert!(!flags.force_linear_blending);
        assert!(!flags.session_management_protocol);
        assert!(!flags.inhibit_hw_cursor);
        assert!(!flags.a11y_manager_without_access_control);
    }

    #[test]
    fn test_debug_control_creation() {
        let ctrl = DebugControl::new();
        assert!(!ctrl.is_exported());
        assert!(!ctrl.flags().force_hdr);
    }

    #[test]
    fn test_set_flag() {
        let mut ctrl = DebugControl::new();
        ctrl.set_flag("force-hdr", true);
        assert!(ctrl.get_flag("force-hdr").unwrap());
    }

    #[test]
    fn test_get_flag() {
        let mut ctrl = DebugControl::new();
        ctrl.set_flag("inhibit-hw-cursor", true);
        assert_eq!(ctrl.get_flag("inhibit-hw-cursor"), Some(true));
        assert_eq!(ctrl.get_flag("unknown-flag"), None);
    }

    #[test]
    fn test_set_all_flags() {
        let mut ctrl = DebugControl::new();
        let flags = DebugFlags {
            force_hdr: true,
            force_linear_blending: true,
            session_management_protocol: false,
            inhibit_hw_cursor: true,
            a11y_manager_without_access_control: false,
        };
        ctrl.set_flags(flags);
        assert!(ctrl.flags().force_hdr);
        assert!(ctrl.flags().force_linear_blending);
        assert!(!ctrl.flags().session_management_protocol);
    }

    #[test]
    fn test_export() {
        let mut ctrl = DebugControl::new();
        assert!(!ctrl.is_exported());
        assert!(ctrl.export().is_ok());
        assert!(ctrl.is_exported());
        ctrl.unexport();
        assert!(!ctrl.is_exported());
    }

    #[test]
    fn test_builder() {
        let ctrl = DebugControlBuilder::new()
            .force_hdr(true)
            .inhibit_hw_cursor(true)
            .build();

        assert!(ctrl.get_flag("force-hdr").unwrap());
        assert!(ctrl.get_flag("inhibit-hw-cursor").unwrap());
        assert!(!ctrl.get_flag("force-linear-blending").unwrap());
    }
}
