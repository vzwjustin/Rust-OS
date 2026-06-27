//! `girnode` matching `girepository/girnode.c`.
//!
//! GIR node tree: internal representation of introspection data
//! used by the parser and writer.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;

/// Node types in the GIR tree (mirrors `GirNodeTag`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum NodeTag {
    #[default]
    Invalid = 0,
    Function = 1,
    Callback = 2,
    Struct = 3,
    Enum = 4,
    Flags = 5,
    Object = 6,
    Interface = 7,
    Constant = 8,
    Union = 9,
    Signal = 10,
    Vfunc = 11,
    Property = 12,
    Field = 13,
    Arg = 14,
    Type = 15,
}

/// GIR node (mirrors `GirNode`).
#[derive(Debug, Clone, Default)]
pub struct Node {
    pub tag: NodeTag,
    pub name: String,
    pub children: Vec<Node>,
    pub attributes: Vec<(String, String)>,
}

impl Node {
    /// Creates a new node.
    pub fn new(tag: NodeTag, name: &str) -> Self {
        Self {
            tag,
            name: name.into(),
            children: Vec::new(),
            attributes: Vec::new(),
        }
    }

    /// Adds a child node.
    pub fn add_child(&mut self, child: Node) {
        self.children.push(child);
    }

    /// Sets an attribute.
    pub fn set_attribute(&mut self, key: &str, value: &str) {
        self.attributes.push((key.into(), value.into()));
    }

    /// Finds children by tag.
    pub fn find_children(&self, tag: NodeTag) -> Vec<&Node> {
        self.children.iter().filter(|c| c.tag == tag).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let n = Node::new(NodeTag::Function, "my_func");
        assert_eq!(n.tag, NodeTag::Function);
        assert_eq!(n.name, "my_func");
    }

    #[test]
    fn test_add_child() {
        let mut parent = Node::new(NodeTag::Object, "MyObject");
        parent.add_child(Node::new(NodeTag::Function, "method1"));
        parent.add_child(Node::new(NodeTag::Property, "prop1"));
        assert_eq!(parent.children.len(), 2);
        let funcs = parent.find_children(NodeTag::Function);
        assert_eq!(funcs.len(), 1);
    }
}
