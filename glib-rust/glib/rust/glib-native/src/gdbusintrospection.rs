//! GIO D-Bus introspection info matching `gio/gdbusintrospection.h` /
//! `gio/gdbusintrospection.c`.
//!
//! Provides the ref-counted info structs that describe a D-Bus interface
//! hierarchy parsed from introspection XML:
//! - `DBusAnnotationInfo` — key/value annotation.
//! - `DBusArgInfo` — method/signal argument.
//! - `DBusMethodInfo` — method (in/out args + annotations).
//! - `DBusSignalInfo` — signal (args + annotations).
//! - `DBusPropertyInfo` — property (signature + access flags).
//! - `DBusInterfaceInfo` — interface (methods + signals + properties).
//! - `DBusNodeInfo` — node (path + interfaces + child nodes).
//! - `DBusPropertyInfoFlags` — none / readable / writable.
//!
//! Plus the lookup helpers (`annotation_info_lookup`,
//! `interface_info_lookup_method` / `_signal` / `_property`,
//! `node_info_lookup_interface`).
//!
//! Ref counting uses `Arc<T>` (simpler and safer than the upstream
//! manual atomic int + malloc/free). XML parsing
//! (`g_dbus_node_info_new_for_xml`) and generation
//! (`g_dbus_interface_info_generate_xml`, `g_dbus_node_info_generate_xml`)
//! plus the per-interface lookup cache (`g_dbus_interface_info_cache_build`
//! / `_release`) are deferred — they need the GMarkup parser and a
//! global cache table respectively.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::sync::Arc;
use alloc::string::String;
use alloc::vec::Vec;

// ───────────────────── GDBusPropertyInfoFlags ─────────────────────────────

/// Access control flags for a D-Bus property (`GDBusPropertyInfoFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct DBusPropertyInfoFlags(pub u32);

impl DBusPropertyInfoFlags {
    /// No flags set (`G_DBUS_PROPERTY_INFO_FLAGS_NONE`).
    pub const NONE: Self = Self(0);
    /// Property is readable (`G_DBUS_PROPERTY_INFO_FLAGS_READABLE`).
    pub const READABLE: Self = Self(1 << 0);
    /// Property is writable (`G_DBUS_PROPERTY_INFO_FLAGS_WRITABLE`).
    pub const WRITABLE: Self = Self(1 << 1);

    /// Returns `true` if `other` is set in `self`.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl core::ops::BitOr for DBusPropertyInfoFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

// ──────────────────────── info structs ────────────────────────────────────

/// Information about a D-Bus annotation (`GDBusAnnotationInfo`).
///
/// Annotations are key/value pairs attachable to any introspection
/// element (arg, method, signal, property, interface, node). They can
/// also be nested.
#[derive(Clone, Debug)]
pub struct DBusAnnotationInfo {
    /// Annotation key, e.g. `"org.freedesktop.DBus.Deprecated"`.
    pub key: String,
    /// Annotation value.
    pub value: String,
    /// Nested annotations.
    pub annotations: Vec<Arc<DBusAnnotationInfo>>,
}

/// Information about a D-Bus argument (`GDBusArgInfo`).
#[derive(Clone, Debug)]
pub struct DBusArgInfo {
    /// Argument name (may be empty if unnamed in the XML).
    pub name: String,
    /// D-Bus type signature (a single complete type).
    pub signature: String,
    /// Annotations on this argument.
    pub annotations: Vec<Arc<DBusAnnotationInfo>>,
}

/// Information about a D-Bus method (`GDBusMethodInfo`).
#[derive(Clone, Debug)]
pub struct DBusMethodInfo {
    /// Method name, e.g. `"RequestName"`.
    pub name: String,
    /// Input arguments (in order).
    pub in_args: Vec<Arc<DBusArgInfo>>,
    /// Output arguments (in order).
    pub out_args: Vec<Arc<DBusArgInfo>>,
    /// Annotations on this method.
    pub annotations: Vec<Arc<DBusAnnotationInfo>>,
}

/// Information about a D-Bus signal (`GDBusSignalInfo`).
#[derive(Clone, Debug)]
pub struct DBusSignalInfo {
    /// Signal name, e.g. `"NameOwnerChanged"`.
    pub name: String,
    /// Signal arguments.
    pub args: Vec<Arc<DBusArgInfo>>,
    /// Annotations on this signal.
    pub annotations: Vec<Arc<DBusAnnotationInfo>>,
}

/// Information about a D-Bus property (`GDBusPropertyInfo`).
#[derive(Clone, Debug)]
pub struct DBusPropertyInfo {
    /// Property name.
    pub name: String,
    /// D-Bus type signature.
    pub signature: String,
    /// Access flags (readable / writable).
    pub flags: DBusPropertyInfoFlags,
    /// Annotations on this property.
    pub annotations: Vec<Arc<DBusAnnotationInfo>>,
}

/// Information about a D-Bus interface (`GDBusInterfaceInfo`).
#[derive(Clone, Debug)]
pub struct DBusInterfaceInfo {
    /// Interface name, e.g. `"org.freedesktop.DBus.Properties"`.
    pub name: String,
    /// Methods exposed by the interface.
    pub methods: Vec<Arc<DBusMethodInfo>>,
    /// Signals emitted by the interface.
    pub signals: Vec<Arc<DBusSignalInfo>>,
    /// Properties exposed by the interface.
    pub properties: Vec<Arc<DBusPropertyInfo>>,
    /// Annotations on this interface.
    pub annotations: Vec<Arc<DBusAnnotationInfo>>,
}

/// Information about a node in a D-Bus object hierarchy
/// (`GDBusNodeInfo`).
#[derive(Clone, Debug)]
pub struct DBusNodeInfo {
    /// Node path (may be relative; `None` if omitted in the XML).
    pub path: Option<String>,
    /// Interfaces implemented by this node.
    pub interfaces: Vec<Arc<DBusInterfaceInfo>>,
    /// Child nodes.
    pub nodes: Vec<Arc<DBusNodeInfo>>,
    /// Annotations on this node.
    pub annotations: Vec<Arc<DBusAnnotationInfo>>,
}

// ──────────────────────── ref / unref ─────────────────────────────────────
//
// Upstream uses `_ref` / `_unref` with an atomic int ref count. With
// `Arc<T>` the equivalent is `Arc::clone` (bumps the strong count) and
// dropping the clone (decrements). We expose `ref_` methods for API
// parity and document that callers should hold the `Arc` to keep the
// info alive.

impl DBusAnnotationInfo {
    /// Bump the ref count (`g_dbus_annotation_info_ref`). Returns a new
    /// `Arc` handle to the same info.
    pub fn ref_(self: &Arc<Self>) -> Arc<Self> {
        Arc::clone(self)
    }
}

impl DBusArgInfo {
    /// Bump the ref count (`g_dbus_arg_info_ref`).
    pub fn ref_(self: &Arc<Self>) -> Arc<Self> {
        Arc::clone(self)
    }
}

impl DBusMethodInfo {
    /// Bump the ref count (`g_dbus_method_info_ref`).
    pub fn ref_(self: &Arc<Self>) -> Arc<Self> {
        Arc::clone(self)
    }
}

impl DBusSignalInfo {
    /// Bump the ref count (`g_dbus_signal_info_ref`).
    pub fn ref_(self: &Arc<Self>) -> Arc<Self> {
        Arc::clone(self)
    }
}

impl DBusPropertyInfo {
    /// Bump the ref count (`g_dbus_property_info_ref`).
    pub fn ref_(self: &Arc<Self>) -> Arc<Self> {
        Arc::clone(self)
    }
}

impl DBusInterfaceInfo {
    /// Bump the ref count (`g_dbus_interface_info_ref`).
    pub fn ref_(self: &Arc<Self>) -> Arc<Self> {
        Arc::clone(self)
    }
}

impl DBusNodeInfo {
    /// Bump the ref count (`g_dbus_node_info_ref`).
    pub fn ref_(self: &Arc<Self>) -> Arc<Self> {
        Arc::clone(self)
    }
}

// ────────────────────────── lookups ───────────────────────────────────────

/// Look up an annotation by key (`g_dbus_annotation_info_lookup`).
///
/// Searches `annotations` linearly for one whose `key` matches `name`.
/// Returns the value of the first match, or `None`.
pub fn dbus_annotation_info_lookup<'a>(
    annotations: &'a [Arc<DBusAnnotationInfo>],
    name: &str,
) -> Option<&'a str> {
    for a in annotations {
        if a.key == name {
            return Some(&a.value);
        }
    }
    None
}

/// Look up a method by name on an interface
/// (`g_dbus_interface_info_lookup_method`).
///
/// Linear search (matching the uncached upstream behaviour). The
/// per-interface lookup cache (`g_dbus_interface_info_cache_build`) is
/// deferred — see the module-level docs.
pub fn dbus_interface_info_lookup_method(
    info: &DBusInterfaceInfo,
    name: &str,
) -> Option<Arc<DBusMethodInfo>> {
    for m in &info.methods {
        if m.name == name {
            return Some(Arc::clone(m));
        }
    }
    None
}

/// Look up a signal by name on an interface
/// (`g_dbus_interface_info_lookup_signal`).
pub fn dbus_interface_info_lookup_signal(
    info: &DBusInterfaceInfo,
    name: &str,
) -> Option<Arc<DBusSignalInfo>> {
    for s in &info.signals {
        if s.name == name {
            return Some(Arc::clone(s));
        }
    }
    None
}

/// Look up a property by name on an interface
/// (`g_dbus_interface_info_lookup_property`).
pub fn dbus_interface_info_lookup_property(
    info: &DBusInterfaceInfo,
    name: &str,
) -> Option<Arc<DBusPropertyInfo>> {
    for p in &info.properties {
        if p.name == name {
            return Some(Arc::clone(p));
        }
    }
    None
}

/// Look up an interface by name on a node
/// (`g_dbus_node_info_lookup_interface`).
pub fn dbus_node_info_lookup_interface(
    info: &DBusNodeInfo,
    name: &str,
) -> Option<Arc<DBusInterfaceInfo>> {
    for i in &info.interfaces {
        if i.name == name {
            return Some(Arc::clone(i));
        }
    }
    None
}

// ───────────────────────────── tests ──────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn anno(key: &str, value: &str) -> Arc<DBusAnnotationInfo> {
        Arc::new(DBusAnnotationInfo {
            key: key.to_owned(),
            value: value.to_owned(),
            annotations: Vec::new(),
        })
    }

    fn arg(name: &str, sig: &str) -> Arc<DBusArgInfo> {
        Arc::new(DBusArgInfo {
            name: name.to_owned(),
            signature: sig.to_owned(),
            annotations: Vec::new(),
        })
    }

    fn method(name: &str, in_args: &[Arc<DBusArgInfo>], out_args: &[Arc<DBusArgInfo>]) -> Arc<DBusMethodInfo> {
        Arc::new(DBusMethodInfo {
            name: name.to_owned(),
            in_args: in_args.to_vec(),
            out_args: out_args.to_vec(),
            annotations: Vec::new(),
        })
    }

    fn signal(name: &str, args: &[Arc<DBusArgInfo>]) -> Arc<DBusSignalInfo> {
        Arc::new(DBusSignalInfo {
            name: name.to_owned(),
            args: args.to_vec(),
            annotations: Vec::new(),
        })
    }

    fn property(name: &str, sig: &str, flags: DBusPropertyInfoFlags) -> Arc<DBusPropertyInfo> {
        Arc::new(DBusPropertyInfo {
            name: name.to_owned(),
            signature: sig.to_owned(),
            flags,
            annotations: Vec::new(),
        })
    }

    fn interface(name: &str, methods: &[Arc<DBusMethodInfo>], signals: &[Arc<DBusSignalInfo>], properties: &[Arc<DBusPropertyInfo>]) -> Arc<DBusInterfaceInfo> {
        Arc::new(DBusInterfaceInfo {
            name: name.to_owned(),
            methods: methods.to_vec(),
            signals: signals.to_vec(),
            properties: properties.to_vec(),
            annotations: Vec::new(),
        })
    }

    fn node(path: Option<&str>, interfaces: &[Arc<DBusInterfaceInfo>], nodes: &[Arc<DBusNodeInfo>]) -> Arc<DBusNodeInfo> {
        Arc::new(DBusNodeInfo {
            path: path.map(|s| s.to_owned()),
            interfaces: interfaces.to_vec(),
            nodes: nodes.to_vec(),
            annotations: Vec::new(),
        })
    }

    #[test]
    fn property_flags_bitor_and_contains() {
        let rw = DBusPropertyInfoFlags::READABLE | DBusPropertyInfoFlags::WRITABLE;
        assert!(rw.contains(DBusPropertyInfoFlags::READABLE));
        assert!(rw.contains(DBusPropertyInfoFlags::WRITABLE));
        assert_eq!(DBusPropertyInfoFlags::NONE.0, 0);
        assert_eq!(DBusPropertyInfoFlags::READABLE.0, 1);
        assert_eq!(DBusPropertyInfoFlags::WRITABLE.0, 2);
    }

    #[test]
    fn annotation_lookup_finds_first_match() {
        let anns = vec![anno("a", "1"), anno("b", "2"), anno("c", "3")];
        assert_eq!(dbus_annotation_info_lookup(&anns, "b"), Some("2"));
        assert_eq!(dbus_annotation_info_lookup(&anns, "missing"), None);
    }

    #[test]
    fn annotation_lookup_empty_returns_none() {
        let anns: Vec<Arc<DBusAnnotationInfo>> = Vec::new();
        assert_eq!(dbus_annotation_info_lookup(&anns, "anything"), None);
    }

    #[test]
    fn interface_lookup_method_finds_match() {
        let m1 = method("Ping", &[arg("in", "s")], &[arg("out", "s")]);
        let m2 = method("Pong", &[], &[arg("out", "u")]);
        let iface = interface("org.test.Foo", &[m1.clone(), m2], &[], &[]);
        assert_eq!(dbus_interface_info_lookup_method(&iface, "Ping").map(|m| m.name.clone()), Some("Ping".to_owned()));
        assert!(dbus_interface_info_lookup_method(&iface, "Missing").is_none());
        // Verify in_args survive the lookup.
        let found = dbus_interface_info_lookup_method(&iface, "Ping").unwrap();
        assert_eq!(found.in_args.len(), 1);
        assert_eq!(found.in_args[0].signature, "s");
    }

    #[test]
    fn interface_lookup_signal_finds_match() {
        let s1 = signal("Changed", &[arg("new_value", "s")]);
        let iface = interface("org.test.Foo", &[], &[s1], &[]);
        assert_eq!(dbus_interface_info_lookup_signal(&iface, "Changed").map(|s| s.name.clone()), Some("Changed".to_owned()));
        assert!(dbus_interface_info_lookup_signal(&iface, "Missing").is_none());
    }

    #[test]
    fn interface_lookup_property_finds_match_and_preserves_flags() {
        let p = property("Version", "s", DBusPropertyInfoFlags::READABLE);
        let iface = interface("org.test.Foo", &[], &[], &[p]);
        let found = dbus_interface_info_lookup_property(&iface, "Version").unwrap();
        assert_eq!(found.signature, "s");
        assert!(found.flags.contains(DBusPropertyInfoFlags::READABLE));
        assert!(!found.flags.contains(DBusPropertyInfoFlags::WRITABLE));
        assert!(dbus_interface_info_lookup_property(&iface, "Missing").is_none());
    }

    #[test]
    fn node_lookup_interface_finds_match() {
        let i1 = interface("org.test.A", &[], &[], &[]);
        let i2 = interface("org.test.B", &[], &[], &[]);
        let n = node(Some("/org/test"), &[i1, i2], &[]);
        assert_eq!(dbus_node_info_lookup_interface(&n, "org.test.B").map(|i| i.name.clone()), Some("org.test.B".to_owned()));
        assert!(dbus_node_info_lookup_interface(&n, "org.test.Missing").is_none());
    }

    #[test]
    fn ref_count_increments_and_decrements() {
        let a = anno("k", "v");
        assert_eq!(Arc::strong_count(&a), 1);
        let a2 = a.ref_();
        assert_eq!(Arc::strong_count(&a), 2);
        assert_eq!(Arc::strong_count(&a2), 2);
        drop(a2);
        assert_eq!(Arc::strong_count(&a), 1);
    }

    #[test]
    fn nested_annotations_round_trip() {
        let inner = anno("inner", "1");
        let mut outer = DBusAnnotationInfo {
            key: "outer".to_owned(),
            value: "2".to_owned(),
            annotations: Vec::new(),
        };
        outer.annotations.push(inner.clone());
        let outer = Arc::new(outer);
        assert_eq!(outer.annotations.len(), 1);
        assert_eq!(dbus_annotation_info_lookup(&outer.annotations, "inner"), Some("1"));
        assert_eq!(outer.key, "outer");
    }

    #[test]
    fn full_hierarchy_construction_and_lookup() {
        // Build a small D-Bus interface hierarchy:
        //   /org/test
        //     org.test.Echo
        //       Echo(in s message, out s reply)
        //       OnEcho(s echo)
        //       Version (readable, signature "s")
        let echo_method = method(
            "Echo",
            &[arg("message", "s")],
            &[arg("reply", "s")],
        );
        let on_echo_signal = signal("OnEcho", &[arg("echo", "s")]);
        let version_prop = property("Version", "s", DBusPropertyInfoFlags::READABLE);
        let iface = interface(
            "org.test.Echo",
            &[echo_method],
            &[on_echo_signal],
            &[version_prop],
        );
        let root = node(Some("/org/test"), &[iface], &[]);

        // Look up the interface.
        let found_iface = dbus_node_info_lookup_interface(&root, "org.test.Echo").unwrap();
        assert_eq!(found_iface.name, "org.test.Echo");

        // Look up the method and verify args.
        let found_method = dbus_interface_info_lookup_method(&found_iface, "Echo").unwrap();
        assert_eq!(found_method.in_args.len(), 1);
        assert_eq!(found_method.in_args[0].name, "message");
        assert_eq!(found_method.in_args[0].signature, "s");
        assert_eq!(found_method.out_args.len(), 1);
        assert_eq!(found_method.out_args[0].name, "reply");

        // Look up the signal.
        let found_signal = dbus_interface_info_lookup_signal(&found_iface, "OnEcho").unwrap();
        assert_eq!(found_signal.args.len(), 1);
        assert_eq!(found_signal.args[0].name, "echo");

        // Look up the property.
        let found_prop = dbus_interface_info_lookup_property(&found_iface, "Version").unwrap();
        assert_eq!(found_prop.signature, "s");
        assert!(found_prop.flags.contains(DBusPropertyInfoFlags::READABLE));
        assert!(!found_prop.flags.contains(DBusPropertyInfoFlags::WRITABLE));
    }

    #[test]
    fn lookup_on_empty_interface_returns_none() {
        let iface = interface("org.test.Empty", &[], &[], &[]);
        assert!(dbus_interface_info_lookup_method(&iface, "x").is_none());
        assert!(dbus_interface_info_lookup_signal(&iface, "x").is_none());
        assert!(dbus_interface_info_lookup_property(&iface, "x").is_none());
    }

    #[test]
    fn node_with_no_path_and_child_nodes() {
        let child_iface = interface("org.test.Child", &[], &[], &[]);
        let child_node = node(Some("/org/test/child"), &[child_iface], &[]);
        let root = node(None, &[], &[child_node]);
        assert!(root.path.is_none());
        assert_eq!(root.nodes.len(), 1);
        assert_eq!(root.nodes[0].path.as_deref(), Some("/org/test/child"));
        // Root has no interfaces.
        assert!(dbus_node_info_lookup_interface(&root, "anything").is_none());
    }
}
