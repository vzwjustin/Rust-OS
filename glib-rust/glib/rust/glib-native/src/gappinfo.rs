//! GAppInfo matching `gio/gappinfo.h`.
//!
//! Upstream `GAppInfo` is an interface for application information
//! (name, executable, icon, launch). We port it as a Rust trait with
//! a simple concrete implementation.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Trait for application info (`GAppInfo`).
pub trait AppInfo {
    /// Gets the ID of the application.
    fn get_id(&self) -> String;

    /// Gets the name of the application.
    fn get_name(&self) -> String;

    /// Gets a short description.
    fn get_description(&self) -> Option<String>;

    /// Gets the executable name.
    fn get_executable(&self) -> String;

    /// Checks if the app supports launching URIs.
    fn supports_uris(&self) -> bool;

    /// Checks if the app supports files.
    fn supports_files(&self) -> bool;

    /// Checks if the app should be shown in menus.
    fn should_show(&self) -> bool;

    /// Checks if two `AppInfo`s are equal.
    fn equal(&self, other: &dyn AppInfo) -> bool {
        self.get_id() == other.get_id()
    }
}

/// A simple concrete `AppInfo` implementation.
pub struct SimpleAppInfo {
    id: String,
    name: String,
    description: Option<String>,
    executable: String,
    supports_uris: bool,
    supports_files: bool,
    should_show: bool,
}

impl SimpleAppInfo {
    pub fn new(id: &str, name: &str, executable: &str) -> Self {
        Self {
            id: id.to_string(),
            name: name.to_string(),
            description: None,
            executable: executable.to_string(),
            supports_uris: false,
            supports_files: true,
            should_show: true,
        }
    }

    pub fn with_description(mut self, desc: &str) -> Self {
        self.description = Some(desc.to_string());
        self
    }

    pub fn with_uris_support(mut self, supports: bool) -> Self {
        self.supports_uris = supports;
        self
    }

    pub fn with_files_support(mut self, supports: bool) -> Self {
        self.supports_files = supports;
        self
    }
}

impl AppInfo for SimpleAppInfo {
    fn get_id(&self) -> String {
        self.id.clone()
    }

    fn get_name(&self) -> String {
        self.name.clone()
    }

    fn get_description(&self) -> Option<String> {
        self.description.clone()
    }

    fn get_executable(&self) -> String {
        self.executable.clone()
    }

    fn supports_uris(&self) -> bool {
        self.supports_uris
    }

    fn supports_files(&self) -> bool {
        self.supports_files
    }

    fn should_show(&self) -> bool {
        self.should_show
    }
}

/// App launch context (`GAppLaunchContext`).
pub struct AppLaunchContext {
    display: Mutex<Option<String>>,
    env: Mutex<Vec<(String, String)>>,
}

impl AppLaunchContext {
    pub fn new() -> Self {
        Self {
            display: Mutex::new(None),
            env: Mutex::new(Vec::new()),
        }
    }

    pub fn set_display(&self, display: &str) {
        *self.display.lock() = Some(display.to_string());
    }

    pub fn get_display(&self) -> Option<String> {
        self.display.lock().clone()
    }

    pub fn setenv(&self, key: &str, value: &str) {
        self.env.lock().push((key.to_string(), value.to_string()));
    }

    pub fn getenv(&self, key: &str) -> Option<String> {
        self.env
            .lock()
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.clone())
    }
}

impl Default for AppLaunchContext {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_app_info_new() {
        let app = SimpleAppInfo::new("org.test.App", "Test App", "/usr/bin/testapp");
        assert_eq!(app.get_id(), "org.test.App");
        assert_eq!(app.get_name(), "Test App");
        assert_eq!(app.get_executable(), "/usr/bin/testapp");
        assert_eq!(app.get_description(), None);
        assert!(!app.supports_uris());
        assert!(app.supports_files());
        assert!(app.should_show());
    }

    #[test]
    fn test_simple_app_info_builder() {
        let app = SimpleAppInfo::new("org.test.App2", "Test App 2", "/usr/bin/testapp2")
            .with_description("A test application")
            .with_uris_support(true)
            .with_files_support(false);
        assert_eq!(
            app.get_description(),
            Some("A test application".to_string())
        );
        assert!(app.supports_uris());
        assert!(!app.supports_files());
    }

    #[test]
    fn test_app_info_equal() {
        let app1 = SimpleAppInfo::new("org.test.App", "App", "/bin/app");
        let app2 = SimpleAppInfo::new("org.test.App", "Different Name", "/bin/other");
        let app3 = SimpleAppInfo::new("org.other.App", "Other", "/bin/other");
        assert!(app1.equal(&app2));
        assert!(!app1.equal(&app3));
    }

    #[test]
    fn test_app_launch_context_new() {
        let ctx = AppLaunchContext::new();
        assert_eq!(ctx.get_display(), None);
    }

    #[test]
    fn test_app_launch_context_display() {
        let ctx = AppLaunchContext::new();
        ctx.set_display(":0");
        assert_eq!(ctx.get_display(), Some(":0".to_string()));
    }

    #[test]
    fn test_app_launch_context_env() {
        let ctx = AppLaunchContext::new();
        ctx.setenv("DISPLAY", ":1");
        ctx.setenv("PATH", "/usr/bin");
        assert_eq!(ctx.getenv("DISPLAY"), Some(":1".to_string()));
        assert_eq!(ctx.getenv("PATH"), Some("/usr/bin".to_string()));
        assert_eq!(ctx.getenv("HOME"), None);
    }
}
