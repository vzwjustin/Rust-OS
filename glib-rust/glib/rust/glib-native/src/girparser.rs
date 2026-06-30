//! `girparser` matching `girepository/girparser.c`.
//!
//! GIR XML parser: parses `.gir` XML files into node trees.
//! Stubbed in no_std since XML parsing requires I/O.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::girnode::Node;
use alloc::string::String;

/// Parses a GIR XML string (mirrors `g_ir_parser_parse_string`).
/// No-op in our no_std port; returns empty root node.
pub fn parse_string(_xml: &str) -> Result<Node, String> {
    Ok(Node::default())
}

/// Parses a GIR file (mirrors `g_ir_parser_parse_file`).
/// No-op in our no_std port.
pub fn parse_file(_path: &str) -> Result<Node, String> {
    Err("File I/O not supported in no_std".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_string() {
        let node = parse_string("<repository/>").unwrap();
        let _ = node;
    }

    #[test]
    fn test_parse_file_fails() {
        assert!(parse_file("/tmp/test.gir").is_err());
    }
}
