//! `gosxappinfo` matching `gio/gosxappinfo.h`.
//!
//! macOS application info using Launch Services. This port models the
//! API with in-memory data since we don't have access to macOS frameworks
//! in the no_std kernel.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gappinfo::AppInfo;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// macOS application info backed by Launch Services.
#[derive(Debug, Clone)]
pub struct OsxAppInfo {
    /// Bundle identifier (e.g. `com.apple.TextEdit`).
    pub bundle_id: String,
    /// Display name (e.g. `TextEdit`).
    pub display_name: String,
    /// Path to the `.app` bundle.
    pub filename: String,
    /// URL schemes this app handles.
    pub schemes: Vec<String>,
    /// Content types this app can open.
    pub content_types: Vec<String>,
}

impl OsxAppInfo {
    /// Creates a new `OsxAppInfo`.
    pub fn new(bundle_id: &str, display_name: &str, filename: &str) -> Self {
        Self {
            bundle_id: bundle_id.to_string(),
            display_name: display_name.to_string(),
            filename: filename.to_string(),
            schemes: Vec::new(),
            content_types: Vec::new(),
        }
    }

    /// Returns the path to the `.app` bundle.
    pub fn get_filename(&self) -> &str {
        &self.filename
    }

    /// Returns all applications registered for a URL scheme.
    ///
    /// Mirrors `g_osx_app_info_get_all_for_scheme`.
    pub fn get_all_for_scheme(scheme: &str) -> Vec<OsxAppInfo> {
        REGISTRY
            .lock()
            .iter()
            .filter(|app| app.schemes.iter().any(|s| s == scheme))
            .cloned()
            .collect()
    }

    /// Returns all registered macOS applications.
    pub fn get_all() -> Vec<OsxAppInfo> {
        REGISTRY.lock().clone()
    }

    /// Registers an application in the in-memory registry.
    pub fn register(app: OsxAppInfo) {
        REGISTRY.lock().push(app);
    }
}

impl AppInfo for OsxAppInfo {
    fn get_id(&self) -> String {
        self.bundle_id.clone()
    }

    fn get_name(&self) -> String {
        self.display_name.clone()
    }

    fn get_description(&self) -> Option<String> {
        Some(self.display_name.clone())
    }

    fn get_executable(&self) -> String {
        self.filename.clone()
    }

    fn supports_uris(&self) -> bool {
        !self.schemes.is_empty()
    }

    fn supports_files(&self) -> bool {
        !self.content_types.is_empty()
    }

    fn should_show(&self) -> bool {
        true
    }
}

/// In-memory app registry.
static REGISTRY: Mutex<Vec<OsxAppInfo>> = Mutex::new(Vec::new());

/// Clears the registry (for testing).
pub fn clear_registry() {
    REGISTRY.lock().clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_app_info() {
        let app = OsxAppInfo::new(
            "com.apple.TextEdit",
            "TextEdit",
            "/Applications/TextEdit.app",
        );
        assert_eq!(app.get_id(), "com.apple.TextEdit");
        assert_eq!(app.get_name(), "TextEdit");
        assert_eq!(app.get_filename(), "/Applications/TextEdit.app");
    }

    #[test]
    fn test_get_all_for_scheme() {
        clear_registry();
        let mut app1 = OsxAppInfo::new("com.apple.Safari", "Safari", "/Applications/Safari.app");
        app1.schemes.push("https".to_string());
        app1.schemes.push("http".to_string());
        OsxAppInfo::register(app1);

        let mut app2 = OsxAppInfo::new("com.apple.Mail", "Mail", "/Applications/Mail.app");
        app2.schemes.push("mailto".to_string());
        OsxAppInfo::register(app2);

        let https_apps = OsxAppInfo::get_all_for_scheme("https");
        assert_eq!(https_apps.len(), 1);
        assert_eq!(https_apps[0].get_name(), "Safari");

        let mailto_apps = OsxAppInfo::get_all_for_scheme("mailto");
        assert_eq!(mailto_apps.len(), 1);
        assert_eq!(mailto_apps[0].get_name(), "Mail");
    }

    #[test]
    fn test_get_all() {
        clear_registry();
        OsxAppInfo::register(OsxAppInfo::new("app1", "App 1", "/app1"));
        OsxAppInfo::register(OsxAppInfo::new("app2", "App 2", "/app2"));
        assert_eq!(OsxAppInfo::get_all().len(), 2);
    }
}
