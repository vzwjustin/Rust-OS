//! `girwriter_private` matching `girepository/girwriter-private.h`.
//!
//! Private internal API for the GIR writer.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::girnode::Node;
use crate::girwriter::write_to_string;
use crate::prelude::*;
use alloc::string::String;

/// Writer state (mirrors `GirWriter` private struct).
#[derive(Debug, Default)]
pub struct WriterState {
    pub indent: usize,
    pub output: String,
}

impl WriterState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Writes a node tree to the writer state (internal).
pub fn write_node(state: &mut WriterState, node: &Node) {
    state.output = write_to_string(node);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::girnode::NodeTag;

    #[test]
    fn test_writer_state() {
        let state = WriterState::new();
        assert_eq!(state.indent, 0);
    }

    #[test]
    fn test_write_node() {
        let mut state = WriterState::new();
        let node = Node::new(NodeTag::Object, "MyObject");
        write_node(&mut state, &node);
        assert!(state.output.contains("MyObject"));
    }
}
