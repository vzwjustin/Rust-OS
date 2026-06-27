//! GDBusObjectManagerServer matching `gio/gdbusobjectmanagerserver.h`.
//!
//! Server-side D-Bus object manager exporting
//! `org.freedesktop.DBus.ObjectManager` with a working `GetManagedObjects`
//! method on the in-process loopback bus.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gdbusconnection::{DBusConnection, MethodCallHandler};
use crate::gdbusintrospection::DBusInterfaceInfo;
use crate::gdbusmessage::{DBusMessage, DBusMessageHeaderField, DBusMessageType};
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::Mutex;

pub const OBJECT_MANAGER_IFACE: &str = "org.freedesktop.DBus.ObjectManager";
pub const MANAGED_OBJECTS_MEMBER: &str = "GetManagedObjects";

/// Managed object entry: path → interface names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedObject {
    pub object_path: String,
    pub interfaces: Vec<String>,
}

/// A D-Bus object manager server (`GDBusObjectManagerServer`).
pub struct DBusObjectManagerServer {
    object_path: Mutex<String>,
    objects: Mutex<BTreeMap<String, Vec<String>>>,
    exported: Mutex<bool>,
    registration_id: Mutex<Option<u64>>,
}

impl DBusObjectManagerServer {
    /// Creates a new object manager at `object_path`.
    pub fn new(object_path: &str) -> Self {
        Self {
            object_path: Mutex::new(object_path.to_string()),
            objects: Mutex::new(BTreeMap::new()),
            exported: Mutex::new(false),
            registration_id: Mutex::new(None),
        }
    }

    /// Returns the manager root path.
    pub fn get_object_path(&self) -> String {
        self.object_path.lock().clone()
    }

    /// Marks the manager exported on `connection_path` (legacy no-op marker).
    pub fn export(&self, connection_path: &str) {
        let _ = connection_path;
        *self.exported.lock() = true;
    }

    /// Registers `GetManagedObjects` on `connection` at this manager's path.
    ///
    /// Returns the registration id from [`DBusConnection::register_object`].
    pub fn export_on_connection(&self, connection: &DBusConnection) -> Result<u64, Error> {
        let path = self.object_path.lock().clone();
        let iface = Arc::new(DBusInterfaceInfo {
            name: OBJECT_MANAGER_IFACE.to_string(),
            methods: Vec::new(),
            signals: Vec::new(),
            properties: Vec::new(),
            annotations: Vec::new(),
        });

        let objects = Arc::new(Mutex::new(BTreeMap::<String, Vec<String>>::new()));
        {
            let guard = self.objects.lock();
            let mut snap = objects.lock();
            snap.extend(guard.iter().map(|(k, v)| (k.clone(), v.clone())));
        }

        let snap = Arc::clone(&objects);
        let handler: MethodCallHandler = Arc::new(move |call: &DBusMessage| {
            if call.get_message_type() != DBusMessageType::MethodCall {
                return Err(Error::new(
                    crate::gioerror::io_error_quark(),
                    crate::gioerror::IOErrorEnum::InvalidArgument.to_code(),
                    "not a method call",
                ));
            }
            let member = call
                .get_header(DBusMessageHeaderField::Member)
                .unwrap_or_default();
            if member != MANAGED_OBJECTS_MEMBER {
                return Err(Error::new(
                    crate::gioerror::io_error_quark(),
                    crate::gioerror::IOErrorEnum::NotSupported.to_code(),
                    format!("unknown method {member}"),
                ));
            }
            let reply = DBusMessage::new_method_reply(call);
            reply.set_body(&encode_managed_objects(&snap.lock()));
            Ok(reply)
        });

        let id = connection.register_object(&path, iface, handler)?;
        *self.exported.lock() = true;
        *self.registration_id.lock() = Some(id);
        Ok(id)
    }

    pub fn unexport(&self) {
        *self.exported.lock() = false;
        *self.registration_id.lock() = None;
    }

    pub fn is_exported(&self) -> bool {
        *self.exported.lock()
    }

    /// Adds a managed object and its interface names.
    pub fn add_object(&self, path: &str, interfaces: Vec<String>) {
        self.objects.lock().insert(path.to_string(), interfaces);
    }

    /// Removes a managed object by path.
    pub fn remove_object(&self, path: &str) -> bool {
        self.objects.lock().remove(path).is_some()
    }

    /// Returns all managed object paths.
    pub fn get_objects(&self) -> Vec<String> {
        self.objects.lock().keys().cloned().collect()
    }

    /// Returns managed objects as structured entries.
    pub fn get_managed_objects(&self) -> Vec<ManagedObject> {
        self.objects
            .lock()
            .iter()
            .map(|(path, ifaces)| ManagedObject {
                object_path: path.clone(),
                interfaces: ifaces.clone(),
            })
            .collect()
    }

    pub fn object_count(&self) -> usize {
        self.objects.lock().len()
    }
}

/// Encodes managed objects as a length-prefixed body for the loopback bus.
///
/// Format: `count:path1:iface_count:iface1,iface2|path2:...`
pub(crate) fn encode_managed_objects(objects: &BTreeMap<String, Vec<String>>) -> String {
    let mut out = format!("{}:", objects.len());
    for (path, ifaces) in objects {
        out.push_str(path);
        out.push(':');
        out.push_str(&ifaces.len().to_string());
        out.push(':');
        for (i, iface) in ifaces.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(iface);
        }
        out.push('|');
    }
    out
}

/// Decodes a body produced by [`encode_managed_objects`].
pub(crate) fn decode_managed_objects(body: &str) -> BTreeMap<String, Vec<String>> {
    let mut out = BTreeMap::new();
    let Some((count_str, rest)) = body.split_once(':') else {
        return out;
    };
    let count: usize = count_str.parse().unwrap_or(0);
    if count == 0 || rest.is_empty() {
        return out;
    }
    for entry in rest.split('|').filter(|s| !s.is_empty()) {
        let Some((path, tail)) = entry.split_once(':') else {
            continue;
        };
        let Some((iface_count_str, iface_list)) = tail.split_once(':') else {
            continue;
        };
        let iface_count: usize = iface_count_str.parse().unwrap_or(0);
        let ifaces: Vec<String> = if iface_count == 0 {
            Vec::new()
        } else {
            iface_list.split(',').map(String::from).collect()
        };
        out.insert(path.to_string(), ifaces);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let s = DBusObjectManagerServer::new("/org/test/mgr");
        assert_eq!(s.get_object_path(), "/org/test/mgr");
        assert!(!s.is_exported());
    }

    #[test]
    fn test_encode_decode_managed_objects() {
        let mut map = BTreeMap::new();
        map.insert(
            "/org/a".to_string(),
            vec!["org.test.A".to_string(), "org.test.B".to_string()],
        );
        map.insert("/org/b".to_string(), vec!["org.test.C".to_string()]);
        let body = encode_managed_objects(&map);
        let decoded = decode_managed_objects(&body);
        assert_eq!(decoded, map);
    }

    #[test]
    fn test_export_on_connection_get_managed_objects() {
        let conn = DBusConnection::new_for_address_sync("loopback:").unwrap();
        let mgr = DBusObjectManagerServer::new("/org/test/mgr");
        mgr.add_object("/org/test/obj1", vec!["org.test.iface".to_string()]);
        mgr.export_on_connection(&conn).unwrap();
        assert!(mgr.is_exported());

        let reply = conn
            .call(
                None,
                "/org/test/mgr",
                OBJECT_MANAGER_IFACE,
                MANAGED_OBJECTS_MEMBER,
                None,
                1000,
            )
            .expect("GetManagedObjects");
        let body = reply.get_body().expect("body");
        let decoded = decode_managed_objects(&body);
        assert_eq!(decoded.len(), 1);
        assert!(decoded.contains_key("/org/test/obj1"));
    }
}
