//! `girnode_private` matching `girepository/girnode-private.h`.
//!
//! Private internal API for `GirNode`.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::girnode::{Node, NodeTag};

/// Creates a new node (internal constructor, same as `Node::new`).
pub fn node_new(tag: NodeTag, name: &str) -> Node {
    Node::new(tag, name)
}

/// Gets an attribute by name (internal).
pub fn node_get_attribute<'a>(node: &'a Node, name: &str) -> Option<&'a str> {
    node.attributes
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.as_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_node_new() {
        let n = node_new(NodeTag::Function, "test");
        assert_eq!(n.tag, NodeTag::Function);
    }

    #[test]
    fn test_get_attribute() {
        let mut n = node_new(NodeTag::Function, "test");
        n.set_attribute("version", "1.0");
        assert_eq!(node_get_attribute(&n, "version"), Some("1.0"));
        assert_eq!(node_get_attribute(&n, "missing"), None);
    }
}
