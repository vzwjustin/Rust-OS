//! GVariant type system matching `gvarianttype.h` / `gvarianttype.c`.
//!
//! Pure type-string parsing and classification. No OS dependencies.
//! Fully `no_std` compatible.

#![allow(missing_docs)]

use crate::prelude::*;

/// A GVariant type, represented as a type string.
///
/// In GLib C, `GVariantType` is just a `const gchar*` pointing to a
/// well-formed type string. Here we wrap a `String` for ownership.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct VariantType {
    type_string: String,
}

/// Variant class (`GVariantClass`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum VariantClass {
    Boolean = b'b',
    Byte = b'y',
    Int16 = b'n',
    UInt16 = b'q',
    Int32 = b'i',
    UInt32 = b'u',
    Int64 = b'x',
    UInt64 = b't',
    Handle = b'h',
    Double = b'd',
    String = b's',
    ObjectPath = b'o',
    Signature = b'g',
    Variant = b'v',
    Maybe = b'm',
    Array = b'a',
    Tuple = b'(',
    DictEntry = b'{',
}

impl VariantClass {
    /// Convert from a character byte.
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            b'b' => Some(Self::Boolean),
            b'y' => Some(Self::Byte),
            b'n' => Some(Self::Int16),
            b'q' => Some(Self::UInt16),
            b'i' => Some(Self::Int32),
            b'u' => Some(Self::UInt32),
            b'x' => Some(Self::Int64),
            b't' => Some(Self::UInt64),
            b'h' => Some(Self::Handle),
            b'd' => Some(Self::Double),
            b's' => Some(Self::String),
            b'o' => Some(Self::ObjectPath),
            b'g' => Some(Self::Signature),
            b'v' => Some(Self::Variant),
            b'm' => Some(Self::Maybe),
            b'a' => Some(Self::Array),
            b'(' => Some(Self::Tuple),
            b'{' => Some(Self::DictEntry),
            _ => None,
        }
    }
}

// Well-known type constants
pub const VARIANT_TYPE_BOOLEAN: &str = "b";
pub const VARIANT_TYPE_BYTE: &str = "y";
pub const VARIANT_TYPE_INT16: &str = "n";
pub const VARIANT_TYPE_UINT16: &str = "q";
pub const VARIANT_TYPE_INT32: &str = "i";
pub const VARIANT_TYPE_UINT32: &str = "u";
pub const VARIANT_TYPE_INT64: &str = "x";
pub const VARIANT_TYPE_UINT64: &str = "t";
pub const VARIANT_TYPE_DOUBLE: &str = "d";
pub const VARIANT_TYPE_STRING: &str = "s";
pub const VARIANT_TYPE_OBJECT_PATH: &str = "o";
pub const VARIANT_TYPE_SIGNATURE: &str = "g";
pub const VARIANT_TYPE_VARIANT: &str = "v";
pub const VARIANT_TYPE_HANDLE: &str = "h";
pub const VARIANT_TYPE_UNIT: &str = "()";
pub const VARIANT_TYPE_ANY: &str = "*";
pub const VARIANT_TYPE_BASIC: &str = "?";
pub const VARIANT_TYPE_MAYBE: &str = "m*";
pub const VARIANT_TYPE_ARRAY: &str = "a*";
pub const VARIANT_TYPE_TUPLE: &str = "r";
pub const VARIANT_TYPE_DICT_ENTRY: &str = "{?*}";
pub const VARIANT_TYPE_DICTIONARY: &str = "a{?*}";
pub const VARIANT_TYPE_STRING_ARRAY: &str = "as";
pub const VARIANT_TYPE_BYTESTRING: &str = "ay";
pub const VARIANT_TYPE_BYTESTRING_ARRAY: &str = "aay";
pub const VARIANT_TYPE_VARDICT: &str = "a{sv}";

impl VariantType {
    /// Create from a type string (`g_variant_type_new`).
    ///
    /// Returns `None` if the string is not a valid type.
    pub fn new(type_str: &str) -> Option<Self> {
        if type_string_is_valid(type_str) {
            Some(Self {
                type_string: type_str.to_owned(),
            })
        } else {
            None
        }
    }

    /// Returns the type string.
    pub fn as_str(&self) -> &str {
        &self.type_string
    }

    /// Returns the type string (alias for `as_str`).
    pub fn type_string(&self) -> &str {
        &self.type_string
    }

    pub fn is_valid(type_str: &str) -> bool {
        type_string_is_valid(type_str)
    }

    /// Returns the string length.
    pub fn string_length(&self) -> usize {
        self.type_string.len()
    }

    /// Get child types of a tuple or dict entry.
    pub fn children(&self) -> Vec<VariantType> {
        if self.is_tuple() && self.type_string != "()" && self.type_string != "r" {
            let inner = &self.type_string[1..self.type_string.len() - 1];
            let mut result = Vec::new();
            let mut rest = inner;
            while !rest.is_empty() {
                if let Some((ts, remaining)) = scan_type_string(rest) {
                    result.push(VariantType { type_string: ts.to_owned() });
                    rest = remaining;
                } else {
                    break;
                }
            }
            result
        } else if self.is_dict_entry() && self.type_string != "{?*}" {
            let inner = &self.type_string[1..self.type_string.len() - 1];
            let mut result = Vec::new();
            let mut rest = inner;
            while !rest.is_empty() {
                if let Some((ts, remaining)) = scan_type_string(rest) {
                    result.push(VariantType { type_string: ts.to_owned() });
                    rest = remaining;
                } else {
                    break;
                }
            }
            result
        } else {
            Vec::new()
        }
    }

    // Convenience constructors for common types
    /// Boolean type (`b`).
    pub fn boolean() -> Self { Self::new(VARIANT_TYPE_BOOLEAN).unwrap() }
    /// Byte type (`y`).
    pub fn byte() -> Self { Self::new(VARIANT_TYPE_BYTE).unwrap() }
    /// Int16 type (`n`).
    pub fn int16() -> Self { Self::new(VARIANT_TYPE_INT16).unwrap() }
    /// UInt16 type (`q`).
    pub fn uint16() -> Self { Self::new(VARIANT_TYPE_UINT16).unwrap() }
    /// Int32 type (`i`).
    pub fn int32() -> Self { Self::new(VARIANT_TYPE_INT32).unwrap() }
    /// UInt32 type (`u`).
    pub fn uint32() -> Self { Self::new(VARIANT_TYPE_UINT32).unwrap() }
    /// Int64 type (`x`).
    pub fn int64() -> Self { Self::new(VARIANT_TYPE_INT64).unwrap() }
    /// UInt64 type (`t`).
    pub fn uint64() -> Self { Self::new(VARIANT_TYPE_UINT64).unwrap() }
    /// Double type (`d`).
    pub fn double() -> Self { Self::new(VARIANT_TYPE_DOUBLE).unwrap() }
    /// String type (`s`).
    pub fn string() -> Self { Self::new(VARIANT_TYPE_STRING).unwrap() }
    /// Object path type (`o`).
    pub fn object_path() -> Self { Self::new(VARIANT_TYPE_OBJECT_PATH).unwrap() }
    /// Signature type (`g`).
    pub fn signature() -> Self { Self::new(VARIANT_TYPE_SIGNATURE).unwrap() }
    /// Variant type (`v`).
    pub fn variant() -> Self { Self::new(VARIANT_TYPE_VARIANT).unwrap() }
    /// Handle type (`h`).
    pub fn handle() -> Self { Self::new(VARIANT_TYPE_HANDLE).unwrap() }
    /// Any type (`*`).
    pub fn any() -> Self { Self::new(VARIANT_TYPE_ANY).unwrap() }
    /// Basic type (`?`).
    pub fn basic() -> Self { Self::new(VARIANT_TYPE_BASIC).unwrap() }

    /// Copy the type string (`g_variant_type_dup_string`).
    pub fn dup_string(&self) -> String {
        self.type_string.clone()
    }

    /// Classify the variant (`g_variant_classify`).
    pub fn classify(&self) -> Option<VariantClass> {
        VariantClass::from_byte(self.type_string.as_bytes()[0])
    }

    /// Returns `true` if this is a definite type (no wildcards).
    pub fn is_definite(&self) -> bool {
        !self.type_string.contains('*') && !self.type_string.contains('?')
    }

    /// Returns `true` if this is a basic (non-container) type.
    pub fn is_basic(&self) -> bool {
        let b = self.type_string.as_bytes()[0];
        matches!(
            b,
            b'b' | b'y' | b'n' | b'q' | b'i' | b'u' | b'x' | b't'
                | b'h' | b'd' | b's' | b'o' | b'g' | b'?'
        ) && self.type_string.len() == 1
    }

    /// Returns `true` if this is a container type.
    pub fn is_container(&self) -> bool {
        let b = self.type_string.as_bytes()[0];
        matches!(
            b,
            b'v' | b'm' | b'a' | b'(' | b'{' | b'r' | b'*'
        ) && !self.is_basic()
    }

    /// Returns `true` if this is a maybe type.
    pub fn is_maybe(&self) -> bool {
        self.type_string.starts_with('m')
    }

    /// Returns `true` if this is an array type.
    pub fn is_array(&self) -> bool {
        self.type_string.starts_with('a') && !self.type_string.starts_with("a{")
    }

    /// Returns `true` if this is a tuple type.
    pub fn is_tuple(&self) -> bool {
        self.type_string.starts_with('(') || self.type_string == "r"
    }

    /// Returns `true` if this is a dict entry type.
    pub fn is_dict_entry(&self) -> bool {
        self.type_string.starts_with('{') || self.type_string == "{?*}"
    }

    /// Returns `true` if this is a variant type.
    pub fn is_variant(&self) -> bool {
        self.type_string == "v"
    }

    /// Returns the element type of an array or maybe (`g_variant_type_element`).
    pub fn element(&self) -> Option<VariantType> {
        if self.is_array() || self.is_maybe() {
            let inner = &self.type_string[1..];
            Some(VariantType {
                type_string: inner.to_owned(),
            })
        } else {
            None
        }
    }

    /// Returns the first type in a tuple (`g_variant_type_first`).
    pub fn first(&self) -> Option<VariantType> {
        if !self.is_tuple() || self.type_string == "()" || self.type_string == "r" {
            return None;
        }
        let inner = &self.type_string[1..self.type_string.len() - 1];
        if inner.is_empty() {
            return None;
        }
        let (type_str, _) = scan_type_string(inner)?;
        Some(VariantType {
            type_string: type_str.to_owned(),
        })
    }

    /// Returns the number of items in a tuple (`g_variant_type_n_items`).
    pub fn n_items(&self) -> usize {
        if !self.is_tuple() || self.type_string == "()" {
            return 0;
        }
        if self.type_string == "r" {
            return 1; // indefinite tuple
        }
        let inner = &self.type_string[1..self.type_string.len() - 1];
        count_items(inner)
    }

    /// Returns the key type of a dict entry (`g_variant_type_key`).
    pub fn key(&self) -> Option<VariantType> {
        if !self.is_dict_entry() || self.type_string == "{?*}" {
            return None;
        }
        let inner = &self.type_string[1..self.type_string.len() - 1];
        let (key_str, _) = scan_type_string(inner)?;
        Some(VariantType {
            type_string: key_str.to_owned(),
        })
    }

    /// Returns the value type of a dict entry (`g_variant_type_value`).
    pub fn value(&self) -> Option<VariantType> {
        if !self.is_dict_entry() || self.type_string == "{?*}" {
            return None;
        }
        let inner = &self.type_string[1..self.type_string.len() - 1];
        let (_, rest) = scan_type_string(inner)?;
        let (val_str, _) = scan_type_string(rest)?;
        Some(VariantType {
            type_string: val_str.to_owned(),
        })
    }

    /// Create an array type (`g_variant_type_new_array`).
    pub fn new_array(element: &VariantType) -> VariantType {
        VariantType {
            type_string: format!("a{}", element.type_string),
        }
    }

    /// Create a maybe type (`g_variant_type_new_maybe`).
    pub fn new_maybe(element: &VariantType) -> VariantType {
        VariantType {
            type_string: format!("m{}", element.type_string),
        }
    }

    /// Create a tuple type (`g_variant_type_new_tuple`).
    pub fn new_tuple(items: &[VariantType]) -> VariantType {
        let mut s = String::from("(");
        for item in items {
            s.push_str(&item.type_string);
        }
        s.push(')');
        VariantType { type_string: s }
    }

    /// Create a dict entry type (`g_variant_type_new_dict_entry`).
    pub fn new_dict_entry(key: &VariantType, value: &VariantType) -> VariantType {
        VariantType {
            type_string: format!("{{{}{}}}", key.type_string, value.type_string),
        }
    }

    /// Check if this type is a subtype of `supertype` (`g_variant_type_is_subtype_of`).
    pub fn is_subtype_of(&self, supertype: &VariantType) -> bool {
        is_subtype_of(&self.type_string, &supertype.type_string)
    }
}

/// Check if a type string is valid (`g_variant_type_string_is_valid`).
pub fn type_string_is_valid(type_str: &str) -> bool {
    match scan_type_string(type_str) {
        Some((_, rest)) => rest.is_empty(),
        None => false,
    }
}

/// Scan a single type from `string` starting at the beginning.
/// Returns `(matched_type_string, remaining_string)`.
///
/// This is the core recursive parser matching `g_variant_type_string_scan`.
pub fn scan_type_string(string: &str) -> Option<(&str, &str)> {
    let bytes = string.as_bytes();
    if bytes.is_empty() {
        return None;
    }

    match bytes[0] {
        // Basic types
        b'b' | b'y' | b'n' | b'q' | b'i' | b'u' | b'x' | b't' | b'h' | b'd'
        | b's' | b'o' | b'g' | b'v' | b'?' => Some((&string[..1], &string[1..])),

        // Wildcard (any type)
        b'*' => Some((&string[..1], &string[1..])),

        // Maybe: m followed by a type
        b'm' => {
            let (_, rest) = scan_type_string(&string[1..])?;
            // Find how many bytes the inner type consumed
            let inner_len = string.len() - rest.len() - 1;
            Some((&string[..1 + inner_len], rest))
        }

        // Array: a followed by a type
        b'a' => {
            let (_, rest) = scan_type_string(&string[1..])?;
            let inner_len = string.len() - rest.len() - 1;
            Some((&string[..1 + inner_len], rest))
        }

        // Tuple: ( followed by types, then )
        b'(' => {
            let mut pos = 1;
            loop {
                if pos >= bytes.len() {
                    return None; // No closing )
                }
                if bytes[pos] == b')' {
                    pos += 1;
                    break;
                }
                let (matched, rest) = scan_type_string(&string[pos..])?;
                pos += matched.len();
                let _ = rest;
            }
            Some((&string[..pos], &string[pos..]))
        }

        // Dict entry: { key value }
        b'{' => {
            let mut pos = 1;
            // Key
            let (key_matched, _) = scan_type_string(&string[pos..])?;
            pos += key_matched.len();
            if pos >= bytes.len() {
                return None;
            }
            // Value
            let (val_matched, _) = scan_type_string(&string[pos..])?;
            pos += val_matched.len();
            if pos >= bytes.len() || bytes[pos] != b'}' {
                return None;
            }
            pos += 1;
            Some((&string[..pos], &string[pos..]))
        }

        // Indefinite tuple
        b'r' => Some((&string[..1], &string[1..])),

        _ => None,
    }
}

/// Count the number of items in a tuple's inner string.
fn count_items(inner: &str) -> usize {
    let mut count = 0;
    let mut rest = inner;
    while !rest.is_empty() {
        match scan_type_string(rest) {
            Some((matched, remaining)) => {
                count += 1;
                rest = remaining;
                let _ = matched;
            }
            None => break,
        }
    }
    count
}

/// Check subtype relationship.
fn is_subtype_of(type_str: &str, supertype_str: &str) -> bool {
    if supertype_str == "*" {
        return true;
    }
    if type_str == supertype_str {
        return true;
    }

    let type_bytes = type_str.as_bytes();
    let super_bytes = supertype_str.as_bytes();

    if super_bytes.is_empty() || type_bytes.is_empty() {
        return false;
    }

    // Basic wildcard
    if supertype_str == "?" {
        return matches!(
            type_bytes[0],
            b'b' | b'y' | b'n' | b'q' | b'i' | b'u' | b'x' | b't'
                | b'h' | b'd' | b's' | b'o' | b'g'
        ) && type_str.len() == 1;
    }

    // Indefinite tuple matches any tuple
    if supertype_str == "r" {
        return type_bytes[0] == b'(';
    }

    // Indefinite dict entry
    if supertype_str == "{?*}" {
        return type_bytes[0] == b'{';
    }

    // Indefinite maybe
    if supertype_str.starts_with("m*") && type_str.starts_with('m') {
        return is_subtype_of(&type_str[1..], "*");
    }

    // Indefinite array
    if supertype_str.starts_with("a*") && type_str.starts_with('a') {
        return is_subtype_of(&type_str[1..], "*");
    }

    // Indefinite dictionary
    if supertype_str == "a{?*}" && type_str.starts_with("a{") {
        return true;
    }

    // For definite types, check recursively
    if type_bytes[0] == super_bytes[0] {
        match type_bytes[0] {
            b'm' | b'a' => is_subtype_of(&type_str[1..], &supertype_str[1..]),
            b'(' => {
                // Both are tuples; check each element
                let type_inner = &type_str[1..type_str.len() - 1];
                let super_inner = &supertype_str[1..supertype_str.len() - 1];
                let mut t_rest = type_inner;
                let mut s_rest = super_inner;
                loop {
                    if s_rest.is_empty() {
                        return t_rest.is_empty();
                    }
                    if t_rest.is_empty() {
                        return false;
                    }
                    let (t_matched, t_remaining) = match scan_type_string(t_rest) {
                        Some(x) => x,
                        None => return false,
                    };
                    let (s_matched, s_remaining) = match scan_type_string(s_rest) {
                        Some(x) => x,
                        None => return false,
                    };
                    if !is_subtype_of(t_matched, s_matched) {
                        return false;
                    }
                    t_rest = t_remaining;
                    s_rest = s_remaining;
                }
            }
            b'{' => {
                let type_inner = &type_str[1..type_str.len() - 1];
                let super_inner = &supertype_str[1..supertype_str.len() - 1];
                let (t_key, t_val_rest) = match scan_type_string(type_inner) {
                    Some(x) => x,
                    None => return false,
                };
                let (t_val, _) = match scan_type_string(t_val_rest) {
                    Some(x) => x,
                    None => return false,
                };
                let (s_key, s_val_rest) = match scan_type_string(super_inner) {
                    Some(x) => x,
                    None => return false,
                };
                let (s_val, _) = match scan_type_string(s_val_rest) {
                    Some(x) => x,
                    None => return false,
                };
                is_subtype_of(t_key, s_key) && is_subtype_of(t_val, s_val)
            }
            _ => type_str == supertype_str,
        }
    } else {
        false
    }
}

/// Check if two type strings are equal (`g_variant_type_equal`).
pub fn type_equal(type1: &str, type2: &str) -> bool {
    type1 == type2
}

/// Hash a type string (`g_variant_type_hash`).
pub fn type_hash(type_str: &str) -> u32 {
    let mut hash: u32 = 5381;
    for &b in type_str.as_bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(b as u32);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_basic_types() {
        assert!(type_string_is_valid("b"));
        assert!(type_string_is_valid("s"));
        assert!(type_string_is_valid("i"));
        assert!(type_string_is_valid("d"));
        assert!(type_string_is_valid("v"));
    }

    #[test]
    fn valid_container_types() {
        assert!(type_string_is_valid("as"));
        assert!(type_string_is_valid("ms"));
        assert!(type_string_is_valid("(ii)"));
        assert!(type_string_is_valid("{sv}"));
        assert!(type_string_is_valid("a{sv}"));
        assert!(type_string_is_valid("(i(ii)s)"));
        assert!(type_string_is_valid("aas"));
        assert!(type_string_is_valid("m(iis)"));
    }

    #[test]
    fn invalid_types() {
        assert!(!type_string_is_valid(""));
        assert!(!type_string_is_valid("("));
        assert!(!type_string_is_valid(")"));
        assert!(!type_string_is_valid("{"));
        assert!(!type_string_is_valid("{}"));
        assert!(!type_string_is_valid("{s}"));
        assert!(!type_string_is_valid("z"));
    }

    #[test]
    fn classification() {
        let t = VariantType::new("b").unwrap();
        assert!(t.is_basic());
        assert!(!t.is_container());
        assert!(t.is_definite());

        let t = VariantType::new("as").unwrap();
        assert!(t.is_array());
        assert!(t.is_container());
        assert!(!t.is_basic());

        let t = VariantType::new("ms").unwrap();
        assert!(t.is_maybe());

        let t = VariantType::new("(ii)").unwrap();
        assert!(t.is_tuple());
        assert_eq!(t.n_items(), 2);

        let t = VariantType::new("{sv}").unwrap();
        assert!(t.is_dict_entry());
    }

    #[test]
    fn element_type() {
        let t = VariantType::new("as").unwrap();
        let e = t.element().unwrap();
        assert_eq!(e.as_str(), "s");

        let t = VariantType::new("ms").unwrap();
        let e = t.element().unwrap();
        assert_eq!(e.as_str(), "s");
    }

    #[test]
    fn dict_key_value() {
        let t = VariantType::new("{sv}").unwrap();
        let k = t.key().unwrap();
        let v = t.value().unwrap();
        assert_eq!(k.as_str(), "s");
        assert_eq!(v.as_str(), "v");
    }

    #[test]
    fn constructors() {
        let elem = VariantType::new("s").unwrap();
        let arr = VariantType::new_array(&elem);
        assert_eq!(arr.as_str(), "as");

        let may = VariantType::new_maybe(&elem);
        assert_eq!(may.as_str(), "ms");

        let items = vec![
            VariantType::new("i").unwrap(),
            VariantType::new("s").unwrap(),
        ];
        let tup = VariantType::new_tuple(&items);
        assert_eq!(tup.as_str(), "(is)");

        let key = VariantType::new("s").unwrap();
        let val = VariantType::new("v").unwrap();
        let de = VariantType::new_dict_entry(&key, &val);
        assert_eq!(de.as_str(), "{sv}");
    }

    #[test]
    fn subtype_check() {
        assert!(VariantType::new("b").unwrap().is_subtype_of(&VariantType::new("?").unwrap()));
        assert!(VariantType::new("s").unwrap().is_subtype_of(&VariantType::new("*").unwrap()));
        assert!(VariantType::new("(ii)").unwrap().is_subtype_of(&VariantType::new("r").unwrap()));
        assert!(VariantType::new("as").unwrap().is_subtype_of(&VariantType::new("a*").unwrap()));
        assert!(VariantType::new("a{sv}").unwrap().is_subtype_of(&VariantType::new("a{?*}").unwrap()));
        assert!(!VariantType::new("b").unwrap().is_subtype_of(&VariantType::new("s").unwrap()));
    }

    #[test]
    fn classify() {
        assert_eq!(
            VariantType::new("b").unwrap().classify(),
            Some(VariantClass::Boolean)
        );
        assert_eq!(
            VariantType::new("as").unwrap().classify(),
            Some(VariantClass::Array)
        );
    }

    #[test]
    fn hash_and_equal() {
        assert!(type_equal("s", "s"));
        assert!(!type_equal("s", "i"));
        let h1 = type_hash("s");
        let h2 = type_hash("s");
        assert_eq!(h1, h2);
        let h3 = type_hash("i");
        assert_ne!(h1, h3);
    }

    #[test]
    fn tuple_first() {
        let t = VariantType::new("(isb)").unwrap();
        let f = t.first().unwrap();
        assert_eq!(f.as_str(), "i");
        assert_eq!(t.n_items(), 3);
    }
}
