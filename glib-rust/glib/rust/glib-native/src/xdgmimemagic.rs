//! `xdgmimemagic` matching `gio/xdgmime/xdgmimemagic.h`.
//!
//! XDG MIME magic: content-based MIME type detection using magic patterns.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::String;
use alloc::string::ToString;
use alloc::vec::Vec;

/// Magic match type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MagicMatchType {
    String,
    Big32,
    Little32,
    Big16,
    Little16,
    Host16,
    Host32,
}

/// A single magic rule.
#[derive(Debug, Clone)]
pub struct MagicRule {
    pub offset: usize,
    pub match_type: MagicMatchType,
    pub pattern: Vec<u8>,
    pub mask: Vec<u8>,
    pub mime_type: String,
    pub priority: i32,
}

impl MagicRule {
    /// Checks if this rule matches the given data.
    pub fn matches(&self, data: &[u8]) -> bool {
        if self.offset + self.pattern.len() > data.len() {
            return false;
        }
        let slice = &data[self.offset..self.offset + self.pattern.len()];
        match self.match_type {
            MagicMatchType::String => {
                if self.mask.is_empty() {
                    slice == self.pattern.as_slice()
                } else {
                    slice
                        .iter()
                        .zip(self.pattern.iter())
                        .zip(self.mask.iter())
                        .all(|((&d, &p), &m)| (d & m) == (p & m))
                }
            }
            MagicMatchType::Big32 | MagicMatchType::Little32 | MagicMatchType::Host32 => {
                slice == self.pattern.as_slice()
            }
            MagicMatchType::Big16 | MagicMatchType::Little16 | MagicMatchType::Host16 => {
                slice == self.pattern.as_slice()
            }
        }
    }
}

/// XDG MIME magic (mirrors `XdgMimeMagic`).
#[derive(Debug, Default)]
pub struct XdgMimeMagic {
    rules: Vec<MagicRule>,
    max_buffer_extents: usize,
}

impl XdgMimeMagic {
    /// Creates a new empty magic database (mirrors `_xdg_mime_magic_new`).
    pub fn new() -> Self {
        Self {
            max_buffer_extents: 4096,
            ..Default::default()
        }
    }

    /// Returns the max buffer extents needed
    /// (mirrors `_xdg_mime_magic_get_buffer_extents`).
    pub fn get_buffer_extents(&self) -> usize {
        self.max_buffer_extents
    }

    /// Looks up the MIME type for the given data
    /// (mirrors `_xdg_mime_magic_lookup_data`).
    pub fn lookup_data(&self, data: &[u8]) -> (String, i32) {
        let mut best_mime = String::new();
        let mut best_priority = -1;
        for rule in &self.rules {
            if rule.matches(data) && rule.priority > best_priority {
                best_priority = rule.priority;
                best_mime = rule.mime_type.clone();
            }
        }
        (best_mime, best_priority)
    }

    /// Adds a magic rule to the database.
    pub fn add_rule(&mut self, rule: MagicRule) {
        let end = rule.offset + rule.pattern.len();
        if self.rules.is_empty() || end > self.max_buffer_extents {
            self.max_buffer_extents = end;
        }
        self.rules.push(rule);
    }

    /// Reads magic rules from file content
    /// (mirrors `_xdg_mime_magic_read_from_file`).
    /// Format: `[offset:type:pattern] mime_type priority`
    pub fn read_from_file_content(&mut self, content: &str) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(bracket_end) = line.find(']') {
                let spec = &line[..bracket_end + 1];
                let rest = line[bracket_end + 1..].trim();
                let mut rest_parts = rest.split_whitespace();
                let mime_type = rest_parts.next().unwrap_or("");
                let priority: i32 = rest_parts.next().and_then(|s| s.parse().ok()).unwrap_or(50);
                if let Some(rule) = parse_magic_spec(spec, mime_type, priority) {
                    self.add_rule(rule);
                }
            }
        }
    }

    /// Returns the number of rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Returns true if the magic database is empty.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

/// Parses a magic spec like `[0:string:PK]`.
fn parse_magic_spec(spec: &str, mime_type: &str, priority: i32) -> Option<MagicRule> {
    let inner = spec.strip_prefix('[')?.strip_suffix(']')?;
    let parts: Vec<&str> = inner.splitn(3, ':').collect();
    if parts.len() < 3 {
        return None;
    }
    let offset: usize = parts[0].parse().ok()?;
    let match_type = match parts[1] {
        "string" => MagicMatchType::String,
        "big32" => MagicMatchType::Big32,
        "little32" => MagicMatchType::Little32,
        "big16" => MagicMatchType::Big16,
        "little16" => MagicMatchType::Little16,
        _ => MagicMatchType::String,
    };
    let pattern = parts[2].as_bytes().to_vec();
    Some(MagicRule {
        offset,
        match_type,
        pattern,
        mask: Vec::new(),
        mime_type: mime_type.to_string(),
        priority,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let magic = XdgMimeMagic::new();
        assert_eq!(magic.get_buffer_extents(), 4096);
        assert!(magic.is_empty());
    }

    #[test]
    fn test_add_rule_and_lookup() {
        let mut magic = XdgMimeMagic::new();
        magic.add_rule(MagicRule {
            offset: 0,
            match_type: MagicMatchType::String,
            pattern: b"PK".to_vec(),
            mask: Vec::new(),
            mime_type: "application/zip".to_string(),
            priority: 50,
        });
        let (mime, prio) = magic.lookup_data(b"PK\x03\x04");
        assert_eq!(mime, "application/zip");
        assert_eq!(prio, 50);
    }

    #[test]
    fn test_lookup_no_match() {
        let magic = XdgMimeMagic::new();
        let (mime, prio) = magic.lookup_data(b"hello");
        assert_eq!(mime, "");
        assert_eq!(prio, -1);
    }

    #[test]
    fn test_priority_selection() {
        let mut magic = XdgMimeMagic::new();
        magic.add_rule(MagicRule {
            offset: 0,
            match_type: MagicMatchType::String,
            pattern: b"\x89PNG".to_vec(),
            mask: Vec::new(),
            mime_type: "image/png".to_string(),
            priority: 50,
        });
        magic.add_rule(MagicRule {
            offset: 0,
            match_type: MagicMatchType::String,
            pattern: b"\x89PNG".to_vec(),
            mask: Vec::new(),
            mime_type: "image/x-png".to_string(),
            priority: 80,
        });
        let (mime, prio) = magic.lookup_data(b"\x89PNG\r\n");
        assert_eq!(mime, "image/x-png");
        assert_eq!(prio, 80);
    }

    #[test]
    fn test_offset_check() {
        let mut magic = XdgMimeMagic::new();
        magic.add_rule(MagicRule {
            offset: 4,
            match_type: MagicMatchType::String,
            pattern: b"ftyp".to_vec(),
            mask: Vec::new(),
            mime_type: "video/mp4".to_string(),
            priority: 50,
        });
        let (mime, _) = magic.lookup_data(b"\x00\x00\x00\x08ftyp");
        assert_eq!(mime, "video/mp4");
        let (mime, _) = magic.lookup_data(b"ftyp");
        assert_eq!(mime, "");
    }

    #[test]
    fn test_max_buffer_extents() {
        let mut magic = XdgMimeMagic::new();
        magic.add_rule(MagicRule {
            offset: 100,
            match_type: MagicMatchType::String,
            pattern: vec![0xFF; 4],
            mask: Vec::new(),
            mime_type: "test/test".to_string(),
            priority: 50,
        });
        assert_eq!(magic.get_buffer_extents(), 104);
    }

    #[test]
    fn test_read_from_file_content() {
        let mut magic = XdgMimeMagic::new();
        magic.read_from_file_content("[0:string:PK] application/zip 50\n");
        assert_eq!(magic.len(), 1);
        let (mime, _) = magic.lookup_data(b"PK");
        assert_eq!(mime, "application/zip");
    }
}
