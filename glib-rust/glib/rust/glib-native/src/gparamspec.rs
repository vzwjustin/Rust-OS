//! GParamSpec — property parameter specifications.
//!
//! ParamSpecs describe object properties: name, type, flags, bounds,
//! and default values. They are used by GObject for property get/set
//! and notification.

use crate::gtype::*;
use crate::gvalue::GValue;
use crate::prelude::*;

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
        }
    }

    /// Create a new ParamSpec for an int property.
    pub fn int(name: &str, nick: &str, blurb: &str, min: i32, max: i32, default: i32, flags: ParamFlags) -> Self {
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
        }
    }

    /// Create a new ParamSpec for a uint property.
    pub fn uint(name: &str, nick: &str, blurb: &str, min: u32, max: u32, default: u32, flags: ParamFlags) -> Self {
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
        }
    }

    /// Create a new ParamSpec for a double property.
    pub fn double(name: &str, nick: &str, blurb: &str, min: f64, max: f64, default: f64, flags: ParamFlags) -> Self {
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
        }
    }

    /// Create a new ParamSpec for a float property.
    pub fn float(name: &str, nick: &str, blurb: &str, min: f32, max: f32, default: f32, flags: ParamFlags) -> Self {
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
        }
    }

    /// Create a new ParamSpec for an int64 property.
    pub fn int64(name: &str, nick: &str, blurb: &str, min: i64, max: i64, default: i64, flags: ParamFlags) -> Self {
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
        }
    }

    /// Create a new ParamSpec for a uint64 property.
    pub fn uint64(name: &str, nick: &str, blurb: &str, min: u64, max: u64, default: u64, flags: ParamFlags) -> Self {
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
        }
    }

    /// Create a new ParamSpec for an object property.
    pub fn object(name: &str, nick: &str, blurb: &str, object_type: GType, flags: ParamFlags) -> Self {
        Self {
            name: name.to_owned(),
            nick: nick.to_owned(),
            blurb: blurb.to_owned(),
            value_type: object_type,
            flags,
            id: 0,
            default_value: GValue::for_type(object_type),
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
pub fn find_property(specs: &[ParamSpec], name: &str) -> Option<&ParamSpec> {
    specs.iter().find(|s| s.name == name)
}

/// Look up a property by ID.
pub fn find_property_by_id(specs: &[ParamSpec], id: ParamID) -> Option<&ParamSpec> {
    specs.iter().find(|s| s.id == id)
}

/// Get all property names.
pub fn property_names(specs: &[ParamSpec]) -> Vec<String> {
    specs.iter().map(|s| s.name.clone()).collect()
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
        let spec = ParamSpec::int("count", "c", "count value", 0, 100, 50, ParamFlags::READWRITE);
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
        let spec = ParamSpec::string("id", "i", "identifier", "", ParamFlags::CONSTRUCT_ONLY | ParamFlags::READWRITE);
        assert!(spec.is_construct_only());
        assert!(!spec.is_readable() || spec.is_writable());
    }

    #[test]
    fn double_spec() {
        type_init();
        let spec = ParamSpec::double("ratio", "r", "aspect ratio", 0.0, 10.0, 1.0, ParamFlags::READWRITE);
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
}
