//! GDesktopAppInfo matching `gio/gdesktopappinfo.h`.
//!
//! Desktop application info wrapping [`SimpleAppInfo`] with `.desktop`
//! entry fields (categories, keywords, MIME types, flags).
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gappinfo::{AppInfo, SimpleAppInfo};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// A desktop application info (`GDesktopAppInfo`).
pub struct DesktopAppInfo {
    desktop_id: String,
    inner: SimpleAppInfo,
    categories: Vec<String>,
    keywords: Vec<String>,
    mime_types: Vec<String>,
    no_display: bool,
    startup_notify: bool,
}

impl DesktopAppInfo {
    /// Creates a new desktop app info with the given id, name, and executable.
    ///
    /// Mirrors `g_desktop_app_info_new_from_filename` (simplified).
    pub fn new(desktop_id: &str, name: &str, executable: &str) -> Self {
        Self {
            desktop_id: desktop_id.to_string(),
            inner: SimpleAppInfo::new(desktop_id, name, executable),
            categories: Vec::new(),
            keywords: Vec::new(),
            mime_types: Vec::new(),
            no_display: false,
            startup_notify: false,
        }
    }

    /// Parses a minimal `.desktop` entry from `contents`.
    ///
    /// Recognises `Name=`, `Exec=`, `MimeType=`, `Categories=`, `Keywords=`,
    /// `NoDisplay=`, and `StartupNotify=` keys inside a `[Desktop Entry]`
    /// group. Semicolon-separated lists are split for MIME types, categories,
    /// and keywords.
    ///
    /// Mirrors `g_desktop_app_info_new_from_keyfile` (string-based port).
    pub fn from_key_file(desktop_id: &str, contents: &str) -> Option<Self> {
        let parsed = parse_desktop_entry(contents)?;
        let name = parsed.name?;
        let exec = parsed.exec?;
        let mut info = Self::new(desktop_id, &name, &exec);
        info.categories = parsed.categories;
        info.keywords = parsed.keywords;
        info.mime_types = parsed.mime_types;
        info.no_display = parsed.no_display;
        info.startup_notify = parsed.startup_notify;
        Some(info)
    }

    /// Returns the desktop entry id (filename or basename).
    pub fn get_desktop_id(&self) -> &str {
        &self.desktop_id
    }

    /// Returns the application categories.
    ///
    /// Mirrors `g_desktop_app_info_get_categories`.
    pub fn get_categories(&self) -> Vec<String> {
        self.categories.clone()
    }

    /// Returns the application keywords.
    ///
    /// Mirrors `g_desktop_app_info_get_keywords`.
    pub fn get_keywords(&self) -> Vec<String> {
        self.keywords.clone()
    }

    /// Returns the MIME types this application handles.
    ///
    /// Mirrors `g_desktop_app_info_get_mime_types`.
    pub fn get_mime_types(&self) -> Vec<String> {
        self.mime_types.clone()
    }

    /// Returns whether the entry has `NoDisplay=true`.
    ///
    /// Mirrors `g_desktop_app_info_get_nodisplay`.
    pub fn get_nodisplay(&self) -> bool {
        self.no_display
    }

    /// Returns whether startup notification is enabled.
    ///
    /// Mirrors `g_desktop_app_info_get_startup_notify`.
    pub fn get_startup_notify(&self) -> bool {
        self.startup_notify
    }

    /// Delegates to the inner [`AppInfo::get_id`].
    pub fn get_id(&self) -> String {
        self.inner.get_id()
    }

    /// Delegates to the inner [`AppInfo::get_name`].
    pub fn get_name(&self) -> String {
        self.inner.get_name()
    }

    /// Delegates to the inner [`AppInfo::get_executable`].
    pub fn get_executable(&self) -> String {
        self.inner.get_executable()
    }
}

impl AppInfo for DesktopAppInfo {
    fn get_id(&self) -> String {
        self.inner.get_id()
    }

    fn get_name(&self) -> String {
        self.inner.get_name()
    }

    fn get_description(&self) -> Option<String> {
        self.inner.get_description()
    }

    fn get_executable(&self) -> String {
        self.inner.get_executable()
    }

    fn supports_uris(&self) -> bool {
        self.inner.supports_uris()
    }

    fn supports_files(&self) -> bool {
        self.inner.supports_files()
    }

    fn should_show(&self) -> bool {
        !self.no_display && self.inner.should_show()
    }
}

// ─────────────────────────── parser helpers ───────────────────────────────

struct ParsedDesktop {
    name: Option<String>,
    exec: Option<String>,
    categories: Vec<String>,
    keywords: Vec<String>,
    mime_types: Vec<String>,
    no_display: bool,
    startup_notify: bool,
}

fn parse_semicolon_list(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

fn parse_bool(value: &str) -> bool {
    matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "true" | "1" | "yes"
    )
}

fn parse_desktop_entry(contents: &str) -> Option<ParsedDesktop> {
    let mut in_desktop_entry = false;
    let mut parsed = ParsedDesktop {
        name: None,
        exec: None,
        categories: Vec::new(),
        keywords: Vec::new(),
        mime_types: Vec::new(),
        no_display: false,
        startup_notify: false,
    };

    for line in contents.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            let section = &line[1..line.len() - 1];
            in_desktop_entry = section == "Desktop Entry";
            continue;
        }
        if !in_desktop_entry {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim();
        match key {
            "Name" => parsed.name = Some(value.to_string()),
            "Exec" => parsed.exec = Some(value.to_string()),
            "MimeType" => parsed.mime_types = parse_semicolon_list(value),
            "Categories" => parsed.categories = parse_semicolon_list(value),
            "Keywords" => parsed.keywords = parse_semicolon_list(value),
            "NoDisplay" => parsed.no_display = parse_bool(value),
            "StartupNotify" => parsed.startup_notify = parse_bool(value),
            _ => {}
        }
    }

    if parsed.name.is_some() && parsed.exec.is_some() {
        Some(parsed)
    } else {
        None
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gappinfo::AppInfo;

    const SAMPLE_DESKTOP: &str = r#"
[Desktop Entry]
Type=Application
Name=Test Application
Exec=/usr/bin/test-app %f
MimeType=text/plain;application/pdf
Categories=Utility;System
Keywords=test;demo
NoDisplay=false
StartupNotify=true
"#;

    #[test]
    fn test_new() {
        let info = DesktopAppInfo::new("test.desktop", "Test", "/usr/bin/test");
        assert_eq!(info.get_desktop_id(), "test.desktop");
        assert_eq!(info.get_id(), "test.desktop");
        assert_eq!(info.get_name(), "Test");
        assert_eq!(info.get_executable(), "/usr/bin/test");
        assert!(info.get_categories().is_empty());
        assert!(!info.get_nodisplay());
        assert!(!info.get_startup_notify());
    }

    #[test]
    fn test_from_key_file() {
        let info = DesktopAppInfo::from_key_file("org.test.App.desktop", SAMPLE_DESKTOP).unwrap();
        assert_eq!(info.get_name(), "Test Application");
        assert_eq!(info.get_executable(), "/usr/bin/test-app %f");
        assert_eq!(info.get_mime_types(), vec!["text/plain", "application/pdf"]);
        assert_eq!(info.get_categories(), vec!["Utility", "System"]);
        assert_eq!(info.get_keywords(), vec!["test", "demo"]);
        assert!(!info.get_nodisplay());
        assert!(info.get_startup_notify());
    }

    #[test]
    fn test_from_key_file_missing_required() {
        let bad = "[Desktop Entry]\nType=Application\n";
        assert!(DesktopAppInfo::from_key_file("bad.desktop", bad).is_none());
    }

    #[test]
    fn test_app_info_trait() {
        let info = DesktopAppInfo::from_key_file("app.desktop", SAMPLE_DESKTOP).unwrap();
        assert!(info.should_show());
        assert!(info.supports_files());
    }

    #[test]
    fn test_nodisplay_hides() {
        let desktop = r#"
[Desktop Entry]
Name=Hidden
Exec=hidden
NoDisplay=true
"#;
        let info = DesktopAppInfo::from_key_file("hidden.desktop", desktop).unwrap();
        assert!(info.get_nodisplay());
        assert!(!info.should_show());
    }
}
