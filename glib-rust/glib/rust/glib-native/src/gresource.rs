//! GResource matching `gio/gresource.h`.
//!
//! Upstream `GResource` is a read-only resource bundle for embedding
//! binary data (icons, UI definitions, etc.) in applications.
//! We port it as a struct with a `Mutex`-protected path→data map.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::bytes::Bytes;
use crate::error::Error;
use crate::ginputstream::{InputStream, MemoryInputStream};
use alloc::collections::BTreeMap;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use spin::Mutex;

/// Lookup flags for resource operations (`GResourceLookupFlags`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceLookupFlags {
    None = 0,
}

/// Error codes for resource operations (`GResourceError`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceError {
    NotFound = 0,
    Internal = 1,
}

impl ResourceError {
    pub fn to_code(self) -> i32 {
        self as i32
    }
}

/// A resource bundle (`GResource`).
pub struct Resource {
    entries: Mutex<BTreeMap<String, Vec<u8>>>,
}

impl Resource {
    /// Creates a new empty resource.
    pub fn new() -> Self {
        Self {
            entries: Mutex::new(BTreeMap::new()),
        }
    }

    /// Creates a resource from a map of path→data entries.
    pub fn from_data(entries: BTreeMap<String, Vec<u8>>) -> Self {
        Self {
            entries: Mutex::new(entries),
        }
    }

    /// Adds an entry to the resource.
    pub fn add_entry(&self, path: &str, data: &[u8]) {
        self.entries.lock().insert(path.to_string(), data.to_vec());
    }

    /// Looks up data by path.
    ///
    /// Mirrors `g_resource_lookup_data`.
    pub fn lookup_data(&self, path: &str, _flags: ResourceLookupFlags) -> Result<Bytes, Error> {
        let entries = self.entries.lock();
        match entries.get(path) {
            Some(data) => Ok(Bytes::new(&data[..])),
            None => Err(Error::new(
                resource_error_quark(),
                ResourceError::NotFound.to_code(),
                "Resource not found",
            )),
        }
    }

    /// Opens a stream for a resource path.
    ///
    /// Mirrors `g_resource_open_stream`.
    pub fn open_stream(
        &self,
        path: &str,
        _flags: ResourceLookupFlags,
    ) -> Result<InputStream, Error> {
        let entries = self.entries.lock();
        match entries.get(path) {
            Some(data) => {
                let bytes = Bytes::new(&data[..]);
                Ok(InputStream::new(MemoryInputStream::new_from_bytes(bytes)))
            }
            None => Err(Error::new(
                resource_error_quark(),
                ResourceError::NotFound.to_code(),
                "Resource not found",
            )),
        }
    }

    /// Enumerates children of a path.
    ///
    /// Mirrors `g_resource_enumerate_children`.
    pub fn enumerate_children(
        &self,
        path: &str,
        _flags: ResourceLookupFlags,
    ) -> Result<Vec<String>, Error> {
        let entries = self.entries.lock();
        let prefix = if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{}/", path)
        };
        let mut children: Vec<String> = Vec::new();
        for key in entries.keys() {
            if let Some(rest) = key.strip_prefix(&prefix) {
                if let Some(slash_pos) = rest.find('/') {
                    let child = &rest[..slash_pos + 1];
                    if !children.contains(&child.to_string()) {
                        children.push(child.to_string());
                    }
                } else {
                    children.push(rest.to_string());
                }
            }
        }
        if children.is_empty() && !entries.keys().any(|k| k.starts_with(&prefix)) {
            return Err(Error::new(
                resource_error_quark(),
                ResourceError::NotFound.to_code(),
                "Path not found",
            ));
        }
        Ok(children)
    }

    /// Gets info about a resource path.
    ///
    /// Mirrors `g_resource_get_info`.
    pub fn get_info(&self, path: &str, _flags: ResourceLookupFlags) -> Result<(usize, u32), Error> {
        let entries = self.entries.lock();
        match entries.get(path) {
            Some(data) => Ok((data.len(), 0)),
            None => Err(Error::new(
                resource_error_quark(),
                ResourceError::NotFound.to_code(),
                "Resource not found",
            )),
        }
    }

    /// Checks if a path has children.
    ///
    /// Mirrors `g_resource_has_children`.
    pub fn has_children(&self, path: &str) -> bool {
        let prefix = if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{}/", path)
        };
        self.entries.lock().keys().any(|k| k.starts_with(&prefix))
    }
}

impl Default for Resource {
    fn default() -> Self {
        Self::new()
    }
}

/// Global registry of resources.
static GLOBAL_RESOURCES: Mutex<Vec<Resource>> = Mutex::new(Vec::new());

/// Registers a resource globally.
///
/// Mirrors `g_resources_register`.
pub fn resources_register(resource: Resource) {
    GLOBAL_RESOURCES.lock().push(resource);
}

/// Unregisters all resources.
pub fn resources_unregister_all() {
    GLOBAL_RESOURCES.lock().clear();
}

/// Looks up data in any registered resource.
///
/// Mirrors `g_resources_lookup_data`.
pub fn resources_lookup_data(path: &str, flags: ResourceLookupFlags) -> Result<Bytes, Error> {
    let resources = GLOBAL_RESOURCES.lock();
    for r in resources.iter() {
        if let Ok(data) = r.lookup_data(path, flags) {
            return Ok(data);
        }
    }
    Err(Error::new(
        resource_error_quark(),
        ResourceError::NotFound.to_code(),
        "Resource not found",
    ))
}

/// Gets the error quark for `GResourceError`.
pub fn resource_error_quark() -> u32 {
    0x1000_0003
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let resource = Resource::new();
        assert!(resource
            .lookup_data("/test", ResourceLookupFlags::None)
            .is_err());
    }

    #[test]
    fn test_add_and_lookup() {
        let resource = Resource::new();
        resource.add_entry("/icons/app.png", b"png data");
        let bytes = resource
            .lookup_data("/icons/app.png", ResourceLookupFlags::None)
            .unwrap();
        assert_eq!(bytes.as_ref(), b"png data");
    }

    #[test]
    fn test_lookup_not_found() {
        let resource = Resource::new();
        let result = resource.lookup_data("/missing", ResourceLookupFlags::None);
        assert!(result.is_err());
    }

    #[test]
    fn test_open_stream() {
        let resource = Resource::new();
        resource.add_entry("/data.txt", b"stream data");
        let stream = resource
            .open_stream("/data.txt", ResourceLookupFlags::None)
            .unwrap();
        let mut buf = [0u8; 11];
        let (n, _) = stream.read_all(&mut buf, None).unwrap();
        assert_eq!(n, 11);
        assert_eq!(&buf, b"stream data");
    }

    #[test]
    fn test_enumerate_children() {
        let resource = Resource::new();
        resource.add_entry("/icons/a.png", b"a");
        resource.add_entry("/icons/b.png", b"b");
        resource.add_entry("/config.xml", b"xml");
        let children = resource
            .enumerate_children("/icons", ResourceLookupFlags::None)
            .unwrap();
        assert_eq!(children.len(), 2);
        assert!(children.contains(&"a.png".to_string()));
        assert!(children.contains(&"b.png".to_string()));
    }

    #[test]
    fn test_get_info() {
        let resource = Resource::new();
        resource.add_entry("/test.bin", b"12345");
        let (size, flags) = resource
            .get_info("/test.bin", ResourceLookupFlags::None)
            .unwrap();
        assert_eq!(size, 5);
        assert_eq!(flags, 0);
    }

    #[test]
    fn test_has_children() {
        let resource = Resource::new();
        resource.add_entry("/dir/a.txt", b"a");
        resource.add_entry("/dir/b.txt", b"b");
        assert!(resource.has_children("/dir"));
        assert!(!resource.has_children("/dir/a.txt"));
    }

    #[test]
    fn test_from_data() {
        let mut entries = BTreeMap::new();
        entries.insert("/x".to_string(), b"hello".to_vec());
        let resource = Resource::from_data(entries);
        let bytes = resource
            .lookup_data("/x", ResourceLookupFlags::None)
            .unwrap();
        assert_eq!(bytes.as_ref(), b"hello");
    }

    #[test]
    fn test_global_register_and_lookup() {
        resources_unregister_all();
        let resource = Resource::new();
        resource.add_entry("/global/test.txt", b"global data");
        resources_register(resource);
        let bytes = resources_lookup_data("/global/test.txt", ResourceLookupFlags::None).unwrap();
        assert_eq!(bytes.as_ref(), b"global data");
        resources_unregister_all();
    }
}
