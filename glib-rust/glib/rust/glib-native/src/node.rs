//! N-ary tree matching `gnode.h` / `gnode.c`.
//!
//! Provides a general-purpose N-ary tree with traversal, search, and
//! manipulation. Fully `no_std` compatible using `alloc`.

#![allow(missing_docs)]

use crate::prelude::*;
use alloc::boxed::Box;

/// Traverse flags (`GTraverseFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TraverseFlags(pub u32);

impl TraverseFlags {
    pub const LEAVES: Self = Self(1 << 0);
    pub const NON_LEAVES: Self = Self(1 << 1);
    pub const ALL: Self = Self(Self::LEAVES.0 | Self::NON_LEAVES.0);
    pub const MASK: Self = Self(0x03);

    pub fn matches(self, is_leaf: bool) -> bool {
        if is_leaf {
            self.0 & Self::LEAVES.0 != 0
        } else {
            self.0 & Self::NON_LEAVES.0 != 0
        }
    }
}

/// Traverse order (`GTraverseType`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraverseType {
    InOrder,
    PreOrder,
    PostOrder,
    LevelOrder,
}

/// An N-ary tree node (`GNode`).
pub struct Node<T> {
    pub data: T,
    pub parent: Option<usize>,
    pub children: Vec<usize>,
}

/// An N-ary tree with arena-based node storage.
pub struct NTree<T> {
    nodes: Vec<Option<Box<Node<T>>>>,
    root: Option<usize>,
    free_list: Vec<usize>,
}

impl<T> NTree<T> {
    /// Create a new empty tree.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            root: None,
            free_list: Vec::new(),
        }
    }

    /// Create a new root node (`g_node_new`).
    pub fn new_root(&mut self, data: T) -> usize {
        let id = self.alloc_node(data);
        self.root = Some(id);
        id
    }

    fn alloc_node(&mut self, data: T) -> usize {
        let node = Box::new(Node {
            data,
            parent: None,
            children: Vec::new(),
        });
        if let Some(id) = self.free_list.pop() {
            self.nodes[id] = Some(node);
            id
        } else {
            let id = self.nodes.len();
            self.nodes.push(Some(node));
            id
        }
    }

    fn get_node(&self, id: usize) -> &Node<T> {
        self.nodes[id].as_ref().expect("node exists")
    }

    fn get_node_mut(&mut self, id: usize) -> &mut Node<T> {
        self.nodes[id].as_mut().expect("node exists")
    }

    /// Insert a child at position (`g_node_insert`).
    /// position = -1 means append at end.
    pub fn insert(&mut self, parent: usize, position: i32, data: T) -> usize {
        let id = self.alloc_node(data);
        self.get_node_mut(id).parent = Some(parent);

        let p = self.get_node_mut(parent);
        let pos = if position < 0 || position as usize >= p.children.len() {
            p.children.len()
        } else {
            position as usize
        };
        p.children.insert(pos, id);
        id
    }

    /// Insert a child before a sibling (`g_node_insert_before`).
    /// If `sibling` is None, appends at end.
    pub fn insert_before(&mut self, parent: usize, sibling: Option<usize>, data: T) -> usize {
        let id = self.alloc_node(data);
        self.get_node_mut(id).parent = Some(parent);

        let pos = match sibling {
            Some(s) => {
                let p = self.get_node_mut(parent);
                p.children.iter().position(|&c| c == s).unwrap_or(p.children.len())
            }
            None => self.get_node(parent).children.len(),
        };
        self.get_node_mut(parent).children.insert(pos, id);
        id
    }

    /// Insert a child after a sibling (`g_node_insert_after`).
    pub fn insert_after(&mut self, parent: usize, sibling: Option<usize>, data: T) -> usize {
        let id = self.alloc_node(data);
        self.get_node_mut(id).parent = Some(parent);

        let pos = match sibling {
            Some(s) => {
                let p = self.get_node_mut(parent);
                p.children.iter().position(|&c| c == s).map(|i| i + 1).unwrap_or(p.children.len())
            }
            None => 0,
        };
        self.get_node_mut(parent).children.insert(pos, id);
        id
    }

    /// Prepend a child as first child of parent (`g_node_prepend`).
    pub fn prepend(&mut self, parent: usize, data: T) -> usize {
        self.insert(parent, 0, data)
    }

    /// Append a child as last child of parent (`g_node_append`).
    pub fn append(&mut self, parent: usize, data: T) -> usize {
        self.insert(parent, -1, data)
    }

    /// Unlink a node from its parent (`g_node_unlink`).
    pub fn unlink(&mut self, id: usize) {
        let parent = self.get_node(id).parent;
        if let Some(p) = parent {
            let p_node = self.get_node_mut(p);
            p_node.children.retain(|&c| c != id);
        }
        if self.root == Some(id) {
            self.root = None;
        }
        self.get_node_mut(id).parent = None;
    }

    /// Destroy a node and its subtree (`g_node_destroy`).
    pub fn destroy(&mut self, id: usize) {
        self.unlink(id);
        let mut stack = vec![id];
        while let Some(nid) = stack.pop() {
            let children = core::mem::take(&mut self.get_node_mut(nid).children);
            stack.extend(children);
            self.nodes[nid] = None;
            self.free_list.push(nid);
        }
    }

    /// Get the root of the tree (`g_node_get_root`).
    pub fn root(&self) -> Option<usize> {
        self.root
    }

    /// Get the root ancestor of a node (`g_node_get_root`).
    pub fn get_root(&self, id: usize) -> usize {
        let mut current = id;
        while let Some(p) = self.get_node(current).parent {
            current = p;
        }
        current
    }

    /// Check if `ancestor` is an ancestor of `descendant` (`g_node_is_ancestor`).
    pub fn is_ancestor(&self, ancestor: usize, descendant: usize) -> bool {
        let mut current = descendant;
        loop {
            match self.get_node(current).parent {
                Some(p) if p == ancestor => return true,
                Some(p) => current = p,
                None => return false,
            }
        }
    }

    /// Get the depth of a node (`g_node_depth`).
    /// Root has depth 1.
    pub fn depth(&self, id: usize) -> u32 {
        let mut depth = 1;
        let mut current = id;
        while let Some(p) = self.get_node(current).parent {
            current = p;
            depth += 1;
        }
        depth
    }

    /// Count nodes in a subtree (`g_node_n_nodes`).
    pub fn n_nodes(&self, id: usize, flags: TraverseFlags) -> u32 {
        let mut count = 0;
        let mut stack = vec![id];
        while let Some(nid) = stack.pop() {
            let node = self.get_node(nid);
            let is_leaf = node.children.is_empty();
            if flags.matches(is_leaf) {
                count += 1;
            }
            stack.extend(node.children.iter().copied().rev());
        }
        count
    }

    /// Count children of a node (`g_node_n_children`).
    pub fn n_children(&self, id: usize) -> u32 {
        self.get_node(id).children.len() as u32
    }

    /// Get the nth child of a node (`g_node_nth_child`).
    pub fn nth_child(&self, id: usize, n: u32) -> Option<usize> {
        self.get_node(id).children.get(n as usize).copied()
    }

    /// Find a node by data (`g_node_find`).
    pub fn find(&self, root: usize, order: TraverseType, flags: TraverseFlags, pred: impl Fn(&T) -> bool) -> Option<usize> {
        match order {
            TraverseType::PreOrder => self.find_pre_order(root, flags, &pred),
            TraverseType::PostOrder => self.find_post_order(root, flags, &pred),
            TraverseType::InOrder => self.find_in_order(root, flags, &pred),
            TraverseType::LevelOrder => self.find_level_order(root, flags, &pred),
        }
    }

    fn find_pre_order(&self, id: usize, flags: TraverseFlags, pred: &impl Fn(&T) -> bool) -> Option<usize> {
        let node = self.get_node(id);
        let is_leaf = node.children.is_empty();
        if flags.matches(is_leaf) && pred(&node.data) {
            return Some(id);
        }
        for &child in &node.children {
            if let Some(found) = self.find_pre_order(child, flags, pred) {
                return Some(found);
            }
        }
        None
    }

    fn find_post_order(&self, id: usize, flags: TraverseFlags, pred: &impl Fn(&T) -> bool) -> Option<usize> {
        let node = self.get_node(id);
        for &child in &node.children {
            if let Some(found) = self.find_post_order(child, flags, pred) {
                return Some(found);
            }
        }
        let is_leaf = node.children.is_empty();
        if flags.matches(is_leaf) && pred(&node.data) {
            Some(id)
        } else {
            None
        }
    }

    fn find_in_order(&self, id: usize, flags: TraverseFlags, pred: &impl Fn(&T) -> bool) -> Option<usize> {
        let node = self.get_node(id);
        let children = &node.children;
        if !children.is_empty() {
            if let Some(found) = self.find_in_order(children[0], flags, pred) {
                return Some(found);
            }
        }
        let is_leaf = children.is_empty();
        if flags.matches(is_leaf) && pred(&node.data) {
            return Some(id);
        }
        for &child in &children[1..] {
            if let Some(found) = self.find_in_order(child, flags, pred) {
                return Some(found);
            }
        }
        None
    }

    fn find_level_order(&self, root: usize, flags: TraverseFlags, pred: &impl Fn(&T) -> bool) -> Option<usize> {
        let mut queue = vec![root];
        while !queue.is_empty() {
            let id = queue.remove(0);
            let node = self.get_node(id);
            let is_leaf = node.children.is_empty();
            if flags.matches(is_leaf) && pred(&node.data) {
                return Some(id);
            }
            queue.extend(node.children.iter().copied());
        }
        None
    }

    /// Traverse all nodes (`g_node_traverse`).
    pub fn traverse(&self, root: usize, order: TraverseType, flags: TraverseFlags, mut func: impl FnMut(usize, &T)) {
        match order {
            TraverseType::PreOrder => self.traverse_pre_order(root, flags, &mut func),
            TraverseType::PostOrder => self.traverse_post_order(root, flags, &mut func),
            TraverseType::InOrder => self.traverse_in_order(root, flags, &mut func),
            TraverseType::LevelOrder => self.traverse_level_order(root, flags, &mut func),
        }
    }

    fn traverse_pre_order(&self, id: usize, flags: TraverseFlags, func: &mut impl FnMut(usize, &T)) {
        let node = self.get_node(id);
        let is_leaf = node.children.is_empty();
        if flags.matches(is_leaf) {
            func(id, &node.data);
        }
        for &child in &node.children {
            self.traverse_pre_order(child, flags, func);
        }
    }

    fn traverse_post_order(&self, id: usize, flags: TraverseFlags, func: &mut impl FnMut(usize, &T)) {
        let node = self.get_node(id);
        for &child in &node.children {
            self.traverse_post_order(child, flags, func);
        }
        let is_leaf = node.children.is_empty();
        if flags.matches(is_leaf) {
            func(id, &node.data);
        }
    }

    fn traverse_in_order(&self, id: usize, flags: TraverseFlags, func: &mut impl FnMut(usize, &T)) {
        let node = self.get_node(id);
        let children = &node.children;
        if !children.is_empty() {
            self.traverse_in_order(children[0], flags, func);
        }
        let is_leaf = children.is_empty();
        if flags.matches(is_leaf) {
            func(id, &node.data);
        }
        for &child in &children[1..] {
            self.traverse_in_order(child, flags, func);
        }
    }

    fn traverse_level_order(&self, root: usize, flags: TraverseFlags, func: &mut impl FnMut(usize, &T)) {
        let mut queue = vec![root];
        while !queue.is_empty() {
            let id = queue.remove(0);
            let node = self.get_node(id);
            let is_leaf = node.children.is_empty();
            if flags.matches(is_leaf) {
                func(id, &node.data);
            }
            queue.extend(node.children.iter().copied());
        }
    }

    /// Get a reference to node data.
    pub fn get(&self, id: usize) -> &T {
        &self.get_node(id).data
    }

    /// Get a mutable reference to node data.
    pub fn get_mut(&mut self, id: usize) -> &mut T {
        &mut self.get_node_mut(id).data
    }

    /// Get the parent of a node.
    pub fn parent(&self, id: usize) -> Option<usize> {
        self.get_node(id).parent
    }

    /// Get the children of a node.
    pub fn children(&self, id: usize) -> &[usize] {
        &self.get_node(id).children
    }

    /// Check if a node is a leaf.
    pub fn is_leaf(&self, id: usize) -> bool {
        self.get_node(id).children.is_empty()
    }

    /// Check if a node is the root.
    pub fn is_root(&self, id: usize) -> bool {
        self.get_node(id).parent.is_none()
    }
}

impl<T> Default for NTree<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_tree() {
        let mut tree: NTree<i32> = NTree::new();
        let root = tree.new_root(1);
        let c1 = tree.append(root, 2);
        let c2 = tree.append(root, 3);
        assert_eq!(tree.n_children(root), 2);
        assert_eq!(tree.nth_child(root, 0), Some(c1));
        assert_eq!(tree.nth_child(root, 1), Some(c2));
    }

    #[test]
    fn depth_and_ancestors() {
        let mut tree: NTree<i32> = NTree::new();
        let root = tree.new_root(1);
        let c1 = tree.append(root, 2);
        let gc1 = tree.append(c1, 3);
        assert_eq!(tree.depth(root), 1);
        assert_eq!(tree.depth(c1), 2);
        assert_eq!(tree.depth(gc1), 3);
        assert!(tree.is_ancestor(root, gc1));
        assert!(tree.is_ancestor(c1, gc1));
        assert!(!tree.is_ancestor(gc1, root));
    }

    #[test]
    fn n_nodes_count() {
        let mut tree: NTree<i32> = NTree::new();
        let root = tree.new_root(1);
        tree.append(root, 2);
        tree.append(root, 3);
        let c1 = tree.append(root, 4);
        tree.append(c1, 5);
        tree.append(c1, 6);
        assert_eq!(tree.n_nodes(root, TraverseFlags::ALL), 6);
        assert_eq!(tree.n_nodes(root, TraverseFlags::LEAVES), 4);
        assert_eq!(tree.n_nodes(root, TraverseFlags::NON_LEAVES), 2);
    }

    #[test]
    fn find_pre_order() {
        let mut tree: NTree<i32> = NTree::new();
        let root = tree.new_root(1);
        tree.append(root, 2);
        tree.append(root, 3);
        tree.append(root, 4);

        let found = tree.find(root, TraverseType::PreOrder, TraverseFlags::ALL, |d| *d == 3);
        assert!(found.is_some());
        assert_eq!(tree.get(found.unwrap()), &3);

        let not_found = tree.find(root, TraverseType::PreOrder, TraverseFlags::ALL, |d| *d == 99);
        assert!(not_found.is_none());
    }

    #[test]
    fn traverse_pre_order() {
        let mut tree: NTree<i32> = NTree::new();
        let root = tree.new_root(1);
        tree.append(root, 2);
        tree.append(root, 3);
        let c1 = tree.append(root, 4);
        tree.append(c1, 5);

        let mut visited = Vec::new();
        tree.traverse(root, TraverseType::PreOrder, TraverseFlags::ALL, |_, data| {
            visited.push(*data);
        });
        assert_eq!(visited, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn traverse_post_order() {
        let mut tree: NTree<i32> = NTree::new();
        let root = tree.new_root(1);
        tree.append(root, 2);
        tree.append(root, 3);
        let c1 = tree.append(root, 4);
        tree.append(c1, 5);

        let mut visited = Vec::new();
        tree.traverse(root, TraverseType::PostOrder, TraverseFlags::ALL, |_, data| {
            visited.push(*data);
        });
        assert_eq!(visited, vec![2, 3, 5, 4, 1]);
    }

    #[test]
    fn unlink_and_destroy() {
        let mut tree: NTree<i32> = NTree::new();
        let root = tree.new_root(1);
        let c1 = tree.append(root, 2);
        tree.append(c1, 3);
        assert_eq!(tree.n_nodes(root, TraverseFlags::ALL), 3);
        tree.destroy(c1);
        assert_eq!(tree.n_nodes(root, TraverseFlags::ALL), 1);
        assert_eq!(tree.n_children(root), 0);
    }

    #[test]
    fn insert_at_position() {
        let mut tree: NTree<i32> = NTree::new();
        let root = tree.new_root(0);
        tree.append(root, 1);
        tree.append(root, 3);
        tree.insert(root, 1, 2);
        let children: Vec<i32> = tree.children(root).iter().map(|&id| *tree.get(id)).collect();
        assert_eq!(children, vec![1, 2, 3]);
    }
}
