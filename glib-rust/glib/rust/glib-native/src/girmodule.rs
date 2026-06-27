//! `girmodule` matching `girepository/girmodule.c`.
//!
//! GIR module: a collection of namespaces parsed from GIR files,
//! used to generate typelibs.
//!
//! Fully `no_std` compatible using `core` and `alloc`.

use crate::girnode::Node;
use crate::prelude::*;
use alloc::string::String;
use alloc::vec::Vec;

/// GIR module (mirrors `GirModule`).
#[derive(Debug, Clone, Default)]
pub struct Module {
    pub name: String,
    pub version: String,
    pub shared_library: String,
    pub c_prefix: String,
    pub entries: Vec<Node>,
    pub dependencies: Vec<String>,
}

impl Module {
    /// Creates a new module.
    pub fn new(name: &str, version: &str) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
            ..Default::default()
        }
    }

    /// Adds an entry node.
    pub fn add_entry(&mut self, node: Node) {
        self.entries.push(node);
    }

    /// Returns the number of entries.
    pub fn n_entries(&self) -> usize {
        self.entries.len()
    }

    /// Adds a dependency.
    pub fn add_dependency(&mut self, dep: &str) {
        self.dependencies.push(dep.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::girnode::NodeTag;

    #[test]
    fn test_new() {
        let m = Module::new("Gio", "2.0");
        assert_eq!(m.name, "Gio");
        assert_eq!(m.version, "2.0");
        assert_eq!(m.n_entries(), 0);
    }

    #[test]
    fn test_add_entry() {
        let mut m = Module::new("Gio", "2.0");
        m.add_entry(Node::new(NodeTag::Object, "Application"));
        assert_eq!(m.n_entries(), 1);
    }
}
