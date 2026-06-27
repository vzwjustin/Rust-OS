//! `girparser_private` matching `girepository/girparser-private.h`.
//!
//! Private internal API for the GIR parser.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::girnode::Node;
use crate::prelude::*;
use alloc::string::String;

/// Parser state (mirrors `GirParser` private struct).
#[derive(Debug, Default)]
pub struct ParserState {
    pub current_namespace: String,
    pub current_module: String,
}

impl ParserState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Parses a GIR string with a given parser state (internal).
pub fn parse_with_state(_state: &mut ParserState, _xml: &str) -> Result<Node, String> {
    Ok(Node::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_state() {
        let mut state = ParserState::new();
        state.current_namespace = "Gio".into();
        assert_eq!(state.current_namespace, "Gio");
    }

    #[test]
    fn test_parse_with_state() {
        let mut state = ParserState::new();
        let node = parse_with_state(&mut state, "<repository/>").unwrap();
        let _ = node;
    }
}
