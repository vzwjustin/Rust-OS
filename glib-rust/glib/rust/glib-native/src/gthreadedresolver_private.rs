//! `gthreadedresolver-private` matching `gio/gthreadedresolver-private.h`.
//!
//! Private threaded resolver API: DNS record parsing and type conversion.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;

/// DNS record types (mirrors `GResolverRecordType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolverRecordType {
    Srv = 1,
    Mx = 2,
    Txt = 3,
    Soa = 4,
    Ns = 5,
}

/// Converts a `ResolverRecordType` to DNS RR type (mirrors `g_resolver_record_type_to_rrtype`).
pub fn record_type_to_rrtype(record_type: ResolverRecordType) -> u16 {
    match record_type {
        ResolverRecordType::Srv => 33, // DNS_TYPE_SRV
        ResolverRecordType::Mx => 15,  // DNS_TYPE_MX
        ResolverRecordType::Txt => 16, // DNS_TYPE_TXT
        ResolverRecordType::Soa => 6,  // DNS_TYPE_SOA
        ResolverRecordType::Ns => 2,   // DNS_TYPE_NS
    }
}

/// A parsed DNS record.
#[derive(Debug, Clone)]
pub struct DnsRecord {
    pub record_type: ResolverRecordType,
    pub name: String,
    pub priority: u16,
    pub weight: u16,
    pub port: u16,
    pub target: String,
    pub ttl: u32,
}

/// Parses DNS records from a raw DNS response (mirrors `g_resolver_records_from_res_query`).
///
/// In our no_std port, we do a minimal parse of the response buffer.
pub fn records_from_res_query(
    rrname: &str,
    _rrtype: ResolverRecordType,
    answer: &[u8],
    _herr: i32,
) -> Result<Vec<DnsRecord>, String> {
    if answer.is_empty() {
        return Err("empty DNS response".to_string());
    }

    // Minimal DNS header parsing: skip 12-byte header
    if answer.len() < 12 {
        return Err("DNS response too short".to_string());
    }

    let qdcount = u16::from_be_bytes([answer[4], answer[5]]);
    let ancount = u16::from_be_bytes([answer[6], answer[7]]);

    if ancount == 0 {
        return Ok(Vec::new());
    }

    // Skip question section
    let mut pos = 12usize;
    for _ in 0..qdcount {
        pos = skip_name(answer, pos)?;
        pos += 4; // qtype + qclass
        if pos > answer.len() {
            return Err("malformed question section".to_string());
        }
    }

    // Parse answer records
    let mut records = Vec::new();
    for _ in 0..ancount {
        if pos >= answer.len() {
            break;
        }
        pos = skip_name(answer, pos)?;
        if pos + 10 > answer.len() {
            break;
        }
        let _type = u16::from_be_bytes([answer[pos], answer[pos + 1]]);
        let _class = u16::from_be_bytes([answer[pos + 2], answer[pos + 3]]);
        let ttl = u32::from_be_bytes([
            answer[pos + 4],
            answer[pos + 5],
            answer[pos + 6],
            answer[pos + 7],
        ]);
        let rdlength = u16::from_be_bytes([answer[pos + 8], answer[pos + 9]]) as usize;
        pos += 10;

        if pos + rdlength > answer.len() {
            break;
        }

        // For SRV records: priority(2) + weight(2) + port(2) + target
        if _type == 33 && rdlength >= 6 {
            let priority = u16::from_be_bytes([answer[pos], answer[pos + 1]]);
            let weight = u16::from_be_bytes([answer[pos + 2], answer[pos + 3]]);
            let port = u16::from_be_bytes([answer[pos + 4], answer[pos + 5]]);
            let target = String::from_utf8_lossy(&answer[pos + 6..pos + rdlength]).to_string();
            records.push(DnsRecord {
                record_type: ResolverRecordType::Srv,
                name: rrname.to_string(),
                priority,
                weight,
                port,
                target,
                ttl,
            });
        }

        pos += rdlength;
    }

    Ok(records)
}

/// Skips a DNS name (possibly compressed) in the answer buffer.
fn skip_name(answer: &[u8], mut pos: usize) -> Result<usize, String> {
    loop {
        if pos >= answer.len() {
            return Err("name extends past end of buffer".to_string());
        }
        let len = answer[pos] as usize;
        if len == 0 {
            pos += 1;
            break;
        }
        if len & 0xC0 == 0xC0 {
            // Compressed pointer
            pos += 2;
            break;
        }
        pos += 1 + len;
    }
    Ok(pos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_type_to_rrtype() {
        assert_eq!(record_type_to_rrtype(ResolverRecordType::Srv), 33);
        assert_eq!(record_type_to_rrtype(ResolverRecordType::Mx), 15);
        assert_eq!(record_type_to_rrtype(ResolverRecordType::Txt), 16);
        assert_eq!(record_type_to_rrtype(ResolverRecordType::Soa), 6);
        assert_eq!(record_type_to_rrtype(ResolverRecordType::Ns), 2);
    }

    #[test]
    fn test_empty_response() {
        let result =
            records_from_res_query("_test._tcp.example.com", ResolverRecordType::Srv, &[], 0);
        assert!(result.is_err());
    }

    #[test]
    fn test_short_response() {
        let result = records_from_res_query(
            "_test._tcp.example.com",
            ResolverRecordType::Srv,
            &[0, 1, 2],
            0,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_no_answers() {
        // DNS header: id=0, flags=0, qd=0, an=0, ns=0, ar=0
        let response = [0u8; 12];
        let result = records_from_res_query(
            "_test._tcp.example.com",
            ResolverRecordType::Srv,
            &response,
            0,
        );
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }
}
