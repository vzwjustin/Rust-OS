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
//! manual atomic int + malloc/free).
//!
//! XML parsing (`g_dbus_node_info_new_for_xml`) and generation
//! (`g_dbus_interface_info_generate_xml`, `g_dbus_node_info_generate_xml`,
//! `g_dbus_annotation_info_generate_xml`) are driven by the `crate::markup`
//! GMarkup parser. The per-interface lookup cache
//! (`g_dbus_interface_info_cache_build` / `_release` / `_lookup`) is backed
//! by a global `spin::Mutex<BTreeMap<...>>`.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::markup::{
    escape_text, markup_error_quark, Element, MarkupError, MarkupNode, MarkupParseFlags,
    MarkupParser,
};
use crate::prelude::*;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::mutex::Mutex;

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

// ──────────────────────── XML parsing ─────────────────────────────────────

/// Parse D-Bus introspection XML into a `DBusNodeInfo`
/// (`g_dbus_node_info_new_for_xml`).
///
/// The XML must have a single `<node>` root element (the standard
/// introspection format). The root's optional `name` attribute becomes
/// `DBusNodeInfo::path`. `<interface>`, `<method>`, `<signal>`,
/// `<property>`, `<arg>`, and `<annotation>` elements are walked
/// recursively via the `crate::markup` GMarkup tree parser.
///
/// Markup parse errors are mapped to a `glib_native::Error` on the
/// `G_MARKUP_ERROR` domain (`markup_error_quark`) with codes matching
/// upstream `GMarkupError`.
pub fn dbus_node_info_new_for_xml(xml_data: &str) -> Result<DBusNodeInfo, Error> {
    let parser = MarkupParser::new(MarkupParseFlags::DEFAULT_FLAGS);
    let root = parser.parse(xml_data).map_err(markup_err_to_error)?;
    if root.name != "node" {
        return Err(markup_err_to_error(MarkupError::UnknownElement));
    }
    parse_node_element(&root).map_err(markup_err_to_error)
}

/// Map a `MarkupError` to a `glib_native::Error` on the markup domain.
fn markup_err_to_error(err: MarkupError) -> Error {
    // Codes mirror upstream GMarkupError: BadUtf8=0, Empty=1, Parse=2,
    // UnknownElement=3, UnknownAttribute=4, InvalidContent=5,
    // MissingAttribute=6.
    let (code, msg): (i32, &'static str) = match err {
        MarkupError::BadUtf8 => (0, "Bad UTF-8 in D-Bus introspection XML"),
        MarkupError::Empty => (1, "Empty D-Bus introspection XML"),
        MarkupError::Parse => (2, "Failed to parse D-Bus introspection XML"),
        MarkupError::UnknownElement => (3, "Unknown element in D-Bus introspection XML"),
        MarkupError::UnknownAttribute => (4, "Unknown attribute in D-Bus introspection XML"),
        MarkupError::InvalidContent => (5, "Invalid content in D-Bus introspection XML"),
        MarkupError::MissingAttribute => (6, "Missing attribute in D-Bus introspection XML"),
    };
    Error::new(markup_error_quark(), code, msg)
}

/// Look up an attribute value on a markup element by name.
fn xml_attr<'a>(element: &'a Element, name: &str) -> Option<&'a str> {
    element
        .attributes
        .iter()
        .find(|a| a.name == name)
        .map(|a| a.value.as_str())
}

/// Build a `DBusNodeInfo` from a `<node>` markup element.
fn parse_node_element(element: &Element) -> Result<DBusNodeInfo, MarkupError> {
    let path = xml_attr(element, "name").map(|s| s.to_owned());
    let mut interfaces = Vec::new();
    let mut nodes = Vec::new();
    let mut annotations = Vec::new();
    for child in &element.children {
        if let MarkupNode::Element(e) = child {
            match e.name.as_str() {
                "interface" => interfaces.push(Arc::new(parse_interface_element(e)?)),
                "node" => nodes.push(Arc::new(parse_node_element(e)?)),
                "annotation" => annotations.push(parse_annotation_element(e)?),
                _ => return Err(MarkupError::UnknownElement),
            }
        }
    }
    Ok(DBusNodeInfo {
        path,
        interfaces,
        nodes,
        annotations,
    })
}

/// Build a `DBusInterfaceInfo` from an `<interface>` markup element.
fn parse_interface_element(element: &Element) -> Result<DBusInterfaceInfo, MarkupError> {
    let name = xml_attr(element, "name")
        .ok_or(MarkupError::MissingAttribute)?
        .to_owned();
    let mut methods = Vec::new();
    let mut signals = Vec::new();
    let mut properties = Vec::new();
    let mut annotations = Vec::new();
    for child in &element.children {
        if let MarkupNode::Element(e) = child {
            match e.name.as_str() {
                "method" => methods.push(Arc::new(parse_method_element(e)?)),
                "signal" => signals.push(Arc::new(parse_signal_element(e)?)),
                "property" => properties.push(Arc::new(parse_property_element(e)?)),
                "annotation" => annotations.push(parse_annotation_element(e)?),
                _ => return Err(MarkupError::UnknownElement),
            }
        }
    }
    Ok(DBusInterfaceInfo {
        name,
        methods,
        signals,
        properties,
        annotations,
    })
}

/// Build a `DBusMethodInfo` from a `<method>` markup element.
///
/// `<arg>` children are partitioned into `in_args` / `out_args` by their
/// `direction` attribute (defaulting to `"in"` when omitted, matching
/// upstream).
fn parse_method_element(element: &Element) -> Result<DBusMethodInfo, MarkupError> {
    let name = xml_attr(element, "name")
        .ok_or(MarkupError::MissingAttribute)?
        .to_owned();
    let mut in_args = Vec::new();
    let mut out_args = Vec::new();
    let mut annotations = Vec::new();
    for child in &element.children {
        if let MarkupNode::Element(e) = child {
            match e.name.as_str() {
                "arg" => {
                    let arg = parse_arg_element(e)?;
                    match xml_attr(e, "direction") {
                        Some("out") => out_args.push(Arc::new(arg)),
                        _ => in_args.push(Arc::new(arg)),
                    }
                }
                "annotation" => annotations.push(parse_annotation_element(e)?),
                _ => return Err(MarkupError::UnknownElement),
            }
        }
    }
    Ok(DBusMethodInfo {
        name,
        in_args,
        out_args,
        annotations,
    })
}

/// Build a `DBusSignalInfo` from a `<signal>` markup element.
fn parse_signal_element(element: &Element) -> Result<DBusSignalInfo, MarkupError> {
    let name = xml_attr(element, "name")
        .ok_or(MarkupError::MissingAttribute)?
        .to_owned();
    let mut args = Vec::new();
    let mut annotations = Vec::new();
    for child in &element.children {
        if let MarkupNode::Element(e) = child {
            match e.name.as_str() {
                "arg" => args.push(Arc::new(parse_arg_element(e)?)),
                "annotation" => annotations.push(parse_annotation_element(e)?),
                _ => return Err(MarkupError::UnknownElement),
            }
        }
    }
    Ok(DBusSignalInfo {
        name,
        args,
        annotations,
    })
}

/// Build a `DBusPropertyInfo` from a `<property>` markup element.
fn parse_property_element(element: &Element) -> Result<DBusPropertyInfo, MarkupError> {
    let name = xml_attr(element, "name")
        .ok_or(MarkupError::MissingAttribute)?
        .to_owned();
    let signature = xml_attr(element, "type")
        .ok_or(MarkupError::MissingAttribute)?
        .to_owned();
    let access = xml_attr(element, "access").ok_or(MarkupError::MissingAttribute)?;
    let flags = match access {
        "read" => DBusPropertyInfoFlags::READABLE,
        "write" => DBusPropertyInfoFlags::WRITABLE,
        "readwrite" => DBusPropertyInfoFlags::READABLE | DBusPropertyInfoFlags::WRITABLE,
        _ => return Err(MarkupError::InvalidContent),
    };
    let mut annotations = Vec::new();
    for child in &element.children {
        if let MarkupNode::Element(e) = child {
            match e.name.as_str() {
                "annotation" => annotations.push(parse_annotation_element(e)?),
                _ => return Err(MarkupError::UnknownElement),
            }
        }
    }
    Ok(DBusPropertyInfo {
        name,
        signature,
        flags,
        annotations,
    })
}

/// Build a `DBusArgInfo` from an `<arg>` markup element.
fn parse_arg_element(element: &Element) -> Result<DBusArgInfo, MarkupError> {
    let name = xml_attr(element, "name").unwrap_or("").to_owned();
    let signature = xml_attr(element, "type")
        .ok_or(MarkupError::MissingAttribute)?
        .to_owned();
    let mut annotations = Vec::new();
    for child in &element.children {
        if let MarkupNode::Element(e) = child {
            match e.name.as_str() {
                "annotation" => annotations.push(parse_annotation_element(e)?),
                _ => return Err(MarkupError::UnknownElement),
            }
        }
    }
    Ok(DBusArgInfo {
        name,
        signature,
        annotations,
    })
}

/// Build a `DBusAnnotationInfo` from an `<annotation>` markup element.
fn parse_annotation_element(element: &Element) -> Result<Arc<DBusAnnotationInfo>, MarkupError> {
    let key = xml_attr(element, "name")
        .ok_or(MarkupError::MissingAttribute)?
        .to_owned();
    let value = xml_attr(element, "value")
        .ok_or(MarkupError::MissingAttribute)?
        .to_owned();
    let mut annotations = Vec::new();
    for child in &element.children {
        if let MarkupNode::Element(e) = child {
            match e.name.as_str() {
                "annotation" => annotations.push(parse_annotation_element(e)?),
                _ => return Err(MarkupError::UnknownElement),
            }
        }
    }
    Ok(Arc::new(DBusAnnotationInfo {
        key,
        value,
        annotations,
    }))
}

// ──────────────────────── XML generation ──────────────────────────────────

/// Append `indent` spaces to `out`.
fn write_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push(' ');
    }
}

/// Render the `access="..."` string for a property's flags.
fn access_string(flags: DBusPropertyInfoFlags) -> &'static str {
    match (
        flags.contains(DBusPropertyInfoFlags::READABLE),
        flags.contains(DBusPropertyInfoFlags::WRITABLE),
    ) {
        (true, true) => "readwrite",
        (true, false) => "read",
        (false, true) => "write",
        (false, false) => "none",
    }
}

/// Serialize a `DBusAnnotationInfo` to introspection XML
/// (`g_dbus_annotation_info_generate_xml`).
///
/// `indent` is the indentation depth (in spaces) at which the
/// `<annotation>` element starts. Self-closes when there are no nested
/// annotations; otherwise opens, emits nested annotations at `indent + 2`,
/// and closes.
pub fn dbus_annotation_info_generate_xml(info: &DBusAnnotationInfo, indent: usize) -> String {
    let mut out = String::new();
    write_indent(&mut out, indent);
    out.push_str("<annotation name=\"");
    out.push_str(&escape_text(&info.key));
    out.push_str("\" value=\"");
    out.push_str(&escape_text(&info.value));
    out.push('"');
    if info.annotations.is_empty() {
        out.push_str("/>\n");
    } else {
        out.push_str(">\n");
        for a in &info.annotations {
            out.push_str(&dbus_annotation_info_generate_xml(a, indent + 2));
        }
        write_indent(&mut out, indent);
        out.push_str("</annotation>\n");
    }
    out
}

/// Render a single `<arg>` element. `direction` is `Some("in"|"out")` for
/// method args and `None` for signal args (which carry no direction).
fn generate_arg_xml(arg: &DBusArgInfo, indent: usize, direction: Option<&str>) -> String {
    let mut out = String::new();
    write_indent(&mut out, indent);
    out.push_str("<arg type=\"");
    out.push_str(&escape_text(&arg.signature));
    out.push('"');
    if !arg.name.is_empty() {
        out.push_str(" name=\"");
        out.push_str(&escape_text(&arg.name));
        out.push('"');
    }
    if let Some(dir) = direction {
        out.push_str(" direction=\"");
        out.push_str(dir);
        out.push('"');
    }
    out.push_str("/>\n");
    out
}

fn generate_method_xml(method: &DBusMethodInfo, indent: usize) -> String {
    let mut out = String::new();
    write_indent(&mut out, indent);
    out.push_str("<method name=\"");
    out.push_str(&escape_text(&method.name));
    out.push_str("\">\n");
    for a in &method.in_args {
        out.push_str(&generate_arg_xml(a, indent + 2, Some("in")));
    }
    for a in &method.out_args {
        out.push_str(&generate_arg_xml(a, indent + 2, Some("out")));
    }
    for a in &method.annotations {
        out.push_str(&dbus_annotation_info_generate_xml(a, indent + 2));
    }
    write_indent(&mut out, indent);
    out.push_str("</method>\n");
    out
}

fn generate_signal_xml(signal: &DBusSignalInfo, indent: usize) -> String {
    let mut out = String::new();
    write_indent(&mut out, indent);
    out.push_str("<signal name=\"");
    out.push_str(&escape_text(&signal.name));
    out.push_str("\">\n");
    for a in &signal.args {
        out.push_str(&generate_arg_xml(a, indent + 2, None));
    }
    for a in &signal.annotations {
        out.push_str(&dbus_annotation_info_generate_xml(a, indent + 2));
    }
    write_indent(&mut out, indent);
    out.push_str("</signal>\n");
    out
}

fn generate_property_xml(prop: &DBusPropertyInfo, indent: usize) -> String {
    let mut out = String::new();
    write_indent(&mut out, indent);
    out.push_str("<property type=\"");
    out.push_str(&escape_text(&prop.signature));
    out.push_str("\" name=\"");
    out.push_str(&escape_text(&prop.name));
    out.push_str("\" access=\"");
    out.push_str(access_string(prop.flags));
    out.push('"');
    if prop.annotations.is_empty() {
        out.push_str("/>\n");
    } else {
        out.push_str(">\n");
        for a in &prop.annotations {
            out.push_str(&dbus_annotation_info_generate_xml(a, indent + 2));
        }
        write_indent(&mut out, indent);
        out.push_str("</property>\n");
    }
    out
}

/// Serialize a `DBusInterfaceInfo` to introspection XML
/// (`g_dbus_interface_info_generate_xml`).
///
/// Emits `<interface name="...">` followed by its methods, signals,
/// properties, and annotations (each indented at `indent + 2`), then the
/// closing `</interface>`. `indent` is the starting indentation depth.
pub fn dbus_interface_info_generate_xml(info: &DBusInterfaceInfo, indent: usize) -> String {
    let mut out = String::new();
    write_indent(&mut out, indent);
    out.push_str("<interface name=\"");
    out.push_str(&escape_text(&info.name));
    out.push_str("\">\n");
    for m in &info.methods {
        out.push_str(&generate_method_xml(m, indent + 2));
    }
    for s in &info.signals {
        out.push_str(&generate_signal_xml(s, indent + 2));
    }
    for p in &info.properties {
        out.push_str(&generate_property_xml(p, indent + 2));
    }
    for a in &info.annotations {
        out.push_str(&dbus_annotation_info_generate_xml(a, indent + 2));
    }
    write_indent(&mut out, indent);
    out.push_str("</interface>\n");
    out
}

/// Serialize a `DBusNodeInfo` to introspection XML
/// (`g_dbus_node_info_generate_xml`).
///
/// Emits `<node` with an optional `name="..."` (from `info.path`), then
/// its interfaces, child nodes, and annotations (each indented at
/// `indent + 2`), then `</node>`. `indent` is the starting indentation
/// depth.
pub fn dbus_node_info_generate_xml(info: &DBusNodeInfo, indent: usize) -> String {
    let mut out = String::new();
    write_indent(&mut out, indent);
    out.push_str("<node");
    if let Some(path) = &info.path {
        out.push_str(" name=\"");
        out.push_str(&escape_text(path));
        out.push('"');
    }
    out.push_str(">\n");
    for iface in &info.interfaces {
        out.push_str(&dbus_interface_info_generate_xml(iface, indent + 2));
    }
    for child in &info.nodes {
        out.push_str(&dbus_node_info_generate_xml(child, indent + 2));
    }
    for a in &info.annotations {
        out.push_str(&dbus_annotation_info_generate_xml(a, indent + 2));
    }
    write_indent(&mut out, indent);
    out.push_str("</node>\n");
    out
}

// ──────────────────── interface info cache ────────────────────────────────

/// Global per-interface lookup cache keyed by interface name
/// (`g_dbus_interface_info_cache`).
static INTERFACE_INFO_CACHE: Mutex<BTreeMap<String, Arc<DBusInterfaceInfo>>> =
    Mutex::new(BTreeMap::new());

/// Insert `info` into the per-interface lookup cache keyed by
/// `info.name` (`g_dbus_interface_info_cache_build`).
///
/// Upstream ref-sinks the info and stores it; with `Arc` we simply clone
/// the handle into the map. If an entry for `info.name` is already
/// present, this is a no-op (the existing entry is kept, matching
/// upstream's "don't replace" semantics).
pub fn dbus_interface_info_cache_build(info: &Arc<DBusInterfaceInfo>) {
    let mut cache = INTERFACE_INFO_CACHE.lock();
    cache
        .entry(info.name.clone())
        .or_insert_with(|| Arc::clone(info));
}

/// Remove the interface named `name` from the lookup cache
/// (`g_dbus_interface_info_cache_release`). No-op if not present.
pub fn dbus_interface_info_cache_release(name: &str) {
    let mut cache = INTERFACE_INFO_CACHE.lock();
    cache.remove(name);
}

/// Look up a cached interface by name
/// (`g_dbus_interface_info_cache_lookup`).
///
/// Returns a new `Arc` handle to the cached info, or `None` if no entry
/// exists. Upstream uses this internally; it is exposed here for parity.
pub fn dbus_interface_info_cache_lookup(name: &str) -> Option<Arc<DBusInterfaceInfo>> {
    let cache = INTERFACE_INFO_CACHE.lock();
    cache.get(name).map(Arc::clone)
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

    fn method(
        name: &str,
        in_args: &[Arc<DBusArgInfo>],
        out_args: &[Arc<DBusArgInfo>],
    ) -> Arc<DBusMethodInfo> {
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

    fn interface(
        name: &str,
        methods: &[Arc<DBusMethodInfo>],
        signals: &[Arc<DBusSignalInfo>],
        properties: &[Arc<DBusPropertyInfo>],
    ) -> Arc<DBusInterfaceInfo> {
        Arc::new(DBusInterfaceInfo {
            name: name.to_owned(),
            methods: methods.to_vec(),
            signals: signals.to_vec(),
            properties: properties.to_vec(),
            annotations: Vec::new(),
        })
    }

    fn node(
        path: Option<&str>,
        interfaces: &[Arc<DBusInterfaceInfo>],
        nodes: &[Arc<DBusNodeInfo>],
    ) -> Arc<DBusNodeInfo> {
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
        assert_eq!(
            dbus_interface_info_lookup_method(&iface, "Ping").map(|m| m.name.clone()),
            Some("Ping".to_owned())
        );
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
        assert_eq!(
            dbus_interface_info_lookup_signal(&iface, "Changed").map(|s| s.name.clone()),
            Some("Changed".to_owned())
        );
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
        assert_eq!(
            dbus_node_info_lookup_interface(&n, "org.test.B").map(|i| i.name.clone()),
            Some("org.test.B".to_owned())
        );
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
        assert_eq!(
            dbus_annotation_info_lookup(&outer.annotations, "inner"),
            Some("1")
        );
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
        let echo_method = method("Echo", &[arg("message", "s")], &[arg("reply", "s")]);
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

    // ── new_for_xml ──

    const SAMPLE_XML: &str = concat!(
        "<node name=\"/org/test\">",
        "  <interface name=\"org.test.Echo\">",
        "    <method name=\"Echo\">",
        "      <arg name=\"message\" type=\"s\" direction=\"in\"/>",
        "      <arg name=\"reply\" type=\"s\" direction=\"out\"/>",
        "      <annotation name=\"org.test.Deprecated\" value=\"true\"/>",
        "    </method>",
        "    <signal name=\"OnEcho\">",
        "      <arg name=\"echo\" type=\"s\"/>",
        "    </signal>",
        "    <property name=\"Version\" type=\"s\" access=\"read\"/>",
        "    <annotation name=\"org.test.InterfaceAnno\" value=\"yes\"/>",
        "  </interface>",
        "  <node name=\"child\"/>",
        "</node>"
    );

    #[test]
    fn new_for_xml_parses_full_hierarchy() {
        let root = dbus_node_info_new_for_xml(SAMPLE_XML).expect("parse should succeed");
        assert_eq!(root.path.as_deref(), Some("/org/test"));

        let iface = dbus_node_info_lookup_interface(&root, "org.test.Echo").unwrap();
        assert_eq!(iface.name, "org.test.Echo");
        assert_eq!(iface.annotations.len(), 1);
        assert_eq!(iface.annotations[0].key, "org.test.InterfaceAnno");
        assert_eq!(iface.annotations[0].value, "yes");

        let method = dbus_interface_info_lookup_method(&iface, "Echo").unwrap();
        assert_eq!(method.in_args.len(), 1);
        assert_eq!(method.in_args[0].name, "message");
        assert_eq!(method.in_args[0].signature, "s");
        assert_eq!(method.out_args.len(), 1);
        assert_eq!(method.out_args[0].name, "reply");
        assert_eq!(method.out_args[0].signature, "s");
        assert_eq!(method.annotations.len(), 1);
        assert_eq!(method.annotations[0].key, "org.test.Deprecated");
        assert_eq!(method.annotations[0].value, "true");

        let signal = dbus_interface_info_lookup_signal(&iface, "OnEcho").unwrap();
        assert_eq!(signal.args.len(), 1);
        assert_eq!(signal.args[0].name, "echo");
        assert_eq!(signal.args[0].signature, "s");

        let prop = dbus_interface_info_lookup_property(&iface, "Version").unwrap();
        assert_eq!(prop.signature, "s");
        assert!(prop.flags.contains(DBusPropertyInfoFlags::READABLE));
        assert!(!prop.flags.contains(DBusPropertyInfoFlags::WRITABLE));

        // Nested node.
        assert_eq!(root.nodes.len(), 1);
        assert_eq!(root.nodes[0].path.as_deref(), Some("child"));
    }

    #[test]
    fn new_for_xml_arg_defaults_to_in_when_direction_omitted() {
        let xml = "<node><interface name=\"org.test.I\">\
                   <method name=\"M\"><arg name=\"a\" type=\"s\"/></method>\
                   </interface></node>";
        let root = dbus_node_info_new_for_xml(xml).unwrap();
        let iface = dbus_node_info_lookup_interface(&root, "org.test.I").unwrap();
        let method = dbus_interface_info_lookup_method(&iface, "M").unwrap();
        assert_eq!(method.in_args.len(), 1);
        assert!(method.out_args.is_empty());
    }

    #[test]
    fn new_for_xml_property_readwrite() {
        let xml = "<node><interface name=\"org.test.I\">\
                   <property name=\"P\" type=\"u\" access=\"readwrite\"/>\
                   </interface></node>";
        let root = dbus_node_info_new_for_xml(xml).unwrap();
        let iface = dbus_node_info_lookup_interface(&root, "org.test.I").unwrap();
        let prop = dbus_interface_info_lookup_property(&iface, "P").unwrap();
        assert!(prop.flags.contains(DBusPropertyInfoFlags::READABLE));
        assert!(prop.flags.contains(DBusPropertyInfoFlags::WRITABLE));
    }

    #[test]
    fn new_for_xml_root_must_be_node() {
        let xml = "<interface name=\"org.test.I\"/>";
        let err = dbus_node_info_new_for_xml(xml).unwrap_err();
        assert_eq!(err.code(), 3); // UnknownElement
        assert_eq!(err.domain(), markup_error_quark());
    }

    #[test]
    fn new_for_xml_missing_interface_name_is_error() {
        let xml = "<node><interface/></node>";
        let err = dbus_node_info_new_for_xml(xml).unwrap_err();
        assert_eq!(err.code(), 6); // MissingAttribute
    }

    #[test]
    fn new_for_xml_empty_input_is_error() {
        let err = dbus_node_info_new_for_xml("").unwrap_err();
        assert_eq!(err.code(), 1); // Empty
    }

    // ── generate_xml ──

    #[test]
    fn generate_xml_emits_key_substrings() {
        let root = dbus_node_info_new_for_xml(SAMPLE_XML).unwrap();
        let xml = dbus_node_info_generate_xml(&root, 0);
        assert!(xml.contains("<node name=\"/org/test\">"));
        assert!(xml.contains("<interface name=\"org.test.Echo\">"));
        assert!(xml.contains("<method name=\"Echo\">"));
        assert!(xml.contains("<arg type=\"s\" name=\"message\" direction=\"in\"/>"));
        assert!(xml.contains("<arg type=\"s\" name=\"reply\" direction=\"out\"/>"));
        assert!(xml.contains("<signal name=\"OnEcho\">"));
        assert!(xml.contains("<arg type=\"s\" name=\"echo\"/>"));
        assert!(xml.contains("<property type=\"s\" name=\"Version\" access=\"read\"/>"));
        assert!(xml.contains("<annotation name=\"org.test.Deprecated\" value=\"true\"/>"));
        assert!(xml.contains("</interface>"));
        assert!(xml.contains("</node>"));
    }

    #[test]
    fn generate_xml_round_trips_parse_generate_parse() {
        let first = dbus_node_info_new_for_xml(SAMPLE_XML).unwrap();
        let xml = dbus_node_info_generate_xml(&first, 0);
        let second = dbus_node_info_new_for_xml(&xml).unwrap();

        // Structural comparison via re-generated XML equality.
        let xml2 = dbus_node_info_generate_xml(&second, 0);
        assert_eq!(xml, xml2);

        // Spot-check key fields on the re-parsed tree.
        assert_eq!(second.path.as_deref(), Some("/org/test"));
        let iface = dbus_node_info_lookup_interface(&second, "org.test.Echo").unwrap();
        let method = dbus_interface_info_lookup_method(&iface, "Echo").unwrap();
        assert_eq!(method.in_args[0].name, "message");
        assert_eq!(method.out_args[0].name, "reply");
        assert_eq!(method.annotations[0].key, "org.test.Deprecated");
        let signal = dbus_interface_info_lookup_signal(&iface, "OnEcho").unwrap();
        assert_eq!(signal.args[0].name, "echo");
        let prop = dbus_interface_info_lookup_property(&iface, "Version").unwrap();
        assert!(prop.flags.contains(DBusPropertyInfoFlags::READABLE));
        assert_eq!(second.nodes.len(), 1);
        assert_eq!(second.nodes[0].path.as_deref(), Some("child"));
    }

    #[test]
    fn generate_xml_indents_children() {
        let iface = interface("org.test.I", &[], &[], &[]);
        let root = node(Some("/p"), &[iface], &[]);
        let xml = dbus_node_info_generate_xml(&root, 0);
        // Root at column 0, interface indented two spaces.
        assert!(xml.contains("<node name=\"/p\">\n"));
        assert!(xml.contains("  <interface name=\"org.test.I\">\n"));
        assert!(xml.contains("  </interface>\n"));
        assert!(xml.contains("</node>\n"));
    }

    #[test]
    fn annotation_generate_xml_self_closes_without_nested() {
        let a = anno("k", "v");
        let xml = dbus_annotation_info_generate_xml(&a, 0);
        assert_eq!(xml, "<annotation name=\"k\" value=\"v\"/>\n");
    }

    // ── cache ──

    #[test]
    fn cache_build_lookup_release() {
        let iface = interface("org.test.Cached", &[], &[], &[]);
        dbus_interface_info_cache_build(&iface);
        let looked = dbus_interface_info_cache_lookup("org.test.Cached");
        assert!(looked.is_some());
        assert_eq!(looked.unwrap().name, "org.test.Cached");
        assert!(dbus_interface_info_cache_lookup("org.test.Missing").is_none());

        dbus_interface_info_cache_release("org.test.Cached");
        assert!(dbus_interface_info_cache_lookup("org.test.Cached").is_none());
    }

    #[test]
    fn cache_duplicate_build_is_noop() {
        let iface1 = interface("org.test.Dup", &[], &[], &[]);
        dbus_interface_info_cache_build(&iface1);
        let first = dbus_interface_info_cache_lookup("org.test.Dup").unwrap();

        // A second build with a distinct allocation of the same name must
        // not replace the cached entry.
        let iface2 = interface("org.test.Dup", &[], &[], &[]);
        dbus_interface_info_cache_build(&iface2);
        let second = dbus_interface_info_cache_lookup("org.test.Dup").unwrap();
        assert!(Arc::ptr_eq(&first, &second));

        dbus_interface_info_cache_release("org.test.Dup");
    }

    #[test]
    fn cache_release_missing_is_noop() {
        dbus_interface_info_cache_release("org.test.NeverBuilt");
    }
}
