//! # Domain Name System (DNS) Implementation
//!
//! This module provides DNS client functionality for domain name resolution
//! in the RustOS network stack, supporting various record types and query operations.

use alloc::vec;
use alloc::{collections::BTreeMap, string::String, vec::Vec};
// use alloc::string::ToString; // Unused
use core::fmt;

use super::{Ipv4Address, NetworkError};

// Helper trait for string lowercase conversion in no_std
trait StringExt {
    fn to_lowercase(&self) -> String;
}

impl StringExt for str {
    fn to_lowercase(&self) -> String {
        crate::glib::ascii_strdown(self)
    }
}

/// DNS message header size (12 bytes)
pub const DNS_HEADER_SIZE: usize = 12;

/// DNS maximum message size over UDP
pub const DNS_MAX_UDP_SIZE: usize = 512;

/// DNS maximum message size over TCP
pub const DNS_MAX_TCP_SIZE: usize = 65535;

/// DNS default port
pub const DNS_PORT: u16 = 53;

/// DNS query classes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum DnsClass {
    /// Internet class
    In = 1,
    /// Chaos class (rarely used)
    Ch = 3,
    /// Hesiod class (rarely used)
    Hs = 4,
}

impl From<u16> for DnsClass {
    fn from(value: u16) -> Self {
        match value {
            1 => DnsClass::In,
            3 => DnsClass::Ch,
            4 => DnsClass::Hs,
            _ => DnsClass::In, // Default to Internet class
        }
    }
}

impl From<DnsClass> for u16 {
    fn from(class: DnsClass) -> Self {
        class as u16
    }
}

/// DNS resource record types
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[repr(u16)]
pub enum DnsRecordType {
    /// IPv4 address record
    A = 1,
    /// Name server record
    Ns = 2,
    /// Mail destination record (obsolete)
    Md = 3,
    /// Mail forwarder record (obsolete)
    Mf = 4,
    /// Canonical name record
    Cname = 5,
    /// Start of authority record
    Soa = 6,
    /// Mailbox record (experimental)
    Mb = 7,
    /// Mail group record (experimental)
    Mg = 8,
    /// Mail rename record (experimental)
    Mr = 9,
    /// Null record (experimental)
    Null = 10,
    /// Well-known service record
    Wks = 11,
    /// Pointer record
    Ptr = 12,
    /// Host information record
    Hinfo = 13,
    /// Mailbox information record
    Minfo = 14,
    /// Mail exchange record
    Mx = 15,
    /// Text record
    Txt = 16,
    /// IPv6 address record
    Aaaa = 28,
    /// Service record
    Srv = 33,
    /// Any record type (query only)
    Any = 255,
}

impl From<u16> for DnsRecordType {
    fn from(value: u16) -> Self {
        match value {
            1 => DnsRecordType::A,
            2 => DnsRecordType::Ns,
            3 => DnsRecordType::Md,
            4 => DnsRecordType::Mf,
            5 => DnsRecordType::Cname,
            6 => DnsRecordType::Soa,
            7 => DnsRecordType::Mb,
            8 => DnsRecordType::Mg,
            9 => DnsRecordType::Mr,
            10 => DnsRecordType::Null,
            11 => DnsRecordType::Wks,
            12 => DnsRecordType::Ptr,
            13 => DnsRecordType::Hinfo,
            14 => DnsRecordType::Minfo,
            15 => DnsRecordType::Mx,
            16 => DnsRecordType::Txt,
            28 => DnsRecordType::Aaaa,
            33 => DnsRecordType::Srv,
            255 => DnsRecordType::Any,
            _ => DnsRecordType::A, // Default to A record
        }
    }
}

impl From<DnsRecordType> for u16 {
    fn from(record_type: DnsRecordType) -> Self {
        record_type as u16
    }
}

impl fmt::Display for DnsRecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DnsRecordType::A => write!(f, "A"),
            DnsRecordType::Ns => write!(f, "NS"),
            DnsRecordType::Cname => write!(f, "CNAME"),
            DnsRecordType::Soa => write!(f, "SOA"),
            DnsRecordType::Ptr => write!(f, "PTR"),
            DnsRecordType::Mx => write!(f, "MX"),
            DnsRecordType::Txt => write!(f, "TXT"),
            DnsRecordType::Aaaa => write!(f, "AAAA"),
            DnsRecordType::Srv => write!(f, "SRV"),
            DnsRecordType::Any => write!(f, "ANY"),
            _ => write!(f, "{}", *self as u16),
        }
    }
}

/// DNS response codes
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DnsResponseCode {
    /// No error
    NoError = 0,
    /// Format error
    FormErr = 1,
    /// Server failure
    ServFail = 2,
    /// Name error (domain doesn't exist)
    NxDomain = 3,
    /// Not implemented
    NotImp = 4,
    /// Query refused
    Refused = 5,
}

impl From<u8> for DnsResponseCode {
    fn from(value: u8) -> Self {
        match value & 0x0F {
            0 => DnsResponseCode::NoError,
            1 => DnsResponseCode::FormErr,
            2 => DnsResponseCode::ServFail,
            3 => DnsResponseCode::NxDomain,
            4 => DnsResponseCode::NotImp,
            5 => DnsResponseCode::Refused,
            _ => DnsResponseCode::ServFail,
        }
    }
}

impl From<DnsResponseCode> for u8 {
    fn from(code: DnsResponseCode) -> Self {
        code as u8
    }
}

impl fmt::Display for DnsResponseCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DnsResponseCode::NoError => write!(f, "NOERROR"),
            DnsResponseCode::FormErr => write!(f, "FORMERR"),
            DnsResponseCode::ServFail => write!(f, "SERVFAIL"),
            DnsResponseCode::NxDomain => write!(f, "NXDOMAIN"),
            DnsResponseCode::NotImp => write!(f, "NOTIMP"),
            DnsResponseCode::Refused => write!(f, "REFUSED"),
        }
    }
}

/// DNS message header
#[derive(Debug, Clone)]
pub struct DnsHeader {
    /// Message ID
    pub id: u16,
    /// Query/Response flag (false = query, true = response)
    pub qr: bool,
    /// Operation code
    pub opcode: u8,
    /// Authoritative answer flag
    pub aa: bool,
    /// Truncation flag
    pub tc: bool,
    /// Recursion desired flag
    pub rd: bool,
    /// Recursion available flag
    pub ra: bool,
    /// Reserved (must be zero)
    pub z: u8,
    /// Response code
    pub rcode: DnsResponseCode,
    /// Number of questions
    pub qdcount: u16,
    /// Number of answer records
    pub ancount: u16,
    /// Number of name server records
    pub nscount: u16,
    /// Number of additional records
    pub arcount: u16,
}

impl DnsHeader {
    /// Create a new DNS header
    pub fn new(id: u16) -> Self {
        Self {
            id,
            qr: false,
            opcode: 0,
            aa: false,
            tc: false,
            rd: true, // Default to recursion desired
            ra: false,
            z: 0,
            rcode: DnsResponseCode::NoError,
            qdcount: 0,
            ancount: 0,
            nscount: 0,
            arcount: 0,
        }
    }

    /// Parse DNS header from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, NetworkError> {
        if data.len() < DNS_HEADER_SIZE {
            return Err(NetworkError::InvalidPacket);
        }

        let id = u16::from_be_bytes([data[0], data[1]]);

        let flags = u16::from_be_bytes([data[2], data[3]]);
        let qr = (flags & 0x8000) != 0;
        let opcode = ((flags >> 11) & 0x0F) as u8;
        let aa = (flags & 0x0400) != 0;
        let tc = (flags & 0x0200) != 0;
        let rd = (flags & 0x0100) != 0;
        let ra = (flags & 0x0080) != 0;
        let z = ((flags >> 4) & 0x07) as u8;
        let rcode = DnsResponseCode::from((flags & 0x0F) as u8);

        let qdcount = u16::from_be_bytes([data[4], data[5]]);
        let ancount = u16::from_be_bytes([data[6], data[7]]);
        let nscount = u16::from_be_bytes([data[8], data[9]]);
        let arcount = u16::from_be_bytes([data[10], data[11]]);

        Ok(Self {
            id,
            qr,
            opcode,
            aa,
            tc,
            rd,
            ra,
            z,
            rcode,
            qdcount,
            ancount,
            nscount,
            arcount,
        })
    }

    /// Convert header to bytes
    pub fn to_bytes(&self) -> [u8; DNS_HEADER_SIZE] {
        let mut bytes = [0u8; DNS_HEADER_SIZE];

        bytes[0..2].copy_from_slice(&self.id.to_be_bytes());

        let flags = (if self.qr { 0x8000 } else { 0 })
            | ((self.opcode as u16) << 11)
            | (if self.aa { 0x0400 } else { 0 })
            | (if self.tc { 0x0200 } else { 0 })
            | (if self.rd { 0x0100 } else { 0 })
            | (if self.ra { 0x0080 } else { 0 })
            | ((self.z as u16) << 4)
            | (self.rcode as u16);

        bytes[2..4].copy_from_slice(&flags.to_be_bytes());
        bytes[4..6].copy_from_slice(&self.qdcount.to_be_bytes());
        bytes[6..8].copy_from_slice(&self.ancount.to_be_bytes());
        bytes[8..10].copy_from_slice(&self.nscount.to_be_bytes());
        bytes[10..12].copy_from_slice(&self.arcount.to_be_bytes());

        bytes
    }
}

/// DNS question structure
#[derive(Debug, Clone)]
pub struct DnsQuestion {
    /// Domain name
    pub name: String,
    /// Query type
    pub qtype: DnsRecordType,
    /// Query class
    pub qclass: DnsClass,
}

impl DnsQuestion {
    pub fn new(name: String, qtype: DnsRecordType, qclass: DnsClass) -> Self {
        Self {
            name,
            qtype,
            qclass,
        }
    }
}

/// DNS resource record
#[derive(Debug, Clone)]
pub struct DnsResourceRecord {
    /// Domain name
    pub name: String,
    /// Record type
    pub rtype: DnsRecordType,
    /// Record class
    pub rclass: DnsClass,
    /// Time to live in seconds
    pub ttl: u32,
    /// Record data
    pub rdata: Vec<u8>,
}

impl DnsResourceRecord {
    pub fn new(
        name: String,
        rtype: DnsRecordType,
        rclass: DnsClass,
        ttl: u32,
        rdata: Vec<u8>,
    ) -> Self {
        Self {
            name,
            rtype,
            rclass,
            ttl,
            rdata,
        }
    }

    /// Get IPv4 address from A record data
    pub fn as_ipv4_address(&self) -> Result<Ipv4Address, NetworkError> {
        if self.rtype != DnsRecordType::A || self.rdata.len() != 4 {
            return Err(NetworkError::ProtocolError);
        }
        Ok([self.rdata[0], self.rdata[1], self.rdata[2], self.rdata[3]])
    }

    /// Get text from TXT record data
    pub fn as_text(&self) -> Result<String, NetworkError> {
        if self.rtype != DnsRecordType::Txt {
            return Err(NetworkError::ProtocolError);
        }

        let mut text = String::new();
        let mut offset = 0;

        while offset < self.rdata.len() {
            let len = self.rdata[offset] as usize;
            offset += 1;

            if offset + len > self.rdata.len() {
                return Err(NetworkError::InvalidPacket);
            }

            let segment = core::str::from_utf8(&self.rdata[offset..offset + len])
                .map_err(|_| NetworkError::ProtocolError)?;

            if !text.is_empty() {
                text.push(' ');
            }
            text.push_str(segment);
            offset += len;
        }

        Ok(text)
    }
}

/// DNS message structure
#[derive(Debug, Clone)]
pub struct DnsMessage {
    /// Message header
    pub header: DnsHeader,
    /// Questions section
    pub questions: Vec<DnsQuestion>,
    /// Answers section
    pub answers: Vec<DnsResourceRecord>,
    /// Authority section
    pub authority: Vec<DnsResourceRecord>,
    /// Additional section
    pub additional: Vec<DnsResourceRecord>,
}

impl DnsMessage {
    /// Create a new DNS query message
    pub fn new_query(id: u16, name: String, qtype: DnsRecordType) -> Self {
        let mut header = DnsHeader::new(id);
        header.qdcount = 1;

        let question = DnsQuestion::new(name, qtype, DnsClass::In);

        Self {
            header,
            questions: vec![question],
            answers: Vec::new(),
            authority: Vec::new(),
            additional: Vec::new(),
        }
    }

    /// Create a DNS response message
    pub fn new_response(query: &DnsMessage) -> Self {
        let mut header = query.header.clone();
        header.qr = true;
        header.rd = false;
        header.ra = true;

        Self {
            header,
            questions: query.questions.clone(),
            answers: Vec::new(),
            authority: Vec::new(),
            additional: Vec::new(),
        }
    }

    /// Parse DNS message from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self, NetworkError> {
        if data.len() < DNS_HEADER_SIZE {
            return Err(NetworkError::InvalidPacket);
        }

        let header = DnsHeader::from_bytes(data)?;
        let mut offset = DNS_HEADER_SIZE;

        // Parse questions
        let mut questions = Vec::new();
        for _ in 0..header.qdcount {
            let (question, new_offset) = Self::parse_question(data, offset)?;
            questions.push(question);
            offset = new_offset;
        }

        // Parse answers
        let mut answers = Vec::new();
        for _ in 0..header.ancount {
            let (record, new_offset) = Self::parse_resource_record(data, offset)?;
            answers.push(record);
            offset = new_offset;
        }

        // Parse authority records
        let mut authority = Vec::new();
        for _ in 0..header.nscount {
            let (record, new_offset) = Self::parse_resource_record(data, offset)?;
            authority.push(record);
            offset = new_offset;
        }

        // Parse additional records
        let mut additional = Vec::new();
        for _ in 0..header.arcount {
            let (record, new_offset) = Self::parse_resource_record(data, offset)?;
            additional.push(record);
            offset = new_offset;
        }

        Ok(Self {
            header,
            questions,
            answers,
            authority,
            additional,
        })
    }

    /// Convert message to bytes
    pub fn to_bytes(&self) -> Result<Vec<u8>, NetworkError> {
        let mut bytes = Vec::new();

        // Add header
        bytes.extend_from_slice(&self.header.to_bytes());

        // Add questions
        for question in &self.questions {
            Self::write_name(&mut bytes, &question.name);
            bytes.extend_from_slice(&u16::to_be_bytes(question.qtype.into()));
            bytes.extend_from_slice(&u16::to_be_bytes(question.qclass.into()));
        }

        // Add answers
        for record in &self.answers {
            Self::write_name(&mut bytes, &record.name);
            bytes.extend_from_slice(&u16::to_be_bytes(record.rtype.into()));
            bytes.extend_from_slice(&u16::to_be_bytes(record.rclass.into()));
            bytes.extend_from_slice(&record.ttl.to_be_bytes());
            bytes.extend_from_slice(&u16::to_be_bytes(record.rdata.len() as u16));
            bytes.extend_from_slice(&record.rdata);
        }

        // Add authority records
        for record in &self.authority {
            Self::write_name(&mut bytes, &record.name);
            bytes.extend_from_slice(&u16::to_be_bytes(record.rtype.into()));
            bytes.extend_from_slice(&u16::to_be_bytes(record.rclass.into()));
            bytes.extend_from_slice(&record.ttl.to_be_bytes());
            bytes.extend_from_slice(&u16::to_be_bytes(record.rdata.len() as u16));
            bytes.extend_from_slice(&record.rdata);
        }

        // Add additional records
        for record in &self.additional {
            Self::write_name(&mut bytes, &record.name);
            bytes.extend_from_slice(&u16::to_be_bytes(record.rtype.into()));
            bytes.extend_from_slice(&u16::to_be_bytes(record.rclass.into()));
            bytes.extend_from_slice(&record.ttl.to_be_bytes());
            bytes.extend_from_slice(&u16::to_be_bytes(record.rdata.len() as u16));
            bytes.extend_from_slice(&record.rdata);
        }

        Ok(bytes)
    }

    fn parse_question(data: &[u8], offset: usize) -> Result<(DnsQuestion, usize), NetworkError> {
        let (name, offset) = Self::parse_name(data, offset)?;

        if offset + 4 > data.len() {
            return Err(NetworkError::InvalidPacket);
        }

        let qtype = DnsRecordType::from(u16::from_be_bytes([data[offset], data[offset + 1]]));
        let qclass = DnsClass::from(u16::from_be_bytes([data[offset + 2], data[offset + 3]]));

        Ok((DnsQuestion::new(name, qtype, qclass), offset + 4))
    }

    fn parse_resource_record(
        data: &[u8],
        offset: usize,
    ) -> Result<(DnsResourceRecord, usize), NetworkError> {
        let (name, mut offset) = Self::parse_name(data, offset)?;

        if offset + 10 > data.len() {
            return Err(NetworkError::InvalidPacket);
        }

        let rtype = DnsRecordType::from(u16::from_be_bytes([data[offset], data[offset + 1]]));
        let rclass = DnsClass::from(u16::from_be_bytes([data[offset + 2], data[offset + 3]]));
        let ttl = u32::from_be_bytes([
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]);
        let rdlength = u16::from_be_bytes([data[offset + 8], data[offset + 9]]) as usize;

        offset += 10;

        if offset + rdlength > data.len() {
            return Err(NetworkError::InvalidPacket);
        }

        let rdata = data[offset..offset + rdlength].to_vec();

        Ok((
            DnsResourceRecord::new(name, rtype, rclass, ttl, rdata),
            offset + rdlength,
        ))
    }

    fn parse_name(data: &[u8], mut offset: usize) -> Result<(String, usize), NetworkError> {
        // Cap total compression-pointer follows to prevent cyclic pointers from
        // hanging the kernel forever. A valid name can never need more jumps than
        // there are bytes in the message.
        const MAX_JUMPS: usize = 128;
        let mut jumps = 0usize;

        let mut name = String::new();
        let mut jumped = false;
        let mut jump_offset = 0;

        loop {
            if offset >= data.len() {
                return Err(NetworkError::InvalidPacket);
            }

            let len = data[offset];

            if len == 0 {
                // End of name
                if jumped {
                    return Ok((name, jump_offset + 1));
                } else {
                    return Ok((name, offset + 1));
                }
            } else if (len & 0xC0) == 0xC0 {
                // Compression pointer
                if offset + 1 >= data.len() {
                    return Err(NetworkError::InvalidPacket);
                }

                if !jumped {
                    jump_offset = offset + 1;
                    jumped = true;
                }

                jumps += 1;
                if jumps > MAX_JUMPS {
                    // Cyclic or malicious compression pointers - bail out.
                    return Err(NetworkError::InvalidPacket);
                }

                offset = (((len & 0x3F) as usize) << 8) | (data[offset + 1] as usize);
                continue;
            } else if len > 63 {
                return Err(NetworkError::InvalidPacket);
            }

            offset += 1;

            if offset + len as usize > data.len() {
                return Err(NetworkError::InvalidPacket);
            }

            if !name.is_empty() {
                name.push('.');
            }

            let label = core::str::from_utf8(&data[offset..offset + len as usize])
                .map_err(|_| NetworkError::ProtocolError)?;
            name.push_str(label);

            offset += len as usize;
        }
    }

    fn write_name(bytes: &mut Vec<u8>, name: &str) {
        for label in name.split('.') {
            bytes.push(label.len() as u8);
            bytes.extend_from_slice(label.as_bytes());
        }
        bytes.push(0); // End of name
    }
}

/// DNS resolver for caching and managing queries
#[derive(Debug)]
pub struct DnsResolver {
    /// DNS servers to query
    servers: Vec<Ipv4Address>,
    /// Query cache
    cache: BTreeMap<(String, DnsRecordType), (Vec<DnsResourceRecord>, u64)>,
    /// Next query ID
    next_id: u16,
    /// Resolver statistics
    stats: DnsResolverStats,
}

impl DnsResolver {
    pub fn new() -> Self {
        Self {
            servers: Vec::new(),
            cache: BTreeMap::new(),
            next_id: 1,
            stats: DnsResolverStats::default(),
        }
    }

    /// Add DNS server
    pub fn add_server(&mut self, server: Ipv4Address) {
        if !self.servers.contains(&server) {
            self.servers.push(server);
        }
    }

    /// Remove DNS server
    pub fn remove_server(&mut self, server: Ipv4Address) {
        self.servers.retain(|&s| s != server);
    }

    /// Create DNS query
    pub fn create_query(&mut self, name: String, record_type: DnsRecordType) -> DnsMessage {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.stats.queries_created += 1;
        DnsMessage::new_query(id, name, record_type)
    }

    /// Process DNS response and update cache
    pub fn process_response(
        &mut self,
        response: DnsMessage,
        timestamp: u64,
    ) -> Result<(), NetworkError> {
        if response.header.rcode != DnsResponseCode::NoError {
            self.stats.errors += 1;
            return Err(NetworkError::ProtocolError);
        }

        // Cache the answers
        for question in &response.questions {
            let answers: Vec<_> = response
                .answers
                .iter()
                .filter(|record| {
                    record.name.eq_ignore_ascii_case(&question.name)
                        && record.rtype == question.qtype
                })
                .cloned()
                .collect();

            if !answers.is_empty() {
                let key = (question.name.clone(), question.qtype);
                self.cache.insert(key, (answers, timestamp));
                self.stats.cache_entries += 1;
            }
        }

        self.stats.responses_processed += 1;
        Ok(())
    }

    /// Resolve name from cache
    pub fn resolve_cached(
        &mut self,
        name: &str,
        record_type: DnsRecordType,
        timestamp: u64,
    ) -> Option<Vec<DnsResourceRecord>> {
        let key = (name.to_lowercase(), record_type);

        if let Some((records, cache_time)) = self.cache.get(&key) {
            // Check if any record is still valid (not expired)
            let valid_records: Vec<_> = records
                .iter()
                .filter(|record| cache_time + record.ttl as u64 > timestamp)
                .cloned()
                .collect();

            if !valid_records.is_empty() {
                self.stats.cache_hits += 1;
                return Some(valid_records);
            } else {
                // Remove expired entries
                self.cache.remove(&key);
                self.stats.cache_entries -= 1;
            }
        }

        self.stats.cache_misses += 1;
        None
    }

    /// Resolve IPv4 address for hostname
    pub fn resolve_ipv4(&mut self, hostname: &str, timestamp: u64) -> Option<Vec<Ipv4Address>> {
        self.resolve_cached(hostname, DnsRecordType::A, timestamp)
            .map(|records| {
                records
                    .iter()
                    .filter_map(|record| record.as_ipv4_address().ok())
                    .collect()
            })
    }

    /// Clean expired entries from cache
    pub fn cleanup_cache(&mut self, timestamp: u64) {
        let expired_keys: Vec<_> = self
            .cache
            .iter()
            .filter_map(|(key, (records, cache_time))| {
                let min_ttl = records.iter().map(|r| r.ttl).min().unwrap_or(0);
                if cache_time + min_ttl as u64 <= timestamp {
                    Some(key.clone())
                } else {
                    None
                }
            })
            .collect();

        for key in expired_keys {
            self.cache.remove(&key);
            self.stats.cache_entries -= 1;
        }
    }

    /// Get resolver statistics
    pub fn stats(&self) -> &DnsResolverStats {
        &self.stats
    }

    /// Get configured servers
    pub fn servers(&self) -> &[Ipv4Address] {
        &self.servers
    }

    /// Clear cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
        self.stats.cache_entries = 0;
    }
}

impl Default for DnsResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// DNS resolver statistics
#[derive(Debug, Default, Clone)]
pub struct DnsResolverStats {
    pub queries_created: u64,
    pub responses_processed: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
    pub cache_entries: u64,
    pub errors: u64,
}

/// Error type for the synchronous DNS resolver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DnsError {
    /// Hostname does not exist (NXDOMAIN)
    NotFound,
    /// Server failure (SERVFAIL / REFUSED)
    ServerError,
    /// Query timed out after all retries
    Timeout,
    /// Could not send or bind the UDP socket
    NetworkError,
    /// Response packet is malformed
    MalformedResponse,
}

impl DnsResolver {
    /// Resolve `hostname` to an IPv4 address using `dns_server`.
    ///
    /// Synchronous spin-wait: sends a UDP query and busy-waits up to 5 seconds
    /// per attempt, retrying up to 3 times.  Successful results are cached.
    ///
    /// # no_std constraints
    /// No floating point.  Uses `crate::time::get_system_time_ms()` for timing.
    pub fn resolve(
        &mut self,
        hostname: &str,
        dns_server: Ipv4Address,
        current_time: u64,
    ) -> Result<Ipv4Address, DnsError> {
        // Cache hit short-circuit
        if let Some(addrs) = self.resolve_ipv4(hostname, current_time) {
            if let Some(&addr) = addrs.first() {
                return Ok(addr);
            }
        }

        const MAX_RETRIES: u8 = 3;
        const TIMEOUT_MS: u64 = 5_000;

        // Ephemeral source port derived from time (avoids 0 and well-known ports)
        let src_port: u16 = 49152u16.wrapping_add((current_time & 0x3FFF) as u16);
        let src_addr = super::NetworkAddress::IPv4([0u8; 4]);
        let dst_addr = super::NetworkAddress::IPv4(dns_server);

        // Bind a UDP socket so the stack routes replies to us
        let _ = super::udp::udp_bind(src_addr, src_port);

        for _attempt in 0..MAX_RETRIES {
            let query = self.create_query(String::from(hostname), DnsRecordType::A);
            let query_id = query.header.id;
            let query_bytes = match query.to_bytes() {
                Ok(b) => b,
                Err(_) => {
                    let _ = super::udp::udp_close(src_addr, src_port);
                    return Err(DnsError::MalformedResponse);
                }
            };

            if super::udp::send_udp_packet(src_addr, src_port, dst_addr, DNS_PORT, &query_bytes)
                .is_err()
            {
                let _ = super::udp::udp_close(src_addr, src_port);
                return Err(DnsError::NetworkError);
            }

            let deadline = crate::time::get_system_time_ms() + TIMEOUT_MS;
            loop {
                let now = crate::time::get_system_time_ms();
                if now >= deadline {
                    break; // timed out — next attempt
                }

                if let Ok(Some((data, _from_ip, _from_port))) =
                    super::udp::udp_recv(src_addr, src_port)
                {
                    match DnsMessage::from_bytes(&data) {
                        Ok(response) if response.header.qr && response.header.id == query_id => {
                            match response.header.rcode {
                                DnsResponseCode::NxDomain => {
                                    let _ = super::udp::udp_close(src_addr, src_port);
                                    return Err(DnsError::NotFound);
                                }
                                DnsResponseCode::ServFail | DnsResponseCode::Refused => {
                                    let _ = super::udp::udp_close(src_addr, src_port);
                                    return Err(DnsError::ServerError);
                                }
                                DnsResponseCode::NoError => {
                                    let addrs: Vec<Ipv4Address> = response
                                        .answers
                                        .iter()
                                        .filter(|r| r.rtype == DnsRecordType::A)
                                        .filter_map(|r| r.as_ipv4_address().ok())
                                        .collect();
                                    if let Some(&addr) = addrs.first() {
                                        // Cache the result
                                        let min_ttl = response
                                            .answers
                                            .iter()
                                            .filter(|r| r.rtype == DnsRecordType::A)
                                            .map(|r| r.ttl)
                                            .min()
                                            .unwrap_or(60);
                                        let records: Vec<DnsResourceRecord> = addrs
                                            .iter()
                                            .map(|&ip| DnsResourceRecord::new(
                                                String::from(hostname),
                                                DnsRecordType::A,
                                                DnsClass::In,
                                                min_ttl,
                                                ip.to_vec(),
                                            ))
                                            .collect();
                                        let key = (
                                            StringExt::to_lowercase(hostname),
                                            DnsRecordType::A,
                                        );
                                        self.cache.insert(key, (records, now));
                                        self.stats.cache_entries += 1;
                                        let _ = super::udp::udp_close(src_addr, src_port);
                                        return Ok(addr);
                                    }
                                    // NoError but no A records — retry
                                }
                                _ => {}
                            }
                        }
                        _ => {} // malformed / wrong ID — keep waiting
                    }
                }
                core::hint::spin_loop();
            }
        }

        let _ = super::udp::udp_close(src_addr, src_port);
        Err(DnsError::Timeout)
    }
}

/// High-level DNS functions
pub fn create_dns_query(id: u16, hostname: &str, record_type: DnsRecordType) -> Vec<u8> {
    let message = DnsMessage::new_query(id, String::from(hostname), record_type);
    message.to_bytes().unwrap_or_default()
}

pub fn parse_dns_response(data: &[u8]) -> Result<DnsMessage, NetworkError> {
    DnsMessage::from_bytes(data)
}

pub fn extract_ipv4_addresses(message: &DnsMessage) -> Vec<Ipv4Address> {
    message
        .answers
        .iter()
        .filter(|record| record.rtype == DnsRecordType::A)
        .filter_map(|record| record.as_ipv4_address().ok())
        .collect()
}

#[cfg(all(test, feature = "std-tests"))]
mod tests {
    use super::*;

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_dns_record_types() {
        assert_eq!(u16::from(DnsRecordType::A), 1);
        assert_eq!(u16::from(DnsRecordType::Ns), 2);
        assert_eq!(u16::from(DnsRecordType::Cname), 5);
        assert_eq!(u16::from(DnsRecordType::Mx), 15);

        assert_eq!(DnsRecordType::from(1u16), DnsRecordType::A);
        assert_eq!(DnsRecordType::from(2u16), DnsRecordType::Ns);
        assert_eq!(DnsRecordType::from(5u16), DnsRecordType::Cname);
        assert_eq!(DnsRecordType::from(15u16), DnsRecordType::Mx);
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_dns_header() {
        let mut header = DnsHeader::new(12345);
        header.qdcount = 1;
        header.rd = true;

        let bytes = header.to_bytes();
        let parsed_header = DnsHeader::from_bytes(&bytes).unwrap();

        assert_eq!(parsed_header.id, 12345);
        assert_eq!(parsed_header.qdcount, 1);
        assert_eq!(parsed_header.rd, true);
        assert_eq!(parsed_header.qr, false);
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_dns_query_creation() {
        let message = DnsMessage::new_query(1001, String::from("example.com"), DnsRecordType::A);

        assert_eq!(message.header.id, 1001);
        assert_eq!(message.header.qdcount, 1);
        assert_eq!(message.questions.len(), 1);
        assert_eq!(message.questions[0].name, "example.com");
        assert_eq!(message.questions[0].qtype, DnsRecordType::A);
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_dns_resource_record() {
        let ip_bytes = vec![192, 168, 1, 1];
        let record = DnsResourceRecord::new(
            String::from("example.com"),
            DnsRecordType::A,
            DnsClass::In,
            3600,
            ip_bytes.clone(),
        );

        assert_eq!(record.name, "example.com");
        assert_eq!(record.rtype, DnsRecordType::A);
        assert_eq!(record.ttl, 3600);
        assert_eq!(record.rdata, ip_bytes);

        let ip_addr = record.as_ipv4_address().unwrap();
        assert_eq!(ip_addr, Ipv4Address::new(192, 168, 1, 1));
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_dns_resolver() {
        let mut resolver = DnsResolver::new();
        let server = Ipv4Address::new(8, 8, 8, 8);

        resolver.add_server(server);
        assert_eq!(resolver.servers().len(), 1);
        assert_eq!(resolver.servers()[0], server);

        let query = resolver.create_query(String::from("example.com"), DnsRecordType::A);
        assert_eq!(query.header.qdcount, 1);
        assert_eq!(query.questions[0].name, "example.com");

        // Test cache miss
        assert!(resolver.resolve_ipv4("example.com", 1000).is_none());
        assert_eq!(resolver.stats().cache_misses, 1);
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_dns_response_codes() {
        assert_eq!(u8::from(DnsResponseCode::NoError), 0);
        assert_eq!(u8::from(DnsResponseCode::FormErr), 1);
        assert_eq!(u8::from(DnsResponseCode::ServFail), 2);
        assert_eq!(u8::from(DnsResponseCode::NxDomain), 3);

        assert_eq!(DnsResponseCode::from(0u8), DnsResponseCode::NoError);
        assert_eq!(DnsResponseCode::from(1u8), DnsResponseCode::FormErr);
        assert_eq!(DnsResponseCode::from(2u8), DnsResponseCode::ServFail);
        assert_eq!(DnsResponseCode::from(3u8), DnsResponseCode::NxDomain);
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_dns_classes() {
        assert_eq!(u16::from(DnsClass::In), 1);
        assert_eq!(u16::from(DnsClass::Ch), 3);
        assert_eq!(u16::from(DnsClass::Hs), 4);

        assert_eq!(DnsClass::from(1u16), DnsClass::In);
        assert_eq!(DnsClass::from(3u16), DnsClass::Ch);
        assert_eq!(DnsClass::from(4u16), DnsClass::Hs);
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_record_type_display() {
        assert_eq!(format!("{}", DnsRecordType::A), "A");
        assert_eq!(format!("{}", DnsRecordType::Ns), "NS");
        assert_eq!(format!("{}", DnsRecordType::Cname), "CNAME");
        assert_eq!(format!("{}", DnsRecordType::Mx), "MX");
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_response_code_display() {
        assert_eq!(format!("{}", DnsResponseCode::NoError), "NOERROR");
        assert_eq!(format!("{}", DnsResponseCode::FormErr), "FORMERR");
        assert_eq!(format!("{}", DnsResponseCode::ServFail), "SERVFAIL");
        assert_eq!(format!("{}", DnsResponseCode::NxDomain), "NXDOMAIN");
    }

    #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_txt_record() {
        let txt_data = vec![
            5, b'h', b'e', b'l', b'l', b'o', 5, b'w', b'o', b'r', b'l', b'd',
        ];
        let record = DnsResourceRecord::new(
            String::from("example.com"),
            DnsRecordType::Txt,
            DnsClass::In,
            3600,
            txt_data,
        );

        let text = record.as_text().unwrap();
        assert_eq!(text, "hello world");
    }
}
