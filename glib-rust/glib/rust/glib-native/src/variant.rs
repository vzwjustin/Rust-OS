//! GVariant value container matching `gvariant.h` / `gvariant.c`.
//!
//! A typed, immutable value container supporting all GVariant type classes.
//! Uses `Clone` for cheap copying. Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use crate::varianttype::VariantClass;
use crate::varianttype::VariantType;

/// The inner value stored in a GVariant.
#[derive(Clone, Debug)]
enum VariantValue {
    Boolean(bool),
    Byte(u8),
    Int16(i16),
    Uint16(u16),
    Int32(i32),
    Uint32(u32),
    Int64(i64),
    Uint64(u64),
    Handle(i32),
    Double(f64),
    String(String),
    ObjectPath(String),
    Signature(String),
    Variant(Box<Variant>),
    Maybe(Option<Box<Variant>>),
    Array(VariantType, Vec<Variant>),
    Tuple(Vec<Variant>),
    DictEntry(Box<Variant>, Box<Variant>),
}

/// A GVariant value (`GVariant`).
///
/// Immutable, reference-counted, typed value. Created via `new_*` constructors.
/// Use `classify`, `get_*`, and `n_children` to inspect.
#[derive(Clone, Debug)]
pub struct Variant {
    type_string: String,
    value: VariantValue,
}

impl Variant {
    /// Get the type string (`g_variant_get_type_string`).
    pub fn type_string(&self) -> &str {
        &self.type_string
    }

    /// Get the type (`g_variant_get_type`).
    pub fn type_(&self) -> VariantType {
        VariantType::new(&self.type_string).unwrap_or(VariantType::any())
    }

    /// Classify the variant (`g_variant_classify`).
    pub fn classify(&self) -> VariantClass {
        VariantClass::from_byte(self.type_string.as_bytes()[0]).unwrap_or(VariantClass::Variant)
    }

    /// Check if the variant is of a given type (`g_variant_is_of_type`).
    pub fn is_of_type(&self, type_: &VariantType) -> bool {
        self.type_string == type_.type_string()
    }

    /// Check if the variant is a container (`g_variant_is_container`).
    pub fn is_container(&self) -> bool {
        matches!(
            self.value,
            VariantValue::Array(..)
                | VariantValue::Tuple(..)
                | VariantValue::DictEntry(..)
                | VariantValue::Maybe(..)
                | VariantValue::Variant(..)
        )
    }

    // --- Constructors ---

    /// Create a boolean variant (`g_variant_new_boolean`).
    pub fn new_boolean(value: bool) -> Self {
        Self {
            type_string: "b".to_owned(),
            value: VariantValue::Boolean(value),
        }
    }

    /// Create a byte variant (`g_variant_new_byte`).
    pub fn new_byte(value: u8) -> Self {
        Self {
            type_string: "y".to_owned(),
            value: VariantValue::Byte(value),
        }
    }

    /// Create an int16 variant (`g_variant_new_int16`).
    pub fn new_int16(value: i16) -> Self {
        Self {
            type_string: "n".to_owned(),
            value: VariantValue::Int16(value),
        }
    }

    /// Create a uint16 variant (`g_variant_new_uint16`).
    pub fn new_uint16(value: u16) -> Self {
        Self {
            type_string: "q".to_owned(),
            value: VariantValue::Uint16(value),
        }
    }

    /// Create an int32 variant (`g_variant_new_int32`).
    pub fn new_int32(value: i32) -> Self {
        Self {
            type_string: "i".to_owned(),
            value: VariantValue::Int32(value),
        }
    }

    /// Create a uint32 variant (`g_variant_new_uint32`).
    pub fn new_uint32(value: u32) -> Self {
        Self {
            type_string: "u".to_owned(),
            value: VariantValue::Uint32(value),
        }
    }

    /// Create an int64 variant (`g_variant_new_int64`).
    pub fn new_int64(value: i64) -> Self {
        Self {
            type_string: "x".to_owned(),
            value: VariantValue::Int64(value),
        }
    }

    /// Create a uint64 variant (`g_variant_new_uint64`).
    pub fn new_uint64(value: u64) -> Self {
        Self {
            type_string: "t".to_owned(),
            value: VariantValue::Uint64(value),
        }
    }

    /// Create a handle variant (`g_variant_new_handle`).
    pub fn new_handle(value: i32) -> Self {
        Self {
            type_string: "h".to_owned(),
            value: VariantValue::Handle(value),
        }
    }

    /// Create a double variant (`g_variant_new_double`).
    pub fn new_double(value: f64) -> Self {
        Self {
            type_string: "d".to_owned(),
            value: VariantValue::Double(value),
        }
    }

    /// Create a string variant (`g_variant_new_string`).
    pub fn new_string(value: &str) -> Self {
        Self {
            type_string: "s".to_owned(),
            value: VariantValue::String(value.to_owned()),
        }
    }

    /// Create an object path variant (`g_variant_new_object_path`).
    pub fn new_object_path(value: &str) -> Self {
        Self {
            type_string: "o".to_owned(),
            value: VariantValue::ObjectPath(value.to_owned()),
        }
    }

    /// Create a signature variant (`g_variant_new_signature`).
    pub fn new_signature(value: &str) -> Self {
        Self {
            type_string: "g".to_owned(),
            value: VariantValue::Signature(value.to_owned()),
        }
    }

    /// Create a variant containing another variant (`g_variant_new_variant`).
    pub fn new_variant(value: Variant) -> Self {
        Self {
            type_string: "v".to_owned(),
            value: VariantValue::Variant(Box::new(value)),
        }
    }

    /// Create a string array variant (`g_variant_new_strv`).
    pub fn new_strv(strv: &[&str]) -> Self {
        let children: Vec<Variant> = strv.iter().map(|s| Variant::new_string(s)).collect();
        Self {
            type_string: "as".to_owned(),
            value: VariantValue::Array(VariantType::string(), children),
        }
    }

    /// Create a maybe variant (`g_variant_new_maybe`).
    pub fn new_maybe(child_type: &VariantType, child: Option<Variant>) -> Self {
        let type_string = format!("m{}", child_type.type_string());
        Self {
            type_string,
            value: VariantValue::Maybe(child.map(Box::new)),
        }
    }

    /// Create an array variant (`g_variant_new_array`).
    pub fn new_array(child_type: &VariantType, children: Vec<Variant>) -> Self {
        let type_string = format!("a{}", child_type.type_string());
        Self {
            type_string,
            value: VariantValue::Array(child_type.clone(), children),
        }
    }

    /// Create a tuple variant (`g_variant_new_tuple`).
    pub fn new_tuple(children: Vec<Variant>) -> Self {
        let type_string = format!(
            "({})",
            children
                .iter()
                .map(|c| c.type_string().to_owned())
                .collect::<String>()
        );
        Self {
            type_string,
            value: VariantValue::Tuple(children),
        }
    }

    /// Create a dict entry variant (`g_variant_new_dict_entry`).
    pub fn new_dict_entry(key: Variant, value: Variant) -> Self {
        let type_string = format!("{{{}{}}}", key.type_string(), value.type_string());
        Self {
            type_string,
            value: VariantValue::DictEntry(Box::new(key), Box::new(value)),
        }
    }

    // --- Getters ---

    /// Get boolean value (`g_variant_get_boolean`).
    pub fn get_boolean(&self) -> bool {
        match &self.value {
            VariantValue::Boolean(b) => *b,
            _ => false,
        }
    }

    /// Get byte value (`g_variant_get_byte`).
    pub fn get_byte(&self) -> u8 {
        match &self.value {
            VariantValue::Byte(b) => *b,
            _ => 0,
        }
    }

    /// Get int16 value (`g_variant_get_int16`).
    pub fn get_int16(&self) -> i16 {
        match &self.value {
            VariantValue::Int16(v) => *v,
            _ => 0,
        }
    }

    /// Get uint16 value (`g_variant_get_uint16`).
    pub fn get_uint16(&self) -> u16 {
        match &self.value {
            VariantValue::Uint16(v) => *v,
            _ => 0,
        }
    }

    /// Get int32 value (`g_variant_get_int32`).
    pub fn get_int32(&self) -> i32 {
        match &self.value {
            VariantValue::Int32(v) => *v,
            _ => 0,
        }
    }

    /// Get uint32 value (`g_variant_get_uint32`).
    pub fn get_uint32(&self) -> u32 {
        match &self.value {
            VariantValue::Uint32(v) => *v,
            _ => 0,
        }
    }

    /// Get int64 value (`g_variant_get_int64`).
    pub fn get_int64(&self) -> i64 {
        match &self.value {
            VariantValue::Int64(v) => *v,
            _ => 0,
        }
    }

    /// Get uint64 value (`g_variant_get_uint64`).
    pub fn get_uint64(&self) -> u64 {
        match &self.value {
            VariantValue::Uint64(v) => *v,
            _ => 0,
        }
    }

    /// Get handle value (`g_variant_get_handle`).
    pub fn get_handle(&self) -> i32 {
        match &self.value {
            VariantValue::Handle(v) => *v,
            _ => 0,
        }
    }

    /// Get double value (`g_variant_get_double`).
    pub fn get_double(&self) -> f64 {
        match &self.value {
            VariantValue::Double(v) => *v,
            _ => 0.0,
        }
    }

    /// Get string value (`g_variant_get_string`).
    pub fn get_string(&self) -> &str {
        match &self.value {
            VariantValue::String(s) | VariantValue::ObjectPath(s) | VariantValue::Signature(s) => s,
            _ => "",
        }
    }

    /// Get the inner variant (`g_variant_get_variant`).
    pub fn get_variant(&self) -> Option<&Variant> {
        match &self.value {
            VariantValue::Variant(v) => Some(v),
            _ => None,
        }
    }

    /// Get the maybe child (`g_variant_get_maybe`).
    pub fn get_maybe(&self) -> Option<&Variant> {
        match &self.value {
            VariantValue::Maybe(opt) => opt.as_ref().map(|v| &**v),
            _ => None,
        }
    }

    /// Get number of children (`g_variant_n_children`).
    pub fn n_children(&self) -> usize {
        match &self.value {
            VariantValue::Array(_, children) => children.len(),
            VariantValue::Tuple(children) => children.len(),
            VariantValue::DictEntry(_, _) => 2,
            VariantValue::Variant(v) => v.n_children(),
            VariantValue::Maybe(Some(v)) => v.n_children(),
            _ => 0,
        }
    }

    /// Get a child by index (`g_variant_get_child_value`).
    pub fn get_child_value(&self, index: usize) -> Option<Variant> {
        match &self.value {
            VariantValue::Array(_, children) => children.get(index).cloned(),
            VariantValue::Tuple(children) => children.get(index).cloned(),
            VariantValue::DictEntry(k, v) => {
                if index == 0 {
                    Some((**k).clone())
                } else if index == 1 {
                    Some((**v).clone())
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Get string array (`g_variant_get_strv`).
    pub fn get_strv(&self) -> Vec<String> {
        match &self.value {
            VariantValue::Array(_, children) => {
                children.iter().map(|c| c.get_string().to_owned()).collect()
            }
            _ => Vec::new(),
        }
    }

    /// Print the variant to a string (`g_variant_print`).
    pub fn print(&self, type_annotate: bool) -> String {
        match &self.value {
            VariantValue::Boolean(b) => {
                if type_annotate {
                    format!("{}<{}>", "b", if *b { "true" } else { "false" })
                } else if *b {
                    "true".to_owned()
                } else {
                    "false".to_owned()
                }
            }
            VariantValue::Byte(b) => {
                if type_annotate {
                    format!("y<0x{:02x}>", b)
                } else {
                    format!("0x{:02x}", b)
                }
            }
            VariantValue::Int16(v) => {
                if type_annotate {
                    format!("n<{}>", v)
                } else {
                    format!("{}", v)
                }
            }
            VariantValue::Uint16(v) => {
                if type_annotate {
                    format!("q<{}>", v)
                } else {
                    format!("{}", v)
                }
            }
            VariantValue::Int32(v) => {
                if type_annotate {
                    format!("i<{}>", v)
                } else {
                    format!("{}", v)
                }
            }
            VariantValue::Uint32(v) => {
                if type_annotate {
                    format!("u<{}>", v)
                } else {
                    format!("{}", v)
                }
            }
            VariantValue::Int64(v) => {
                if type_annotate {
                    format!("x<{}>", v)
                } else {
                    format!("{}", v)
                }
            }
            VariantValue::Uint64(v) => {
                if type_annotate {
                    format!("t<{}>", v)
                } else {
                    format!("{}", v)
                }
            }
            VariantValue::Handle(v) => {
                if type_annotate {
                    format!("h<{}>", v)
                } else {
                    format!("{}", v)
                }
            }
            VariantValue::Double(v) => {
                if type_annotate {
                    format!("d<{}>", v)
                } else {
                    format!("{}", v)
                }
            }
            VariantValue::String(s) => {
                if type_annotate {
                    format!("s<'{}'>", s)
                } else {
                    format!("'{}'", s)
                }
            }
            VariantValue::ObjectPath(s) => {
                if type_annotate {
                    format!("o<{}>", s)
                } else {
                    format!("'{}'", s)
                }
            }
            VariantValue::Signature(s) => {
                if type_annotate {
                    format!("g<'{}'>", s)
                } else {
                    format!("'{}'", s)
                }
            }
            VariantValue::Variant(v) => {
                format!("<{}>", v.print(type_annotate))
            }
            VariantValue::Maybe(Some(v)) => {
                format!("just {}", v.print(type_annotate))
            }
            VariantValue::Maybe(None) => "nothing".to_owned(),
            VariantValue::Array(_, children) => {
                let items: Vec<String> = children.iter().map(|c| c.print(type_annotate)).collect();
                format!("[{}]", items.join(", "))
            }
            VariantValue::Tuple(children) => {
                let items: Vec<String> = children.iter().map(|c| c.print(type_annotate)).collect();
                format!("({})", items.join(", "))
            }
            VariantValue::DictEntry(k, v) => {
                format!("{{{}, {}}}", k.print(type_annotate), v.print(type_annotate))
            }
        }
    }

    /// Compare two variants (`g_variant_equal`).
    pub fn equal(&self, other: &Variant) -> bool {
        self.print(false) == other.print(false)
    }

    /// Check if a string is a valid object path (`g_variant_is_object_path`).
    pub fn is_object_path(string: &str) -> bool {
        if string.is_empty() {
            return false;
        }
        if string == "/" {
            return true;
        }
        if !string.starts_with('/') {
            return false;
        }
        if string.ends_with('/') {
            return false;
        }
        // No empty components (no //)
        let mut prev_was_slash = true;
        for c in string.bytes().skip(1) {
            if c == b'/' {
                if prev_was_slash {
                    return false;
                }
                prev_was_slash = true;
            } else {
                if !c.is_ascii_alphanumeric() && c != b'_' {
                    return false;
                }
                prev_was_slash = false;
            }
        }
        true
    }

    /// Check if a string is a valid signature (`g_variant_is_signature`).
    pub fn is_signature(string: &str) -> bool {
        if string.is_empty() || string.len() > 255 {
            return false;
        }
        // A signature is a sequence of zero or more complete type strings.
        let mut rest = string;
        while !rest.is_empty() {
            match crate::varianttype::scan_type_string(rest) {
                Some((_, remaining)) => rest = remaining,
                None => return false,
            }
        }
        true
    }
}

/// A variant builder (`GVariantBuilder`).
pub struct VariantBuilder {
    type_string: String,
    children: Vec<Variant>,
    open_stack: Vec<(String, Vec<Variant>)>,
}

impl VariantBuilder {
    /// Create a new builder (`g_variant_builder_new`).
    pub fn new(type_: &VariantType) -> Self {
        Self {
            type_string: type_.type_string().to_owned(),
            children: Vec::new(),
            open_stack: Vec::new(),
        }
    }

    /// Add a value (`g_variant_builder_add_value`).
    pub fn add_value(&mut self, value: Variant) {
        if let Some((_, children)) = self.open_stack.last_mut() {
            children.push(value);
        } else {
            self.children.push(value);
        }
    }

    /// Open a container (`g_variant_builder_open`).
    pub fn open(&mut self, type_: &VariantType) {
        let ts = type_.type_string().to_owned();
        self.open_stack.push((ts, Vec::new()));
    }

    /// Close the current container (`g_variant_builder_close`).
    pub fn close(&mut self) {
        if let Some((ts, children)) = self.open_stack.pop() {
            let v = if ts.starts_with('a') {
                let child_type = VariantType::new(&ts[1..]).unwrap_or(VariantType::any());
                Variant::new_array(&child_type, children)
            } else if ts.starts_with('(') {
                Variant::new_tuple(children)
            } else if ts.starts_with('m') {
                let child_type = VariantType::new(&ts[1..]).unwrap_or(VariantType::any());
                Variant::new_maybe(&child_type, children.into_iter().next())
            } else {
                Variant::new_tuple(children)
            };
            self.add_value(v);
        }
    }

    /// Finish building (`g_variant_builder_end`).
    pub fn end(self) -> Variant {
        if self.type_string.starts_with('a') {
            let child_type = VariantType::new(&self.type_string[1..]).unwrap_or(VariantType::any());
            Variant::new_array(&child_type, self.children)
        } else if self.type_string.starts_with('(') {
            Variant::new_tuple(self.children)
        } else if self.type_string.starts_with('m') {
            let child_type = VariantType::new(&self.type_string[1..]).unwrap_or(VariantType::any());
            Variant::new_maybe(&child_type, self.children.into_iter().next())
        } else if self.type_string == "v" {
            Variant::new_variant(
                self.children
                    .into_iter()
                    .next()
                    .unwrap_or(Variant::new_boolean(false)),
            )
        } else {
            self.children
                .into_iter()
                .next()
                .unwrap_or(Variant::new_boolean(false))
        }
    }
}

/// Variant parse error codes (`GVariantParseError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum VariantParseError {
    Failed,
    BasicTypeExpected,
    CannotInferType,
    DefiniteTypeExpected,
    InputNotAtEnd,
    InvalidCharacter,
    InvalidFormatString,
    InvalidObjectPath,
    InvalidSignature,
    InvalidTypeString,
    NoCommonType,
    NumberOutOfRange,
    NumberTooBig,
    TypeError,
    UnexpectedToken,
    UnknownKeyword,
    UnterminatedStringConstant,
    ValueExpected,
    Recursion,
}

/// Parse a text GVariant (`g_variant_parse`).
///
/// Supports basic types, strings, arrays, tuples, and nested variants.
pub fn parse(type_: &VariantType, text: &str) -> Result<Variant, VariantParseError> {
    let trimmed = text.trim();
    parse_value(type_, trimmed)
}

fn parse_value(type_: &VariantType, text: &str) -> Result<Variant, VariantParseError> {
    let ts = type_.type_string();
    let trimmed = text.trim();

    if ts == "b" {
        match trimmed {
            "true" => Ok(Variant::new_boolean(true)),
            "false" => Ok(Variant::new_boolean(false)),
            _ => Err(VariantParseError::ValueExpected),
        }
    } else if ts == "y" {
        let v = trimmed
            .strip_prefix("0x")
            .map(|h| u8::from_str_radix(h, 16))
            .unwrap_or_else(|| trimmed.parse::<u8>())
            .map_err(|_| VariantParseError::NumberOutOfRange)?;
        Ok(Variant::new_byte(v))
    } else if ts == "n" {
        let v = trimmed
            .parse::<i16>()
            .map_err(|_| VariantParseError::NumberOutOfRange)?;
        Ok(Variant::new_int16(v))
    } else if ts == "q" {
        let v = trimmed
            .parse::<u16>()
            .map_err(|_| VariantParseError::NumberOutOfRange)?;
        Ok(Variant::new_uint16(v))
    } else if ts == "i" {
        let v = trimmed
            .parse::<i32>()
            .map_err(|_| VariantParseError::NumberOutOfRange)?;
        Ok(Variant::new_int32(v))
    } else if ts == "u" {
        let v = trimmed
            .parse::<u32>()
            .map_err(|_| VariantParseError::NumberOutOfRange)?;
        Ok(Variant::new_uint32(v))
    } else if ts == "x" {
        let v = trimmed
            .parse::<i64>()
            .map_err(|_| VariantParseError::NumberOutOfRange)?;
        Ok(Variant::new_int64(v))
    } else if ts == "t" {
        let v = trimmed
            .parse::<u64>()
            .map_err(|_| VariantParseError::NumberOutOfRange)?;
        Ok(Variant::new_uint64(v))
    } else if ts == "d" {
        let v = trimmed
            .parse::<f64>()
            .map_err(|_| VariantParseError::NumberOutOfRange)?;
        Ok(Variant::new_double(v))
    } else if ts == "s" || ts == "o" || ts == "g" {
        let s = trimmed
            .strip_prefix('\'')
            .and_then(|s| s.strip_suffix('\''))
            .or_else(|| trimmed.strip_prefix('"').and_then(|s| s.strip_suffix('"')))
            .ok_or(VariantParseError::UnterminatedStringConstant)?;
        if ts == "s" {
            Ok(Variant::new_string(s))
        } else if ts == "o" {
            Ok(Variant::new_object_path(s))
        } else {
            Ok(Variant::new_signature(s))
        }
    } else if ts == "v" {
        let inner = trimmed
            .strip_prefix('<')
            .and_then(|s| s.strip_suffix('>'))
            .ok_or(VariantParseError::ValueExpected)?;
        let inner_v = parse_value(&VariantType::any(), inner)?;
        Ok(Variant::new_variant(inner_v))
    } else if ts.starts_with('a') && !ts.starts_with("a{") {
        let child_type = VariantType::new(&ts[1..]).ok_or(VariantParseError::InvalidTypeString)?;
        let inner = trimmed
            .strip_prefix('[')
            .and_then(|s| s.strip_suffix(']'))
            .ok_or(VariantParseError::ValueExpected)?;
        let items: Vec<&str> = if inner.trim().is_empty() {
            Vec::new()
        } else {
            split_top_level(inner)
        };
        let children: Vec<Variant> = items
            .iter()
            .map(|s| parse_value(&child_type, s.trim()))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Variant::new_array(&child_type, children))
    } else if ts.starts_with('(') {
        let inner = trimmed
            .strip_prefix('(')
            .and_then(|s| s.strip_suffix(')'))
            .ok_or(VariantParseError::ValueExpected)?;
        let items: Vec<&str> = if inner.trim().is_empty() {
            Vec::new()
        } else {
            split_top_level(inner)
        };
        let child_types = type_.children();
        let children: Vec<Variant> = items
            .iter()
            .enumerate()
            .map(|(i, s)| {
                let fallback = VariantType::any();
                let ct = child_types.get(i).unwrap_or(&fallback);
                parse_value(ct, s.trim())
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Variant::new_tuple(children))
    } else if ts == "*" || ts == "?" {
        // Infer type from value
        if trimmed == "true" {
            return Ok(Variant::new_boolean(true));
        }
        if trimmed == "false" {
            return Ok(Variant::new_boolean(false));
        }
        if trimmed.starts_with('\'') || trimmed.starts_with('"') {
            return parse_value(&VariantType::string(), trimmed);
        }
        if trimmed.starts_with('<') {
            return parse_value(&VariantType::variant(), trimmed);
        }
        if trimmed.starts_with('[') {
            return parse_value(&VariantType::new("as").unwrap(), trimmed);
        }
        if trimmed.starts_with('(') {
            return parse_value(&VariantType::new("()").unwrap(), trimmed);
        }
        // Try integer (default i32)
        if let Ok(v) = trimmed.parse::<i32>() {
            return Ok(Variant::new_int32(v));
        }
        if let Ok(v) = trimmed.parse::<i64>() {
            return Ok(Variant::new_int64(v));
        }
        if let Ok(v) = trimmed.parse::<f64>() {
            return Ok(Variant::new_double(v));
        }
        Err(VariantParseError::Failed)
    } else {
        Err(VariantParseError::Failed)
    }
}

fn split_top_level(s: &str) -> Vec<&str> {
    let mut result = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0;
    let bytes = s.as_bytes();
    let mut in_string = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_string {
            if c == b'\'' || c == b'"' {
                in_string = false;
            }
        } else {
            match c {
                b'\'' | b'"' => in_string = true,
                b'[' | b'(' | b'{' | b'<' => depth += 1,
                b']' | b')' | b'}' | b'>' => depth -= 1,
                b',' if depth == 0 => {
                    result.push(&s[start..i]);
                    start = i + 1;
                }
                _ => {}
            }
        }
        i += 1;
    }
    if start < s.len() {
        result.push(&s[start..]);
    }
    result
}

/// Get the variant parse error quark (`g_variant_parse_error_quark`).
pub fn variant_parse_error_quark() -> u32 {
    31
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn boolean_variant() {
        let v = Variant::new_boolean(true);
        assert_eq!(v.classify(), VariantClass::Boolean);
        assert_eq!(v.get_boolean(), true);
        assert_eq!(v.type_string(), "b");
    }

    #[test]
    fn int32_variant() {
        let v = Variant::new_int32(42);
        assert_eq!(v.classify(), VariantClass::Int32);
        assert_eq!(v.get_int32(), 42);
    }

    #[test]
    fn string_variant() {
        let v = Variant::new_string("hello");
        assert_eq!(v.classify(), VariantClass::String);
        assert_eq!(v.get_string(), "hello");
    }

    #[test]
    fn variant_in_variant() {
        let inner = Variant::new_int32(42);
        let outer = Variant::new_variant(inner);
        assert_eq!(outer.classify(), VariantClass::Variant);
        assert_eq!(outer.get_variant().unwrap().get_int32(), 42);
    }

    #[test]
    fn array_variant() {
        let v = Variant::new_strv(&["a", "b", "c"]);
        assert_eq!(v.classify(), VariantClass::Array);
        assert_eq!(v.n_children(), 3);
        assert_eq!(
            v.get_strv(),
            vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]
        );
    }

    #[test]
    fn tuple_variant() {
        let v = Variant::new_tuple(vec![Variant::new_string("hello"), Variant::new_int32(42)]);
        assert_eq!(v.classify(), VariantClass::Tuple);
        assert_eq!(v.n_children(), 2);
        assert_eq!(v.get_child_value(0).unwrap().get_string(), "hello");
        assert_eq!(v.get_child_value(1).unwrap().get_int32(), 42);
    }

    #[test]
    fn dict_entry_variant() {
        let v = Variant::new_dict_entry(Variant::new_string("key"), Variant::new_int32(42));
        assert_eq!(v.classify(), VariantClass::DictEntry);
        assert_eq!(v.n_children(), 2);
        assert_eq!(v.get_child_value(0).unwrap().get_string(), "key");
        assert_eq!(v.get_child_value(1).unwrap().get_int32(), 42);
    }

    #[test]
    fn maybe_variant() {
        let v = Variant::new_maybe(&VariantType::int32(), Some(Variant::new_int32(42)));
        assert_eq!(v.classify(), VariantClass::Maybe);
        assert_eq!(v.get_maybe().unwrap().get_int32(), 42);

        let none = Variant::new_maybe(&VariantType::int32(), None);
        assert!(none.get_maybe().is_none());
    }

    #[test]
    fn print_boolean() {
        assert_eq!(Variant::new_boolean(true).print(false), "true");
        assert_eq!(Variant::new_boolean(false).print(false), "false");
        assert_eq!(Variant::new_boolean(true).print(true), "b<true>");
    }

    #[test]
    fn print_string() {
        assert_eq!(Variant::new_string("hello").print(false), "'hello'");
    }

    #[test]
    fn print_array() {
        let v = Variant::new_strv(&["a", "b"]);
        assert_eq!(v.print(false), "['a', 'b']");
    }

    #[test]
    fn print_tuple() {
        let v = Variant::new_tuple(vec![Variant::new_int32(1), Variant::new_string("x")]);
        assert_eq!(v.print(false), "(1, 'x')");
    }

    #[test]
    fn equal_variants() {
        let a = Variant::new_int32(42);
        let b = Variant::new_int32(42);
        let c = Variant::new_int32(43);
        assert!(a.equal(&b));
        assert!(!a.equal(&c));
    }

    #[test]
    fn is_object_path() {
        assert!(Variant::is_object_path("/"));
        assert!(Variant::is_object_path("/foo"));
        assert!(Variant::is_object_path("/foo/bar"));
        assert!(!Variant::is_object_path(""));
        assert!(!Variant::is_object_path("foo"));
        assert!(!Variant::is_object_path("/foo/"));
        assert!(!Variant::is_object_path("//foo"));
    }

    #[test]
    fn is_signature() {
        assert!(Variant::is_signature("i"));
        assert!(Variant::is_signature("si"));
        assert!(!Variant::is_signature(""));
        assert!(!Variant::is_signature("z"));
    }

    #[test]
    fn builder_array() {
        let mut builder = VariantBuilder::new(&VariantType::new("as").unwrap());
        builder.add_value(Variant::new_string("a"));
        builder.add_value(Variant::new_string("b"));
        let v = builder.end();
        assert_eq!(v.classify(), VariantClass::Array);
        assert_eq!(v.n_children(), 2);
    }

    #[test]
    fn builder_tuple() {
        let mut builder = VariantBuilder::new(&VariantType::new("(si)").unwrap());
        builder.add_value(Variant::new_string("hello"));
        builder.add_value(Variant::new_int32(42));
        let v = builder.end();
        assert_eq!(v.classify(), VariantClass::Tuple);
        assert_eq!(v.get_child_value(0).unwrap().get_string(), "hello");
        assert_eq!(v.get_child_value(1).unwrap().get_int32(), 42);
    }

    #[test]
    fn parse_int32() {
        let v = parse(&VariantType::int32(), "42").unwrap();
        assert_eq!(v.get_int32(), 42);
    }

    #[test]
    fn parse_string() {
        let v = parse(&VariantType::string(), "'hello'").unwrap();
        assert_eq!(v.get_string(), "hello");
    }

    #[test]
    fn parse_boolean() {
        let v = parse(&VariantType::boolean(), "true").unwrap();
        assert_eq!(v.get_boolean(), true);
    }

    #[test]
    fn parse_array() {
        let v = parse(&VariantType::new("as").unwrap(), "['a', 'b', 'c']").unwrap();
        assert_eq!(v.n_children(), 3);
        assert_eq!(
            v.get_strv(),
            vec!["a".to_owned(), "b".to_owned(), "c".to_owned()]
        );
    }

    #[test]
    fn parse_tuple() {
        let v = parse(&VariantType::new("(si)").unwrap(), "('hello', 42)").unwrap();
        assert_eq!(v.get_child_value(0).unwrap().get_string(), "hello");
        assert_eq!(v.get_child_value(1).unwrap().get_int32(), 42);
    }

    #[test]
    fn parse_variant() {
        let v = parse(&VariantType::variant(), "<42>").unwrap();
        assert_eq!(v.classify(), VariantClass::Variant);
        assert_eq!(v.get_variant().unwrap().get_int32(), 42);
    }
}
