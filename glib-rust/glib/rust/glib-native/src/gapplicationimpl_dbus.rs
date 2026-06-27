//! GApplicationImplDBus matching `gio/gapplicationimpl-dbus.c`.
//!
//! D-Bus backend for `GApplication`. Manages the D-Bus connection,
//! exports the `org.gtk.Application` and `org.freedesktop.Application`
//! interfaces, and handles incoming D-Bus method calls (Activate, Open,
//! CommandLine).
//!
//! In this no_std port we model the D-Bus state with flags and queues.
//! Actual D-Bus transport is deferred.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::{String, ToString};
use spin::Mutex;

/// D-Bus interface XML for `org.gtk.Application`.
pub const ORG_GTK_APPLICATION_XML: &str = "\
<node>\
  <interface name='org.gtk.Application'>\
    <method name='Activate'><arg type='a{sv}' name='platform-data' direction='in'/></method>\
    <method name='Open'><arg type='as' name='uris' direction='in'/>\
    <arg type='s' name='hint' direction='in'/>\
    <arg type='a{sv}' name='platform-data' direction='in'/></method>\
    <method name='CommandLine'><arg type='o' name='path' direction='in'/>\
    <arg type='aay' name='arguments' direction='in'/>\
    <arg type='a{sv}' name='platform-data' direction='in'/>\
    <arg type='i' name='exit-status' direction='out'/></method>\
    <property name='Busy' type='b' access='read'/>\
  </interface>\
</node>";

/// D-Bus interface XML for `org.freedesktop.Application`.
pub const ORG_FREEDESKTOP_APPLICATION_XML: &str = "\
<node>\
  <interface name='org.freedesktop.Application'>\
    <method name='Activate'><arg type='a{sv}' name='platform-data' direction='in'/></method>\
    <method name='Open'><arg type='as' name='uris' direction='in'/>\
    <arg type='a{sv}' name='platform-data' direction='in'/></method>\
    <method name='CommandLine'><arg type='o' name='path' direction='in'/>\
    <arg type='aay' name='arguments' direction='in'/>\
    <arg type='a{sv}' name='platform-data' direction='in'/>\
    <arg type='i' name='exit-status' direction='out'/></method>\
    <method name='AddAction'><arg type='s' name='action' direction='in'/></method>\
    <method name='RemoveAction'><arg type='s' name='action' direction='in'/></method>\
    <method name='ActivateAction'><arg type='s' name='action' direction='in'/>\
    <arg type='av' name='parameter' direction='in'/>\
    <arg type='a{sv}' name='platform-data' direction='in'/></method>\
    <property name='Actions' type='a{sbv}' access='read'/>\
  </interface>\
</node>";

/// The D-Bus application implementation (`GApplicationImplDBus`).
pub struct ApplicationImplDBus {
    app_id: String,
    object_path: String,
    bus_name: String,
    registered: Mutex<bool>,
    busy: Mutex<bool>,
    actions_exported: Mutex<bool>,
}

impl ApplicationImplDBus {
    /// Creates a new D-Bus application implementation.
    ///
    /// Mirrors `g_application_impl_new`.
    pub fn new(app_id: &str) -> Self {
        let object_path = app_id_to_object_path(app_id);
        Self {
            app_id: app_id.to_string(),
            object_path,
            bus_name: app_id.to_string(),
            registered: Mutex::new(false),
            busy: Mutex::new(false),
            actions_exported: Mutex::new(false),
        }
    }

    /// Returns the application ID.
    pub fn app_id(&self) -> &str {
        &self.app_id
    }

    /// Returns the D-Bus object path.
    pub fn object_path(&self) -> &str {
        &self.object_path
    }

    /// Returns the D-Bus bus name.
    pub fn bus_name(&self) -> &str {
        &self.bus_name
    }

    /// Registers the application on the bus.
    ///
    /// Mirrors `g_application_impl_register`.
    pub fn register(&self) -> bool {
        *self.registered.lock() = true;
        true
    }

    /// Returns whether the application is registered.
    pub fn is_registered(&self) -> bool {
        *self.registered.lock()
    }

    /// Sets the busy property.
    ///
    /// Mirrors `g_application_impl_set_busy`.
    pub fn set_busy(&self, busy: bool) {
        *self.busy.lock() = busy;
    }

    /// Returns whether the application is busy.
    pub fn is_busy(&self) -> bool {
        *self.busy.lock()
    }

    /// Exports actions on the bus.
    ///
    /// Mirrors `g_application_impl_publish_actions`.
    pub fn publish_actions(&self) {
        *self.actions_exported.lock() = true;
    }

    /// Returns whether actions are exported.
    pub fn actions_exported(&self) -> bool {
        *self.actions_exported.lock()
    }

    /// Withdraws the application from the bus.
    ///
    /// Mirrors `g_application_impl_flush` / `g_application_impl_destroy`.
    pub fn destroy(&self) {
        *self.registered.lock() = false;
        *self.actions_exported.lock() = false;
    }
}

/// Converts an application ID to a D-Bus object path.
///
/// Replaces `.` with `/` and prepends `/`.
fn app_id_to_object_path(app_id: &str) -> String {
    let mut path = String::from("/");
    path.push_str(&app_id.replace('.', "/"));
    path
}

impl Default for ApplicationImplDBus {
    fn default() -> Self {
        Self::new("org.example.App")
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let impl_ = ApplicationImplDBus::new("org.example.App");
        assert_eq!(impl_.app_id(), "org.example.App");
        assert_eq!(impl_.object_path(), "/org/example/App");
        assert_eq!(impl_.bus_name(), "org.example.App");
    }

    #[test]
    fn test_register() {
        let impl_ = ApplicationImplDBus::new("org.test.App");
        assert!(!impl_.is_registered());
        assert!(impl_.register());
        assert!(impl_.is_registered());
    }

    #[test]
    fn test_busy() {
        let impl_ = ApplicationImplDBus::new("org.test.App");
        assert!(!impl_.is_busy());
        impl_.set_busy(true);
        assert!(impl_.is_busy());
    }

    #[test]
    fn test_publish_actions() {
        let impl_ = ApplicationImplDBus::new("org.test.App");
        assert!(!impl_.actions_exported());
        impl_.publish_actions();
        assert!(impl_.actions_exported());
    }

    #[test]
    fn test_destroy() {
        let impl_ = ApplicationImplDBus::new("org.test.App");
        impl_.register();
        impl_.publish_actions();
        impl_.destroy();
        assert!(!impl_.is_registered());
        assert!(!impl_.actions_exported());
    }

    #[test]
    fn test_object_path_conversion() {
        assert_eq!(app_id_to_object_path("a.b.c"), "/a/b/c");
        assert_eq!(app_id_to_object_path("org.gnome.Test"), "/org/gnome/Test");
    }
}
