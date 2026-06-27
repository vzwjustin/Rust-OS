//! Markup (XML subset) parser matching `gmarkup.h` / `gmarkup.c`.
//!
//! A simple streaming XML parser with callback-based event handling.
//! Fully `no_std` compatible using `alloc`.

#![allow(missing_docs)]

use crate::prelude::*;

/// Markup error codes (`GMarkupError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MarkupError {
    BadUtf8,
    Empty,
    Parse,
    UnknownElement,
    UnknownAttribute,
    InvalidContent,
    MissingAttribute,
}

/// Markup parse flags (`GMarkupParseFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MarkupParseFlags(pub u32);

impl MarkupParseFlags {
    pub const DEFAULT_FLAGS: Self = Self(0);
    pub const TREAT_CDATA_AS_TEXT: Self = Self(1 << 1);
    pub const PREFIX_ERROR_POSITION: Self = Self(1 << 2);
    pub const IGNORE_QUALIFIED: Self = Self(1 << 3);
}

/// A markup element attribute (name, value pair).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    pub value: String,
}

/// A parsed markup element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub attributes: Vec<Attribute>,
    pub children: Vec<MarkupNode>,
}

/// A node in the markup tree.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkupNode {
    /// An element with children.
    Element(Element),
    /// Text content.
    Text(String),
    /// A comment or processing instruction.
    Passthrough(String),
}

/// A simple markup (XML) document parser.
///
/// This is a minimal XML subset parser supporting elements, attributes,
/// text content, comments, and CDATA sections. It does not support
/// DTDs, entity declarations, or namespace resolution.
pub struct MarkupParser {
    flags: MarkupParseFlags,
}

impl MarkupParser {
    /// Create a new parser with the given flags.
    pub fn new(flags: MarkupParseFlags) -> Self {
        Self { flags }
    }

    /// Parse a complete document (`g_markup_parse_context_parse` + end).
    pub fn parse(&self, text: &str) -> Result<Element, MarkupError> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return Err(MarkupError::Empty);
        }

        let bytes = trimmed.as_bytes();
        let mut pos = 0;
        let mut root: Option<Element> = None;
        let mut stack: Vec<Element> = Vec::new();

        while pos < bytes.len() {
            // Skip whitespace
            while pos < bytes.len() && is_space(bytes[pos]) {
                pos += 1;
            }
            if pos >= bytes.len() {
                break;
            }

            if bytes[pos] != b'<' {
                // Text content
                let start = pos;
                while pos < bytes.len() && bytes[pos] != b'<' {
                    pos += 1;
                }
                let text = decode_entities(&trimmed[start..pos]);
                if !text.is_empty() {
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(MarkupNode::Text(text));
                    }
                }
                continue;
            }

            // We're at '<'
            pos += 1;
            if pos >= bytes.len() {
                return Err(MarkupError::Parse);
            }

            match bytes[pos] {
                b'!' => {
                    // Comment, CDATA, or DOCTYPE
                    if pos + 2 < bytes.len() && &bytes[pos..pos + 3] == b"!--" {
                        // Comment
                        pos += 3;
                        let start = pos;
                        while pos + 2 < bytes.len() && &bytes[pos..pos + 3] != b"-->" {
                            pos += 1;
                        }
                        if pos + 2 >= bytes.len() {
                            return Err(MarkupError::Parse);
                        }
                        let comment = trimmed[start..pos].to_owned();
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(MarkupNode::Passthrough(comment));
                        }
                        pos += 3; // skip -->
                    } else if pos + 7 < bytes.len() && &bytes[pos..pos + 8] == b"![CDATA[" {
                        // CDATA section
                        pos += 8;
                        let start = pos;
                        while pos + 2 < bytes.len() && &bytes[pos..pos + 2] != b"]]" {
                            pos += 1;
                        }
                        if pos + 2 >= bytes.len() {
                            return Err(MarkupError::Parse);
                        }
                        let cdata = trimmed[start..pos].to_owned();
                        if self.flags.0 & MarkupParseFlags::TREAT_CDATA_AS_TEXT.0 != 0 {
                            if let Some(parent) = stack.last_mut() {
                                parent.children.push(MarkupNode::Text(cdata));
                            }
                        } else {
                            if let Some(parent) = stack.last_mut() {
                                parent.children.push(MarkupNode::Passthrough(cdata));
                            }
                        }
                        pos += 2; // skip ]]
                        if pos < bytes.len() && bytes[pos] == b'>' {
                            pos += 1;
                        }
                    } else {
                        // DOCTYPE or other - skip to >
                        while pos < bytes.len() && bytes[pos] != b'>' {
                            pos += 1;
                        }
                        if pos < bytes.len() {
                            pos += 1;
                        }
                    }
                }
                b'?' => {
                    // Processing instruction
                    pos += 1;
                    let start = pos;
                    while pos + 1 < bytes.len() && &bytes[pos..pos + 2] != b"?>" {
                        pos += 1;
                    }
                    if pos + 1 >= bytes.len() {
                        return Err(MarkupError::Parse);
                    }
                    let pi = trimmed[start..pos].to_owned();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(MarkupNode::Passthrough(pi));
                    }
                    pos += 2; // skip ?>
                }
                b'/' => {
                    // Closing tag
                    pos += 1;
                    let start = pos;
                    while pos < bytes.len() && bytes[pos] != b'>' {
                        pos += 1;
                    }
                    if pos >= bytes.len() {
                        return Err(MarkupError::Parse);
                    }
                    let name = trimmed[start..pos].trim().to_owned();
                    pos += 1; // skip >

                    let elem = stack.pop();
                    match elem {
                        Some(e) if e.name == name => {
                            if let Some(parent) = stack.last_mut() {
                                parent.children.push(MarkupNode::Element(e));
                            } else {
                                root = Some(e);
                            }
                        }
                        Some(e) => {
                            // Mismatched tag - push back
                            stack.push(e);
                            return Err(MarkupError::Parse);
                        }
                        None => return Err(MarkupError::Parse),
                    }
                }
                _ => {
                    // Opening tag
                    let start = pos;
                    while pos < bytes.len()
                        && !is_space(bytes[pos])
                        && bytes[pos] != b'>'
                        && bytes[pos] != b'/'
                    {
                        pos += 1;
                    }
                    if pos >= bytes.len() {
                        return Err(MarkupError::Parse);
                    }
                    let name = trimmed[start..pos].to_owned();

                    // Parse attributes
                    let mut attributes = Vec::new();
                    let mut self_closing = false;
                    loop {
                        // Skip whitespace
                        while pos < bytes.len() && is_space(bytes[pos]) {
                            pos += 1;
                        }
                        if pos >= bytes.len() {
                            return Err(MarkupError::Parse);
                        }
                        if bytes[pos] == b'>' {
                            pos += 1;
                            break;
                        }
                        if bytes[pos] == b'/' {
                            pos += 1;
                            if pos < bytes.len() && bytes[pos] == b'>' {
                                pos += 1;
                                self_closing = true;
                                break;
                            }
                            return Err(MarkupError::Parse);
                        }

                        // Attribute name
                        let attr_start = pos;
                        while pos < bytes.len()
                            && bytes[pos] != b'='
                            && !is_space(bytes[pos])
                        {
                            pos += 1;
                        }
                        if pos >= bytes.len() || bytes[pos] != b'=' {
                            return Err(MarkupError::Parse);
                        }
                        let attr_name = trimmed[attr_start..pos].to_owned();
                        pos += 1; // skip =

                        // Skip whitespace
                        while pos < bytes.len() && is_space(bytes[pos]) {
                            pos += 1;
                        }
                        if pos >= bytes.len() {
                            return Err(MarkupError::Parse);
                        }

                        // Quoted value
                        let quote = bytes[pos];
                        if quote != b'"' && quote != b'\'' {
                            return Err(MarkupError::Parse);
                        }
                        pos += 1;
                        let val_start = pos;
                        while pos < bytes.len() && bytes[pos] != quote {
                            pos += 1;
                        }
                        if pos >= bytes.len() {
                            return Err(MarkupError::Parse);
                        }
                        let attr_value = decode_entities(&trimmed[val_start..pos]);
                        pos += 1; // skip closing quote

                        attributes.push(Attribute {
                            name: attr_name,
                            value: attr_value,
                        });
                    }

                    if self_closing {
                        let elem = Element {
                            name,
                            attributes,
                            children: Vec::new(),
                        };
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(MarkupNode::Element(elem));
                        } else {
                            root = Some(elem);
                        }
                        continue;
                    }

                    // Push element onto stack
                    stack.push(Element {
                        name,
                        attributes,
                        children: Vec::new(),
                    });
                }
            }
        }

        if !stack.is_empty() {
            return Err(MarkupError::Parse);
        }

        root.ok_or(MarkupError::Empty)
    }
}

fn is_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

fn decode_entities(s: &str) -> String {
    if !s.contains('&') {
        return s.to_owned();
    }
    let mut result = String::with_capacity(s.len());
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'&' {
            if let Some(semi) = bytes[i..].iter().position(|&b| b == b';') {
                let entity = &s[i + 1..i + semi];
                match entity {
                    "amp" => result.push('&'),
                    "lt" => result.push('<'),
                    "gt" => result.push('>'),
                    "quot" => result.push('"'),
                    "apos" => result.push('\''),
                    _ => {
                        if let Some(rest) = entity.strip_prefix('#') {
                            if let Some(hex) = rest.strip_prefix('x') {
                                if let Ok(code) = u32::from_str_radix(hex, 16) {
                                    if let Some(c) = char::from_u32(code) {
                                        result.push(c);
                                    } else {
                                        result.push('?');
                                    }
                                } else {
                                    result.push('?');
                                }
                            } else if let Ok(code) = rest.parse::<u32>() {
                                if let Some(c) = char::from_u32(code) {
                                    result.push(c);
                                } else {
                                    result.push('?');
                                }
                            } else {
                                result.push('?');
                            }
                        } else {
                            // Unknown entity, keep as-is
                            result.push('&');
                            result.push_str(entity);
                            result.push(';');
                        }
                    }
                }
                i += semi + 1;
            } else {
                result.push('&');
                i += 1;
            }
        } else {
            // Handle multi-byte UTF-8
            let ch_len = utf8_char_len(bytes[i]);
            let end = core::cmp::min(i + ch_len, bytes.len());
            if let Ok(s) = core::str::from_utf8(&bytes[i..end]) {
                result.push_str(s);
            }
            i = end;
        }
    }
    result
}

fn utf8_char_len(b: u8) -> usize {
    if b < 0xC0 {
        1
    } else if b < 0xE0 {
        2
    } else if b < 0xF0 {
        3
    } else {
        4
    }
}

/// Escape text for XML output (`g_markup_escape_text`).
pub fn escape_text(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => result.push_str("&amp;"),
            '<' => result.push_str("&lt;"),
            '>' => result.push_str("&gt;"),
            '"' => result.push_str("&quot;"),
            '\'' => result.push_str("&apos;"),
            _ => result.push(c),
        }
    }
    result
}

/// Get the error quark for markup errors (`g_markup_error_quark`).
pub fn markup_error_quark() -> u32 {
    30 // G_MARKUP_ERROR quark
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_element() {
        let parser = MarkupParser::new(MarkupParseFlags::DEFAULT_FLAGS);
        let root = parser.parse("<root></root>").unwrap();
        assert_eq!(root.name, "root");
        assert!(root.children.is_empty());
    }

    #[test]
    fn element_with_text() {
        let parser = MarkupParser::new(MarkupParseFlags::DEFAULT_FLAGS);
        let root = parser.parse("<root>Hello</root>").unwrap();
        assert_eq!(root.name, "root");
        assert_eq!(root.children.len(), 1);
        match &root.children[0] {
            MarkupNode::Text(t) => assert_eq!(t, "Hello"),
            _ => panic!("expected text node"),
        }
    }

    #[test]
    fn nested_elements() {
        let parser = MarkupParser::new(MarkupParseFlags::DEFAULT_FLAGS);
        let root = parser.parse("<root><child>A</child><child>B</child></root>").unwrap();
        assert_eq!(root.name, "root");
        assert_eq!(root.children.len(), 2);
    }

    #[test]
    fn attributes() {
        let parser = MarkupParser::new(MarkupParseFlags::DEFAULT_FLAGS);
        let root = parser.parse(r#"<root attr="value" foo="bar"></root>"#).unwrap();
        assert_eq!(root.attributes.len(), 2);
        assert_eq!(root.attributes[0].name, "attr");
        assert_eq!(root.attributes[0].value, "value");
        assert_eq!(root.attributes[1].name, "foo");
        assert_eq!(root.attributes[1].value, "bar");
    }

    #[test]
    fn self_closing() {
        let parser = MarkupParser::new(MarkupParseFlags::DEFAULT_FLAGS);
        let root = parser.parse(r#"<root><br/></root>"#).unwrap();
        assert_eq!(root.children.len(), 1);
        match &root.children[0] {
            MarkupNode::Element(e) => {
                assert_eq!(e.name, "br");
                assert!(e.children.is_empty());
            }
            _ => panic!("expected element"),
        }
    }

    #[test]
    fn comment() {
        let parser = MarkupParser::new(MarkupParseFlags::DEFAULT_FLAGS);
        let root = parser.parse("<root><!-- comment --></root>").unwrap();
        assert_eq!(root.children.len(), 1);
        match &root.children[0] {
            MarkupNode::Passthrough(s) => assert_eq!(s, " comment "),
            _ => panic!("expected passthrough"),
        }
    }

    #[test]
    fn entities() {
        let parser = MarkupParser::new(MarkupParseFlags::DEFAULT_FLAGS);
        let root = parser.parse("<root>&amp;&lt;&gt;&quot;&apos;</root>").unwrap();
        match &root.children[0] {
            MarkupNode::Text(t) => assert_eq!(t, "&<>\"'"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn escape_and_unescape() {
        let original = "a&b<c>d\"e'f";
        let escaped = escape_text(original);
        assert_eq!(escaped, "a&amp;b&lt;c&gt;d&quot;e&apos;f");
        let parser = MarkupParser::new(MarkupParseFlags::DEFAULT_FLAGS);
        let root = parser.parse(&format!("<root>{}</root>", escaped)).unwrap();
        match &root.children[0] {
            MarkupNode::Text(t) => assert_eq!(t, original),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn cdata() {
        let parser = MarkupParser::new(MarkupParseFlags::DEFAULT_FLAGS);
        let root = parser.parse("<root><![CDATA[hello <world>]]></root>").unwrap();
        match &root.children[0] {
            MarkupNode::Passthrough(s) => assert_eq!(s, "hello <world>"),
            _ => panic!("expected passthrough"),
        }
    }

    #[test]
    fn cdata_as_text() {
        let parser = MarkupParser::new(MarkupParseFlags::TREAT_CDATA_AS_TEXT);
        let root = parser.parse("<root><![CDATA[hello]]></root>").unwrap();
        match &root.children[0] {
            MarkupNode::Text(t) => assert_eq!(t, "hello"),
            _ => panic!("expected text"),
        }
    }

    #[test]
    fn empty_document() {
        let parser = MarkupParser::new(MarkupParseFlags::DEFAULT_FLAGS);
        assert_eq!(parser.parse("").unwrap_err(), MarkupError::Empty);
    }

    #[test]
    fn mismatched_tags() {
        let parser = MarkupParser::new(MarkupParseFlags::DEFAULT_FLAGS);
        assert_eq!(parser.parse("<root></other>").unwrap_err(), MarkupError::Parse);
    }

    #[test]
    fn numeric_entities() {
        let parser = MarkupParser::new(MarkupParseFlags::DEFAULT_FLAGS);
        let root = parser.parse("<root>&#65;&#x42;</root>").unwrap();
        match &root.children[0] {
            MarkupNode::Text(t) => assert_eq!(t, "AB"),
            _ => panic!("expected text"),
        }
    }
}
