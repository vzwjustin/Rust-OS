//! GKeyfileSettingsBackend matching `gio/gkeyfilesettingsbackend.h` /
//! `gio/gkeyfilesettingsbackend.c`.
//!
//! A settings backend that persists to a key file (INI format). Maps
//! GSettings paths to keyfile groups/keys using a configurable prefix
//! and optional root group.
//!
//! The upstream uses GObject properties for construction (`filename`,
//! `root-path`, `root-group`, `defaults-dir`). We port it as a plain
//! struct with a `new()` constructor that takes those parameters
//! directly. File I/O is modeled with `load_from_data` / `to_data`
//! from the `KeyFile` module; actual disk persistence is deferred to
//! a platform file abstraction.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::gsimplepermission::SimplePermission;
use crate::keyfile::{KeyFile, KeyFileFlags};
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A keyfile-backed settings backend (`GKeyfileSettingsBackend`).
///
/// Stores settings in a `KeyFile` (INI format). All settings keys
/// must fall under a configurable prefix (root path). Keys directly
/// under the root path can optionally be stored in a named root group.
pub struct KeyfileSettingsBackend {
    /// The in-memory keyfile holding user settings.
    keyfile: Mutex<KeyFile>,
    /// Whether the backend is writable (based on directory permissions).
    writable: Mutex<bool>,
    /// The path prefix that all keys must start with (e.g. `/apps/example/`).
    prefix: String,
    /// Length of `prefix` (cached for fast comparison).
    prefix_len: usize,
    /// Optional root group name for keys directly under the prefix.
    root_group: Option<String>,
    /// Length of `root_group` if set.
    root_group_len: usize,
    /// System keyfile for defaults.
    system_keyfile: Mutex<KeyFile>,
    /// Set of locked keys (from system locks file).
    system_locks: Mutex<Vec<String>>,
    /// The filename for the keyfile (for future disk I/O).
    filename: String,
    /// Directory for system defaults and locks.
    defaults_dir: Option<String>,
}

impl KeyfileSettingsBackend {
    /// Creates a new keyfile settings backend.
    ///
    /// Mirrors `g_keyfile_settings_backend_new`.
    ///
    /// # Parameters
    /// - `filename`: The keyfile path (stored for future I/O).
    /// - `root_path`: The path prefix that all keys must fall under.
    ///   Must start and end with `/` and not contain `//`.
    /// - `root_group`: Optional group name for keys directly under
    ///   `root_path`. If `None`, keys directly under root are disallowed.
    pub fn new(filename: &str, root_path: &str, root_group: Option<&str>) -> Self {
        debug_assert!(root_path.starts_with('/'), "root_path must start with '/'");
        debug_assert!(root_path.ends_with('/'), "root_path must end with '/'");
        debug_assert!(!root_path.contains("//"), "root_path must not contain '//'");

        let (root_group_str, root_group_len) = match root_group {
            Some(g) => (Some(g.to_string()), g.len()),
            None => (None, 0),
        };

        Self {
            keyfile: Mutex::new(KeyFile::new()),
            writable: Mutex::new(true),
            prefix: root_path.to_string(),
            prefix_len: root_path.len(),
            root_group: root_group_str,
            root_group_len,
            system_keyfile: Mutex::new(KeyFile::new()),
            system_locks: Mutex::new(Vec::new()),
            filename: filename.to_string(),
            defaults_dir: None,
        }
    }

    /// Creates a backend with default settings (prefix `/`, no root group).
    pub fn new_default() -> Self {
        Self::new("settings.conf", "/", None)
    }

    /// Loads settings from a data string (INI format).
    ///
    /// Mirrors the internal `g_keyfile_settings_backend_keyfile_reload`
    /// using `g_key_file_load_from_data`.
    pub fn load_from_data(&self, data: &str) {
        let mut kf = self.keyfile.lock();
        *kf = KeyFile::new();
        let _ = kf.load_from_data(data, KeyFileFlags::KEEP_COMMENTS);
    }

    /// Serializes the current settings to a string.
    ///
    /// Mirrors `g_key_file_to_data` used in
    /// `g_keyfile_settings_backend_keyfile_write`.
    pub fn to_data(&self) -> String {
        self.keyfile.lock().to_data()
    }

    /// Reads a key's value from the keyfile.
    ///
    /// Mirrors `g_keyfile_settings_backend_read`.
    pub fn read(&self, key: &str) -> Option<String> {
        let (group, name) = self.convert_path(key)?;

        // Check system keyfile first (if key is locked or user value is missing)
        let sys_val = {
            let sys_kf = self.system_keyfile.lock();
            sys_kf.get_value(&group, &name).ok()
        };
        let user_val = {
            let kf = self.keyfile.lock();
            kf.get_value(&group, &name).ok()
        };

        let locks = self.system_locks.lock();
        let is_locked = locks.iter().any(|l| l == key);

        if sys_val.is_some() && (is_locked || user_val.is_none()) {
            return sys_val;
        }

        user_val
    }

    /// Writes a key's value to the keyfile. Returns `true` on success.
    ///
    /// Mirrors `g_keyfile_settings_backend_write`.
    pub fn write(&self, key: &str, value: &str) -> bool {
        if !*self.writable.lock() {
            return false;
        }

        if !self.set_to_keyfile(key, Some(value)) {
            return false;
        }

        true
    }

    /// Resets a key (removes it from the keyfile).
    ///
    /// Mirrors `g_keyfile_settings_backend_reset`.
    pub fn reset(&self, key: &str) {
        let _ = self.set_to_keyfile(key, None);
    }

    /// Returns whether a key is writable.
    ///
    /// Mirrors `g_keyfile_settings_backend_get_writable`.
    pub fn get_writable(&self, name: &str) -> bool {
        *self.writable.lock()
            && !self.system_locks.lock().iter().any(|l| l == name)
            && self.path_is_valid(name)
    }

    /// Returns a permission for this backend (always allowed in this port).
    ///
    /// Mirrors `g_keyfile_settings_backend_get_permission`.
    pub fn get_permission(&self) -> SimplePermission {
        SimplePermission::new(true)
    }

    /// Returns the filename associated with this backend.
    pub fn filename(&self) -> &str {
        &self.filename
    }

    /// Returns the root path prefix.
    pub fn root_path(&self) -> &str {
        &self.prefix
    }

    /// Returns the root group name, if any.
    pub fn root_group(&self) -> Option<&str> {
        self.root_group.as_deref()
    }

    /// Loads system defaults from a data string (INI format).
    pub fn load_system_defaults(&self, data: &str) {
        let mut sys_kf = self.system_keyfile.lock();
        *sys_kf = KeyFile::new();
        let _ = sys_kf.load_from_data(data, KeyFileFlags::NONE);
    }

    /// Loads system locks from a newline-separated list of key paths.
    pub fn load_system_locks(&self, data: &str) {
        let mut locks = self.system_locks.lock();
        locks.clear();
        for line in data.lines() {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('#') {
                locks.push(trimmed.to_string());
            }
        }
    }

    /// Sets the writable flag (e.g. based on directory permissions).
    pub fn set_writable(&self, writable: bool) {
        *self.writable.lock() = writable;
    }

    // ────────────────── Internal helpers ────────────────────────────────

    /// Converts a GSettings path to a (group, key_name) pair in the keyfile.
    ///
    /// Mirrors the `convert_path` function in the C source.
    fn convert_path(&self, key: &str) -> Option<(String, String)> {
        let key_len = key.len();
        if key_len < self.prefix_len || !key.starts_with(&self.prefix) {
            return None;
        }

        let rest = &key[self.prefix_len..];

        // Disallow empty key names
        if rest.is_empty() {
            return None;
        }

        let last_slash = rest.rfind('/');

        // Validate: no empty group or key names
        if let Some(pos) = last_slash {
            if pos == 0 || pos == rest.len() - 1 {
                return None;
            }
        }

        if let Some(ref root_group) = self.root_group {
            // If root_group is set, disallow paths that ghost the root group name
            if let Some(pos) = last_slash {
                let group_part = &rest[..pos];
                if group_part == root_group.as_str() {
                    return None;
                }
            }
        } else {
            // If no root_group, require at least one slash (a sub-path)
            if last_slash.is_none() {
                return None;
            }
        }

        let (group, name) = match last_slash {
            Some(pos) => (rest[..pos].to_string(), rest[pos + 1..].to_string()),
            None => (
                self.root_group.clone().unwrap_or_default(),
                rest.to_string(),
            ),
        };

        if name.is_empty() {
            return None;
        }

        Some((group, name))
    }

    /// Checks whether a path is valid for this backend.
    ///
    /// Mirrors `path_is_valid`.
    fn path_is_valid(&self, path: &str) -> bool {
        self.convert_path(path).is_some()
    }

    /// Sets or removes a key in the keyfile.
    ///
    /// Mirrors `set_to_keyfile`. Returns `false` if the key is locked
    /// or the path is invalid.
    fn set_to_keyfile(&self, key: &str, value: Option<&str>) -> bool {
        {
            let locks = self.system_locks.lock();
            if locks.iter().any(|l| l == key) {
                return false;
            }
        }

        let (group, name) = match self.convert_path(key) {
            Some(pair) => pair,
            None => return false,
        };

        let mut kf = self.keyfile.lock();
        match value {
            Some(v) => {
                kf.set_value(&group, &name, v);
            }
            None => {
                // If name is empty, remove the whole group; otherwise remove key
                if name.is_empty() {
                    let _ = kf.remove_group(&group);
                } else {
                    let _ = kf.remove_key(&group, &name);
                }
            }
        }
        true
    }
}

impl Default for KeyfileSettingsBackend {
    fn default() -> Self {
        Self::new_default()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_backend() -> KeyfileSettingsBackend {
        KeyfileSettingsBackend::new("test.conf", "/apps/test/", Some("toplevel"))
    }

    #[test]
    fn test_new() {
        let b = make_backend();
        assert_eq!(b.root_path(), "/apps/test/");
        assert_eq!(b.root_group(), Some("toplevel"));
        assert_eq!(b.filename(), "test.conf");
    }

    #[test]
    fn test_write_and_read() {
        let b = make_backend();
        assert!(b.write("/apps/test/enabled", "true"));
        assert_eq!(b.read("/apps/test/enabled").unwrap(), "true");
    }

    #[test]
    fn test_write_subpath() {
        let b = make_backend();
        assert!(b.write("/apps/test/profile/default", "myprofile"));
        assert_eq!(b.read("/apps/test/profile/default").unwrap(), "myprofile");
    }

    #[test]
    fn test_read_missing() {
        let b = make_backend();
        assert!(b.read("/apps/test/nonexistent").is_none());
    }

    #[test]
    fn test_reset() {
        let b = make_backend();
        b.write("/apps/test/key1", "val1");
        b.reset("/apps/test/key1");
        assert!(b.read("/apps/test/key1").is_none());
    }

    #[test]
    fn test_get_writable() {
        let b = make_backend();
        assert!(b.get_writable("/apps/test/key"));
        assert!(!b.get_writable("/other/key"));
    }

    #[test]
    fn test_not_writable() {
        let b = make_backend();
        b.set_writable(false);
        assert!(!b.write("/apps/test/key", "val"));
        assert!(!b.get_writable("/apps/test/key"));
    }

    #[test]
    fn test_system_locks() {
        let b = make_backend();
        b.load_system_locks("/apps/test/locked\n/app/other\n");
        assert!(!b.write("/apps/test/locked", "val"));
        assert!(b.write("/apps/test/unlocked", "val"));
    }

    #[test]
    fn test_system_defaults() {
        let b = make_backend();
        b.load_system_defaults("[toplevel]\nfoo=bar\n");
        // System default should be returned when user value is missing
        assert_eq!(b.read("/apps/test/foo").unwrap(), "bar");
    }

    #[test]
    fn test_load_from_data() {
        let b = make_backend();
        b.load_from_data("[toplevel]\nenabled=true\n[profile]\nname=test\n");
        assert_eq!(b.read("/apps/test/enabled").unwrap(), "true");
        assert_eq!(b.read("/apps/test/profile/name").unwrap(), "test");
    }

    #[test]
    fn test_to_data() {
        let b = make_backend();
        b.write("/apps/test/key", "val");
        let data = b.to_data();
        assert!(data.contains("key=val"));
    }

    #[test]
    fn test_invalid_prefix() {
        let b = make_backend();
        // Key outside prefix is invalid
        assert!(!b.write("/other/key", "val"));
        assert!(b.read("/other/key").is_none());
    }

    #[test]
    fn test_no_root_group_requires_subpath() {
        let b = KeyfileSettingsBackend::new("test.conf", "/apps/test/", None);
        // Without root_group, keys directly under root are disallowed
        assert!(!b.write("/apps/test/key", "val"));
        // But subpath keys work
        assert!(b.write("/apps/test/group/key", "val"));
    }
}
