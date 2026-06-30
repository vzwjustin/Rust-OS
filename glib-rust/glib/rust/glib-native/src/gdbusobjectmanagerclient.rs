//! GDBusObjectManagerClient matching `gio/gdbusobjectmanagerclient.h`.
//!
//! Client-side D-Bus object manager that populates its proxy registry from a
//! real `GetManagedObjects` call on the loopback bus.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gdbusconnection::DBusConnection;
use crate::gdbusobjectmanagerserver::{
    decode_managed_objects, MANAGED_OBJECTS_MEMBER, OBJECT_MANAGER_IFACE,
};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// A D-Bus object manager client (`GDBusObjectManagerClient`).
pub struct DBusObjectManagerClient {
    bus_name: Mutex<String>,
    object_path: Mutex<String>,
    proxies: Mutex<BTreeMap<String, Vec<String>>>,
}

impl DBusObjectManagerClient {
    /// Creates a new client for the manager at `bus_name` + `object_path`.
    pub fn new(bus_name: &str, object_path: &str) -> Self {
        Self {
            bus_name: Mutex::new(bus_name.to_string()),
            object_path: Mutex::new(object_path.to_string()),
            proxies: Mutex::new(BTreeMap::new()),
        }
    }

    /// Creates a client and synchronously calls `GetManagedObjects`.
    ///
    /// On bare metal only the in-process loopback connection is supported.
    pub fn new_for_bus_sync(
        connection: &DBusConnection,
        _flags: u32,
        bus_name: Option<&str>,
        object_path: &str,
    ) -> Result<Self, Error> {
        let client = Self::new(bus_name.unwrap_or(""), object_path);
        client.refresh_sync(connection)?;
        Ok(client)
    }

    pub fn get_bus_name(&self) -> String {
        self.bus_name.lock().clone()
    }

    pub fn get_object_path(&self) -> String {
        self.object_path.lock().clone()
    }

    /// Calls `GetManagedObjects` and replaces the local proxy cache.
    pub fn refresh_sync(&self, connection: &DBusConnection) -> Result<(), Error> {
        let path = self.object_path.lock().clone();
        let bus = self.bus_name.lock().clone();
        let bus_ref = if bus.is_empty() {
            None
        } else {
            Some(bus.as_str())
        };

        let reply = connection.call(
            bus_ref,
            &path,
            OBJECT_MANAGER_IFACE,
            MANAGED_OBJECTS_MEMBER,
            None,
            1000,
        )?;
        let body = reply.get_body().unwrap_or_default();
        let decoded = decode_managed_objects(&body);
        *self.proxies.lock() = decoded;
        Ok(())
    }

    pub fn add_proxy(&self, object_path: &str, interfaces: Vec<String>) {
        self.proxies
            .lock()
            .insert(object_path.to_string(), interfaces);
    }

    pub fn remove_proxy(&self, object_path: &str) -> bool {
        self.proxies.lock().remove(object_path).is_some()
    }

    pub fn get_proxy(&self, object_path: &str) -> Option<Vec<String>> {
        self.proxies.lock().get(object_path).cloned()
    }

    pub fn get_all_paths(&self) -> Vec<String> {
        self.proxies.lock().keys().cloned().collect()
    }

    pub fn proxy_count(&self) -> usize {
        self.proxies.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gdbusobjectmanagerserver::DBusObjectManagerServer;

    #[test]
    fn test_new_for_bus_sync() {
        let conn = Arc::new(DBusConnection::new_for_address_sync("loopback:").unwrap());
        let server = DBusObjectManagerServer::new("/org/test/mgr");
        server.add_object("/org/test/o1", vec!["org.test.I".to_string()]);
        server.export_on_connection(&conn).unwrap();

        let client =
            DBusObjectManagerClient::new_for_bus_sync(&conn, 0, None, "/org/test/mgr").unwrap();
        assert_eq!(client.proxy_count(), 1);
        assert_eq!(
            client.get_proxy("/org/test/o1"),
            Some(vec!["org.test.I".to_string()])
        );
    }
}
