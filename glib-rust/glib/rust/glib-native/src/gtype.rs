//! GObject type system — GType IDs, type registry, fundamental types.
//!
//! Core of the GLib/GObject dynamic type system in `no_std` Rust.

use crate::prelude::*;
use alloc::sync::Arc;
use alloc::collections::BTreeMap;
use spin::rwlock::RwLock;

/// A GType identifier (`GType` in C).
pub type GType = usize;

pub const G_TYPE_INVALID: GType = 0;
pub const G_TYPE_NONE: GType = 1 << 2;
pub const G_TYPE_INTERFACE: GType = 2 << 2;
pub const G_TYPE_CHAR: GType = 3 << 2;
pub const G_TYPE_UCHAR: GType = 4 << 2;
pub const G_TYPE_BOOLEAN: GType = 5 << 2;
pub const G_TYPE_INT: GType = 6 << 2;
pub const G_TYPE_UINT: GType = 7 << 2;
pub const G_TYPE_LONG: GType = 8 << 2;
pub const G_TYPE_ULONG: GType = 9 << 2;
pub const G_TYPE_INT64: GType = 10 << 2;
pub const G_TYPE_UINT64: GType = 11 << 2;
pub const G_TYPE_ENUM: GType = 12 << 2;
pub const G_TYPE_FLAGS: GType = 13 << 2;
pub const G_TYPE_FLOAT: GType = 14 << 2;
pub const G_TYPE_DOUBLE: GType = 15 << 2;
pub const G_TYPE_STRING: GType = 16 << 2;
pub const G_TYPE_POINTER: GType = 17 << 2;
pub const G_TYPE_BOXED: GType = 18 << 2;
pub const G_TYPE_PARAM: GType = 19 << 2;
pub const G_TYPE_OBJECT: GType = 20 << 2;
pub const G_TYPE_VARIANT: GType = 21 << 2;

pub const G_TYPE_FUNDAMENTAL_MAX: GType = 255 << 2;
pub const G_TYPE_FUNDAMENTAL_SHIFT: u32 = 2;

pub const fn g_type_make_fundamental(n: GType) -> GType {
    n << G_TYPE_FUNDAMENTAL_SHIFT
}

/// Fundamental type flags (`GTypeFundamentalFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct GTypeFundamentalFlags(pub u32);

impl GTypeFundamentalFlags {
    pub const NONE: Self = Self(0);
    pub const CLASSED: Self = Self(1 << 0);
    pub const INSTANTIATABLE: Self = Self(1 << 1);
    pub const DERIVABLE: Self = Self(1 << 2);
    pub const DEEP_DERIVABLE: Self = Self(1 << 3);
    pub fn contains(self, other: Self) -> bool { self.0 & other.0 == other.0 }
}

impl core::ops::BitOr for GTypeFundamentalFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}

/// Type flags (`GTypeFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct GTypeFlags(pub u32);

impl GTypeFlags {
    pub const NONE: Self = Self(0);
    pub const ABSTRACT: Self = Self(1 << 4);
    pub const VALUE_ABSTRACT: Self = Self(1 << 5);
    pub const FINAL: Self = Self(1 << 6);
    pub const DEPRECATED: Self = Self(1 << 7);
    pub fn contains(self, other: Self) -> bool { self.0 & other.0 == other.0 }
}

impl core::ops::BitOr for GTypeFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}

/// Internal storage for a GValue's data.
#[derive(Clone, Debug, Default)]
pub struct GValueData {
    pub v_int: i32,
    pub v_uint: u32,
    pub v_long: i64,
    pub v_ulong: u64,
    pub v_float: f32,
    pub v_double: f64,
    pub v_pointer: Option<Arc<dyn core::any::Any + Send + Sync>>,
}

/// Function table for GValue handling (`GTypeValueTable`).
#[derive(Clone)]
pub struct GTypeValueTable {
    pub value_init: fn(&mut GValueData),
    pub value_free: fn(&mut GValueData),
    pub value_copy: fn(&GValueData, &mut GValueData),
    pub collect_format: &'static str,
    pub lcopy_format: &'static str,
}

/// Type initialization info (`GTypeInfo`).
#[derive(Clone)]
pub struct GTypeInfo {
    pub class_size: u16,
    pub instance_size: u16,
    pub class_init: Option<fn(&mut TypeClassData)>,
    pub instance_init: Option<fn(&mut TypeInstanceData)>,
    pub value_table: Option<GTypeValueTable>,
}

impl Default for GTypeInfo {
    fn default() -> Self {
        Self {
            class_size: 0,
            instance_size: 0,
            class_init: None,
            instance_init: None,
            value_table: None,
        }
    }
}

/// Data passed to `class_init` callbacks.
#[derive(Default)]
pub struct TypeClassData {
    pub parent_type: GType,
    pub type_id: GType,
    pub signals: Vec<SignalDef>,
    pub properties: Vec<ParamSpec>,
}

/// Data passed to `instance_init` callbacks.
#[derive(Default)]
pub struct TypeInstanceData {
    pub type_id: GType,
}

/// Signal definition for class init.
#[derive(Clone)]
pub struct SignalDef {
    pub name: String,
    pub return_type: GType,
    pub param_types: Vec<GType>,
}

/// Parameter specification (property).
#[derive(Clone)]
pub struct ParamSpec {
    pub name: String,
    pub nick: String,
    pub blurb: String,
    pub value_type: GType,
    pub flags: ParamFlags,
}

/// Parameter flags (`GParamFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct ParamFlags(pub u32);

impl ParamFlags {
    pub const READABLE: Self = Self(1 << 0);
    pub const WRITABLE: Self = Self(1 << 1);
    pub const READWRITE: Self = Self(Self::READABLE.0 | Self::WRITABLE.0);
    pub const CONSTRUCT: Self = Self(1 << 2);
    pub const CONSTRUCT_ONLY: Self = Self(1 << 3);
    pub const LAX_VALIDATION: Self = Self(1 << 4);
    pub const STATIC_NAME: Self = Self(1 << 5);
    pub const STATIC_NICK: Self = Self(1 << 6);
    pub const STATIC_BLURB: Self = Self(1 << 7);
    pub const EXPLICIT_NOTIFY: Self = Self(1 << 8);
    pub fn contains(self, other: Self) -> bool { self.0 & other.0 == other.0 }
}

impl core::ops::BitOr for ParamFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self { Self(self.0 | rhs.0) }
}

// ── Type node (internal) ─────────────────────────────────────────────

#[derive(Clone)]
struct TypeNode {
    type_id: GType,
    name: String,
    parent: Option<GType>,
    children: Vec<GType>,
    interfaces: Vec<GType>,
    fundamental_flags: GTypeFundamentalFlags,
    flags: GTypeFlags,
    info: GTypeInfo,
    is_classed: bool,
    is_instantiatable: bool,
}

impl TypeNode {
    fn is_fundamental(&self) -> bool {
        self.parent.is_none()
    }
    fn is_a(&self, ancestor: GType) -> bool {
        if self.type_id == ancestor {
            return true;
        }
        if let Some(p) = self.parent {
            return type_is_a(p, ancestor);
        }
        false
    }
}

// ── Global type registry ──────────────────────────────────────────────

static TYPE_REGISTRY: RwLock<Option<TypeRegistry>> = RwLock::new(None);

struct TypeRegistry {
    nodes: BTreeMap<GType, TypeNode>,
    name_to_id: BTreeMap<String, GType>,
    next_id: GType,
    next_fundamental: GType,
    registration_serial: u32,
}

fn ensure_registry() {
    let mut guard = TYPE_REGISTRY.write();
    if guard.is_none() {
        let mut reg = TypeRegistry {
            nodes: BTreeMap::new(),
            name_to_id: BTreeMap::new(),
            next_id: 4096,
            next_fundamental: 40 << 2,
            registration_serial: 0,
        };
        register_fundamental_types(&mut reg);
        *guard = Some(reg);
    }
}

fn register_fundamental_types(reg: &mut TypeRegistry) {
    let fundamentals: &[(GType, &str)] = &[
        (G_TYPE_NONE, "void"),
        (G_TYPE_INTERFACE, "GInterface"),
        (G_TYPE_CHAR, "gchar"),
        (G_TYPE_UCHAR, "guchar"),
        (G_TYPE_BOOLEAN, "gboolean"),
        (G_TYPE_INT, "gint"),
        (G_TYPE_UINT, "guint"),
        (G_TYPE_LONG, "glong"),
        (G_TYPE_ULONG, "gulong"),
        (G_TYPE_INT64, "gint64"),
        (G_TYPE_UINT64, "guint64"),
        (G_TYPE_ENUM, "GEnum"),
        (G_TYPE_FLAGS, "GFlags"),
        (G_TYPE_FLOAT, "gfloat"),
        (G_TYPE_DOUBLE, "gdouble"),
        (G_TYPE_STRING, "gchararray"),
        (G_TYPE_POINTER, "gpointer"),
        (G_TYPE_BOXED, "GBoxed"),
        (G_TYPE_PARAM, "GParam"),
        (G_TYPE_OBJECT, "GObject"),
        (G_TYPE_VARIANT, "GVariant"),
    ];
    for &(id, name) in fundamentals {
        let node = TypeNode {
            type_id: id,
            name: name.to_owned(),
            parent: None,
            children: Vec::new(),
            interfaces: Vec::new(),
            fundamental_flags: GTypeFundamentalFlags::NONE,
            flags: GTypeFlags::NONE,
            info: GTypeInfo::default(),
            is_classed: false,
            is_instantiatable: false,
        };
        reg.nodes.insert(id, node);
        reg.name_to_id.insert(name.to_owned(), id);
    }
}

// ── Public API ────────────────────────────────────────────────────────

/// Initialize the type system (`g_type_init`).
pub fn type_init() {
    ensure_registry();
}

/// Get the type registration serial (`g_type_get_type_registration_serial`).
pub fn type_get_type_registration_serial() -> u32 {
    ensure_registry();
    TYPE_REGISTRY.read().as_ref().unwrap().registration_serial
}

/// Look up a type ID by name (`g_type_from_name`).
pub fn type_from_name(name: &str) -> GType {
    ensure_registry();
    TYPE_REGISTRY.read().as_ref().unwrap()
        .name_to_id.get(name).copied().unwrap_or(G_TYPE_INVALID)
}

/// Look up a type name by ID (`g_type_name`).
pub fn type_name(type_id: GType) -> Option<String> {
    ensure_registry();
    TYPE_REGISTRY.read().as_ref().unwrap()
        .nodes.get(&type_id).map(|n| n.name.clone())
}

/// Get the parent type (`g_type_parent`).
pub fn type_parent(type_id: GType) -> GType {
    ensure_registry();
    TYPE_REGISTRY.read().as_ref().unwrap()
        .nodes.get(&type_id)
        .and_then(|n| n.parent)
        .unwrap_or(G_TYPE_INVALID)
}

/// Get the fundamental type of `type_id` (`g_type_fundamental`).
pub fn type_fundamental(type_id: GType) -> GType {
    ensure_registry();
    let guard = TYPE_REGISTRY.read();
    let reg = guard.as_ref().unwrap();
    let mut current = type_id;
    loop {
        match reg.nodes.get(&current) {
            Some(node) if node.parent.is_some() => {
                current = node.parent.unwrap();
            }
            Some(_) => return current,
            None => return G_TYPE_INVALID,
        }
    }
}

/// Get the next free fundamental type ID (`g_type_fundamental_next`).
pub fn type_fundamental_next() -> GType {
    ensure_registry();
    let guard = TYPE_REGISTRY.read();
    let reg = guard.as_ref().unwrap();
    let next = reg.next_fundamental;
    if next <= G_TYPE_FUNDAMENTAL_MAX { next } else { 0 }
}

/// Check if `type_id` is a descendant of `is_a_type` (`g_type_is_a`).
pub fn type_is_a(type_id: GType, is_a_type: GType) -> bool {
    if type_id == is_a_type {
        return true;
    }
    ensure_registry();
    let guard = TYPE_REGISTRY.read();
    let reg = guard.as_ref().unwrap();
    let mut current = type_id;
    loop {
        match reg.nodes.get(&current) {
            Some(node) => {
                if node.type_id == is_a_type {
                    return true;
                }
                if node.interfaces.contains(&is_a_type) {
                    return true;
                }
                match node.parent {
                    Some(p) => current = p,
                    None => return false,
                }
            }
            None => return false,
        }
    }
}

/// Get the depth of a type in the hierarchy (`g_type_depth`).
pub fn type_depth(type_id: GType) -> u32 {
    ensure_registry();
    let guard = TYPE_REGISTRY.read();
    let reg = guard.as_ref().unwrap();
    let mut depth = 0u32;
    let mut current = type_id;
    loop {
        match reg.nodes.get(&current) {
            Some(node) => {
                depth += 1;
                match node.parent {
                    Some(p) => current = p,
                    None => break,
                }
            }
            None => break,
        }
    }
    depth
}

/// Get the number of children of a type (`g_type_children`).
pub fn type_children(type_id: GType) -> Vec<GType> {
    ensure_registry();
    TYPE_REGISTRY.read().as_ref().unwrap()
        .nodes.get(&type_id)
        .map(|n| n.children.clone())
        .unwrap_or_default()
}

/// Get the interfaces implemented by a type (`g_type_interfaces`).
pub fn type_interfaces(type_id: GType) -> Vec<GType> {
    ensure_registry();
    TYPE_REGISTRY.read().as_ref().unwrap()
        .nodes.get(&type_id)
        .map(|n| n.interfaces.clone())
        .unwrap_or_default()
}

/// Check if a type is classed (`G_TYPE_IS_CLASSED`).
pub fn type_is_classed(type_id: GType) -> bool {
    ensure_registry();
    TYPE_REGISTRY.read().as_ref().unwrap()
        .nodes.get(&type_id)
        .map(|n| n.is_classed)
        .unwrap_or(false)
}

/// Check if a type is instantiatable (`G_TYPE_IS_INSTANTIATABLE`).
pub fn type_is_instantiatable(type_id: GType) -> bool {
    ensure_registry();
    TYPE_REGISTRY.read().as_ref().unwrap()
        .nodes.get(&type_id)
        .map(|n| n.is_instantiatable)
        .unwrap_or(false)
}

/// Check if a type is abstract (`G_TYPE_IS_ABSTRACT`).
pub fn type_is_abstract(type_id: GType) -> bool {
    ensure_registry();
    TYPE_REGISTRY.read().as_ref().unwrap()
        .nodes.get(&type_id)
        .map(|n| n.flags.contains(GTypeFlags::ABSTRACT))
        .unwrap_or(false)
}

/// Check if a type is final (`G_TYPE_IS_FINAL`).
pub fn type_is_final(type_id: GType) -> bool {
    ensure_registry();
    TYPE_REGISTRY.read().as_ref().unwrap()
        .nodes.get(&type_id)
        .map(|n| n.flags.contains(GTypeFlags::FINAL))
        .unwrap_or(false)
}

/// Register a fundamental type (`g_type_register_fundamental`).
pub fn type_register_fundamental(
    type_id: GType,
    type_name: &str,
    info: GTypeInfo,
    finfo: GTypeFundamentalFlags,
    flags: GTypeFlags,
) -> GType {
    ensure_registry();
    let mut guard = TYPE_REGISTRY.write();
    let reg = guard.as_mut().unwrap();

    if type_id == 0 || type_id > G_TYPE_FUNDAMENTAL_MAX {
        return G_TYPE_INVALID;
    }
    if reg.nodes.contains_key(&type_id) {
        return G_TYPE_INVALID;
    }
    if reg.name_to_id.contains_key(type_name) {
        return G_TYPE_INVALID;
    }

    let node = TypeNode {
        type_id,
        name: type_name.to_owned(),
        parent: None,
        children: Vec::new(),
        interfaces: Vec::new(),
        fundamental_flags: finfo,
        flags,
        info,
        is_classed: finfo.contains(GTypeFundamentalFlags::CLASSED),
        is_instantiatable: finfo.contains(GTypeFundamentalFlags::INSTANTIATABLE),
    };
    reg.nodes.insert(type_id, node);
    reg.name_to_id.insert(type_name.to_owned(), type_id);
    reg.registration_serial = reg.registration_serial.wrapping_add(1);

    if type_id >= reg.next_fundamental {
        reg.next_fundamental = type_id + (1 << G_TYPE_FUNDAMENTAL_SHIFT);
    }

    type_id
}

/// Register a static (non-dynamic) derived type (`g_type_register_static`).
pub fn type_register_static(
    parent_type: GType,
    type_name: &str,
    info: &GTypeInfo,
    flags: GTypeFlags,
) -> GType {
    ensure_registry();
    let mut guard = TYPE_REGISTRY.write();
    let reg = guard.as_mut().unwrap();

    if parent_type == G_TYPE_INVALID || !reg.nodes.contains_key(&parent_type) {
        return G_TYPE_INVALID;
    }
    if reg.name_to_id.contains_key(type_name) {
        return G_TYPE_INVALID;
    }

    let type_id = reg.next_id;
    reg.next_id += 1;

    let parent_node = reg.nodes.get(&parent_type).unwrap();
    let is_classed = parent_node.is_classed;
    let is_instantiatable = parent_node.is_instantiatable;

    let node = TypeNode {
        type_id,
        name: type_name.to_owned(),
        parent: Some(parent_type),
        children: Vec::new(),
        interfaces: Vec::new(),
        fundamental_flags: GTypeFundamentalFlags::NONE,
        flags,
        info: info.clone(),
        is_classed,
        is_instantiatable,
    };
    reg.nodes.insert(type_id, node);
    reg.name_to_id.insert(type_name.to_owned(), type_id);

    let pnode = reg.nodes.get_mut(&parent_type).unwrap();
    pnode.children.push(type_id);

    reg.registration_serial = reg.registration_serial.wrapping_add(1);

    type_id
}

/// Register a static type with simplified parameters (`g_type_register_static_simple`).
pub fn type_register_static_simple(
    parent_type: GType,
    type_name: &str,
    class_size: u16,
    class_init: Option<fn(&mut TypeClassData)>,
    instance_size: u16,
    instance_init: Option<fn(&mut TypeInstanceData)>,
    flags: GTypeFlags,
) -> GType {
    let info = GTypeInfo {
        class_size,
        instance_size,
        class_init,
        instance_init,
        value_table: None,
    };
    type_register_static(parent_type, type_name, &info, flags)
}

/// Get the instance size of a type (`g_type_instance_size`).
pub fn type_instance_size(type_id: GType) -> u16 {
    ensure_registry();
    TYPE_REGISTRY.read().as_ref().unwrap()
        .nodes.get(&type_id)
        .map(|n| n.info.instance_size)
        .unwrap_or(0)
}

/// Get the class size of a type.
pub fn type_class_size(type_id: GType) -> u16 {
    ensure_registry();
    TYPE_REGISTRY.read().as_ref().unwrap()
        .nodes.get(&type_id)
        .map(|n| n.info.class_size)
        .unwrap_or(0)
}

/// Get the value table for a type.
pub fn type_value_table(type_id: GType) -> Option<GTypeValueTable> {
    ensure_registry();
    let guard = TYPE_REGISTRY.read();
    let reg = guard.as_ref().unwrap();
    let mut current = type_id;
    loop {
        match reg.nodes.get(&current) {
            Some(node) => {
                if let Some(ref vt) = node.info.value_table {
                    return Some(vt.clone());
                }
                match node.parent {
                    Some(p) => current = p,
                    None => return None,
                }
            }
            None => return None,
        }
    }
}

/// Add an interface to a type.
pub fn type_add_interface(instance_type: GType, interface_type: GType) -> bool {
    ensure_registry();
    let mut guard = TYPE_REGISTRY.write();
    let reg = guard.as_mut().unwrap();
    if !reg.nodes.contains_key(&instance_type) || !reg.nodes.contains_key(&interface_type) {
        return false;
    }
    let node = reg.nodes.get_mut(&instance_type).unwrap();
    if !node.interfaces.contains(&interface_type) {
        node.interfaces.push(interface_type);
        reg.registration_serial = reg.registration_serial.wrapping_add(1);
    }
    true
}

/// Query type info (`g_type_query`).
#[derive(Default, Clone)]
pub struct TypeQuery {
    pub type_id: GType,
    pub type_name: String,
    pub class_size: u16,
    pub instance_size: u16,
}

pub fn type_query(type_id: GType) -> Option<TypeQuery> {
    ensure_registry();
    TYPE_REGISTRY.read().as_ref().unwrap()
        .nodes.get(&type_id)
        .map(|n| TypeQuery {
            type_id: n.type_id,
            type_name: n.name.clone(),
            class_size: n.info.class_size,
            instance_size: n.info.instance_size,
        })
}

/// Get all registered type IDs.
pub fn type_get_all() -> Vec<GType> {
    ensure_registry();
    TYPE_REGISTRY.read().as_ref().unwrap()
        .nodes.keys().copied().collect()
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fundamental_types_registered() {
        type_init();
        assert_eq!(type_from_name("gint"), G_TYPE_INT);
        assert_eq!(type_from_name("GObject"), G_TYPE_OBJECT);
        assert_eq!(type_name(G_TYPE_STRING), Some("gchararray".to_owned()));
        assert_eq!(type_name(G_TYPE_INVALID), None);
    }

    #[test]
    fn type_name_roundtrip() {
        type_init();
        let id = type_from_name("gboolean");
        assert_eq!(id, G_TYPE_BOOLEAN);
        assert_eq!(type_name(id), Some("gboolean".to_owned()));
    }

    #[test]
    fn register_static_type() {
        type_init();
        let info = GTypeInfo {
            class_size: 64,
            instance_size: 32,
            class_init: None,
            instance_init: None,
            value_table: None,
        };
        let id = type_register_static(G_TYPE_OBJECT, "MyObject", &info, GTypeFlags::NONE);
        assert!(id != G_TYPE_INVALID);
        assert_eq!(type_name(id), Some("MyObject".to_owned()));
        assert_eq!(type_parent(id), G_TYPE_OBJECT);
        assert!(type_is_a(id, G_TYPE_OBJECT));
        assert!(!type_is_a(id, G_TYPE_STRING));
        assert_eq!(type_depth(id), 2);
        assert_eq!(type_instance_size(id), 32);
        assert_eq!(type_class_size(id), 64);
    }

    #[test]
    fn type_children_test() {
        type_init();
        let info = GTypeInfo::default();
        let id1 = type_register_static(G_TYPE_OBJECT, "ChildA", &info, GTypeFlags::NONE);
        let id2 = type_register_static(G_TYPE_OBJECT, "ChildB", &info, GTypeFlags::NONE);
        let children = type_children(G_TYPE_OBJECT);
        assert!(children.contains(&id1));
        assert!(children.contains(&id2));
    }

    #[test]
    fn type_is_a_self() {
        type_init();
        assert!(type_is_a(G_TYPE_INT, G_TYPE_INT));
        assert!(type_is_a(G_TYPE_OBJECT, G_TYPE_OBJECT));
    }

    #[test]
    fn type_fundamental_test() {
        type_init();
        assert_eq!(type_fundamental(G_TYPE_INT), G_TYPE_INT);
        let info = GTypeInfo::default();
        let id = type_register_static(G_TYPE_OBJECT, "DerivedObj", &info, GTypeFlags::NONE);
        assert_eq!(type_fundamental(id), G_TYPE_OBJECT);
    }

    #[test]
    fn duplicate_name_fails() {
        type_init();
        let info = GTypeInfo::default();
        let id1 = type_register_static(G_TYPE_OBJECT, "UniqueType", &info, GTypeFlags::NONE);
        assert!(id1 != G_TYPE_INVALID);
        let id2 = type_register_static(G_TYPE_OBJECT, "UniqueType", &info, GTypeFlags::NONE);
        assert_eq!(id2, G_TYPE_INVALID);
    }

    #[test]
    fn type_flags() {
        type_init();
        let info = GTypeInfo::default();
        let id = type_register_static(G_TYPE_OBJECT, "AbstractType", &info, GTypeFlags::ABSTRACT);
        assert!(type_is_abstract(id));
        assert!(!type_is_final(id));
    }

    #[test]
    fn type_query_info() {
        type_init();
        let info = GTypeInfo {
            class_size: 128,
            instance_size: 64,
            class_init: None,
            instance_init: None,
            value_table: None,
        };
        let id = type_register_static(G_TYPE_OBJECT, "QueriedType", &info, GTypeFlags::NONE);
        let q = type_query(id).unwrap();
        assert_eq!(q.type_name, "QueriedType");
        assert_eq!(q.class_size, 128);
        assert_eq!(q.instance_size, 64);
    }

    #[test]
    fn registration_serial_increments() {
        type_init();
        let s1 = type_get_type_registration_serial();
        let info = GTypeInfo::default();
        type_register_static(G_TYPE_OBJECT, "SerialTest", &info, GTypeFlags::NONE);
        let s2 = type_get_type_registration_serial();
        assert!(s2 > s1);
    }

    #[test]
    fn type_add_interface_works() {
        type_init();
        let info = GTypeInfo::default();
        let id = type_register_static(G_TYPE_OBJECT, "IfaceImpl", &info, GTypeFlags::NONE);
        assert!(type_add_interface(id, G_TYPE_INTERFACE));
        let ifaces = type_interfaces(id);
        assert!(ifaces.contains(&G_TYPE_INTERFACE));
    }
}
