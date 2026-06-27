//! GDBusMessage matching `gio/gdbusmessage.h`.
//!
//! Upstream `GDBusMessage` represents a D-Bus message with headers,
//! body, and type information. We port it as a plain Rust struct.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// D-Bus message type (`GDBusMessageType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DBusMessageType {
    Invalid = 0,
    MethodCall = 1,
    MethodReturn = 2,
    Error = 3,
    Signal = 4,
}

/// D-Bus message flags (`GDBusMessageFlags`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DBusMessageFlags {
    None = 0,
    NoReplyExpected = 1,
    NoAutoStart = 2,
    AllowInteractiveAuthorization = 4,
}

/// D-Bus byte order (`GDBusMessageByteOrder`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DBusMessageByteOrder {
    BigEndian = 66,
    LittleEndian = 108,
}

/// D-Bus header field (`GDBusMessageHeaderField`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DBusMessageHeaderField {
    Path = 1,
    Interface = 2,
    Member = 3,
    ErrorName = 4,
    ReplySerial = 5,
    Destination = 6,
    Sender = 7,
    Signature = 8,
    UnixFds = 9,
}

impl DBusMessageHeaderField {
    pub fn to_code(self) -> u8 {
        self as u8
    }
}

/// A D-Bus message (`GDBusMessage`).
pub struct DBusMessage {
    message_type: Mutex<DBusMessageType>,
    flags: Mutex<DBusMessageFlags>,
    serial: Mutex<u32>,
    reply_serial: Mutex<Option<u32>>,
    byte_order: Mutex<DBusMessageByteOrder>,
    headers: Mutex<BTreeMap<u8, String>>,
    body: Mutex<Option<String>>,
    locked: Mutex<bool>,
}

impl DBusMessage {
    /// Creates a new empty message.
    ///
    /// Mirrors `g_dbus_message_new`.
    pub fn new() -> Self {
        Self {
            message_type: Mutex::new(DBusMessageType::Invalid),
            flags: Mutex::new(DBusMessageFlags::None),
            serial: Mutex::new(0),
            reply_serial: Mutex::new(None),
            byte_order: Mutex::new(DBusMessageByteOrder::LittleEndian),
            headers: Mutex::new(BTreeMap::new()),
            body: Mutex::new(None),
            locked: Mutex::new(false),
        }
    }

    /// Creates a new signal message.
    ///
    /// Mirrors `g_dbus_message_new_signal`.
    pub fn new_signal(path: &str, interface: &str, signal: &str) -> Self {
        let msg = Self::new();
        *msg.message_type.lock() = DBusMessageType::Signal;
        msg.set_header(DBusMessageHeaderField::Path, path);
        msg.set_header(DBusMessageHeaderField::Interface, interface);
        msg.set_header(DBusMessageHeaderField::Member, signal);
        msg
    }

    /// Creates a new method call message.
    ///
    /// Mirrors `g_dbus_message_new_method_call`.
    pub fn new_method_call(name: &str, path: &str, interface: &str, method: &str) -> Self {
        let msg = Self::new();
        *msg.message_type.lock() = DBusMessageType::MethodCall;
        if !name.is_empty() {
            msg.set_header(DBusMessageHeaderField::Destination, name);
        }
        msg.set_header(DBusMessageHeaderField::Path, path);
        msg.set_header(DBusMessageHeaderField::Interface, interface);
        msg.set_header(DBusMessageHeaderField::Member, method);
        msg
    }

    /// Creates a new method reply message.
    ///
    /// Mirrors `g_dbus_message_new_method_reply`.
    pub fn new_method_reply(method_call: &DBusMessage) -> Self {
        let msg = Self::new();
        *msg.message_type.lock() = DBusMessageType::MethodReturn;
        msg.set_reply_serial(method_call.get_serial());
        if let Some(sender) = method_call.get_header(DBusMessageHeaderField::Sender) {
            msg.set_header(DBusMessageHeaderField::Destination, &sender);
        }
        msg
    }

    /// Creates a new method error message.
    ///
    /// Mirrors `g_dbus_message_new_method_error_literal`.
    pub fn new_method_error(
        method_call: &DBusMessage,
        error_name: &str,
        error_message: &str,
    ) -> Self {
        let msg = Self::new();
        *msg.message_type.lock() = DBusMessageType::Error;
        msg.set_reply_serial(method_call.get_serial());
        msg.set_header(DBusMessageHeaderField::ErrorName, error_name);
        if let Some(sender) = method_call.get_header(DBusMessageHeaderField::Sender) {
            msg.set_header(DBusMessageHeaderField::Destination, &sender);
        }
        msg.set_body(error_message);
        msg
    }

    pub fn get_message_type(&self) -> DBusMessageType {
        *self.message_type.lock()
    }

    pub fn set_message_type(&self, msg_type: DBusMessageType) {
        if *self.locked.lock() {
            return;
        }
        *self.message_type.lock() = msg_type;
    }

    pub fn get_flags(&self) -> DBusMessageFlags {
        *self.flags.lock()
    }

    pub fn set_flags(&self, flags: DBusMessageFlags) {
        if *self.locked.lock() {
            return;
        }
        *self.flags.lock() = flags;
    }

    pub fn get_serial(&self) -> u32 {
        *self.serial.lock()
    }

    pub fn set_serial(&self, serial: u32) {
        *self.serial.lock() = serial;
    }

    pub fn get_reply_serial(&self) -> Option<u32> {
        *self.reply_serial.lock()
    }

    pub fn set_reply_serial(&self, serial: u32) {
        if *self.locked.lock() {
            return;
        }
        *self.reply_serial.lock() = Some(serial);
    }

    pub fn get_byte_order(&self) -> DBusMessageByteOrder {
        *self.byte_order.lock()
    }

    pub fn set_byte_order(&self, order: DBusMessageByteOrder) {
        if *self.locked.lock() {
            return;
        }
        *self.byte_order.lock() = order;
    }

    pub fn get_header(&self, field: DBusMessageHeaderField) -> Option<String> {
        self.headers.lock().get(&field.to_code()).cloned()
    }

    pub fn set_header(&self, field: DBusMessageHeaderField, value: &str) {
        if *self.locked.lock() {
            return;
        }
        self.headers
            .lock()
            .insert(field.to_code(), value.to_string());
    }

    pub fn get_header_fields(&self) -> Vec<u8> {
        self.headers.lock().keys().copied().collect()
    }

    pub fn get_body(&self) -> Option<String> {
        self.body.lock().clone()
    }

    pub fn set_body(&self, body: &str) {
        if *self.locked.lock() {
            return;
        }
        *self.body.lock() = Some(body.to_string());
    }

    pub fn get_locked(&self) -> bool {
        *self.locked.lock()
    }

    pub fn lock(&self) {
        *self.locked.lock() = true;
    }

    /// Prints the message in a human-readable format.
    ///
    /// Mirrors `g_dbus_message_print`.
    pub fn print(&self, indent: usize) -> String {
        let prefix = " ".repeat(indent);
        let mut result = alloc::string::String::new();
        result.push_str(&format!("{}type: {:?}\n", prefix, self.get_message_type()));
        result.push_str(&format!("{}flags: {:?}\n", prefix, self.get_flags()));
        result.push_str(&format!("{}serial: {}\n", prefix, self.get_serial()));
        if let Some(rs) = self.get_reply_serial() {
            result.push_str(&format!("{}reply_serial: {}\n", prefix, rs));
        }
        for (field, value) in self.headers.lock().iter() {
            result.push_str(&format!("{}header[{}]: {}\n", prefix, field, value));
        }
        if let Some(body) = self.get_body() {
            result.push_str(&format!("{}body: {}\n", prefix, body));
        }
        result
    }
}

impl Default for DBusMessage {
    fn default() -> Self {
        Self::new()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let msg = DBusMessage::new();
        assert_eq!(msg.get_message_type(), DBusMessageType::Invalid);
        assert_eq!(msg.get_flags(), DBusMessageFlags::None);
        assert_eq!(msg.get_serial(), 0);
        assert!(!msg.get_locked());
    }

    #[test]
    fn test_new_signal() {
        let msg = DBusMessage::new_signal("/org/test", "org.test.Interface", "Changed");
        assert_eq!(msg.get_message_type(), DBusMessageType::Signal);
        assert_eq!(
            msg.get_header(DBusMessageHeaderField::Path).unwrap(),
            "/org/test"
        );
        assert_eq!(
            msg.get_header(DBusMessageHeaderField::Interface).unwrap(),
            "org.test.Interface"
        );
        assert_eq!(
            msg.get_header(DBusMessageHeaderField::Member).unwrap(),
            "Changed"
        );
    }

    #[test]
    fn test_new_method_call() {
        let msg = DBusMessage::new_method_call(
            "org.test.Service",
            "/org/test/Object",
            "org.test.Interface",
            "TestMethod",
        );
        assert_eq!(msg.get_message_type(), DBusMessageType::MethodCall);
        assert_eq!(
            msg.get_header(DBusMessageHeaderField::Destination).unwrap(),
            "org.test.Service"
        );
        assert_eq!(
            msg.get_header(DBusMessageHeaderField::Path).unwrap(),
            "/org/test/Object"
        );
        assert_eq!(
            msg.get_header(DBusMessageHeaderField::Member).unwrap(),
            "TestMethod"
        );
    }

    #[test]
    fn test_new_method_reply() {
        let call = DBusMessage::new_method_call(
            "org.test.Service",
            "/org/test",
            "org.test.Iface",
            "Method",
        );
        call.set_serial(42);
        let reply = DBusMessage::new_method_reply(&call);
        assert_eq!(reply.get_message_type(), DBusMessageType::MethodReturn);
        assert_eq!(reply.get_reply_serial(), Some(42));
    }

    #[test]
    fn test_new_method_error() {
        let call = DBusMessage::new_method_call("", "/test", "test.iface", "Method");
        call.set_serial(99);
        let error = DBusMessage::new_method_error(&call, "org.test.Error", "Something went wrong");
        assert_eq!(error.get_message_type(), DBusMessageType::Error);
        assert_eq!(error.get_reply_serial(), Some(99));
        assert_eq!(
            error.get_header(DBusMessageHeaderField::ErrorName).unwrap(),
            "org.test.Error"
        );
        assert_eq!(error.get_body().unwrap(), "Something went wrong");
    }

    #[test]
    fn test_set_and_get_serial() {
        let msg = DBusMessage::new();
        msg.set_serial(12345);
        assert_eq!(msg.get_serial(), 12345);
    }

    #[test]
    fn test_set_and_get_body() {
        let msg = DBusMessage::new();
        msg.set_body("response data");
        assert_eq!(msg.get_body().unwrap(), "response data");
    }

    #[test]
    fn test_lock() {
        let msg = DBusMessage::new();
        msg.set_body("original");
        msg.lock();
        assert!(msg.get_locked());
        msg.set_body("modified");
        assert_eq!(msg.get_body().unwrap(), "original");
    }

    #[test]
    fn test_header_fields() {
        let msg = DBusMessage::new_signal("/path", "iface", "signal");
        let fields = msg.get_header_fields();
        assert!(fields.contains(&DBusMessageHeaderField::Path.to_code()));
        assert!(fields.contains(&DBusMessageHeaderField::Interface.to_code()));
        assert!(fields.contains(&DBusMessageHeaderField::Member.to_code()));
    }

    #[test]
    fn test_print() {
        let msg = DBusMessage::new_signal("/test", "org.test", "Sig");
        msg.set_serial(1);
        let printed = msg.print(0);
        assert!(printed.contains("type: Signal"));
        assert!(printed.contains("serial: 1"));
    }

    #[test]
    fn test_message_type_values() {
        assert_eq!(DBusMessageType::MethodCall as u8, 1);
        assert_eq!(DBusMessageType::MethodReturn as u8, 2);
        assert_eq!(DBusMessageType::Error as u8, 3);
        assert_eq!(DBusMessageType::Signal as u8, 4);
    }

    #[test]
    fn test_byte_order() {
        let msg = DBusMessage::new();
        assert_eq!(msg.get_byte_order(), DBusMessageByteOrder::LittleEndian);
        msg.set_byte_order(DBusMessageByteOrder::BigEndian);
        assert_eq!(msg.get_byte_order(), DBusMessageByteOrder::BigEndian);
    }
}
