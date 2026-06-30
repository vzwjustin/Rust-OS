//! GParamSpec — property parameter specifications.
//!
//! ParamSpecs describe object properties: name, type, flags, bounds,
//! and default values. They are used by GObject for property get/set
//! and notification.

use crate::gtype::*;
use crate::gvalue::GValue;
use crate::prelude::*;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use spin::rwlock::RwLock;

// Re-export ParamFlags from gtype so callers of `gparamspec` can use it
// without having to import gtype separately.
pub use crate::gtype::ParamFlags;

/// Parameter type identifier.
pub type ParamID = u32;

/// Base parameter specification (`GParamSpec`).
#[derive(Clone)]
pub struct ParamSpec {
    pub name: String,
    pub nick: String,
    pub blurb: String,
    pub value_type: GType,
    pub flags: ParamFlags,
    pub id: ParamID,
    pub default_value: GValue,
    /// Origin of an override spec (`GParamSpecOverride` in upstream): the
    /// `(parent_type, overridden_property_name)` this spec redirects to. `None`
    /// for concrete specs. Resolved lazily via a [`ParamSpecPool`].
    pub override_origin: Option<(GType, String)>,
}

impl ParamSpec {
    /// Create a new ParamSpec for a boolean property.
    pub fn boolean(name: &str, nick: &str, blurb: &str, default: bool, flags: ParamFlags) -> Self {
        let mut default_val = GValue::for_type(G_TYPE_BOOLEAN);
        default_val.set_boolean(default);
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_BOOLEAN,
            flags,
            id: 0,
            default_value: default_val,
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for an int property.
    pub fn int(
        name: &str,
        nick: &str,
        blurb: &str,
        _min: i32,
        _max: i32,
        default: i32,
        flags: ParamFlags,
    ) -> Self {
        let mut default_val = GValue::for_type(G_TYPE_INT);
        default_val.set_int(default);
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_INT,
            flags,
            id: 0,
            default_value: default_val,
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for a uint property.
    pub fn uint(
        name: &str,
        nick: &str,
        blurb: &str,
        _min: u32,
        _max: u32,
        default: u32,
        flags: ParamFlags,
    ) -> Self {
        let mut default_val = GValue::for_type(G_TYPE_UINT);
        default_val.set_uint(default);
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_UINT,
            flags,
            id: 0,
            default_value: default_val,
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for a string property.
    pub fn string(name: &str, nick: &str, blurb: &str, default: &str, flags: ParamFlags) -> Self {
        let mut default_val = GValue::for_type(G_TYPE_STRING);
        default_val.set_string(default);
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_STRING,
            flags,
            id: 0,
            default_value: default_val,
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for a double property.
    pub fn double(
        name: &str,
        nick: &str,
        blurb: &str,
        _min: f64,
        _max: f64,
        default: f64,
        flags: ParamFlags,
    ) -> Self {
        let mut default_val = GValue::for_type(G_TYPE_DOUBLE);
        default_val.set_double(default);
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_DOUBLE,
            flags,
            id: 0,
            default_value: default_val,
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for a float property.
    pub fn float(
        name: &str,
        nick: &str,
        blurb: &str,
        _min: f32,
        _max: f32,
        default: f32,
        flags: ParamFlags,
    ) -> Self {
        let mut default_val = GValue::for_type(G_TYPE_FLOAT);
        default_val.set_float(default);
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_FLOAT,
            flags,
            id: 0,
            default_value: default_val,
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for an enum property.
    pub fn enum_(name: &str, nick: &str, blurb: &str, default: i32, flags: ParamFlags) -> Self {
        let mut default_val = GValue::for_type(G_TYPE_ENUM);
        default_val.set_enum(default);
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_ENUM,
            flags,
            id: 0,
            default_value: default_val,
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for a flags property.
    pub fn flags(name: &str, nick: &str, blurb: &str, default: u32, flags: ParamFlags) -> Self {
        let mut default_val = GValue::for_type(G_TYPE_FLAGS);
        default_val.set_flags(default);
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_FLAGS,
            flags,
            id: 0,
            default_value: default_val,
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for an int64 property.
    pub fn int64(
        name: &str,
        nick: &str,
        blurb: &str,
        _min: i64,
        _max: i64,
        default: i64,
        flags: ParamFlags,
    ) -> Self {
        let mut default_val = GValue::for_type(G_TYPE_INT64);
        default_val.set_int64(default);
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_INT64,
            flags,
            id: 0,
            default_value: default_val,
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for a uint64 property.
    pub fn uint64(
        name: &str,
        nick: &str,
        blurb: &str,
        _min: u64,
        _max: u64,
        default: u64,
        flags: ParamFlags,
    ) -> Self {
        let mut default_val = GValue::for_type(G_TYPE_UINT64);
        default_val.set_uint64(default);
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_UINT64,
            flags,
            id: 0,
            default_value: default_val,
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for a char property.
    pub fn char(name: &str, nick: &str, blurb: &str, default: i8, flags: ParamFlags) -> Self {
        let mut default_val = GValue::for_type(G_TYPE_CHAR);
        default_val.set_char(default);
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_CHAR,
            flags,
            id: 0,
            default_value: default_val,
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for a uchar property.
    pub fn uchar(name: &str, nick: &str, blurb: &str, default: u8, flags: ParamFlags) -> Self {
        let mut default_val = GValue::for_type(G_TYPE_UCHAR);
        default_val.set_uchar(default);
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_UCHAR,
            flags,
            id: 0,
            default_value: default_val,
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for a long property.
    pub fn long(name: &str, nick: &str, blurb: &str, default: i64, flags: ParamFlags) -> Self {
        let mut default_val = GValue::for_type(G_TYPE_LONG);
        default_val.set_long(default);
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_LONG,
            flags,
            id: 0,
            default_value: default_val,
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for a ulong property.
    pub fn ulong(name: &str, nick: &str, blurb: &str, default: u64, flags: ParamFlags) -> Self {
        let mut default_val = GValue::for_type(G_TYPE_ULONG);
        default_val.set_ulong(default);
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_ULONG,
            flags,
            id: 0,
            default_value: default_val,
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for an object property.
    pub fn object(
        name: &str,
        nick: &str,
        blurb: &str,
        object_type: GType,
        flags: ParamFlags,
    ) -> Self {
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: object_type,
            flags,
            id: 0,
            default_value: GValue::for_type(object_type),
            override_origin: None,
        }
    }

    /// Create a new ParamSpec for a pointer property.
    pub fn pointer(name: &str, nick: &str, blurb: &str, flags: ParamFlags) -> Self {
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: G_TYPE_POINTER,
            flags,
            id: 0,
            default_value: GValue::for_type(G_TYPE_POINTER),
            override_origin: None,
        }
    }

    /// Check if the property is readable.
    pub fn is_readable(&self) -> bool {
        self.flags.contains(ParamFlags::READABLE)
    }

    /// Check if the property is writable.
    pub fn is_writable(&self) -> bool {
        self.flags.contains(ParamFlags::WRITABLE)
    }

    /// Check if the property is construct-only.
    pub fn is_construct_only(&self) -> bool {
        self.flags.contains(ParamFlags::CONSTRUCT_ONLY)
    }

    /// Get the default value (`g_param_spec_get_default_value`).
    pub fn get_default_value(&self) -> &GValue {
        &self.default_value
    }

    /// Validate a value against this spec (`g_param_value_validate`).
    pub fn value_validate(&self, value: &mut GValue) -> bool {
        if value.value_type() != self.value_type {
            return false;
        }
        true
    }
}

/// Install properties on a class (`g_object_class_install_property`).
pub fn install_properties(specs: &mut [ParamSpec]) {
    for (i, spec) in specs.iter_mut().enumerate() {
        spec.id = (i + 1) as ParamID;
    }
}

/// Look up a property by name.
pub fn find_property<'a>(specs: &'a [ParamSpec], name: &str) -> Option<&'a ParamSpec> {
    specs.iter().find(|s| s.name == name)
}

/// Look up a property by ID.
pub fn find_property_by_id<'a>(specs: &'a [ParamSpec], id: ParamID) -> Option<&'a ParamSpec> {
    specs.iter().find(|s| s.id == id)
}

/// Get all property names.
pub fn property_names(specs: &[ParamSpec]) -> Vec<String> {
    specs.iter().map(|s| s.name.clone()).collect()
}

// ── GParamSpec pool ───────────────────────────────────────────────────

/// Pool of `GParamSpec`s keyed by owner type (`GParamSpecPool` in upstream).
///
/// Mirrors `g_param_spec_pool_*`: a thread-safe (via `spin::RwLock`) mapping
/// from an owner `GType` to the set of `ParamSpec`s registered for that type.
/// Specs are stored as `Arc<ParamSpec>` so a lookup can hand out a cheap
/// shared reference.
pub struct ParamSpecPool {
    by_owner: RwLock<BTreeMap<GType, Vec<Arc<ParamSpec>>>>,
}

impl ParamSpecPool {
    /// Create an empty pool (`g_param_spec_pool_new`).
    pub fn new() -> Self {
        Self {
            by_owner: RwLock::new(BTreeMap::new()),
        }
    }

    /// Insert a `ParamSpec` for `owner_type` (`g_param_spec_pool_insert`).
    ///
    /// Upstream asserts on a duplicate `(owner_type, name)` pair; here we
    /// surface that as `Err` with a clear message so callers in a `no_std`
    /// kernel can react without aborting. On success the spec is `Arc`'d and
    /// stored, returning `Ok(())`.
    pub fn insert(&self, owner_type: GType, pspec: ParamSpec) -> Result<(), &'static str> {
        let mut guard = self.by_owner.write();
        let entry = guard.entry(owner_type).or_insert_with(Vec::new);
        if entry.iter().any(|s| s.name == pspec.name) {
            return Err("g_param_spec_pool_insert: duplicate (owner_type, name)");
        }
        entry.push(Arc::new(pspec));
        Ok(())
    }

    /// Look up a `ParamSpec` by `owner_type` and property name
    /// (`g_param_spec_pool_lookup`).
    pub fn lookup(&self, owner_type: GType, name: &str) -> Option<Arc<ParamSpec>> {
        let guard = self.by_owner.read();
        guard
            .get(&owner_type)
            .and_then(|v| v.iter().find(|s| s.name == name).cloned())
    }

    /// Remove a `ParamSpec` from `owner_type` by its `id`
    /// (`g_param_spec_pool_remove`). Returns `true` if a spec was removed.
    pub fn remove(&self, owner_type: GType, pspec_id: ParamID) -> bool {
        let mut guard = self.by_owner.write();
        if let Some(entry) = guard.get_mut(&owner_type) {
            let before = entry.len();
            entry.retain(|s| s.id != pspec_id);
            return entry.len() != before;
        }
        false
    }

    /// List all `ParamSpec`s registered for `owner_type`
    /// (`g_param_spec_pool_list`).
    pub fn list(&self, owner_type: GType) -> Vec<Arc<ParamSpec>> {
        let guard = self.by_owner.read();
        guard.get(&owner_type).cloned().unwrap_or_default()
    }

    /// List specs whose owner type is exactly `owner_type`, excluding inherited
    /// specs (`g_param_spec_pool_list_owned`). With the flat per-owner storage
    /// here this is equivalent to [`ParamSpecPool::list`].
    pub fn list_owned(&self, owner_type: GType) -> Vec<Arc<ParamSpec>> {
        self.list(owner_type)
    }
}

impl Default for ParamSpecPool {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a new `ParamSpecPool` (`g_param_spec_pool_new`).
pub fn param_spec_pool_new() -> ParamSpecPool {
    ParamSpecPool::new()
}

/// Insert a `ParamSpec` into a pool (`g_param_spec_pool_insert`). Thin wrapper
/// around [`ParamSpecPool::insert`] that panics on duplicate `(owner_type,
/// name)`, matching upstream's `g_return_if_fail`/assert behaviour.
pub fn param_spec_pool_insert(pool: &ParamSpecPool, owner_type: GType, pspec: ParamSpec) {
    if let Err(e) = pool.insert(owner_type, pspec) {
        panic!("{}", e);
    }
}

/// Look up a spec in a pool (`g_param_spec_pool_lookup`).
pub fn param_spec_pool_lookup(
    pool: &ParamSpecPool,
    owner_type: GType,
    name: &str,
) -> Option<Arc<ParamSpec>> {
    pool.lookup(owner_type, name)
}

/// Remove a spec from a pool by id (`g_param_spec_pool_remove`).
pub fn param_spec_pool_remove(pool: &ParamSpecPool, owner_type: GType, pspec_id: ParamID) -> bool {
    pool.remove(owner_type, pspec_id)
}

/// List specs in a pool for `owner_type` (`g_param_spec_pool_list`).
pub fn param_spec_pool_list(pool: &ParamSpecPool, owner_type: GType) -> Vec<Arc<ParamSpec>> {
    pool.list(owner_type)
}

// ── GParamSpec override ───────────────────────────────────────────────

/// Construct a `GParamSpecOverride` (`g_param_spec_override`).
///
/// The returned `ParamSpec` carries `override_name` as its `name` and stores
/// `(parent_type, overridden_name)` in [`ParamSpec::override_origin`]. The
/// redirected spec is resolved lazily against a [`ParamSpecPool`] via
/// [`param_spec_override_resolve`]. The `value_type` and `flags` are copied
/// from the resolved parent spec at resolution time; until then the override
/// carries `G_TYPE_INVALID` and `ParamFlags::NONE` as placeholders.
pub fn param_spec_override(
    override_name: &str,
    parent_type: GType,
    overridden_name: &str,
) -> ParamSpec {
    ParamSpec {
        name: override_name.to_owned(),
        nick: override_name.to_owned(),
        blurb: String::new(),
        value_type: G_TYPE_INVALID,
        flags: ParamFlags::NONE,
        id: 0,
        default_value: GValue::for_type(G_TYPE_INVALID),
        override_origin: Some((parent_type, overridden_name.to_owned())),
    }
}

/// Resolve a `GParamSpecOverride` against a pool, returning the parent spec it
/// redirects to. Returns `None` for non-override specs or when the parent
/// property is not registered under `parent_type`.
pub fn param_spec_override_resolve(
    pool: &ParamSpecPool,
    pspec: &ParamSpec,
) -> Option<Arc<ParamSpec>> {
    let (parent_type, overridden_name) = pspec.override_origin.as_ref()?;
    pool.lookup(*parent_type, overridden_name)
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boolean_spec() {
        type_init();
        let spec = ParamSpec::boolean("visible", "v", "visibility", true, ParamFlags::READWRITE);
        assert_eq!(spec.name, "visible");
        assert_eq!(spec.value_type, G_TYPE_BOOLEAN);
        assert!(spec.is_readable());
        assert!(spec.is_writable());
        assert!(spec.get_default_value().get_boolean());
    }

    #[test]
    fn int_spec() {
        type_init();
        let spec = ParamSpec::int(
            "count",
            "c",
            "count value",
            0,
            100,
            50,
            ParamFlags::READWRITE,
        );
        assert_eq!(spec.value_type, G_TYPE_INT);
        assert_eq!(spec.get_default_value().get_int(), 50);
    }

    #[test]
    fn string_spec() {
        type_init();
        let spec = ParamSpec::string("name", "n", "object name", "default", ParamFlags::READWRITE);
        assert_eq!(spec.value_type, G_TYPE_STRING);
        assert_eq!(spec.get_default_value().get_string(), Some("default"));
    }

    #[test]
    fn install_and_find() {
        type_init();
        let mut specs = vec![
            ParamSpec::int("x", "x", "x coord", 0, 100, 0, ParamFlags::READWRITE),
            ParamSpec::int("y", "y", "y coord", 0, 100, 0, ParamFlags::READWRITE),
            ParamSpec::string("label", "l", "label text", "", ParamFlags::READWRITE),
        ];
        install_properties(&mut specs);
        assert_eq!(specs[0].id, 1);
        assert_eq!(specs[1].id, 2);
        assert_eq!(specs[2].id, 3);
        assert!(find_property(&specs, "x").is_some());
        assert!(find_property(&specs, "y").is_some());
        assert!(find_property(&specs, "label").is_some());
        assert!(find_property(&specs, "z").is_none());
        assert!(find_property_by_id(&specs, 2).is_some());
    }

    #[test]
    fn construct_only_flag() {
        type_init();
        let spec = ParamSpec::string(
            "id",
            "i",
            "identifier",
            "",
            ParamFlags::CONSTRUCT_ONLY | ParamFlags::READWRITE,
        );
        assert!(spec.is_construct_only());
        assert!(!spec.is_readable() || spec.is_writable());
    }

    #[test]
    fn double_spec() {
        type_init();
        let spec = ParamSpec::double(
            "ratio",
            "r",
            "aspect ratio",
            0.0,
            10.0,
            1.0,
            ParamFlags::READWRITE,
        );
        assert_eq!(spec.value_type, G_TYPE_DOUBLE);
        assert!((spec.get_default_value().get_double() - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn property_names_list() {
        type_init();
        let specs = vec![
            ParamSpec::int("a", "a", "a", 0, 1, 0, ParamFlags::READWRITE),
            ParamSpec::int("b", "b", "b", 0, 1, 0, ParamFlags::READWRITE),
        ];
        let names = property_names(&specs);
        assert_eq!(names, vec!["a", "b"]);
    }

    #[test]
    fn pool_insert_lookup_remove() {
        type_init();
        let pool = param_spec_pool_new();
        let mut spec = ParamSpec::int("count", "c", "count", 0, 100, 0, ParamFlags::READWRITE);
        spec.id = 1;
        param_spec_pool_insert(&pool, G_TYPE_OBJECT, spec);
        assert!(param_spec_pool_lookup(&pool, G_TYPE_OBJECT, "count").is_some());
        assert!(param_spec_pool_lookup(&pool, G_TYPE_OBJECT, "missing").is_none());
        assert_eq!(param_spec_pool_list(&pool, G_TYPE_OBJECT).len(), 1);
        assert!(param_spec_pool_remove(&pool, G_TYPE_OBJECT, 1));
        assert!(param_spec_pool_lookup(&pool, G_TYPE_OBJECT, "count").is_none());
        // Removing again is a no-op (returns false).
        assert!(!param_spec_pool_remove(&pool, G_TYPE_OBJECT, 1));
    }

    #[test]
    fn pool_duplicate_rejected() {
        type_init();
        let pool = param_spec_pool_new();
        let mut a = ParamSpec::int("x", "x", "x", 0, 1, 0, ParamFlags::READWRITE);
        a.id = 1;
        param_spec_pool_insert(&pool, G_TYPE_OBJECT, a);
        let mut b = ParamSpec::int("x", "x", "x", 0, 1, 0, ParamFlags::READWRITE);
        b.id = 2;
        // Duplicate (owner, name) -> Result::Err on the method.
        assert!(pool.insert(G_TYPE_OBJECT, b).is_err());
        // Only one entry remains.
        assert_eq!(param_spec_pool_list(&pool, G_TYPE_OBJECT).len(), 1);
    }

    #[test]
    fn pool_list_empty_owner() {
        type_init();
        let pool = param_spec_pool_new();
        assert!(param_spec_pool_list(&pool, G_TYPE_OBJECT).is_empty());
    }

    #[test]
    fn override_construction_and_resolution() {
        type_init();
        let pool = param_spec_pool_new();
        let mut parent = ParamSpec::string("label", "l", "label", "", ParamFlags::READWRITE);
        parent.id = 1;
        param_spec_pool_insert(&pool, G_TYPE_OBJECT, parent);

        let ov = param_spec_override("my-label", G_TYPE_OBJECT, "label");
        assert_eq!(ov.name, "my-label");
        assert!(ov.override_origin.is_some());
        assert_eq!(ov.value_type, G_TYPE_INVALID);

        let resolved = param_spec_override_resolve(&pool, &ov).expect("parent spec resolved");
        assert_eq!(resolved.name, "label");
        assert_eq!(resolved.value_type, G_TYPE_STRING);

        // A concrete spec has no override origin.
        let concrete = ParamSpec::int("n", "n", "n", 0, 1, 0, ParamFlags::READWRITE);
        assert!(param_spec_override_resolve(&pool, &concrete).is_none());
    }

    #[test]
    #[should_panic]
    fn pool_insert_panics_on_duplicate_via_fn() {
        type_init();
        let pool = param_spec_pool_new();
        let mut a = ParamSpec::int("dup", "d", "d", 0, 1, 0, ParamFlags::READWRITE);
        a.id = 1;
        param_spec_pool_insert(&pool, G_TYPE_OBJECT, a);
        let mut b = ParamSpec::int("dup", "d", "d", 0, 1, 0, ParamFlags::READWRITE);
        b.id = 2;
        param_spec_pool_insert(&pool, G_TYPE_OBJECT, b);
    }
}
