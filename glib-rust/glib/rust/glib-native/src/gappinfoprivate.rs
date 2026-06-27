//! `gappinfoprivate` matching `gio/gappinfoprivate.h`.
//!
//! Private app info API: monitor fire, create from commandline,
//! and `_impl` wrappers for app info queries.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gappinfo::AppInfo;
use crate::prelude::*;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Fires the app info monitor signal (mirrors `g_app_info_monitor_fire`).
pub fn monitor_fire() {
    *MONITOR_FIRED.lock() += 1;
}

/// Returns how many times `monitor_fire` was called.
pub fn monitor_fire_count() -> u64 {
    *MONITOR_FIRED.lock()
}

/// Resets the monitor fire count (for testing).
pub fn reset_monitor() {
    *MONITOR_FIRED.lock() = 0;
}

/// App info create flags (mirrors `GAppInfoCreateFlags`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppInfoCreateFlags {
    None,
    NeedsTerminal,
    SupportsUris,
}

/// Creates an `AppInfo` from a commandline (mirrors `g_app_info_create_from_commandline_impl`).
pub fn create_from_commandline_impl(
    commandline: &str,
    application_name: Option<&str>,
    _flags: AppInfoCreateFlags,
) -> Result<SimpleAppInfo, String> {
    if commandline.is_empty() {
        return Err("commandline must not be empty".to_string());
    }
    Ok(SimpleAppInfo {
        name: application_name.unwrap_or("Unknown").to_string(),
        commandline: commandline.to_string(),
    })
}

/// Simple in-memory `AppInfo` for the private API.
#[derive(Debug, Clone)]
pub struct SimpleAppInfo {
    pub name: String,
    pub commandline: String,
}

impl AppInfo for SimpleAppInfo {
    fn get_id(&self) -> String {
        self.name.clone()
    }
    fn get_name(&self) -> String {
        self.name.clone()
    }
    fn get_description(&self) -> Option<String> {
        None
    }
    fn get_executable(&self) -> String {
        self.commandline.clone()
    }
    fn supports_uris(&self) -> bool {
        false
    }
    fn supports_files(&self) -> bool {
        true
    }
    fn should_show(&self) -> bool {
        true
    }
}

/// Registered app infos by content type.
static APP_INFOS: Mutex<Vec<(String, Vec<SimpleAppInfo>)>> = Mutex::new(Vec::new());

/// Returns recommended apps for a content type (mirrors `g_app_info_get_recommended_for_type_impl`).
pub fn get_recommended_for_type_impl(content_type: &str) -> Vec<SimpleAppInfo> {
    get_all_for_type_impl(content_type)
}

/// Returns fallback apps for a content type (mirrors `g_app_info_get_fallback_for_type_impl`).
pub fn get_fallback_for_type_impl(content_type: &str) -> Vec<SimpleAppInfo> {
    get_all_for_type_impl(content_type)
}

/// Returns all apps for a content type (mirrors `g_app_info_get_all_for_type_impl`).
pub fn get_all_for_type_impl(content_type: &str) -> Vec<SimpleAppInfo> {
    APP_INFOS
        .lock()
        .iter()
        .filter(|(ct, _)| ct == content_type)
        .flat_map(|(_, apps)| apps.iter().cloned())
        .collect()
}

/// Resets type associations (mirrors `g_app_info_reset_type_associations_impl`).
pub fn reset_type_associations_impl(content_type: &str) {
    APP_INFOS.lock().retain(|(ct, _)| ct != content_type);
}

/// Returns the default app for a content type (mirrors `g_app_info_get_default_for_type_impl`).
pub fn get_default_for_type_impl(
    content_type: &str,
    _must_support_uris: bool,
) -> Option<SimpleAppInfo> {
    get_all_for_type_impl(content_type).into_iter().next()
}

/// Returns the default app for a URI scheme (mirrors `g_app_info_get_default_for_uri_scheme_impl`).
pub fn get_default_for_uri_scheme_impl(_uri_scheme: &str) -> Option<SimpleAppInfo> {
    None
}

/// Returns all registered apps (mirrors `g_app_info_get_all_impl`).
pub fn get_all_impl() -> Vec<SimpleAppInfo> {
    APP_INFOS
        .lock()
        .iter()
        .flat_map(|(_, apps)| apps.iter().cloned())
        .collect()
}

/// Registers an app for a content type.
pub fn register_app(content_type: &str, app: SimpleAppInfo) {
    let mut infos = APP_INFOS.lock();
    if let Some(entry) = infos.iter_mut().find(|(ct, _)| ct == content_type) {
        entry.1.push(app);
    } else {
        infos.push((content_type.to_string(), vec![app]));
    }
}

/// Clears all registrations (for testing).
pub fn clear_all() {
    APP_INFOS.lock().clear();
    reset_monitor();
}

static MONITOR_FIRED: Mutex<u64> = Mutex::new(0);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monitor_fire() {
        reset_monitor();
        assert_eq!(monitor_fire_count(), 0);
        monitor_fire();
        monitor_fire();
        assert_eq!(monitor_fire_count(), 2);
    }

    #[test]
    fn test_create_from_commandline() {
        let app =
            create_from_commandline_impl("test --foo", Some("Test"), AppInfoCreateFlags::None);
        assert!(app.is_ok());
        let app = app.unwrap();
        assert_eq!(app.get_name(), "Test");
        assert_eq!(app.get_executable(), "test --foo");
    }

    #[test]
    fn test_create_empty_commandline() {
        assert!(create_from_commandline_impl("", None, AppInfoCreateFlags::None).is_err());
    }

    #[test]
    fn test_register_and_get_for_type() {
        clear_all();
        register_app(
            "text/plain",
            SimpleAppInfo {
                name: "Editor".to_string(),
                commandline: "editor".to_string(),
            },
        );
        let apps = get_all_for_type_impl("text/plain");
        assert_eq!(apps.len(), 1);
        assert_eq!(apps[0].get_name(), "Editor");
    }

    #[test]
    fn test_reset_associations() {
        clear_all();
        register_app(
            "text/plain",
            SimpleAppInfo {
                name: "Editor".to_string(),
                commandline: "editor".to_string(),
            },
        );
        assert!(!get_all_for_type_impl("text/plain").is_empty());
        reset_type_associations_impl("text/plain");
        assert!(get_all_for_type_impl("text/plain").is_empty());
    }
}
