//! TZif binary timezone database parser (RFC 8536).
//!
//! Parses version 1 (32-bit transition times) and version 2/3 (64-bit) zoneinfo
//! files. Fully `no_std` compatible using `alloc`.

use crate::prelude::*;

/// Parsed TZif timezone data.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TzifData {
    transitions: Vec<i64>,
    type_indices: Vec<u8>,
    types: Vec<TzifType>,
    abbreviations: Vec<u8>,
}

/// One local-time type entry from a TZif file.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TzifType {
    /// UTC offset in seconds.
    pub ut_offset: i32,
    /// Whether this type is daylight saving time.
    pub is_dst: bool,
    /// Index into the abbreviation character array.
    pub abbr_index: u8,
}

/// Errors while parsing TZif data.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TzifError {
    /// Input too short for a TZif header.
    TooShort,
    /// Magic bytes are not `TZif`.
    BadMagic,
    /// Unsupported version byte.
    UnsupportedVersion,
    /// Truncated or malformed data block.
    Truncated,
    /// Header counts are inconsistent with payload size.
    InvalidCounts,
    /// No local-time type records present.
    NoTypes,
}

struct TzifHeader {
    version: u8,
    ttisutcnt: u32,
    ttisgmtcnt: u32,
    leapcnt: u32,
    timecnt: u32,
    typecnt: u32,
    charcnt: u32,
}

impl TzifData {
    /// Parse TZif bytes (v1 or v2/v3).
    pub fn parse(data: &[u8]) -> Result<Self, TzifError> {
        let mut offset = 0usize;
        let header = read_header(data, &mut offset)?;
        let version = header.version;

        if version == 0 {
            let block = parse_data_block(data, &mut offset, false, &header)?;
            return Ok(block);
        }

        if version == b'2' || version == b'3' {
            // Version 2/3 files embed a v1-compatible block first, then the real block.
            let _v1 = parse_data_block(data, &mut offset, false, &header)?;
            let header2 = read_header(data, &mut offset)?;
            if header2.version != version {
                return Err(TzifError::Truncated);
            }
            let block = parse_data_block(data, &mut offset, true, &header2)?;
            return Ok(block);
        }

        Err(TzifError::UnsupportedVersion)
    }

    /// UTC offset in seconds at a Unix timestamp.
    pub fn offset_at(&self, unix: i64) -> i32 {
        let type_index = self.type_index_at(unix);
        self.types.get(type_index).map(|t| t.ut_offset).unwrap_or(0)
    }

    /// Whether DST is in effect at a Unix timestamp.
    pub fn is_dst_at(&self, unix: i64) -> bool {
        let type_index = self.type_index_at(unix);
        self.types
            .get(type_index)
            .map(|t| t.is_dst)
            .unwrap_or(false)
    }

    /// Abbreviation string (e.g. `EST`) at a Unix timestamp.
    pub fn abbreviation_at(&self, unix: i64) -> &str {
        let type_index = self.type_index_at(unix);
        let abbr_index = self
            .types
            .get(type_index)
            .map(|t| t.abbr_index as usize)
            .unwrap_or(0);
        abbreviation_at(&self.abbreviations, abbr_index)
    }

    /// Transition timestamps (sorted ascending).
    pub fn transitions(&self) -> &[i64] {
        &self.transitions
    }

    /// Local-time type records.
    pub fn types(&self) -> &[TzifType] {
        &self.types
    }

    fn type_index_at(&self, unix: i64) -> usize {
        if self.transitions.is_empty() || unix < self.transitions[0] {
            return 0;
        }
        let mut idx = 0usize;
        for (i, &transition) in self.transitions.iter().enumerate() {
            if unix < transition {
                break;
            }
            idx = i;
        }
        self.type_indices.get(idx).copied().unwrap_or(0) as usize
    }
}

fn abbreviation_at(abbrs: &[u8], index: usize) -> &str {
    if index >= abbrs.len() {
        return "";
    }
    let tail = &abbrs[index..];
    let end = tail.iter().position(|&b| b == 0).unwrap_or(tail.len());
    core::str::from_utf8(&tail[..end]).unwrap_or("")
}

fn read_header(data: &[u8], offset: &mut usize) -> Result<TzifHeader, TzifError> {
    if data.len().saturating_sub(*offset) < 44 {
        return if data.len().saturating_sub(*offset) < 4 {
            Err(TzifError::TooShort)
        } else if &data[*offset..*offset + 4] == b"TZif" {
            Err(TzifError::Truncated)
        } else {
            Err(TzifError::TooShort)
        };
    }
    if &data[*offset..*offset + 4] != b"TZif" {
        return Err(TzifError::BadMagic);
    }
    *offset += 4;
    let version = data[*offset];
    *offset += 1;
    *offset += 15; // reserved
    let ttisutcnt = read_be_u32(data, offset)?;
    let ttisgmtcnt = read_be_u32(data, offset)?;
    let leapcnt = read_be_u32(data, offset)?;
    let timecnt = read_be_u32(data, offset)?;
    let typecnt = read_be_u32(data, offset)?;
    let charcnt = read_be_u32(data, offset)?;
    Ok(TzifHeader {
        version,
        ttisutcnt,
        ttisgmtcnt,
        leapcnt,
        timecnt,
        typecnt,
        charcnt,
    })
}

fn parse_data_block(
    data: &[u8],
    offset: &mut usize,
    wide_times: bool,
    header: &TzifHeader,
) -> Result<TzifData, TzifError> {
    if header.typecnt == 0 {
        return Err(TzifError::NoTypes);
    }

    let time_size = if wide_times { 8 } else { 4 };
    let leap_size = if wide_times { 12 } else { 8 };

    let transitions_len = header
        .timecnt
        .checked_mul(time_size)
        .ok_or(TzifError::InvalidCounts)?;
    let type_indices_len = header.timecnt;
    let types_len = header
        .typecnt
        .checked_mul(6)
        .ok_or(TzifError::InvalidCounts)?;
    let leap_len = header
        .leapcnt
        .checked_mul(leap_size)
        .ok_or(TzifError::InvalidCounts)?;
    let footer_len = header
        .ttisgmtcnt
        .checked_add(header.ttisutcnt)
        .ok_or(TzifError::InvalidCounts)?;
    let needed = transitions_len as usize
        + type_indices_len as usize
        + types_len as usize
        + header.charcnt as usize
        + leap_len as usize
        + footer_len as usize;
    if data.len().saturating_sub(*offset) < needed {
        return Err(TzifError::Truncated);
    }

    let mut transitions = Vec::with_capacity(header.timecnt as usize);
    for _ in 0..header.timecnt {
        transitions.push(read_transition_time(data, offset, wide_times)?);
    }

    let mut type_indices = Vec::with_capacity(header.timecnt as usize);
    for _ in 0..header.timecnt {
        let idx = read_u8(data, offset)?;
        if idx as u32 >= header.typecnt {
            return Err(TzifError::InvalidCounts);
        }
        type_indices.push(idx);
    }

    let mut types = Vec::with_capacity(header.typecnt as usize);
    for _ in 0..header.typecnt {
        types.push(TzifType {
            ut_offset: read_be_i32(data, offset)?,
            is_dst: read_u8(data, offset)? != 0,
            abbr_index: read_u8(data, offset)?,
        });
    }

    let abbreviations = read_bytes(data, offset, header.charcnt as usize)?;

    for abbr in &types {
        if abbr.abbr_index as usize >= abbreviations.len() {
            return Err(TzifError::InvalidCounts);
        }
    }

    *offset += leap_len as usize;
    *offset += footer_len as usize;

    Ok(TzifData {
        transitions,
        type_indices,
        types,
        abbreviations,
    })
}

fn read_transition_time(data: &[u8], offset: &mut usize, wide: bool) -> Result<i64, TzifError> {
    if wide {
        read_be_i64(data, offset)
    } else {
        read_be_i32(data, offset).map(i64::from)
    }
}

fn read_be_i32(data: &[u8], offset: &mut usize) -> Result<i32, TzifError> {
    if data.len().saturating_sub(*offset) < 4 {
        return Err(TzifError::Truncated);
    }
    let value = i32::from_be_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]);
    *offset += 4;
    Ok(value)
}

fn read_be_i64(data: &[u8], offset: &mut usize) -> Result<i64, TzifError> {
    if data.len().saturating_sub(*offset) < 8 {
        return Err(TzifError::Truncated);
    }
    let value = i64::from_be_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
        data[*offset + 4],
        data[*offset + 5],
        data[*offset + 6],
        data[*offset + 7],
    ]);
    *offset += 8;
    Ok(value)
}

fn read_be_u32(data: &[u8], offset: &mut usize) -> Result<u32, TzifError> {
    if data.len().saturating_sub(*offset) < 4 {
        return Err(TzifError::Truncated);
    }
    let value = u32::from_be_bytes([
        data[*offset],
        data[*offset + 1],
        data[*offset + 2],
        data[*offset + 3],
    ]);
    *offset += 4;
    Ok(value)
}

fn read_u8(data: &[u8], offset: &mut usize) -> Result<u8, TzifError> {
    if *offset >= data.len() {
        return Err(TzifError::Truncated);
    }
    let value = data[*offset];
    *offset += 1;
    Ok(value)
}

fn read_bytes(data: &[u8], offset: &mut usize, len: usize) -> Result<Vec<u8>, TzifError> {
    if data.len().saturating_sub(*offset) < len {
        return Err(TzifError::Truncated);
    }
    let slice = &data[*offset..*offset + len];
    *offset += len;
    Ok(slice.to_vec())
}

/// Minimal TZif v1 fixture: UTC only (no transitions, offset 0).
pub fn fixture_utc_v1() -> Vec<u8> {
    let mut data = Vec::new();
    append_header(&mut data, 0, 0, 0, 0, 1, 4);
    append_type(&mut data, 0, false, 0);
    data.extend_from_slice(b"UTC\0");
    data
}

/// Minimal TZif v2 fixture: UTC only (two blocks, 64-bit times).
pub fn fixture_utc_v2() -> Vec<u8> {
    let mut data = Vec::new();
    append_header(&mut data, b'2', 0, 0, 0, 1, 4);
    append_type(&mut data, 0, false, 0);
    data.extend_from_slice(b"UTC\0");
    append_header(&mut data, b'2', 0, 0, 0, 1, 4);
    append_type(&mut data, 0, false, 0);
    data.extend_from_slice(b"UTC\0");
    data
}

/// Hand-crafted America/New_York-like TZif v1 with EST/EDT transitions.
///
/// Transitions (Unix):
/// - 2007-03-11 07:00 UTC → EDT (-4h)
/// - 2007-11-04 06:00 UTC → EST (-5h)
/// - 2008-03-09 07:00 UTC → EDT
pub fn fixture_new_york_v1() -> Vec<u8> {
    const TRANSITIONS: [i64; 3] = [1_173_596_400, 1_194_156_000, 1_205_046_000];
    let mut data = Vec::new();
    append_header(&mut data, 0, 0, 0, 3, 2, 7);
    for t in TRANSITIONS {
        data.extend_from_slice(&(t as i32).to_be_bytes());
    }
    data.extend_from_slice(&[1, 0, 1]); // EDT, EST, EDT after each transition
    append_type(&mut data, -5 * 3600, false, 0);
    append_type(&mut data, -4 * 3600, true, 4);
    data.extend_from_slice(b"EST\0EDT\0");
    data
}

fn append_header(
    data: &mut Vec<u8>,
    version: u8,
    ttisutcnt: u32,
    ttisgmtcnt: u32,
    timecnt: u32,
    typecnt: u32,
    charcnt: u32,
) {
    data.extend_from_slice(b"TZif");
    data.push(version);
    data.extend([0u8; 15]);
    data.extend_from_slice(&ttisutcnt.to_be_bytes());
    data.extend_from_slice(&ttisgmtcnt.to_be_bytes());
    data.extend_from_slice(&0u32.to_be_bytes()); // leapcnt
    data.extend_from_slice(&timecnt.to_be_bytes());
    data.extend_from_slice(&typecnt.to_be_bytes());
    data.extend_from_slice(&charcnt.to_be_bytes());
}

fn append_type(data: &mut Vec<u8>, offset: i32, is_dst: bool, abbr_index: u8) {
    data.extend_from_slice(&offset.to_be_bytes());
    data.push(u8::from(is_dst));
    data.push(abbr_index);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_utc_v1() {
        let tz = TzifData::parse(&fixture_utc_v1()).unwrap();
        assert_eq!(tz.types().len(), 1);
        assert_eq!(tz.types()[0].ut_offset, 0);
        assert_eq!(tz.offset_at(0), 0);
        assert_eq!(tz.offset_at(1_700_000_000), 0);
        assert_eq!(tz.abbreviation_at(0), "UTC");
    }

    #[test]
    fn parse_utc_v2() {
        let tz = TzifData::parse(&fixture_utc_v2()).unwrap();
        assert_eq!(tz.offset_at(0), 0);
        assert_eq!(tz.abbreviation_at(0), "UTC");
    }

    #[test]
    fn parse_new_york_winter() {
        let tz = TzifData::parse(&fixture_new_york_v1()).unwrap();
        // 2008-01-15 12:00 UTC — EST
        assert_eq!(tz.offset_at(1_200_398_400), -5 * 3600);
        assert!(!tz.is_dst_at(1_200_398_400));
        assert_eq!(tz.abbreviation_at(1_200_398_400), "EST");
    }

    #[test]
    fn parse_new_york_summer() {
        let tz = TzifData::parse(&fixture_new_york_v1()).unwrap();
        // 2007-07-15 12:00 UTC — EDT
        assert_eq!(tz.offset_at(1_184_500_800), -4 * 3600);
        assert!(tz.is_dst_at(1_184_500_800));
        assert_eq!(tz.abbreviation_at(1_184_500_800), "EDT");
    }

    #[test]
    fn bad_magic() {
        let mut data = fixture_utc_v1();
        data[0] = b'X';
        assert_eq!(TzifData::parse(&data), Err(TzifError::BadMagic));
    }

    #[test]
    fn truncated_input() {
        assert_eq!(TzifData::parse(b"TZ"), Err(TzifError::TooShort));
        let data = fixture_utc_v1();
        assert_eq!(TzifData::parse(&data[..20]), Err(TzifError::Truncated));
    }

    #[test]
    fn before_first_transition_uses_type_zero() {
        let tz = TzifData::parse(&fixture_new_york_v1()).unwrap();
        // Before 2007-03-11: type index 0 (EST)
        assert_eq!(tz.offset_at(1_000_000_000), -5 * 3600);
    }
}
