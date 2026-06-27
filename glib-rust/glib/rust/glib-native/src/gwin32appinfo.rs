//! gwin32appinfo matching `gio/gwin32appinfo.c`.
//!
//! Windows application info implementation. Manages file associations,
//! URL schemes, application handlers, shell verbs, and UWP app
//! registration via an in-memory registry stub (no real Win32 calls).
//!
//! The C implementation is 6000+ lines. This port covers the key public
//! API: `AppInfo` trait, registry lookups, command-line expansion, and
//! verb selection.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gappinfo::AppInfo;
use crate::prelude::*;
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Classic Win32 app vs UWP (Store) app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Win32AppKind {
    Classic,
    Uwp,
}

/// A shell verb (e.g. `"open"`, `"edit"`, `"print"`).
#[derive(Debug, Clone)]
pub struct ShellVerb {
    pub verb_name: String,
    pub display_name: Option<String>,
    pub command: Option<String>,
    pub executable: Option<String>,
    pub is_uwp: bool,
    pub app_id: Option<String>,
}

/// Handler for a file extension or URL scheme (ProgID / class name).
#[derive(Debug, Clone)]
pub struct AppHandler {
    pub prog_id: String,
    pub verbs: Vec<ShellVerb>,
    pub uwp_aumid: Option<String>,
}

/// File extension association.
#[derive(Debug, Clone)]
pub struct FileExtension {
    pub extension: String,
    pub chosen_handler: Option<AppHandler>,
    pub handlers: Vec<AppHandler>,
}

/// URL scheme association.
#[derive(Debug, Clone)]
pub struct UrlScheme {
    pub scheme: String,
    pub chosen_handler: Option<AppHandler>,
    pub handlers: Vec<AppHandler>,
}

/// Registered application metadata (internal store).
#[derive(Debug, Clone)]
pub struct Application {
    pub id: String,
    pub name: String,
    pub display_name: Option<String>,
    pub description: Option<String>,
    pub executable: Option<String>,
    pub command: Option<String>,
    pub kind: Win32AppKind,
    pub app_user_model_id: Option<String>,
    pub supports_uris: bool,
    pub supports_files: bool,
    pub should_show: bool,
    pub supported_types: Vec<String>,
    pub supported_schemes: Vec<String>,
    pub icon: Option<String>,
}

/// A single Windows `GAppInfo` wrapper implementing [`AppInfo`].
#[derive(Debug, Clone)]
pub struct Win32AppInfo {
    app: Application,
    handler_prog_id: Option<String>,
    verb_name: Option<String>,
}

impl Win32AppInfo {
    /// Creates app info from stored application data.
    pub fn from_application(app: Application) -> Self {
        Self {
            app,
            handler_prog_id: None,
            verb_name: None,
        }
    }

    /// Creates app info bound to a specific handler and verb.
    pub fn from_handler(app: Application, handler: &AppHandler, verb: &ShellVerb) -> Self {
        Self {
            app,
            handler_prog_id: Some(handler.prog_id.clone()),
            verb_name: Some(verb.verb_name.clone()),
        }
    }

    pub fn kind(&self) -> Win32AppKind {
        self.app.kind
    }

    pub fn is_uwp(&self) -> bool {
        self.app.kind == Win32AppKind::Uwp
    }

    pub fn app_user_model_id(&self) -> Option<&str> {
        self.app.app_user_model_id.as_deref()
    }

    pub fn handler_prog_id(&self) -> Option<&str> {
        self.handler_prog_id.as_deref()
    }

    pub fn verb_name(&self) -> Option<&str> {
        self.verb_name.as_deref()
    }

    pub fn get_commandline(&self) -> Option<String> {
        if self.is_uwp() {
            return None;
        }
        self.app.command.clone()
    }

    pub fn get_display_name(&self) -> String {
        self.app
            .display_name
            .clone()
            .unwrap_or_else(|| self.app.name.clone())
    }

    pub fn get_supported_types(&self) -> Vec<String> {
        self.app.supported_types.clone()
    }

    /// Expands a shell command template for the given files/URIs.
    ///
    /// Mirrors Windows `%1`, `%*`, `%u`, `%U` macro expansion used in
    /// `g_win32_app_info_launch_internal`.
    pub fn expand_command_line(template: &str, files: &[&str], uris: &[&str]) -> String {
        let mut out = String::new();
        let mut chars = template.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '%' {
                if let Some(&next) = chars.peek() {
                    if next == '%' {
                        chars.next();
                        out.push('%');
                        continue;
                    }
                    chars.next();
                    if let Some(expanded) = expand_command_macro(next, files, uris) {
                        out.push_str(&expanded);
                    } else {
                        out.push('%');
                        out.push(next);
                    }
                    continue;
                }
            }
            out.push(ch);
        }

        out
    }

    /// Selects the verb to use per Windows shell rules:
    /// 1) `"open"` if present, 2) shell default, 3) first verb.
    pub fn select_verb<'a>(
        verbs: &'a [ShellVerb],
        shell_default: Option<&str>,
    ) -> Option<&'a ShellVerb> {
        if verbs.is_empty() {
            return None;
        }
        if let Some(v) = verbs
            .iter()
            .find(|v| v.verb_name.eq_ignore_ascii_case("open"))
        {
            return Some(v);
        }
        if let Some(default) = shell_default {
            if let Some(v) = verbs
                .iter()
                .find(|v| v.verb_name.eq_ignore_ascii_case(default))
            {
                return Some(v);
            }
        }
        Some(&verbs[0])
    }
}

impl AppInfo for Win32AppInfo {
    fn get_id(&self) -> String {
        self.app.id.clone()
    }

    fn get_name(&self) -> String {
        self.app.name.clone()
    }

    fn get_description(&self) -> Option<String> {
        self.app.description.clone()
    }

    fn get_executable(&self) -> String {
        if self.is_uwp() {
            return String::new();
        }
        self.app.executable.clone().unwrap_or_default()
    }

    fn supports_uris(&self) -> bool {
        self.app.supports_uris
    }

    fn supports_files(&self) -> bool {
        self.app.supports_files
    }

    fn should_show(&self) -> bool {
        self.app.should_show
    }

    fn equal(&self, other: &dyn AppInfo) -> bool {
        self.get_id().eq_ignore_ascii_case(&other.get_id())
    }
}

/// In-memory registry backing file/URL association lookups.
pub struct Win32AppInfoRegistry {
    extensions: Mutex<BTreeMap<String, FileExtension>>,
    url_schemes: Mutex<BTreeMap<String, UrlScheme>>,
    apps_by_id: Mutex<BTreeMap<String, Application>>,
}

impl Win32AppInfoRegistry {
    pub fn new() -> Self {
        Self {
            extensions: Mutex::new(BTreeMap::new()),
            url_schemes: Mutex::new(BTreeMap::new()),
            apps_by_id: Mutex::new(BTreeMap::new()),
        }
    }

    pub fn register_app(&self, app: Application) {
        self.apps_by_id.lock().insert(app.id.clone(), app);
    }

    pub fn register_extension(&self, ext: FileExtension) {
        let key = ext.extension.to_ascii_lowercase();
        self.extensions.lock().insert(key, ext);
    }

    pub fn register_url_scheme(&self, scheme: UrlScheme) {
        let key = scheme.scheme.to_ascii_lowercase();
        self.url_schemes.lock().insert(key, scheme);
    }

    /// Mirrors `g_app_info_get_all_impl`.
    pub fn get_all(&self) -> Vec<Win32AppInfo> {
        self.apps_by_id
            .lock()
            .values()
            .cloned()
            .map(Win32AppInfo::from_application)
            .collect()
    }

    /// Mirrors `g_app_info_get_default_for_type_impl`.
    pub fn get_default_for_type(
        &self,
        content_type: &str,
        must_support_uris: bool,
    ) -> Option<Win32AppInfo> {
        let ext = self
            .extensions
            .lock()
            .get(&content_type.to_ascii_lowercase())?
            .clone();
        self.app_from_extension(&ext, must_support_uris)
    }

    /// Mirrors `g_app_info_get_default_for_uri_scheme_impl`.
    pub fn get_default_for_uri_scheme(&self, uri_scheme: &str) -> Option<Win32AppInfo> {
        let scheme_key = uri_scheme.to_ascii_lowercase();
        if scheme_key == "file" {
            return None;
        }
        let scheme = self.url_schemes.lock().get(&scheme_key)?.clone();
        self.app_from_url_scheme(&scheme)
    }

    /// Mirrors `g_app_info_get_all_for_type_impl`.
    pub fn get_all_for_type(&self, content_type: &str) -> Vec<Win32AppInfo> {
        let ext_key = content_type.to_ascii_lowercase();
        let ext = match self.extensions.lock().get(&ext_key) {
            Some(e) => e.clone(),
            None => return Vec::new(),
        };

        let mut result = Vec::new();
        let mut seen = Vec::new();
        let apps = self.apps_by_id.lock();

        let mut consider = |handler: &AppHandler| {
            for verb in &handler.verbs {
                let Some(app_id) = verb.app_id.as_deref() else {
                    continue;
                };
                let Some(app) = apps.get(app_id) else {
                    continue;
                };
                if seen.iter().any(|id: &String| id == app_id) {
                    continue;
                }
                seen.push(app_id.to_string());
                result.push(Win32AppInfo::from_handler(app.clone(), handler, verb));
            }
        };

        if let Some(handler) = &ext.chosen_handler {
            consider(handler);
        }
        for handler in &ext.handlers {
            consider(handler);
        }

        result
    }

    /// Mirrors `g_app_info_get_fallback_for_type_impl`.
    pub fn get_fallback_for_type(&self, content_type: &str) -> Vec<Win32AppInfo> {
        self.get_all_for_type(content_type)
    }

    /// Mirrors `g_app_info_get_recommended_for_type_impl`.
    pub fn get_recommended_for_type(&self, content_type: &str) -> Vec<Win32AppInfo> {
        self.get_all_for_type(content_type)
    }

    pub fn get_command_for_verb(&self, ext: &str, verb: &str) -> Option<String> {
        let ext_lower = ext.to_ascii_lowercase();
        let extensions = self.extensions.lock();
        let ext_ref = extensions.get(&ext_lower)?;
        for handler in ext_ref.chosen_handler.iter().chain(ext_ref.handlers.iter()) {
            for v in &handler.verbs {
                if v.verb_name.eq_ignore_ascii_case(verb) {
                    return v.command.clone();
                }
            }
        }
        None
    }

    pub fn launch(&self, app_id: &str, files: &[&str]) -> Result<String, String> {
        let app = self
            .apps_by_id
            .lock()
            .get(app_id)
            .cloned()
            .ok_or_else(|| "application not found".to_string())?;

        if !app.supports_files && !files.is_empty() {
            return Err("application does not support files".to_string());
        }

        if app.kind == Win32AppKind::Uwp {
            let aumid = app
                .app_user_model_id
                .clone()
                .ok_or_else(|| "UWP app missing AppUserModelID".to_string())?;
            return Ok(format!("uwp:activate:{aumid}"));
        }

        let template = app
            .command
            .clone()
            .ok_or_else(|| "application has no command line".to_string())?;
        Ok(Win32AppInfo::expand_command_line(&template, files, &[]))
    }
}

impl Default for Win32AppInfoRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Win32AppInfoRegistry {
    fn app_from_extension(
        &self,
        ext: &FileExtension,
        must_support_uris: bool,
    ) -> Option<Win32AppInfo> {
        if let Some(handler) = &ext.chosen_handler {
            if let Some(info) = self.app_from_handler(handler, must_support_uris) {
                return Some(info);
            }
        }
        for handler in &ext.handlers {
            if let Some(info) = self.app_from_handler(handler, must_support_uris) {
                return Some(info);
            }
        }
        None
    }

    fn app_from_url_scheme(&self, scheme: &UrlScheme) -> Option<Win32AppInfo> {
        if let Some(handler) = &scheme.chosen_handler {
            if let Some(info) = self.app_from_handler(handler, false) {
                return Some(info);
            }
        }
        for handler in &scheme.handlers {
            if let Some(info) = self.app_from_handler(handler, false) {
                return Some(info);
            }
        }
        None
    }

    fn app_from_handler(
        &self,
        handler: &AppHandler,
        must_support_uris: bool,
    ) -> Option<Win32AppInfo> {
        let verb = Win32AppInfo::select_verb(&handler.verbs, None)?;
        let app_id = verb.app_id.as_deref()?;
        let app = self.apps_by_id.lock().get(app_id)?.clone();
        if must_support_uris && !app.supports_uris {
            return None;
        }
        Some(Win32AppInfo::from_handler(app, handler, verb))
    }
}

fn expand_command_macro(macro_ch: char, files: &[&str], uris: &[&str]) -> Option<String> {
    match macro_ch {
        '1' | '2' | '3' | '4' | '5' | '6' | '7' | '8' | '9' => {
            let idx = (macro_ch as u8 - b'1') as usize;
            files.get(idx).map(|s| (*s).to_string())
        }
        '*' => {
            if !files.is_empty() {
                Some(files.join(" "))
            } else {
                None
            }
        }
        'u' => uris.first().map(|s| (*s).to_string()),
        'U' => {
            if !uris.is_empty() {
                Some(uris.join(" "))
            } else {
                None
            }
        }
        'l' | 'd' => files.first().map(|s| (*s).to_string()),
        _ => None,
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn classic_app(id: &str, name: &str, cmd: &str, exe: &str) -> Application {
        Application {
            id: id.to_string(),
            name: name.to_string(),
            display_name: None,
            description: None,
            executable: Some(exe.to_string()),
            command: Some(cmd.to_string()),
            kind: Win32AppKind::Classic,
            app_user_model_id: None,
            supports_uris: false,
            supports_files: true,
            should_show: true,
            supported_types: vec![".txt".to_string()],
            supported_schemes: vec![],
            icon: None,
        }
    }

    fn uwp_app(id: &str, name: &str, aumid: &str) -> Application {
        Application {
            id: id.to_string(),
            name: name.to_string(),
            display_name: Some(format!("{name} (Store)")),
            description: Some("UWP application".to_string()),
            executable: None,
            command: None,
            kind: Win32AppKind::Uwp,
            app_user_model_id: Some(aumid.to_string()),
            supports_uris: true,
            supports_files: true,
            should_show: true,
            supported_types: vec![],
            supported_schemes: vec!["ms-windows-store".to_string()],
            icon: None,
        }
    }

    fn handler(prog_id: &str, verb: &str, cmd: &str, app_id: &str) -> AppHandler {
        AppHandler {
            prog_id: prog_id.to_string(),
            verbs: vec![ShellVerb {
                verb_name: verb.to_string(),
                display_name: None,
                command: Some(cmd.to_string()),
                executable: Some(format!("C:\\{app_id}.exe")),
                is_uwp: false,
                app_id: Some(app_id.to_string()),
            }],
            uwp_aumid: None,
        }
    }

    fn uwp_handler(prog_id: &str, app_id: &str, aumid: &str) -> AppHandler {
        AppHandler {
            prog_id: prog_id.to_string(),
            verbs: vec![ShellVerb {
                verb_name: "open".to_string(),
                display_name: None,
                command: None,
                executable: None,
                is_uwp: true,
                app_id: Some(app_id.to_string()),
            }],
            uwp_aumid: Some(aumid.to_string()),
        }
    }

    #[test]
    fn test_app_info_trait() {
        let info = Win32AppInfo::from_application(classic_app(
            "notepad",
            "Notepad",
            "notepad %1",
            "C:\\Windows\\notepad.exe",
        ));
        assert_eq!(info.get_id(), "notepad");
        assert_eq!(info.get_name(), "Notepad");
        assert_eq!(info.get_executable(), "C:\\Windows\\notepad.exe");
        assert!(!info.is_uwp());
        assert!(info.supports_files());
    }

    #[test]
    fn test_uwp_app_info() {
        let info = Win32AppInfo::from_application(uwp_app(
            "store",
            "Store",
            "Microsoft.WindowsStore_8wekyb3d8bbwe!App",
        ));
        assert!(info.is_uwp());
        assert_eq!(info.get_executable(), "");
        assert_eq!(
            info.app_user_model_id(),
            Some("Microsoft.WindowsStore_8wekyb3d8bbwe!App")
        );
        assert!(info.supports_uris());
    }

    #[test]
    fn test_get_default_for_type() {
        let reg = Win32AppInfoRegistry::new();
        reg.register_app(classic_app(
            "notepad",
            "Notepad",
            "notepad %1",
            "notepad.exe",
        ));
        reg.register_extension(FileExtension {
            extension: ".txt".to_string(),
            chosen_handler: Some(handler("txtfile", "open", "notepad %1", "notepad")),
            handlers: vec![],
        });

        let app = reg.get_default_for_type(".txt", false).unwrap();
        assert_eq!(app.get_id(), "notepad");
    }

    #[test]
    fn test_get_default_for_uri_scheme() {
        let reg = Win32AppInfoRegistry::new();
        reg.register_app(uwp_app(
            "store",
            "Store",
            "Microsoft.WindowsStore_8wekyb3d8bbwe!App",
        ));
        reg.register_url_scheme(UrlScheme {
            scheme: "ms-windows-store".to_string(),
            chosen_handler: Some(uwp_handler(
                "WindowsStore",
                "store",
                "Microsoft.WindowsStore_8wekyb3d8bbwe!App",
            )),
            handlers: vec![],
        });

        let app = reg.get_default_for_uri_scheme("ms-windows-store").unwrap();
        assert_eq!(app.get_id(), "store");
        assert!(app.is_uwp());
        assert!(reg.get_default_for_uri_scheme("file").is_none());
    }

    #[test]
    fn test_get_all_and_recommended() {
        let reg = Win32AppInfoRegistry::new();
        reg.register_app(classic_app("a", "A", "a %1", "a.exe"));
        reg.register_app(classic_app("b", "B", "b %1", "b.exe"));
        reg.register_extension(FileExtension {
            extension: ".png".to_string(),
            chosen_handler: Some(handler("pngfile", "open", "a %1", "a")),
            handlers: vec![handler("pngfile2", "open", "b %1", "b")],
        });

        assert_eq!(reg.get_all().len(), 2);
        assert_eq!(reg.get_recommended_for_type(".png").len(), 2);
        assert_eq!(reg.get_fallback_for_type(".png").len(), 2);
    }

    #[test]
    fn test_expand_command_line() {
        let cmd = Win32AppInfo::expand_command_line(
            "app %1 %* %u %U",
            &["a.txt", "b.txt"],
            &["http://x", "http://y"],
        );
        assert!(cmd.contains("a.txt"));
        assert!(cmd.contains("a.txt b.txt"));
        assert!(cmd.contains("http://x"));
        assert!(cmd.contains("http://x http://y"));
    }

    #[test]
    fn test_select_verb_prefers_open() {
        let verbs = vec![
            ShellVerb {
                verb_name: "print".to_string(),
                display_name: None,
                command: None,
                executable: None,
                is_uwp: false,
                app_id: None,
            },
            ShellVerb {
                verb_name: "open".to_string(),
                display_name: None,
                command: None,
                executable: None,
                is_uwp: false,
                app_id: None,
            },
        ];
        assert_eq!(
            Win32AppInfo::select_verb(&verbs, None).unwrap().verb_name,
            "open"
        );
    }

    #[test]
    fn test_launch_uwp() {
        let reg = Win32AppInfoRegistry::new();
        reg.register_app(uwp_app(
            "store",
            "Store",
            "Microsoft.WindowsStore_8wekyb3d8bbwe!App",
        ));
        let result = reg.launch("store", &[]).unwrap();
        assert!(result.starts_with("uwp:activate:"));
    }
}
