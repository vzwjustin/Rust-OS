//! Application launch context ported from GNOME Mutter (src/core/meta-launch-context.c).
//!
//! Provides context for launching applications, including environment variables,
//! workspace assignment, and startup notification integration.
//!
//! Ported from: /home/justin/Downloads/mutter-main/src/core/meta-launch-context.c
//! Omitted: GObject property machinery, GAppLaunchContext parent class integration,
//! X11/Wayland startup notification (requires display server integration),
//! D-Bus service integration

use crate::desktop::window_manager::WindowId;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

/// Workspace identifier for launch context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct WorkspaceId(pub usize);

/// Launch context for spawning applications.
///
/// Tracks environment variables, target workspace, and launch timestamp
/// to establish proper context for launched applications.
pub struct LaunchContext {
    /// Environment variables for the launched process.
    env_vars: BTreeMap<String, String>,
    /// Target workspace for the application.
    workspace_id: Option<WorkspaceId>,
    /// X11/Wayland timestamp for the launch.
    timestamp: u32,
    /// Startup ID for notification (if any).
    startup_id: Option<String>,
}

impl LaunchContext {
    /// Create a new launch context with current environment and timestamp.
    pub fn new(timestamp: u32) -> Self {
        LaunchContext {
            env_vars: BTreeMap::new(),
            workspace_id: None,
            timestamp,
            startup_id: None,
        }
    }

    /// Set the target workspace for the launched application.
    pub fn set_workspace(&mut self, workspace_id: WorkspaceId) {
        self.workspace_id = Some(workspace_id);
    }

    /// Get the target workspace for this launch.
    pub fn workspace(&self) -> Option<WorkspaceId> {
        self.workspace_id
    }

    /// Set the timestamp for this launch.
    pub fn set_timestamp(&mut self, timestamp: u32) {
        self.timestamp = timestamp;
    }

    /// Get the launch timestamp.
    pub fn timestamp(&self) -> u32 {
        self.timestamp
    }

    /// Set an environment variable for the launched process.
    pub fn setenv(&mut self, name: &str, value: &str) {
        self.env_vars
            .insert(String::from(name), String::from(value));
    }

    /// Get an environment variable.
    pub fn getenv(&self, name: &str) -> Option<&str> {
        self.env_vars.get(name).map(|s| s.as_str())
    }

    /// Remove an environment variable.
    pub fn unsetenv(&mut self, name: &str) {
        self.env_vars.remove(name);
    }

    /// Get all environment variables as a vector of (name, value) pairs.
    pub fn env_vars(&self) -> Vec<(&str, &str)> {
        self.env_vars
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect()
    }

    /// Set the startup ID for X11/Wayland startup notification.
    ///
    /// The startup ID is used to associate future windows with this launch.
    pub fn set_startup_id(&mut self, startup_id: &str) {
        self.startup_id = Some(String::from(startup_id));
    }

    /// Get the startup ID for this launch.
    pub fn startup_id(&self) -> Option<&str> {
        self.startup_id.as_deref()
    }
}

impl Default for LaunchContext {
    fn default() -> Self {
        Self::new(0)
    }
}

/// Launch configuration builder for applications.
pub struct LaunchContextBuilder {
    timestamp: u32,
    workspace_id: Option<WorkspaceId>,
    env_vars: BTreeMap<String, String>,
    startup_id: Option<String>,
}

impl LaunchContextBuilder {
    /// Create a new launch context builder.
    pub fn new() -> Self {
        LaunchContextBuilder {
            timestamp: 0,
            workspace_id: None,
            env_vars: BTreeMap::new(),
            startup_id: None,
        }
    }

    /// Set the launch timestamp.
    pub fn timestamp(mut self, ts: u32) -> Self {
        self.timestamp = ts;
        self
    }

    /// Set the target workspace.
    pub fn workspace(mut self, ws: WorkspaceId) -> Self {
        self.workspace_id = Some(ws);
        self
    }

    /// Add an environment variable.
    pub fn env(mut self, name: &str, value: &str) -> Self {
        self.env_vars
            .insert(String::from(name), String::from(value));
        self
    }

    /// Set the startup ID.
    pub fn startup_id(mut self, id: &str) -> Self {
        self.startup_id = Some(String::from(id));
        self
    }

    /// Build the launch context.
    pub fn build(self) -> LaunchContext {
        let mut ctx = LaunchContext {
            env_vars: self.env_vars,
            workspace_id: self.workspace_id,
            timestamp: self.timestamp,
            startup_id: self.startup_id,
        };

        // Initialize with default display environment variables
        // In a full implementation, these would be set from the compositor
        // Omitted: getenv("DISPLAY"), getenv("WAYLAND_DISPLAY") - requires environment integration

        ctx
    }
}

impl Default for LaunchContextBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_launch_context_creation() {
        let ctx = LaunchContext::new(1000);
        assert_eq!(ctx.timestamp(), 1000);
        assert_eq!(ctx.workspace(), None);
    }

    #[test]
    fn test_setenv_getenv() {
        let mut ctx = LaunchContext::new(1000);
        ctx.setenv("TEST_VAR", "test_value");
        assert_eq!(ctx.getenv("TEST_VAR"), Some("test_value"));
    }

    #[test]
    fn test_unsetenv() {
        let mut ctx = LaunchContext::new(1000);
        ctx.setenv("TEST_VAR", "test_value");
        ctx.unsetenv("TEST_VAR");
        assert_eq!(ctx.getenv("TEST_VAR"), None);
    }

    #[test]
    fn test_set_workspace() {
        let mut ctx = LaunchContext::new(1000);
        let ws = WorkspaceId(2);
        ctx.set_workspace(ws);
        assert_eq!(ctx.workspace(), Some(ws));
    }

    #[test]
    fn test_startup_id() {
        let mut ctx = LaunchContext::new(1000);
        ctx.set_startup_id("startup-123");
        assert_eq!(ctx.startup_id(), Some("startup-123"));
    }

    #[test]
    fn test_builder() {
        let ctx = LaunchContextBuilder::new()
            .timestamp(2000)
            .workspace(WorkspaceId(1))
            .env("VAR1", "value1")
            .env("VAR2", "value2")
            .startup_id("test-startup")
            .build();

        assert_eq!(ctx.timestamp(), 2000);
        assert_eq!(ctx.workspace(), Some(WorkspaceId(1)));
        assert_eq!(ctx.getenv("VAR1"), Some("value1"));
        assert_eq!(ctx.getenv("VAR2"), Some("value2"));
        assert_eq!(ctx.startup_id(), Some("test-startup"));
    }

    #[test]
    fn test_env_vars_vector() {
        let mut ctx = LaunchContext::new(1000);
        ctx.setenv("VAR1", "val1");
        ctx.setenv("VAR2", "val2");

        let vars = ctx.env_vars();
        assert_eq!(vars.len(), 2);
    }
}
