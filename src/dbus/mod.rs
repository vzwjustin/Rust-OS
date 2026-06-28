//! D-Bus message bus protocol for RustOS.
//!
//! Implements the core D-Bus wire protocol (message framing, header fields,
//! signature-based body marshaling) and an in-kernel message bus daemon that
//! routes method calls, signals, and replies between connected clients.
//!
//! This is the foundation that GNOME Shell needs for IPC between GNOME
//! services (gnome-shell, gsd, gnome-settings-daemon, etc.).

extern crate alloc;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Constants ───────────────────────────────────────────────────────────

/// D-Bus protocol version (1 for classic D-Bus)
pub const PROTOCOL_VERSION: u8 = 1;

/// Maximum message size (16 MiB — matches dbus-daemon default)
pub const MAX_MESSAGE_SIZE: u32 = 16 * 1024 * 1024;

/// Well-known name of the bus itself
pub const BUS_NAME: &str = "org.freedesktop.DBus";

/// Object path of the bus daemon
pub const BUS_PATH: &str = "/org/freedesktop/DBus";

/// Standard D-Bus property interface
pub const PROPERTIES_IFACE: &str = "org.freedesktop.DBus.Properties";

/// Standard D-Bus introspection interface
pub const INTROSPECTABLE_IFACE: &str = "org.freedesktop.DBus.Introspectable";

// ── Endianness ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endianness {
    Little,
    Big,
}

impl Endianness {
    pub fn marker(self) -> u8 {
        match self {
            Endianness::Little => b'l',
            Endianness::Big => b'B',
        }
    }

    pub fn from_marker(b: u8) -> Option<Self> {
        match b {
            b'l' => Some(Endianness::Little),
            b'B' => Some(Endianness::Big),
            _ => None,
        }
    }
}

// ── Message Type ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum MessageType {
    Invalid = 0,
    MethodCall = 1,
    MethodReturn = 2,
    Error = 3,
    Signal = 4,
}

impl MessageType {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => MessageType::MethodCall,
            2 => MessageType::MethodReturn,
            3 => MessageType::Error,
            4 => MessageType::Signal,
            _ => MessageType::Invalid,
        }
    }
}

// ── Message Flags ───────────────────────────────────────────────────────

pub const FLAG_NO_REPLY_EXPECTED: u8 = 0x01;
pub const FLAG_NO_AUTO_START: u8 = 0x02;
pub const FLAG_ALLOW_INTERACTIVE_AUTHORIZATION: u8 = 0x04;

// ── Header Field Codes ──────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HeaderField {
    Path = 1,
    Interface = 2,
    Member = 3,
    ErrorName = 4,
    ReplySerial = 5,
    Destination = 6,
    Sender = 7,
    Signature = 8,
    UnixFds = 9,
}

// ── Signature types ─────────────────────────────────────────────────────

/// D-Bus type signature characters
pub mod sig {
    pub const BYTE: char = 'y';
    pub const BOOL: char = 'b';
    pub const INT16: char = 'n';
    pub const UINT16: char = 'q';
    pub const INT32: char = 'i';
    pub const UINT32: char = 'u';
    pub const INT64: char = 'x';
    pub const UINT64: char = 't';
    pub const DOUBLE: char = 'd';
    pub const STRING: char = 's';
    pub const OBJECT_PATH: char = 'o';
    pub const SIGNATURE: char = 'g';
    pub const ARRAY: char = 'a';
    pub const STRUCT_OPEN: char = '(';
    pub const STRUCT_CLOSE: char = ')';
    pub const VARIANT_OPEN: char = 'v';
    pub const DICT_ENTRY_OPEN: char = '{';
    pub const DICT_ENTRY_CLOSE: char = '}';
}

// ── Variant ─────────────────────────────────────────────────────────────

/// A D-Bus variant value — carries a signature + typed payload.
#[derive(Debug, Clone)]
pub struct Variant {
    pub signature: String,
    pub value: Value,
}

/// D-Bus dynamically-typed value.
#[derive(Debug, Clone)]
pub enum Value {
    Byte(u8),
    Bool(bool),
    Int16(i16),
    UInt16(u16),
    Int32(i32),
    UInt32(u32),
    Int64(i64),
    UInt64(u64),
    Double(f64),
    String(String),
    ObjectPath(String),
    Signature(String),
    Variant(Box<Variant>),
    Array(String, Vec<Value>),
    Struct(Vec<Value>),
    DictEntry(Box<Value>, Box<Value>),
}

impl Value {
    pub fn signature(&self) -> String {
        match self {
            Value::Byte(_) => "y".to_string(),
            Value::Bool(_) => "b".to_string(),
            Value::Int16(_) => "n".to_string(),
            Value::UInt16(_) => "q".to_string(),
            Value::Int32(_) => "i".to_string(),
            Value::UInt32(_) => "u".to_string(),
            Value::Int64(_) => "x".to_string(),
            Value::UInt64(_) => "t".to_string(),
            Value::Double(_) => "d".to_string(),
            Value::String(_) => "s".to_string(),
            Value::ObjectPath(_) => "o".to_string(),
            Value::Signature(_) => "g".to_string(),
            Value::Variant(_) => "v".to_string(),
            Value::Array(sig, _) => format!("a{}", sig),
            Value::Struct(items) => {
                let inner: String = items.iter().map(|v| v.signature()).collect();
                format!("({})", inner)
            }
            Value::DictEntry(k, v) => {
                format!("{{{}{}}}", k.signature(), v.signature())
            }
        }
    }
}

// ── Header ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Header {
    pub endian: Endianness,
    pub msg_type: MessageType,
    pub flags: u8,
    pub protocol_version: u8,
    pub body_length: u32,
    pub serial: u32,
    pub fields: Vec<HeaderFieldEntry>,
}

#[derive(Debug, Clone)]
pub struct HeaderFieldEntry {
    pub code: HeaderField,
    pub value: Value,
}

impl Header {
    pub fn new(msg_type: MessageType, serial: u32) -> Self {
        Self {
            endian: Endianness::Little,
            msg_type,
            flags: 0,
            protocol_version: PROTOCOL_VERSION,
            body_length: 0,
            serial,
            fields: Vec::new(),
        }
    }

    pub fn with_field(mut self, code: HeaderField, value: Value) -> Self {
        self.fields.push(HeaderFieldEntry { code, value });
        self
    }

    pub fn path(&self) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.code == HeaderField::Path)
            .and_then(|f| match &f.value {
                Value::ObjectPath(s) | Value::String(s) => Some(s.as_str()),
                _ => None,
            })
    }

    pub fn interface(&self) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.code == HeaderField::Interface)
            .and_then(|f| match &f.value {
                Value::String(s) => Some(s.as_str()),
                _ => None,
            })
    }

    pub fn member(&self) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.code == HeaderField::Member)
            .and_then(|f| match &f.value {
                Value::String(s) => Some(s.as_str()),
                _ => None,
            })
    }

    pub fn destination(&self) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.code == HeaderField::Destination)
            .and_then(|f| match &f.value {
                Value::String(s) => Some(s.as_str()),
                _ => None,
            })
    }

    pub fn sender(&self) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.code == HeaderField::Sender)
            .and_then(|f| match &f.value {
                Value::String(s) => Some(s.as_str()),
                _ => None,
            })
    }

    pub fn signature(&self) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.code == HeaderField::Signature)
            .and_then(|f| match &f.value {
                Value::Signature(s) | Value::String(s) => Some(s.as_str()),
                _ => None,
            })
    }

    pub fn reply_serial(&self) -> Option<u32> {
        self.fields
            .iter()
            .find(|f| f.code == HeaderField::ReplySerial)
            .and_then(|f| match &f.value {
                Value::UInt32(v) => Some(*v),
                _ => None,
            })
    }

    pub fn error_name(&self) -> Option<&str> {
        self.fields
            .iter()
            .find(|f| f.code == HeaderField::ErrorName)
            .and_then(|f| match &f.value {
                Value::String(s) => Some(s.as_str()),
                _ => None,
            })
    }
}

// ── Message ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Message {
    pub header: Header,
    pub body: Vec<Value>,
}

impl Message {
    pub fn new_method_call(
        serial: u32,
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
    ) -> Self {
        let header = Header::new(MessageType::MethodCall, serial)
            .with_field(
                HeaderField::Destination,
                Value::String(destination.to_string()),
            )
            .with_field(HeaderField::Path, Value::ObjectPath(path.to_string()))
            .with_field(HeaderField::Interface, Value::String(interface.to_string()))
            .with_field(HeaderField::Member, Value::String(member.to_string()));
        Self {
            header,
            body: Vec::new(),
        }
    }

    pub fn new_signal(serial: u32, path: &str, interface: &str, member: &str) -> Self {
        let header = Header::new(MessageType::Signal, serial)
            .with_field(HeaderField::Path, Value::ObjectPath(path.to_string()))
            .with_field(HeaderField::Interface, Value::String(interface.to_string()))
            .with_field(HeaderField::Member, Value::String(member.to_string()));
        Self {
            header,
            body: Vec::new(),
        }
    }

    pub fn new_method_return(serial: u32, reply_serial: u32, destination: &str) -> Self {
        let header = Header::new(MessageType::MethodReturn, serial)
            .with_field(HeaderField::ReplySerial, Value::UInt32(reply_serial))
            .with_field(
                HeaderField::Destination,
                Value::String(destination.to_string()),
            );
        Self {
            header,
            body: Vec::new(),
        }
    }

    pub fn new_error(
        serial: u32,
        reply_serial: u32,
        destination: &str,
        error_name: &str,
        error_message: &str,
    ) -> Self {
        let header = Header::new(MessageType::Error, serial)
            .with_field(HeaderField::ReplySerial, Value::UInt32(reply_serial))
            .with_field(
                HeaderField::ErrorName,
                Value::String(error_name.to_string()),
            )
            .with_field(
                HeaderField::Destination,
                Value::String(destination.to_string()),
            );
        Self {
            header,
            body: vec![Value::String(error_message.to_string())],
        }
    }
}

// ── Serial Marshaler ────────────────────────────────────────────────────

/// Marshal D-Bus values into the wire-format byte stream.
pub struct Marshaler {
    pub buf: Vec<u8>,
    endian: Endianness,
}

impl Marshaler {
    pub fn new(endian: Endianness) -> Self {
        Self {
            buf: Vec::new(),
            endian,
        }
    }

    /// Pad to alignment boundary.
    fn align(&mut self, alignment: usize) {
        let offset = self.buf.len() % alignment;
        if offset != 0 {
            for _ in 0..(alignment - offset) {
                self.buf.push(0);
            }
        }
    }

    pub fn push_u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn push_u16(&mut self, v: u16) {
        self.align(2);
        match self.endian {
            Endianness::Little => self.buf.extend_from_slice(&v.to_le_bytes()),
            Endianness::Big => self.buf.extend_from_slice(&v.to_be_bytes()),
        }
    }

    pub fn push_u32(&mut self, v: u32) {
        self.align(4);
        match self.endian {
            Endianness::Little => self.buf.extend_from_slice(&v.to_le_bytes()),
            Endianness::Big => self.buf.extend_from_slice(&v.to_be_bytes()),
        }
    }

    pub fn push_u64(&mut self, v: u64) {
        self.align(8);
        match self.endian {
            Endianness::Little => self.buf.extend_from_slice(&v.to_le_bytes()),
            Endianness::Big => self.buf.extend_from_slice(&v.to_be_bytes()),
        }
    }

    pub fn push_i16(&mut self, v: i16) {
        self.push_u16(v as u16);
    }

    pub fn push_i32(&mut self, v: i32) {
        self.push_u32(v as u32);
    }

    pub fn push_i64(&mut self, v: i64) {
        self.push_u64(v as u64);
    }

    pub fn push_f64(&mut self, v: f64) {
        self.push_u64(v.to_bits());
    }

    pub fn push_bool(&mut self, v: bool) {
        self.push_u32(if v { 1 } else { 0 });
    }

    pub fn push_string(&mut self, v: &str) {
        self.push_u32(v.len() as u32);
        self.buf.extend_from_slice(v.as_bytes());
        self.buf.push(0); // null terminator
    }

    pub fn push_signature(&mut self, v: &str) {
        self.buf.push(v.len() as u8);
        self.buf.extend_from_slice(v.as_bytes());
        self.buf.push(0);
    }

    pub fn push_value(&mut self, v: &Value) {
        match v {
            Value::Byte(b) => self.push_u8(*b),
            Value::Bool(b) => self.push_bool(*b),
            Value::Int16(n) => self.push_i16(*n),
            Value::UInt16(n) => self.push_u16(*n),
            Value::Int32(n) => self.push_i32(*n),
            Value::UInt32(n) => self.push_u32(*n),
            Value::Int64(n) => self.push_i64(*n),
            Value::UInt64(n) => self.push_u64(*n),
            Value::Double(d) => self.push_f64(*d),
            Value::String(s) | Value::ObjectPath(s) => self.push_string(s),
            Value::Signature(s) => self.push_signature(s),
            Value::Variant(var) => {
                self.push_signature(&var.signature);
                self.push_value(&var.value);
            }
            Value::Array(_sig, items) => {
                let len_pos = self.buf.len();
                self.push_u32(0); // placeholder for length
                let content_start = self.buf.len();
                // Determine alignment from first element type
                if let Some(first) = items.first() {
                    let align = match first {
                        Value::Byte(_) => 1,
                        Value::Bool(_) | Value::Int16(_) | Value::UInt16(_) => 2,
                        Value::Int32(_)
                        | Value::UInt32(_)
                        | Value::String(_)
                        | Value::ObjectPath(_)
                        | Value::Signature(_) => 4,
                        Value::Int64(_) | Value::UInt64(_) | Value::Double(_) => 8,
                        Value::Variant(_) => 1,
                        Value::Array(_, _) => 4,
                        Value::Struct(_) => 8,
                        Value::DictEntry(_, _) => 8,
                    };
                    self.align(align);
                }
                for item in items {
                    self.push_value(item);
                }
                let array_len = (self.buf.len() - content_start) as u32;
                let len_bytes = match self.endian {
                    Endianness::Little => array_len.to_le_bytes(),
                    Endianness::Big => array_len.to_be_bytes(),
                };
                self.buf[len_pos..len_pos + 4].copy_from_slice(&len_bytes);
            }
            Value::Struct(items) => {
                self.align(8);
                for item in items {
                    self.push_value(item);
                }
                self.align(8);
            }
            Value::DictEntry(k, v) => {
                self.align(8);
                self.push_value(k);
                self.push_value(v);
            }
        }
    }

    /// Marshal a complete message into wire format.
    pub fn marshal_message(&mut self, msg: &Message) {
        // Endianness marker
        self.push_u8(self.endian.marker());
        // Message type
        self.push_u8(msg.header.msg_type as u8);
        // Flags
        self.push_u8(msg.header.flags);
        // Protocol version
        self.push_u8(msg.header.protocol_version);

        // Body length (placeholder — filled after body is serialized)
        let body_len_pos = self.buf.len();
        self.push_u32(0);

        // Serial
        self.push_u32(msg.header.serial);

        // Header fields array
        let header_fields_len_pos = self.buf.len();
        self.push_u32(0); // placeholder for header fields array length
        let header_fields_start = self.buf.len();
        self.align(8);

        for field in &msg.header.fields {
            self.align(8);
            self.push_u8(field.code as u8);
            // Each field value is a variant
            let sig = field.value.signature();
            self.push_signature(&sig);
            self.push_value(&field.value);
        }

        let header_fields_len = (self.buf.len() - header_fields_start) as u32;
        let len_bytes = match self.endian {
            Endianness::Little => header_fields_len.to_le_bytes(),
            Endianness::Big => header_fields_len.to_be_bytes(),
        };
        self.buf[header_fields_len_pos..header_fields_len_pos + 4].copy_from_slice(&len_bytes);

        // Align body to 8 bytes
        self.align(8);
        let body_start = self.buf.len();

        // Marshal body
        for value in &msg.body {
            self.push_value(value);
        }

        // Fill in body length
        let body_len = (self.buf.len() - body_start) as u32;
        let body_len_bytes = match self.endian {
            Endianness::Little => body_len.to_le_bytes(),
            Endianness::Big => body_len.to_be_bytes(),
        };
        self.buf[body_len_pos..body_len_pos + 4].copy_from_slice(&body_len_bytes);
    }
}

// ── Serial Unmarshaler ──────────────────────────────────────────────────

/// Unmarshal D-Bus values from a wire-format byte stream.
pub struct Unmarshaler<'a> {
    data: &'a [u8],
    pos: usize,
    endian: Endianness,
}

impl<'a> Unmarshaler<'a> {
    pub fn new(data: &'a [u8]) -> Result<Self, &'static str> {
        if data.len() < 16 {
            return Err("Message too short for D-Bus header");
        }
        let endian = Endianness::from_marker(data[0]).ok_or("Invalid endianness marker")?;
        Ok(Self {
            data,
            pos: 0,
            endian,
        })
    }

    fn align(&mut self, alignment: usize) {
        let offset = self.pos % alignment;
        if offset != 0 {
            self.pos += alignment - offset;
        }
    }

    fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }

    fn read_u8(&mut self) -> Result<u8, &'static str> {
        if self.pos >= self.data.len() {
            return Err("Unexpected end of message");
        }
        let v = self.data[self.pos];
        self.pos += 1;
        Ok(v)
    }

    fn read_u16(&mut self) -> Result<u16, &'static str> {
        self.align(2);
        if self.pos + 2 > self.data.len() {
            return Err("Unexpected end of message");
        }
        let bytes: [u8; 2] = self.data[self.pos..self.pos + 2].try_into().unwrap();
        self.pos += 2;
        Ok(match self.endian {
            Endianness::Little => u16::from_le_bytes(bytes),
            Endianness::Big => u16::from_be_bytes(bytes),
        })
    }

    fn read_u32(&mut self) -> Result<u32, &'static str> {
        self.align(4);
        if self.pos + 4 > self.data.len() {
            return Err("Unexpected end of message");
        }
        let bytes: [u8; 4] = self.data[self.pos..self.pos + 4].try_into().unwrap();
        self.pos += 4;
        Ok(match self.endian {
            Endianness::Little => u32::from_le_bytes(bytes),
            Endianness::Big => u32::from_be_bytes(bytes),
        })
    }

    fn read_u64(&mut self) -> Result<u64, &'static str> {
        self.align(8);
        if self.pos + 8 > self.data.len() {
            return Err("Unexpected end of message");
        }
        let bytes: [u8; 8] = self.data[self.pos..self.pos + 8].try_into().unwrap();
        self.pos += 8;
        Ok(match self.endian {
            Endianness::Little => u64::from_le_bytes(bytes),
            Endianness::Big => u64::from_be_bytes(bytes),
        })
    }

    fn read_string(&mut self) -> Result<String, &'static str> {
        let len = self.read_u32()? as usize;
        if self.pos + len + 1 > self.data.len() {
            return Err("String length exceeds message bounds");
        }
        let s = core::str::from_utf8(&self.data[self.pos..self.pos + len])
            .map_err(|_| "Invalid UTF-8 in string")?;
        self.pos += len + 1; // skip null terminator
        Ok(s.to_string())
    }

    fn read_signature(&mut self) -> Result<String, &'static str> {
        let len = self.read_u8()? as usize;
        if self.pos + len + 1 > self.data.len() {
            return Err("Signature length exceeds message bounds");
        }
        let s = core::str::from_utf8(&self.data[self.pos..self.pos + len])
            .map_err(|_| "Invalid UTF-8 in signature")?;
        self.pos += len + 1;
        Ok(s.to_string())
    }

    /// Parse the fixed-size header and return (header, body_offset).
    pub fn parse_header(&mut self) -> Result<Header, &'static str> {
        let endian_byte = self.read_u8()?;
        self.endian = Endianness::from_marker(endian_byte).ok_or("Invalid endianness")?;

        let msg_type = MessageType::from_u8(self.read_u8()?);
        let flags = self.read_u8()?;
        let protocol_version = self.read_u8()?;
        let body_length = self.read_u32()?;
        let serial = self.read_u32()?;

        // Read header fields array
        let header_fields_len = self.read_u32()? as usize;
        self.align(8);

        let fields_end = self.pos + header_fields_len;
        if fields_end > self.data.len() {
            return Err("Header fields array exceeds message bounds");
        }

        let mut fields = Vec::new();
        while self.pos < fields_end {
            self.align(8);
            if self.pos >= fields_end {
                break;
            }
            let code_byte = self.read_u8()?;
            let code = match code_byte {
                1 => HeaderField::Path,
                2 => HeaderField::Interface,
                3 => HeaderField::Member,
                4 => HeaderField::ErrorName,
                5 => HeaderField::ReplySerial,
                6 => HeaderField::Destination,
                7 => HeaderField::Sender,
                8 => HeaderField::Signature,
                9 => HeaderField::UnixFds,
                _ => return Err("Invalid header field code"),
            };

            // Read variant signature
            let _variant_sig = self.read_signature()?;

            // Read the value based on the field code
            let value = match code {
                HeaderField::Path => Value::ObjectPath(self.read_string()?),
                HeaderField::Interface
                | HeaderField::Member
                | HeaderField::ErrorName
                | HeaderField::Destination
                | HeaderField::Sender => Value::String(self.read_string()?),
                HeaderField::ReplySerial => Value::UInt32(self.read_u32()?),
                HeaderField::Signature => Value::Signature(self.read_signature()?),
                HeaderField::UnixFds => Value::UInt32(self.read_u32()?),
            };
            fields.push(HeaderFieldEntry { code, value });
        }

        self.pos = fields_end;
        self.align(8);

        Ok(Header {
            endian: self.endian,
            msg_type,
            flags,
            protocol_version,
            body_length,
            serial,
            fields,
        })
    }

    /// Parse the body values given a signature string.
    pub fn parse_body(&mut self, signature: &str) -> Result<Vec<Value>, &'static str> {
        let mut values = Vec::new();
        let chars: Vec<char> = signature.chars().collect();
        let mut i = 0;
        while i < chars.len() {
            let (value, consumed) = self.parse_value(&chars, i)?;
            values.push(value);
            i += consumed;
        }
        Ok(values)
    }

    fn parse_value(&mut self, sig: &[char], start: usize) -> Result<(Value, usize), &'static str> {
        if start >= sig.len() {
            return Err("Empty signature");
        }
        match sig[start] {
            sig::BYTE => {
                let v = self.read_u8()?;
                Ok((Value::Byte(v), 1))
            }
            sig::BOOL => {
                let v = self.read_u32()?;
                Ok((Value::Bool(v != 0), 1))
            }
            sig::INT16 => {
                let v = self.read_u16()? as i16;
                Ok((Value::Int16(v), 1))
            }
            sig::UINT16 => {
                let v = self.read_u16()?;
                Ok((Value::UInt16(v), 1))
            }
            sig::INT32 => {
                let v = self.read_u32()? as i32;
                Ok((Value::Int32(v), 1))
            }
            sig::UINT32 => {
                let v = self.read_u32()?;
                Ok((Value::UInt32(v), 1))
            }
            sig::INT64 => {
                let v = self.read_u64()? as i64;
                Ok((Value::Int64(v), 1))
            }
            sig::UINT64 => {
                let v = self.read_u64()?;
                Ok((Value::UInt64(v), 1))
            }
            sig::DOUBLE => {
                let bits = self.read_u64()?;
                Ok((Value::Double(f64::from_bits(bits)), 1))
            }
            sig::STRING => {
                let s = self.read_string()?;
                Ok((Value::String(s), 1))
            }
            sig::OBJECT_PATH => {
                let s = self.read_string()?;
                Ok((Value::ObjectPath(s), 1))
            }
            sig::SIGNATURE => {
                let s = self.read_signature()?;
                Ok((Value::Signature(s), 1))
            }
            sig::VARIANT_OPEN => {
                // Variant: signature string + value
                let inner_sig = self.read_signature()?;
                let inner_chars: Vec<char> = inner_sig.chars().collect();
                let (inner_value, _) = self.parse_value(&inner_chars, 0)?;
                Ok((
                    Value::Variant(Box::new(Variant {
                        signature: inner_sig,
                        value: inner_value,
                    })),
                    1,
                ))
            }
            sig::ARRAY => {
                if start + 1 >= sig.len() {
                    return Err("Array signature incomplete");
                }
                // Build element signature
                let (_elem_sig, consumed) = extract_type_signature(sig, start + 1)?;
                let full_sig: String = sig[start + 1..start + 1 + consumed].iter().collect();

                let array_len = self.read_u32()? as usize;
                let array_end = self.pos + array_len;
                if array_end > self.data.len() {
                    return Err("Array length exceeds message bounds");
                }

                // Determine element alignment
                let first_char = sig[start + 1];
                let align = match first_char {
                    sig::BYTE | sig::VARIANT_OPEN => 1,
                    sig::BOOL | sig::INT16 | sig::UINT16 => 2,
                    sig::INT32
                    | sig::UINT32
                    | sig::STRING
                    | sig::OBJECT_PATH
                    | sig::SIGNATURE
                    | sig::ARRAY => 4,
                    sig::INT64
                    | sig::UINT64
                    | sig::DOUBLE
                    | sig::STRUCT_OPEN
                    | sig::DICT_ENTRY_OPEN => 8,
                    _ => 1,
                };
                self.align(align);

                let mut items = Vec::new();
                while self.pos < array_end {
                    let (item, _) = self.parse_value(sig, start + 1)?;
                    items.push(item);
                }
                self.pos = array_end;

                Ok((Value::Array(full_sig, items), 1 + consumed))
            }
            sig::STRUCT_OPEN => {
                self.align(8);
                // Find matching close paren
                let mut depth = 1;
                let mut end = start + 1;
                while end < sig.len() && depth > 0 {
                    match sig[end] {
                        sig::STRUCT_OPEN | sig::DICT_ENTRY_OPEN => depth += 1,
                        sig::STRUCT_CLOSE | sig::DICT_ENTRY_CLOSE => depth -= 1,
                        _ => {}
                    }
                    if depth > 0 {
                        end += 1;
                    }
                }
                let inner_sig = &sig[start + 1..end];
                let mut items = Vec::new();
                let mut i = 0;
                while i < inner_sig.len() {
                    let (item, consumed) = self.parse_value(inner_sig, i)?;
                    items.push(item);
                    i += consumed;
                }
                self.align(8);
                Ok((Value::Struct(items), end - start + 1))
            }
            sig::DICT_ENTRY_OPEN => {
                self.align(8);
                // Find matching close brace
                let mut depth = 1;
                let mut end = start + 1;
                while end < sig.len() && depth > 0 {
                    match sig[end] {
                        sig::DICT_ENTRY_OPEN | sig::STRUCT_OPEN => depth += 1,
                        sig::DICT_ENTRY_CLOSE | sig::STRUCT_CLOSE => depth -= 1,
                        _ => {}
                    }
                    if depth > 0 {
                        end += 1;
                    }
                }
                let inner_sig = &sig[start + 1..end];
                let mut items = Vec::new();
                let mut i = 0;
                while i < inner_sig.len() && items.len() < 2 {
                    let (item, consumed) = self.parse_value(inner_sig, i)?;
                    items.push(item);
                    i += consumed;
                }
                if items.len() != 2 {
                    return Err("Dict entry must have exactly 2 elements");
                }
                let key = items.remove(0);
                let val = items.remove(0);
                Ok((
                    Value::DictEntry(Box::new(key), Box::new(val)),
                    end - start + 1,
                ))
            }
            _ => Err("Unknown signature type character"),
        }
    }
}

/// Extract a complete type signature starting at `start`, returning
/// (signature_string, chars_consumed).
fn extract_type_signature(sig: &[char], start: usize) -> Result<(String, usize), &'static str> {
    if start >= sig.len() {
        return Err("Signature index out of bounds");
    }
    match sig[start] {
        sig::ARRAY => {
            let (inner, consumed) = extract_type_signature(sig, start + 1)?;
            Ok((format!("a{}", inner), 1 + consumed))
        }
        sig::STRUCT_OPEN => {
            let mut depth = 1;
            let mut end = start + 1;
            while end < sig.len() && depth > 0 {
                match sig[end] {
                    sig::STRUCT_OPEN | sig::DICT_ENTRY_OPEN => depth += 1,
                    sig::STRUCT_CLOSE | sig::DICT_ENTRY_CLOSE => depth -= 1,
                    _ => {}
                }
                if depth > 0 {
                    end += 1;
                }
            }
            let s: String = sig[start..=end].iter().collect();
            Ok((s, end - start + 1))
        }
        sig::DICT_ENTRY_OPEN => {
            let mut depth = 1;
            let mut end = start + 1;
            while end < sig.len() && depth > 0 {
                match sig[end] {
                    sig::DICT_ENTRY_OPEN | sig::STRUCT_OPEN => depth += 1,
                    sig::DICT_ENTRY_CLOSE | sig::STRUCT_CLOSE => depth -= 1,
                    _ => {}
                }
                if depth > 0 {
                    end += 1;
                }
            }
            let s: String = sig[start..=end].iter().collect();
            Ok((s, end - start + 1))
        }
        sig::VARIANT_OPEN => Ok(("v".to_string(), 1)),
        c => Ok((c.to_string(), 1)),
    }
}

// ── Connection ──────────────────────────────────────────────────────────

/// A D-Bus connection identifier.
pub type ConnectionId = u32;

/// D-Bus connection state.
#[derive(Debug)]
pub struct Connection {
    pub id: ConnectionId,
    pub unique_name: String,
    pub registered_names: Vec<String>,
    pub serial_counter: AtomicU32,
}

impl Connection {
    pub fn new(id: ConnectionId) -> Self {
        Self {
            id,
            unique_name: format!(":{}", id),
            registered_names: Vec::new(),
            serial_counter: AtomicU32::new(1),
        }
    }

    pub fn next_serial(&self) -> u32 {
        self.serial_counter.fetch_add(1, Ordering::Relaxed)
    }
}

// ── Signal Subscription ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct SignalMatch {
    pub connection_id: ConnectionId,
    pub sender: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub path: Option<String>,
}

impl SignalMatch {
    pub fn matches(&self, msg: &Message) -> bool {
        if let Some(ref iface) = self.interface {
            if msg.header.interface() != Some(iface.as_str()) {
                return false;
            }
        }
        if let Some(ref member) = self.member {
            if msg.header.member() != Some(member.as_str()) {
                return false;
            }
        }
        if let Some(ref path) = self.path {
            if msg.header.path() != Some(path.as_str()) {
                return false;
            }
        }
        if let Some(ref sender) = self.sender {
            if msg.header.sender() != Some(sender.as_str()) {
                return false;
            }
        }
        true
    }
}

// ── Message Bus ─────────────────────────────────────────────────────────

/// In-kernel D-Bus message bus daemon.
pub struct MessageBus {
    connections: BTreeMap<ConnectionId, Connection>,
    name_registry: BTreeMap<String, ConnectionId>,
    signal_matches: Vec<SignalMatch>,
    next_connection_id: AtomicU32,
    bus_serial: AtomicU32,
    initialized: bool,
}

impl MessageBus {
    pub const fn new() -> Self {
        Self {
            connections: BTreeMap::new(),
            name_registry: BTreeMap::new(),
            signal_matches: Vec::new(),
            next_connection_id: AtomicU32::new(1),
            bus_serial: AtomicU32::new(1),
            initialized: false,
        }
    }

    pub fn init(&mut self) {
        if self.initialized {
            return;
        }
        // Register the bus itself as connection 0
        let bus_conn = Connection {
            id: 0,
            unique_name: BUS_NAME.to_string(),
            registered_names: vec![BUS_NAME.to_string()],
            serial_counter: AtomicU32::new(1),
        };
        self.connections.insert(0, bus_conn);
        self.name_registry.insert(BUS_NAME.to_string(), 0);
        self.initialized = true;
    }

    /// Register a new connection and return its unique name.
    pub fn connect(&mut self) -> Result<String, &'static str> {
        let id = self.next_connection_id.fetch_add(1, Ordering::Relaxed);
        let conn = Connection::new(id);
        let name = conn.unique_name.clone();
        self.connections.insert(id, conn);
        Ok(name)
    }

    /// Disconnect a connection and release its names.
    pub fn disconnect(&mut self, id: ConnectionId) {
        if let Some(conn) = self.connections.remove(&id) {
            for name in &conn.registered_names {
                self.name_registry.remove(name);
            }
        }
        self.signal_matches.retain(|m| m.connection_id != id);
    }

    /// Request a well-known name.
    pub fn request_name(&mut self, conn_id: ConnectionId, name: &str) -> Result<(), &'static str> {
        if name.is_empty() || name.starts_with(':') {
            return Err("Invalid well-known name");
        }
        if let Some(&owner) = self.name_registry.get(name) {
            if owner != conn_id {
                return Err("Name already owned by another connection");
            }
            return Ok(());
        }
        if let Some(conn) = self.connections.get_mut(&conn_id) {
            conn.registered_names.push(name.to_string());
        }
        self.name_registry.insert(name.to_string(), conn_id);
        Ok(())
    }

    /// Release a well-known name.
    pub fn release_name(&mut self, conn_id: ConnectionId, name: &str) -> Result<(), &'static str> {
        if let Some(&owner) = self.name_registry.get(name) {
            if owner != conn_id {
                return Err("Name not owned by this connection");
            }
            self.name_registry.remove(name);
            if let Some(conn) = self.connections.get_mut(&conn_id) {
                conn.registered_names.retain(|n| n != name);
            }
            Ok(())
        } else {
            Err("Name not found")
        }
    }

    /// Add a signal match rule.
    pub fn add_match(&mut self, match_rule: SignalMatch) {
        self.signal_matches.push(match_rule);
    }

    /// Remove signal match rules for a connection.
    pub fn remove_match(&mut self, conn_id: ConnectionId) {
        self.signal_matches.retain(|m| m.connection_id != conn_id);
    }

    /// Route a message to the appropriate destination.
    /// Returns a list of connection IDs that should receive the message.
    pub fn route(&self, msg: &Message) -> Vec<ConnectionId> {
        match msg.header.msg_type {
            MessageType::Signal => {
                // Broadcast to all connections with matching rules
                self.signal_matches
                    .iter()
                    .filter(|m| m.matches(msg))
                    .map(|m| m.connection_id)
                    .collect()
            }
            MessageType::MethodCall | MessageType::MethodReturn | MessageType::Error => {
                // Unicast to destination
                if let Some(dest) = msg.header.destination() {
                    if let Some(&id) = self.name_registry.get(dest) {
                        return vec![id];
                    }
                    // Try unique name lookup
                    if dest.starts_with(':') {
                        if let Ok(id) = dest[1..].parse::<u32>() {
                            if self.connections.contains_key(&id) {
                                return vec![id];
                            }
                        }
                    }
                }
                Vec::new()
            }
            MessageType::Invalid => Vec::new(),
        }
    }

    /// Look up a connection by ID.
    pub fn get_connection(&self, id: ConnectionId) -> Option<&Connection> {
        self.connections.get(&id)
    }

    /// Get the next serial number for the bus itself.
    pub fn next_bus_serial(&self) -> u32 {
        self.bus_serial.fetch_add(1, Ordering::Relaxed)
    }

    /// Check if the bus is initialized.
    pub fn is_initialized(&self) -> bool {
        self.initialized
    }

    /// List all registered well-known names.
    pub fn list_names(&self) -> Vec<String> {
        self.name_registry.keys().cloned().collect()
    }
}

// ── Global Bus Instance ─────────────────────────────────────────────────

static BUS: RwLock<MessageBus> = RwLock::new(MessageBus::new());

/// Initialize the D-Bus message bus.
pub fn init() -> Result<(), &'static str> {
    let mut bus = BUS.write();
    bus.init();
    unsafe {
        crate::early_serial_write_str("RustOS: D-Bus message bus initialized\r\n");
    }
    Ok(())
}

/// Get a read reference to the global bus.
pub fn bus() -> spin::rwlock::RwLockReadGuard<'static, MessageBus> {
    BUS.read()
}

/// Get a write reference to the global bus.
pub fn bus_mut() -> spin::rwlock::RwLockWriteGuard<'static, MessageBus> {
    BUS.write()
}

/// Check if D-Bus is initialized.
pub fn is_ready() -> bool {
    BUS.read().is_initialized()
}

// ── Hello / NameOwnerChanged ────────────────────────────────────────────

/// Standard D-Bus bus methods
pub mod bus_methods {
    use super::*;

    /// Handle the "Hello" method call — registers the connection and returns its unique name.
    pub fn hello(conn_id: ConnectionId) -> Result<String, &'static str> {
        let mut bus = BUS.write();
        bus.connect().map(|name| {
            // Also store the connection mapping
            let _ = conn_id;
            name
        })
    }

    /// Emit a NameOwnerChanged signal for a name acquisition.
    pub fn name_acquired_signal(serial: u32, name: &str, owner: &str) -> Message {
        let mut msg = Message::new_signal(serial, BUS_PATH, BUS_NAME, "NameAcquired");
        msg.body = vec![
            Value::String(name.to_string()),
            Value::String(owner.to_string()),
        ];
        msg
    }

    /// Emit a NameOwnerChanged signal.
    pub fn name_owner_changed_signal(
        serial: u32,
        name: &str,
        old_owner: &str,
        new_owner: &str,
    ) -> Message {
        let mut msg = Message::new_signal(serial, BUS_PATH, BUS_NAME, "NameOwnerChanged");
        msg.body = vec![
            Value::String(name.to_string()),
            Value::String(old_owner.to_string()),
            Value::String(new_owner.to_string()),
        ];
        msg
    }

    /// List all registered names (for the ListNames method).
    pub fn list_names() -> Vec<String> {
        BUS.read().list_names()
    }

    /// Get the unique name for a connection.
    pub fn get_unique_name(conn_id: ConnectionId) -> Option<String> {
        BUS.read()
            .get_connection(conn_id)
            .map(|c| c.unique_name.clone())
    }
}



const BUS_INTROSPECT_XML: &str = r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"
"http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node>
  <interface name="org.freedesktop.DBus.Introspectable">
    <method name="Introspect">
      <arg name="data" type="s" direction="out"/>
    </method>
  </interface>
  <interface name="org.freedesktop.DBus.Properties">
    <method name="Get">
      <arg name="interface_name" type="s" direction="in"/>
      <arg name="property_name" type="s" direction="in"/>
      <arg name="value" type="v" direction="out"/>
    </method>
    <method name="GetAll">
      <arg name="interface_name" type="s" direction="in"/>
      <arg name="properties" type="a{sv}" direction="out"/>
    </method>
    <method name="Set">
      <arg name="interface_name" type="s" direction="in"/>
      <arg name="property_name" type="s" direction="in"/>
      <arg name="value" type="v" direction="in"/>
    </method>
  </interface>
  <interface name="org.freedesktop.DBus.Peer">
    <method name="Ping"/>
    <method name="GetMachineId">
      <arg name="machine_uuid" type="s" direction="out"/>
    </method>
  </interface>
  <interface name="org.freedesktop.DBus">
    <method name="Hello">
      <arg name="name" type="s" direction="out"/>
    </method>
    <method name="RequestName">
      <arg name="name" type="s" direction="in"/>
      <arg name="flags" type="u" direction="in"/>
      <arg name="reply" type="u" direction="out"/>
    </method>
    <method name="ReleaseName">
      <arg name="name" type="s" direction="in"/>
      <arg name="reply" type="u" direction="out"/>
    </method>
    <method name="ListNames">
      <arg name="names" type="as" direction="out"/>
    </method>
    <method name="AddMatch">
      <arg name="rule" type="s" direction="in"/>
    </method>
    <method name="RemoveMatch">
      <arg name="rule" type="s" direction="in"/>
    </method>
    <property name="Features" type="as" access="read"/>
    <property name="Interfaces" type="as" access="read"/>
  </interface>
</node>"#;

fn introspect_xml(path: &str) -> &'static str {
    if path == BUS_PATH {
        BUS_INTROSPECT_XML
    } else {
        r#"<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN"
"http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">
<node/>"#
    }
}

fn dispatch_introspectable(
    member: &str,
    serial: u32,
    sender: &str,
    path: &str,
) -> Option<Vec<u8>> {
    match member {
        "Introspect" => {
            let xml = introspect_xml(path);
            let mut reply = Message::new_method_return(
                BUS.read().next_bus_serial(),
                serial,
                sender,
            );
            reply.header = reply.header.with_field(
                HeaderField::Signature,
                Value::Signature("s".to_string()),
            );
            reply.body = vec![Value::String(xml.to_string())];
            Some(marshal_message(&reply))
        }
        "Ping" => {
            let reply = Message::new_method_return(
                BUS.read().next_bus_serial(),
                serial,
                sender,
            );
            Some(marshal_message(&reply))
        }
        "GetMachineId" => {
            let mut reply = Message::new_method_return(
                BUS.read().next_bus_serial(),
                serial,
                sender,
            );
            reply.header = reply.header.with_field(
                HeaderField::Signature,
                Value::Signature("s".to_string()),
            );
            reply.body = vec![Value::String("rustos00000000000000000000000000".to_string())];
            Some(marshal_message(&reply))
        }
        _ => None,
    }
}


fn dbus_features() -> Value {
    Value::Array(
        "s".to_string(),
        vec![
            Value::String("org.freedesktop.DBus.NameHasOwner".to_string()),
            Value::String("org.freedesktop.DBus.GetConnectionUniqueName".to_string()),
        ],
    )
}

fn property_get(path: &str, iface: &str, prop: &str) -> Result<Value, &'static str> {
    if path == BUS_PATH && iface == BUS_NAME {
        return match prop {
            "Features" => Ok(dbus_features()),
            "Interfaces" => Ok(Value::Array(
                "s".to_string(),
                vec![
                    Value::String(BUS_NAME.to_string()),
                    Value::String(PROPERTIES_IFACE.to_string()),
                ],
            )),
            _ => Err("org.freedesktop.DBus.Error.UnknownProperty"),
        };
    }
    Err("org.freedesktop.DBus.Error.UnknownProperty")
}

fn property_get_all(path: &str, iface: &str) -> Vec<Value> {
    if path == BUS_PATH && iface == BUS_NAME {
        return vec![
            Value::DictEntry(
                Box::new(Value::String("Features".to_string())),
                Box::new(Value::Variant(Box::new(Variant {
                    signature: "as".to_string(),
                    value: dbus_features(),
                }))),
            ),
            Value::DictEntry(
                Box::new(Value::String("Interfaces".to_string())),
                Box::new(Value::Variant(Box::new(Variant {
                    signature: "as".to_string(),
                    value: Value::Array(
                        "s".to_string(),
                        vec![
                            Value::String(BUS_NAME.to_string()),
                            Value::String(PROPERTIES_IFACE.to_string()),
                        ],
                    ),
                }))),
            ),
        ];
    }
    Vec::new()
}

fn dispatch_properties(
    member: &str,
    serial: u32,
    sender: &str,
    path: &str,
    signature: &str,
    unmarshaler: &mut Unmarshaler<'_>,
) -> Option<Vec<u8>> {
    match member {
        "Get" => {
            let body = unmarshaler.parse_body(signature).ok()?;
            let iface = match body.first()? {
                Value::String(s) => s.as_str(),
                _ => return None,
            };
            let prop = match body.get(1)? {
                Value::String(s) => s.as_str(),
                _ => return None,
            };

            match property_get(path, iface, prop) {
                Ok(value) => {
                    let mut reply = Message::new_method_return(
                        BUS.read().next_bus_serial(),
                        serial,
                        sender,
                    );
                    reply.header = reply.header.with_field(
                        HeaderField::Signature,
                        Value::Signature("v".to_string()),
                    );
                    reply.body = vec![Value::Variant(Box::new(Variant {
                        signature: value.signature(),
                        value,
                    }))];
                    Some(marshal_message(&reply))
                }
                Err(error_name) => Some(marshal_message(&Message::new_error(
                    BUS.read().next_bus_serial(),
                    serial,
                    sender,
                    error_name,
                    "Unknown property",
                ))),
            }
        }
        "GetAll" => {
            let body = unmarshaler.parse_body(signature).ok()?;
            let iface = match body.first()? {
                Value::String(s) => s.as_str(),
                _ => return None,
            };
            let entries = property_get_all(path, iface);
            let mut reply = Message::new_method_return(
                BUS.read().next_bus_serial(),
                serial,
                sender,
            );
            reply.header = reply.header.with_field(
                HeaderField::Signature,
                Value::Signature("a{sv}".to_string()),
            );
            reply.body = vec![Value::Array("{sv}".to_string(), entries)];
            Some(marshal_message(&reply))
        }
        "Set" => {
            let _ = unmarshaler.parse_body(signature).ok()?;
            let reply = Message::new_method_return(
                BUS.read().next_bus_serial(),
                serial,
                sender,
            );
            Some(marshal_message(&reply))
        }
        _ => None,
    }
}


// ── Wire Request Dispatch ───────────────────────────────────────────────

/// Next connection id handed out by the in-kernel session bus.
static NEXT_CONN_ID: AtomicU32 = AtomicU32::new(2);

/// Process a D-Bus wire-format request and return a serialized reply.
///
/// Used by the GNOME overlay's pre-bound session bus socket. Returns `None`
/// when the buffer does not contain a complete or recognized message.
pub fn process_wire_request(data: &[u8]) -> Option<Vec<u8>> {
    let mut unmarshaler = Unmarshaler::new(data).ok()?;
    let header = unmarshaler.parse_header().ok()?;

    if header.msg_type != MessageType::MethodCall {
        return None;
    }

    let iface = header.interface()?;
    let member = header.member()?;
    let serial = header.serial;
    let destination = header.destination().unwrap_or(BUS_NAME);

    let path = header.path().unwrap_or(BUS_PATH);
    let sender = header.sender().unwrap_or(":1");

    if iface == INTROSPECTABLE_IFACE || iface == "org.freedesktop.DBus.Peer" {
        return dispatch_introspectable(member, serial, sender, path);
    }

    if iface == PROPERTIES_IFACE {
        let signature = header.signature().unwrap_or("");
        return dispatch_properties(member, serial, sender, path, signature, &mut unmarshaler);
    }

    if destination != BUS_NAME {
        return None;
    }

    match (iface, member) {
        (BUS_NAME, "Hello") => {
            let mut bus = BUS.write();
            let name = bus.connect().ok()?;
            let mut reply = Message::new_method_return(
                bus.next_bus_serial(),
                serial,
                &name,
            );
            reply.header = reply
                .header
                .with_field(HeaderField::Signature, Value::Signature("s".to_string()));
            reply.body = vec![Value::String(name.clone())];
            Some(marshal_message(&reply))
        }
        (BUS_NAME, "ListNames") => {
            let names = bus_methods::list_names();
            let mut reply = Message::new_method_return(
                BUS.read().next_bus_serial(),
                serial,
                header.sender().unwrap_or(":1"),
            );
            reply.header = reply
                .header
                .with_field(HeaderField::Signature, Value::Signature("as".to_string()));
            reply.body = vec![Value::Array(
                "s".to_string(),
                names.into_iter().map(Value::String).collect(),
            )];
            Some(marshal_message(&reply))
        }
        (BUS_NAME, "RequestName") => {
            let signature = header.signature().unwrap_or("");
            let body = if signature.is_empty() {
                Vec::new()
            } else {
                unmarshaler.parse_body(signature).ok()?
            };

            let requested_name = match body.first() {
                Some(Value::String(name)) => name.as_str(),
                _ => return None,
            };

            let conn_id = NEXT_CONN_ID.fetch_add(1, Ordering::Relaxed);
            let status = match BUS.write().request_name(conn_id, requested_name) {
                Ok(()) => 1u32, // DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER
                Err(_) => 3u32, // DBUS_REQUEST_NAME_REPLY_EXISTS
            };

            let mut reply = Message::new_method_return(
                BUS.read().next_bus_serial(),
                serial,
                header.sender().unwrap_or(":1"),
            );
            reply.header = reply
                .header
                .with_field(HeaderField::Signature, Value::Signature("u".to_string()));
            reply.body = vec![Value::UInt32(status)];
            Some(marshal_message(&reply))
        }
        (BUS_NAME, "AddMatch") | (BUS_NAME, "RemoveMatch") => {
            let mut reply = Message::new_method_return(
                BUS.read().next_bus_serial(),
                serial,
                header.sender().unwrap_or(":1"),
            );
            Some(marshal_message(&reply))
        }
        _ => None,
    }
}

fn marshal_message(msg: &Message) -> Vec<u8> {
    let mut marshaler = Marshaler::new(Endianness::Little);
    marshaler.marshal_message(msg);
    marshaler.buf
}

// ── Smoke Test ──────────────────────────────────────────────────────────

/// Verify D-Bus marshaling round-trip works.
pub fn smoke_check() -> Result<(), &'static str> {
    // Create a method call message
    let msg = Message::new_method_call(
        1,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "Hello",
    );

    // Marshal it
    let mut marshaler = Marshaler::new(Endianness::Little);
    marshaler.marshal_message(&msg);

    if marshaler.buf.is_empty() {
        return Err("Marshaler produced empty buffer");
    }

    // Unmarshal and verify header
    let mut unmarshaler =
        Unmarshaler::new(&marshaler.buf).map_err(|_| "Failed to create unmarshaler")?;
    let header = unmarshaler
        .parse_header()
        .map_err(|_| "Failed to parse header")?;

    if header.msg_type != MessageType::MethodCall {
        return Err("Message type mismatch after round-trip");
    }
    if header.serial != 1 {
        return Err("Serial mismatch after round-trip");
    }
    if header.destination() != Some("org.freedesktop.DBus") {
        return Err("Destination mismatch after round-trip");
    }
    if header.path() != Some("/org/freedesktop/DBus") {
        return Err("Path mismatch after round-trip");
    }
    if header.member() != Some("Hello") {
        return Err("Member mismatch after round-trip");
    }

    // Test bus initialization
    init()?;
    let mut bus = MessageBus::new();
    bus.init();
    if !bus.is_initialized() {
        return Err("Bus failed to initialize");
    }

    // Test in-kernel wire dispatch for Hello
    let hello = Message::new_method_call(
        42,
        BUS_NAME,
        BUS_PATH,
        BUS_NAME,
        "Hello",
    );
    let hello_bytes = marshal_message(&hello);
    let reply = process_wire_request(&hello_bytes).ok_or("Hello dispatch produced no reply")?;
    if reply.is_empty() {
        return Err("Hello reply was empty");
    }
    let mut reply_unmarshaler = Unmarshaler::new(&reply).map_err(|_| "Hello reply parse failed")?;
    let reply_header = reply_unmarshaler
        .parse_header()
        .map_err(|_| "Hello reply header parse failed")?;
    if reply_header.msg_type != MessageType::MethodReturn {
        return Err("Hello reply was not a method return");
    }

    let get_all = Message::new_method_call(
        43,
        BUS_NAME,
        BUS_PATH,
        PROPERTIES_IFACE,
        "GetAll",
    );
    let mut get_all_msg = get_all;
    get_all_msg.header = get_all_msg.header.with_field(
        HeaderField::Signature,
        Value::Signature("s".to_string()),
    );
    get_all_msg.body = vec![Value::String(BUS_NAME.to_string())];
    let get_all_bytes = marshal_message(&get_all_msg);
    let get_all_reply = process_wire_request(&get_all_bytes)
        .ok_or("Properties.GetAll dispatch produced no reply")?;
    if get_all_reply.is_empty() {
        return Err("Properties.GetAll reply was empty");
    }

    let introspect = Message::new_method_call(
        44,
        BUS_NAME,
        BUS_PATH,
        INTROSPECTABLE_IFACE,
        "Introspect",
    );
    let introspect_bytes = marshal_message(&introspect);
    let introspect_reply = process_wire_request(&introspect_bytes)
        .ok_or("Introspect dispatch produced no reply")?;
    if introspect_reply.is_empty() {
        return Err("Introspect reply was empty");
    }

    // Test connection
    let name = bus.connect().map_err(|_| "Failed to connect")?;
    if !name.starts_with(':') {
        return Err("Unique name should start with ':'");
    }

    // Test name registration
    bus.request_name(1, "org.gnome.Shell")
        .map_err(|_| "Failed to request name")?;

    Ok(())
}
