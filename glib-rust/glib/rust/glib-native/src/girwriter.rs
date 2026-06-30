//! `girwriter` matching `girepository/girwriter.c`.
//!
//! GIR XML writer: serializes node trees to `.gir` XML format.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::girnode::{Node, NodeTag};
use alloc::string::String;

/// Writes a node tree to GIR XML string (mirrors `g_ir_writer_write`).
pub fn write_to_string(node: &Node) -> String {
    let mut out = String::new();
    write_node(node, &mut out, 0);
    out
}

fn indent(out: &mut String, depth: usize) {
    for _ in 0..depth {
        out.push_str("  ");
    }
}

fn write_node(node: &Node, out: &mut String, depth: usize) {
    let tag_name = tag_to_string(node.tag);
    indent(out, depth);
    if node.children.is_empty() && node.attributes.is_empty() {
        out.push_str(&format!("<{} name=\"{}\"/>\n", tag_name, node.name));
    } else if node.children.is_empty() {
        out.push_str(&format!("<{} name=\"{}\"", tag_name, node.name));
        for (k, v) in &node.attributes {
            out.push_str(&format!(" {}=\"{}\"", k, v));
        }
        out.push_str("/>\n");
    } else {
        out.push_str(&format!("<{} name=\"{}\">\n", tag_name, node.name));
        for child in &node.children {
            write_node(child, out, depth + 1);
        }
        indent(out, depth);
        out.push_str(&format!("</{}>\n", tag_name));
    }
}

fn tag_to_string(tag: NodeTag) -> &'static str {
    match tag {
        NodeTag::Invalid => "invalid",
        NodeTag::Function => "function",
        NodeTag::Callback => "callback",
        NodeTag::Struct => "record",
        NodeTag::Enum => "enumeration",
        NodeTag::Flags => "bitfield",
        NodeTag::Object => "class",
        NodeTag::Interface => "interface",
        NodeTag::Constant => "constant",
        NodeTag::Union => "union",
        NodeTag::Signal => "glib:signal",
        NodeTag::Vfunc => "glib:vfunc",
        NodeTag::Property => "property",
        NodeTag::Field => "field",
        NodeTag::Arg => "parameter",
        NodeTag::Type => "type",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_empty() {
        let node = Node::default();
        let xml = write_to_string(&node);
        assert!(xml.contains("invalid"));
    }

    #[test]
    fn test_write_with_children() {
        let mut parent = Node::new(NodeTag::Object, "MyObject");
        parent.add_child(Node::new(NodeTag::Function, "method1"));
        let xml = write_to_string(&parent);
        assert!(xml.contains("class"));
        assert!(xml.contains("MyObject"));
        assert!(xml.contains("function"));
        assert!(xml.contains("method1"));
    }
}
