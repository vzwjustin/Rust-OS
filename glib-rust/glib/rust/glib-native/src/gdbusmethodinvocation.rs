//! GDBusMethodInvocation matching `gio/gdbusmethodinvocation.h`.
//!
//! Represents an incoming D-Bus method call. Carries sender, object path,
//! interface, method name, parameters, and provides `return_value` /
//! `return_error` for replies.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A D-Bus method invocation (`GDBusMethodInvocation`).
pub struct DBusMethodInvocation {
    sender: String,
    object_path: String,
    interface_name: String,
    method_name: String,
    parameters: Vec<String>,
    reply: Mutex<Option<DBusReply>>,
}

/// The reply to a method invocation.
#[derive(Debug, Clone, PartialEq)]
pub enum DBusReply {
    /// Successful return with parameters.
    Value(Vec<String>),
    /// Error return with error name and message.
    Error(String, String),
}

impl DBusMethodInvocation {
    /// Creates a new method invocation.
    pub fn new(
        sender: &str,
        object_path: &str,
        interface_name: &str,
        method_name: &str,
        parameters: Vec<String>,
    ) -> Self {
        Self {
            sender: sender.to_string(),
            object_path: object_path.to_string(),
            interface_name: interface_name.to_string(),
            method_name: method_name.to_string(),
            parameters,
            reply: Mutex::new(None),
        }
    }

    /// Gets the sender bus name.
    ///
    /// Mirrors `g_dbus_method_invocation_get_sender`.
    pub fn get_sender(&self) -> &str {
        &self.sender
    }

    /// Gets the object path.
    ///
    /// Mirrors `g_dbus_method_invocation_get_object_path`.
    pub fn get_object_path(&self) -> &str {
        &self.object_path
    }

    /// Gets the interface name.
    ///
    /// Mirrors `g_dbus_method_invocation_get_interface_name`.
    pub fn get_interface_name(&self) -> &str {
        &self.interface_name
    }

    /// Gets the method name.
    ///
    /// Mirrors `g_dbus_method_invocation_get_method_name`.
    pub fn get_method_name(&self) -> &str {
        &self.method_name
    }

    /// Gets the parameters.
    ///
    /// Mirrors `g_dbus_method_invocation_get_parameters`.
    pub fn get_parameters(&self) -> &[String] {
        &self.parameters
    }

    /// Returns a value reply.
    ///
    /// Mirrors `g_dbus_method_invocation_return_value`.
    pub fn return_value(&self, parameters: Vec<String>) {
        *self.reply.lock() = Some(DBusReply::Value(parameters));
    }

    /// Returns an error reply.
    ///
    /// Mirrors `g_dbus_method_invocation_return_error_literal`.
    pub fn return_error(&self, error_name: &str, error_message: &str) {
        *self.reply.lock() = Some(DBusReply::Error(
            error_name.to_string(),
            error_message.to_string(),
        ));
    }

    /// Gets the reply, if one has been set.
    pub fn get_reply(&self) -> Option<DBusReply> {
        self.reply.lock().clone()
    }

    /// Returns true if a reply has been set.
    pub fn has_reply(&self) -> bool {
        self.reply.lock().is_some()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_invocation() -> DBusMethodInvocation {
        DBusMethodInvocation::new(
            ":1.42",
            "/org/test/Object",
            "org.test.Interface",
            "TestMethod",
            vec!["param1".to_string(), "param2".to_string()],
        )
    }

    #[test]
    fn test_new() {
        let inv = make_invocation();
        assert_eq!(inv.get_sender(), ":1.42");
        assert_eq!(inv.get_object_path(), "/org/test/Object");
        assert_eq!(inv.get_interface_name(), "org.test.Interface");
        assert_eq!(inv.get_method_name(), "TestMethod");
        assert!(!inv.has_reply());
    }

    #[test]
    fn test_get_parameters() {
        let inv = make_invocation();
        let params = inv.get_parameters();
        assert_eq!(params.len(), 2);
        assert_eq!(params[0], "param1");
        assert_eq!(params[1], "param2");
    }

    #[test]
    fn test_return_value() {
        let inv = make_invocation();
        inv.return_value(vec!["result".to_string()]);
        assert!(inv.has_reply());
        match inv.get_reply().unwrap() {
            DBusReply::Value(params) => assert_eq!(params, vec!["result".to_string()]),
            _ => panic!("expected Value reply"),
        }
    }

    #[test]
    fn test_return_error() {
        let inv = make_invocation();
        inv.return_error("org.test.Error.Failed", "Something went wrong");
        assert!(inv.has_reply());
        match inv.get_reply().unwrap() {
            DBusReply::Error(name, msg) => {
                assert_eq!(name, "org.test.Error.Failed");
                assert_eq!(msg, "Something went wrong");
            }
            _ => panic!("expected Error reply"),
        }
    }

    #[test]
    fn test_no_reply() {
        let inv = make_invocation();
        assert!(!inv.has_reply());
        assert!(inv.get_reply().is_none());
    }

    #[test]
    fn test_empty_parameters() {
        let inv = DBusMethodInvocation::new(":1.1", "/test", "test.iface", "NoArgs", vec![]);
        assert_eq!(inv.get_parameters().len(), 0);
    }

    #[test]
    fn test_return_value_empty() {
        let inv = make_invocation();
        inv.return_value(vec![]);
        match inv.get_reply().unwrap() {
            DBusReply::Value(params) => assert!(params.is_empty()),
            _ => panic!("expected Value reply"),
        }
    }
}
