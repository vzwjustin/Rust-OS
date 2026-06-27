//! `GAppInfoMonitor` — monitors the list of installed applications.
//!
//! Matches `gio/gappinfomonitor.h`. In GIO the monitor emits a "changed"
//! signal whenever the set of installed applications changes. This no_std
//! port replaces the signal with an atomic change counter that callers can
//! poll; `register_app`, `unregister_app`, and `remove_app` each increment
//! it so callers can detect that a refresh is needed.
//!
//! Fully `no_std` compatible — uses `alloc` and `spin::Mutex`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

// ---------------------------------------------------------------------------
// AppEntry
// ---------------------------------------------------------------------------

/// Describes a single application known to the monitor.
///
/// Mirrors the per-application data that `g_app_info_get_*` would return.
#[derive(Debug, Clone)]
pub struct AppEntry {
    /// Application desktop ID (e.g. `"firefox.desktop"`).
    pub id: String,
    /// Human-readable application name.
    pub name: String,
    /// Command used to launch the application.
    pub exec: String,
    /// Whether the application is currently considered installed.
    ///
    /// `unregister_app` sets this to `false` but keeps the entry so that
    /// callers that cached the entry can detect the transition. `remove_app`
    /// drops the entry entirely.
    pub installed: bool,
}

// ---------------------------------------------------------------------------
// AppInfoMonitor
// ---------------------------------------------------------------------------

/// Monitors the list of installed applications (`GAppInfoMonitor`).
///
/// # Thread safety
///
/// All fields are guarded by `spin::Mutex` so the monitor can be shared
/// across threads (or kernel tasks) without `std::sync`.
pub struct AppInfoMonitor {
    apps: Mutex<Vec<AppEntry>>,
    change_count: Mutex<u32>,
}

impl AppInfoMonitor {
    /// Creates an empty monitor with no registered applications.
    pub fn new() -> Self {
        Self {
            apps: Mutex::new(Vec::new()),
            change_count: Mutex::new(0),
        }
    }

    // -----------------------------------------------------------------------
    // Mutation
    // -----------------------------------------------------------------------

    /// Registers an application, marking it as installed.
    ///
    /// If an entry with the same `id` already exists it is updated in-place
    /// (name, exec, and installed flag). Either way the change counter is
    /// incremented.
    pub fn register_app(&self, id: &str, name: &str, exec: &str) {
        let mut apps = self.apps.lock();
        if let Some(entry) = apps.iter_mut().find(|e| e.id == id) {
            entry.name = name.to_string();
            entry.exec = exec.to_string();
            entry.installed = true;
        } else {
            apps.push(AppEntry {
                id: id.to_string(),
                name: name.to_string(),
                exec: exec.to_string(),
                installed: true,
            });
        }
        drop(apps);
        *self.change_count.lock() += 1;
    }

    /// Marks an application as uninstalled (soft-remove).
    ///
    /// The entry is kept with `installed = false` so callers that hold a
    /// reference to an `AppEntry` can detect the transition. Returns `true`
    /// if an entry with `id` was found (regardless of its prior state).
    pub fn unregister_app(&self, id: &str) -> bool {
        let mut apps = self.apps.lock();
        let found = if let Some(entry) = apps.iter_mut().find(|e| e.id == id) {
            entry.installed = false;
            true
        } else {
            false
        };
        drop(apps);
        if found {
            *self.change_count.lock() += 1;
        }
        found
    }

    /// Fully removes an application entry from the monitor.
    ///
    /// Unlike `unregister_app` this erases the entry completely. Returns
    /// `true` if an entry was found and removed.
    pub fn remove_app(&self, id: &str) -> bool {
        let mut apps = self.apps.lock();
        let before = apps.len();
        apps.retain(|e| e.id != id);
        let removed = apps.len() < before;
        drop(apps);
        if removed {
            *self.change_count.lock() += 1;
        }
        removed
    }

    // -----------------------------------------------------------------------
    // Queries
    // -----------------------------------------------------------------------

    /// Returns the IDs of all currently installed applications.
    pub fn get_all_apps(&self) -> Vec<String> {
        self.apps
            .lock()
            .iter()
            .filter(|e| e.installed)
            .map(|e| e.id.clone())
            .collect()
    }

    /// Returns `(name, exec)` for the given `id` if it exists and is installed.
    pub fn get_app_info(&self, id: &str) -> Option<(String, String)> {
        self.apps.lock().iter().find_map(|e| {
            if e.id == id && e.installed {
                Some((e.name.clone(), e.exec.clone()))
            } else {
                None
            }
        })
    }

    /// Returns the number of currently installed applications.
    pub fn app_count(&self) -> usize {
        self.apps.lock().iter().filter(|e| e.installed).count()
    }

    /// Returns the total number of changes since the monitor was created (or
    /// since the last `reset_change_count`).
    pub fn change_count(&self) -> u32 {
        *self.change_count.lock()
    }

    /// Resets the change counter to zero.
    pub fn reset_change_count(&self) {
        *self.change_count.lock() = 0;
    }

    /// Returns `true` if an application with the given `id` is registered and
    /// currently installed.
    pub fn is_registered(&self, id: &str) -> bool {
        self.apps.lock().iter().any(|e| e.id == id && e.installed)
    }
}

impl Default for AppInfoMonitor {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn monitor_with_two() -> AppInfoMonitor {
        let m = AppInfoMonitor::new();
        m.register_app("firefox.desktop", "Firefox", "/usr/bin/firefox");
        m.register_app("gedit.desktop", "gedit", "/usr/bin/gedit");
        m
    }

    #[test]
    fn new_monitor_is_empty() {
        let m = AppInfoMonitor::new();
        assert_eq!(m.app_count(), 0);
        assert_eq!(m.change_count(), 0);
        assert!(m.get_all_apps().is_empty());
    }

    #[test]
    fn register_adds_app_and_increments_change_count() {
        let m = AppInfoMonitor::new();
        m.register_app("foo.desktop", "Foo", "/usr/bin/foo");
        assert_eq!(m.app_count(), 1);
        assert_eq!(m.change_count(), 1);
        assert!(m.is_registered("foo.desktop"));
    }

    #[test]
    fn register_twice_updates_in_place() {
        let m = AppInfoMonitor::new();
        m.register_app("foo.desktop", "Foo", "/usr/bin/foo");
        m.register_app("foo.desktop", "Foo v2", "/usr/bin/foo2");
        assert_eq!(m.app_count(), 1, "should not duplicate");
        assert_eq!(m.change_count(), 2, "each register increments");
        let info = m.get_app_info("foo.desktop").expect("should exist");
        assert_eq!(info.0, "Foo v2");
        assert_eq!(info.1, "/usr/bin/foo2");
    }

    #[test]
    fn unregister_marks_as_not_installed() {
        let m = monitor_with_two();
        let found = m.unregister_app("firefox.desktop");
        assert!(found);
        assert!(!m.is_registered("firefox.desktop"));
        assert_eq!(m.app_count(), 1, "only gedit remains installed");
        let ids = m.get_all_apps();
        assert!(!ids.contains(&"firefox.desktop".to_string()));
    }

    #[test]
    fn unregister_nonexistent_returns_false_and_no_change() {
        let m = AppInfoMonitor::new();
        let found = m.unregister_app("nonexistent.desktop");
        assert!(!found);
        assert_eq!(m.change_count(), 0, "no change for missing id");
    }

    #[test]
    fn remove_app_erases_entry() {
        let m = monitor_with_two();
        let initial_count = m.change_count(); // 2
        let removed = m.remove_app("gedit.desktop");
        assert!(removed);
        assert_eq!(m.app_count(), 1);
        assert_eq!(m.change_count(), initial_count + 1);
        assert!(m.get_app_info("gedit.desktop").is_none());
    }

    #[test]
    fn remove_nonexistent_returns_false() {
        let m = monitor_with_two();
        let before = m.change_count();
        assert!(!m.remove_app("missing.desktop"));
        assert_eq!(m.change_count(), before, "counter must not change");
    }

    #[test]
    fn get_all_apps_returns_only_installed() {
        let m = monitor_with_two();
        m.unregister_app("firefox.desktop");
        let ids = m.get_all_apps();
        assert_eq!(ids.len(), 1);
        assert_eq!(ids[0], "gedit.desktop");
    }

    #[test]
    fn reset_change_count_zeroes_counter() {
        let m = monitor_with_two(); // change_count == 2
        assert_eq!(m.change_count(), 2);
        m.reset_change_count();
        assert_eq!(m.change_count(), 0);
        m.register_app("vim.desktop", "Vim", "/usr/bin/vim");
        assert_eq!(m.change_count(), 1);
    }

    #[test]
    fn default_creates_empty_monitor() {
        let m = AppInfoMonitor::default();
        assert_eq!(m.app_count(), 0);
        assert_eq!(m.change_count(), 0);
    }

    #[test]
    fn re_register_after_unregister_restores_installed() {
        let m = AppInfoMonitor::new();
        m.register_app("vim.desktop", "Vim", "/usr/bin/vim");
        m.unregister_app("vim.desktop");
        assert!(!m.is_registered("vim.desktop"));
        m.register_app("vim.desktop", "Vim", "/usr/bin/vim");
        assert!(m.is_registered("vim.desktop"));
        assert_eq!(m.app_count(), 1, "still only one entry");
        // change_count: register + unregister + re-register = 3
        assert_eq!(m.change_count(), 3);
    }
}
