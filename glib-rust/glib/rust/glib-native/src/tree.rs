//! Balanced binary tree matching `gtree.h` / `gtree.c`.
//!
//! GLib stores nodes in a threaded AVL tree: child pointers double as in-order
//! predecessor/successor links when [`GTreeNode::left_child`] or
//! [`GTreeNode::right_child`] is false. Rotations and balance updates follow
//! the C implementation so height, foreach order, and deprecated
//! [`Tree::traverse`] pre/post-order sequences match upstream tests.
//!
//! A [`alloc::collections::BTreeMap`] keyed by owned values would provide sorted
//! iteration with the same *compare* semantics, but would not reproduce GLib's
//! internal topology, [`Tree::height`] values, or pre/post-order walks.

use crate::prelude::*;
use crate::refcount::AtomicRefCount;
use core::cell::RefCell;
use core::ffi::c_void;
use core::mem::ManuallyDrop;
use core::ptr::{self, NonNull};

/// Comparison function with user data (`GCompareDataFunc`).
pub type CompareDataFn = fn(*const c_void, *const c_void, *mut c_void) -> i32;

/// Comparison function without user data (`GCompareFunc`).
pub type CompareFn = fn(*const c_void, *const c_void) -> i32;

/// Key or value destructor (`GDestroyNotify`).
pub type DestroyNotify = fn(*mut c_void);

/// Foreach callback; return `true` to stop (`GTraverseFunc`).
pub type TraverseFn = fn(*mut c_void, *mut c_void, *mut c_void) -> bool;

/// Foreach-node callback; return `true` to stop (`GTraverseNodeFunc`).
pub type TraverseNodeFn = fn(*mut GTreeNode, *mut c_void) -> bool;

/// Tree walk order (`GTraverseType`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum TraverseType {
    /// In-order (sorted key order).
    InOrder = 0,
    /// Pre-order (root, left, right).
    PreOrder = 1,
    /// Post-order (left, right, root).
    PostOrder = 2,
    /// Not implemented in GLib either.
    LevelOrder = 3,
}

/// Balanced tree node (`GTreeNode`).
///
/// Child flags distinguish real subtrees from in-order thread links stored in
/// [`GTreeNode::left`] / [`GTreeNode::right`].
#[repr(C)]
#[derive(Debug)]
pub struct GTreeNode {
    /// Key stored at this node.
    pub key: *mut c_void,
    /// Value stored at this node.
    pub value: *mut c_void,
    /// Left subtree, or in-order predecessor when [`GTreeNode::left_child`] is false.
    pub left: *mut GTreeNode,
    /// Right subtree, or in-order successor when [`GTreeNode::right_child`] is false.
    pub right: *mut GTreeNode,
    /// Height difference: right height minus left height.
    pub balance: i8,
    /// Whether [`GTreeNode::left`] is a child subtree.
    pub left_child: u8,
    /// Whether [`GTreeNode::right`] is a child subtree.
    pub right_child: u8,
}

const MAX_GTREE_HEIGHT: usize = 48;

enum KeyCompare {
    Plain(CompareFn),
    WithData(CompareDataFn, *mut c_void),
}

impl KeyCompare {
    fn compare(&self, a: *const c_void, b: *const c_void) -> i32 {
        match self {
            Self::Plain(f) => f(a, b),
            Self::WithData(f, data) => f(a, b, *data),
        }
    }
}

struct TreeInner {
    key_compare: KeyCompare,
    key_destroy_func: Option<DestroyNotify>,
    value_destroy_func: Option<DestroyNotify>,
    root: RefCell<*mut GTreeNode>,
    nnodes: RefCell<u32>,
    ref_count: AtomicRefCount,
}

/// Type-erased balanced binary tree (`GTree`).
pub struct Tree {
    inner: NonNull<TreeInner>,
}

impl Tree {
    /// Create a tree (`g_tree_new`).
    pub fn new(key_compare_func: CompareFn) -> Self {
        Self::from_inner(TreeInner {
            key_compare: KeyCompare::Plain(key_compare_func),
            key_destroy_func: None,
            value_destroy_func: None,
            root: RefCell::new(ptr::null_mut()),
            nnodes: RefCell::new(0),
            ref_count: AtomicRefCount::new(),
        })
    }

    /// Create a tree with compare user data (`g_tree_new_with_data`).
    pub fn new_with_data(key_compare_func: CompareDataFn, key_compare_data: *mut c_void) -> Self {
        Self::new_full(key_compare_func, key_compare_data, None, None)
    }

    /// Create a tree with compare and destroy callbacks (`g_tree_new_full`).
    pub fn new_full(
        key_compare_func: CompareDataFn,
        key_compare_data: *mut c_void,
        key_destroy_func: Option<DestroyNotify>,
        value_destroy_func: Option<DestroyNotify>,
    ) -> Self {
        Self::from_inner(TreeInner {
            key_compare: KeyCompare::WithData(key_compare_func, key_compare_data),
            key_destroy_func,
            value_destroy_func,
            root: RefCell::new(ptr::null_mut()),
            nnodes: RefCell::new(0),
            ref_count: AtomicRefCount::new(),
        })
    }

    /// Increment the reference count (`g_tree_ref`).
    #[must_use]
    pub fn ref_(&self) -> Self {
        self.inner().ref_count.inc();
        Self { inner: self.inner }
    }

    /// Decrement the reference count; frees at zero (`g_tree_unref`).
    pub fn unref(self) {
        let this = ManuallyDrop::new(self);
        if this.inner().ref_count.dec() {
            release_tree(this.inner);
        }
    }

    /// Remove all nodes and decrement the reference count (`g_tree_destroy`).
    pub fn destroy(self) {
        let this = ManuallyDrop::new(self);
        this.inner().remove_all_nodes();
        if this.inner().ref_count.dec() {
            release_tree(this.inner);
        }
    }

    /// Remove every node, invoking destroy callbacks (`g_tree_remove_all`).
    pub fn remove_all(&self) {
        self.inner().remove_all_nodes();
    }

    /// Insert a key/value pair (`g_tree_insert`).
    pub fn insert(&self, key: *mut c_void, value: *mut c_void) {
        let _ = self
            .inner()
            .insert_replace_internal(key, value, false, false);
    }

    /// Insert and return the node (`g_tree_insert_node`).
    pub fn insert_node(&self, key: *mut c_void, value: *mut c_void) -> Option<*mut GTreeNode> {
        self.inner()
            .insert_replace_internal(key, value, false, true)
    }

    /// Replace an existing key or insert (`g_tree_replace`).
    pub fn replace(&self, key: *mut c_void, value: *mut c_void) {
        let _ = self
            .inner()
            .insert_replace_internal(key, value, true, false);
    }

    /// Replace and return the node (`g_tree_replace_node`).
    pub fn replace_node(&self, key: *mut c_void, value: *mut c_void) -> Option<*mut GTreeNode> {
        self.inner().insert_replace_internal(key, value, true, true)
    }

    /// Look up a value by key (`g_tree_lookup`).
    pub fn lookup(&self, key: *const c_void) -> *mut c_void {
        self.lookup_node(key)
            .map(|node| unsafe { (*node).value })
            .unwrap_or(ptr::null_mut())
    }

    /// Look up the node for a key (`g_tree_lookup_node`).
    pub fn lookup_node(&self, key: *const c_void) -> Option<*mut GTreeNode> {
        self.inner().find_node(key)
    }

    /// Look up key and value pointers (`g_tree_lookup_extended`).
    pub fn lookup_extended(
        &self,
        lookup_key: *const c_void,
        orig_key: Option<&mut *mut c_void>,
        value: Option<&mut *mut c_void>,
    ) -> bool {
        match self.inner().find_node(lookup_key) {
            Some(node) => {
                unsafe {
                    if let Some(orig_key) = orig_key {
                        *orig_key = (*node).key;
                    }
                    if let Some(value) = value {
                        *value = (*node).value;
                    }
                }
                true
            }
            None => false,
        }
    }

    /// Remove a key, calling destroy funcs (`g_tree_remove`).
    pub fn remove(&self, key: *const c_void) -> bool {
        self.inner().remove_internal(key, false)
    }

    /// Remove a key without destroy funcs (`g_tree_steal`).
    pub fn steal(&self, key: *const c_void) -> bool {
        self.inner().remove_internal(key, true)
    }

    /// Call `func` for each pair in sorted order (`g_tree_foreach`).
    pub fn foreach(&self, func: TraverseFn, user_data: *mut c_void) {
        self.inner().foreach(func, user_data);
    }

    /// Call `func` for each node in sorted order (`g_tree_foreach_node`).
    pub fn foreach_node(&self, func: TraverseNodeFn, user_data: *mut c_void) {
        self.inner().foreach_node(func, user_data);
    }

    /// Walk the tree in the given order (`g_tree_traverse`).
    pub fn traverse(
        &self,
        traverse_func: TraverseFn,
        traverse_type: TraverseType,
        user_data: *mut c_void,
    ) {
        self.inner()
            .traverse(traverse_func, traverse_type, user_data);
    }

    /// Search with a custom compare against keys (`g_tree_search`).
    pub fn search(&self, search_func: CompareFn, user_data: *const c_void) -> *mut c_void {
        self.search_node(search_func, user_data)
            .map(|node| unsafe { (*node).value })
            .unwrap_or(ptr::null_mut())
    }

    /// Search and return the matching node (`g_tree_search_node`).
    pub fn search_node(
        &self,
        search_func: CompareFn,
        user_data: *const c_void,
    ) -> Option<*mut GTreeNode> {
        let root = *self.inner().root.borrow();
        if root.is_null() {
            return None;
        }
        node_search(root, search_func, user_data)
    }

    /// First node with key >= `key` (`g_tree_lower_bound`).
    pub fn lower_bound(&self, key: *const c_void) -> Option<*mut GTreeNode> {
        self.inner().lower_bound(key)
    }

    /// First node with key > `key` (`g_tree_upper_bound`).
    pub fn upper_bound(&self, key: *const c_void) -> Option<*mut GTreeNode> {
        self.inner().upper_bound(key)
    }

    /// Tree height (`g_tree_height`).
    pub fn height(&self) -> i32 {
        self.inner().height()
    }

    /// Number of nodes (`g_tree_nnodes`).
    pub fn nnodes(&self) -> i32 {
        *self.inner().nnodes.borrow() as i32
    }

    /// First in-order node (`g_tree_node_first`).
    pub fn node_first(&self) -> Option<*mut GTreeNode> {
        self.inner().node_first()
    }

    /// Last in-order node (`g_tree_node_last`).
    pub fn node_last(&self) -> Option<*mut GTreeNode> {
        self.inner().node_last()
    }

    /// Previous in-order node (`g_tree_node_previous`).
    pub fn node_previous(&self, node: *mut GTreeNode) -> Option<*mut GTreeNode> {
        self.inner().node_previous(node)
    }

    /// Next in-order node (`g_tree_node_next`).
    pub fn node_next(&self, node: *mut GTreeNode) -> Option<*mut GTreeNode> {
        self.inner().node_next(node)
    }

    /// Key at a node (`g_tree_node_key`).
    ///
    /// # Safety
    ///
    /// `node` must be a valid node belonging to a tree.
    pub unsafe fn node_key(node: *mut GTreeNode) -> *mut c_void {
        unsafe { (*node).key }
    }

    /// Value at a node (`g_tree_node_value`).
    ///
    /// # Safety
    ///
    /// `node` must be a valid node belonging to a tree.
    pub unsafe fn node_value(node: *mut GTreeNode) -> *mut c_void {
        unsafe { (*node).value }
    }

    fn from_inner(inner: TreeInner) -> Self {
        Self {
            inner: NonNull::from(Box::leak(Box::new(inner))),
        }
    }

    fn inner(&self) -> &TreeInner {
        unsafe { self.inner.as_ref() }
    }
}

impl Clone for Tree {
    fn clone(&self) -> Self {
        self.ref_()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        if self.inner().ref_count.dec() {
            release_tree(self.inner);
        }
    }
}

impl TreeInner {
    fn remove_all_nodes(&self) {
        let Some(mut node) = self.node_first() else {
            *self.root.borrow_mut() = ptr::null_mut();
            *self.nnodes.borrow_mut() = 0;
            return;
        };

        while !node.is_null() {
            let next = self.node_next(node);
            if let Some(key_destroy) = self.key_destroy_func {
                key_destroy(unsafe { (*node).key });
            }
            if let Some(value_destroy) = self.value_destroy_func {
                value_destroy(unsafe { (*node).value });
            }
            free_tree_node(node);
            node = next.unwrap_or(ptr::null_mut());
        }

        *self.root.borrow_mut() = ptr::null_mut();
        *self.nnodes.borrow_mut() = 0;
    }

    fn foreach(&self, func: TraverseFn, user_data: *mut c_void) {
        let Some(mut node) = self.node_first() else {
            return;
        };
        loop {
            let stop = unsafe { func((*node).key, (*node).value, user_data) };
            if stop {
                break;
            }
            match self.node_next(node) {
                Some(next) => node = next,
                None => break,
            }
        }
    }

    fn foreach_node(&self, func: TraverseNodeFn, user_data: *mut c_void) {
        let Some(mut node) = self.node_first() else {
            return;
        };
        loop {
            let stop = func(node, user_data);
            if stop {
                break;
            }
            match self.node_next(node) {
                Some(next) => node = next,
                None => break,
            }
        }
    }

    fn traverse(
        &self,
        traverse_func: TraverseFn,
        traverse_type: TraverseType,
        user_data: *mut c_void,
    ) {
        let root = *self.root.borrow();
        if root.is_null() {
            return;
        }
        match traverse_type {
            TraverseType::PreOrder => {
                let _ = node_pre_order(root, traverse_func, user_data);
            }
            TraverseType::InOrder => {
                let _ = node_in_order(root, traverse_func, user_data);
            }
            TraverseType::PostOrder => {
                let _ = node_post_order(root, traverse_func, user_data);
            }
            TraverseType::LevelOrder => {
                gwarn!("glib-native: Tree::traverse: G_LEVEL_ORDER isn't implemented");
            }
        }
    }

    fn height(&self) -> i32 {
        let root = *self.root.borrow();
        if root.is_null() {
            return 0;
        }
        let mut height = 0;
        let mut node = root;
        loop {
            height += 1 + i32::from(unsafe { (*node).balance.max(0) });
            if unsafe { (*node).left_child == 0 } {
                return height;
            }
            node = unsafe { (*node).left };
        }
    }

    fn node_first(&self) -> Option<*mut GTreeNode> {
        let root = *self.root.borrow();
        if root.is_null() {
            return None;
        }
        let mut tmp = root;
        while unsafe { (*tmp).left_child != 0 } {
            tmp = unsafe { (*tmp).left };
        }
        Some(tmp)
    }

    fn node_last(&self) -> Option<*mut GTreeNode> {
        let root = *self.root.borrow();
        if root.is_null() {
            return None;
        }
        let mut tmp = root;
        while unsafe { (*tmp).right_child != 0 } {
            tmp = unsafe { (*tmp).right };
        }
        Some(tmp)
    }

    fn node_previous(&self, node: *mut GTreeNode) -> Option<*mut GTreeNode> {
        debug_assert!(!node.is_null());
        let mut tmp = unsafe { (*node).left };
        if unsafe { (*node).left_child != 0 } {
            while unsafe { (*tmp).right_child != 0 } {
                tmp = unsafe { (*tmp).right };
            }
        }
        if tmp.is_null() {
            None
        } else {
            Some(tmp)
        }
    }

    fn node_next(&self, node: *mut GTreeNode) -> Option<*mut GTreeNode> {
        debug_assert!(!node.is_null());
        let mut tmp = unsafe { (*node).right };
        if unsafe { (*node).right_child != 0 } {
            while unsafe { (*tmp).left_child != 0 } {
                tmp = unsafe { (*tmp).left };
            }
        }
        if tmp.is_null() {
            None
        } else {
            Some(tmp)
        }
    }

    fn find_node(&self, key: *const c_void) -> Option<*mut GTreeNode> {
        let mut node = *self.root.borrow();
        if node.is_null() {
            return None;
        }
        loop {
            let cmp = self.key_compare.compare(key, unsafe { (*node).key });
            if cmp == 0 {
                return Some(node);
            }
            if cmp < 0 {
                if unsafe { (*node).left_child == 0 } {
                    return None;
                }
                node = unsafe { (*node).left };
            } else if unsafe { (*node).right_child == 0 } {
                return None;
            } else {
                node = unsafe { (*node).right };
            }
        }
    }

    fn lower_bound(&self, key: *const c_void) -> Option<*mut GTreeNode> {
        let mut node = *self.root.borrow();
        if node.is_null() {
            return None;
        }
        let mut result = None;
        loop {
            let cmp = self.key_compare.compare(key, unsafe { (*node).key });
            if cmp <= 0 {
                result = Some(node);
                if unsafe { (*node).left_child == 0 } {
                    return result;
                }
                node = unsafe { (*node).left };
            } else if unsafe { (*node).right_child == 0 } {
                return result;
            } else {
                node = unsafe { (*node).right };
            }
        }
    }

    fn upper_bound(&self, key: *const c_void) -> Option<*mut GTreeNode> {
        let mut node = *self.root.borrow();
        if node.is_null() {
            return None;
        }
        let mut result = None;
        loop {
            let cmp = self.key_compare.compare(key, unsafe { (*node).key });
            if cmp < 0 {
                result = Some(node);
                if unsafe { (*node).left_child == 0 } {
                    return result;
                }
                node = unsafe { (*node).left };
            } else if unsafe { (*node).right_child == 0 } {
                return result;
            } else {
                node = unsafe { (*node).right };
            }
        }
    }

    fn nnodes_inc_checked(&self, overflow_fatal: bool) -> bool {
        let mut nnodes = self.nnodes.borrow_mut();
        if *nnodes == u32::MAX {
            if overflow_fatal {
                panic!("Incrementing Tree nnodes counter would overflow");
            }
            return false;
        }
        *nnodes += 1;
        true
    }

    fn insert_replace_internal(
        &self,
        key: *mut c_void,
        value: *mut c_void,
        replace: bool,
        null_ret_ok: bool,
    ) -> Option<*mut GTreeNode> {
        let mut root = *self.root.borrow();
        if root.is_null() {
            root = tree_node_new(key, value);
            *self.root.borrow_mut() = root;
            *self.nnodes.borrow_mut() += 1;
            return Some(root);
        }

        let mut path: [*mut GTreeNode; MAX_GTREE_HEIGHT] = [ptr::null_mut(); MAX_GTREE_HEIGHT];
        let mut idx = 0usize;
        path[idx] = ptr::null_mut();
        idx += 1;
        let mut node = root;

        let retnode = loop {
            let cmp = self.key_compare.compare(key, unsafe { (*node).key });
            if cmp == 0 {
                if let Some(value_destroy) = self.value_destroy_func {
                    value_destroy(unsafe { (*node).value });
                }
                unsafe {
                    (*node).value = value;
                }
                if replace {
                    if let Some(key_destroy) = self.key_destroy_func {
                        key_destroy(unsafe { (*node).key });
                    }
                    unsafe {
                        (*node).key = key;
                    }
                } else if let Some(key_destroy) = self.key_destroy_func {
                    key_destroy(key);
                }
                return Some(node);
            }
            if cmp < 0 {
                if unsafe { (*node).left_child != 0 } {
                    path[idx] = node;
                    idx += 1;
                    node = unsafe { (*node).left };
                } else {
                    if !self.nnodes_inc_checked(!null_ret_ok) {
                        return None;
                    }
                    let child = tree_node_new(key, value);
                    unsafe {
                        (*child).left = (*node).left;
                        (*child).right = node;
                        (*node).left = child;
                        (*node).left_child = 1;
                        (*node).balance -= 1;
                    }
                    break child;
                }
            } else if unsafe { (*node).right_child != 0 } {
                path[idx] = node;
                idx += 1;
                node = unsafe { (*node).right };
            } else {
                if !self.nnodes_inc_checked(!null_ret_ok) {
                    return None;
                }
                let child = tree_node_new(key, value);
                unsafe {
                    (*child).right = (*node).right;
                    (*child).left = node;
                    (*node).right = child;
                    (*node).right_child = 1;
                    (*node).balance += 1;
                }
                break child;
            }
        };

        loop {
            idx -= 1;
            let bparent = path[idx];
            let left_node = !bparent.is_null() && node == unsafe { (*bparent).left };
            debug_assert!(
                bparent.is_null() || unsafe { (*bparent).left == node || (*bparent).right == node }
            );

            if unsafe { (*node).balance < -1 || (*node).balance > 1 } {
                node = node_balance(node);
                if bparent.is_null() {
                    root = node;
                } else if left_node {
                    unsafe {
                        (*bparent).left = node;
                    }
                } else {
                    unsafe {
                        (*bparent).right = node;
                    }
                }
            }

            if unsafe { (*node).balance == 0 } || bparent.is_null() {
                break;
            }

            if left_node {
                unsafe {
                    (*bparent).balance -= 1;
                }
            } else {
                unsafe {
                    (*bparent).balance += 1;
                }
            }
            node = bparent;
        }

        *self.root.borrow_mut() = root;
        Some(retnode)
    }

    fn remove_internal(&self, key: *const c_void, steal: bool) -> bool {
        let mut root = *self.root.borrow();
        if root.is_null() {
            return false;
        }

        let mut path: [*mut GTreeNode; MAX_GTREE_HEIGHT] = [ptr::null_mut(); MAX_GTREE_HEIGHT];
        let mut idx = 0usize;
        path[idx] = ptr::null_mut();
        idx += 1;
        let mut node = root;

        loop {
            let cmp = self.key_compare.compare(key, unsafe { (*node).key });
            if cmp == 0 {
                break;
            }
            if cmp < 0 {
                if unsafe { (*node).left_child == 0 } {
                    return false;
                }
                path[idx] = node;
                idx += 1;
                node = unsafe { (*node).left };
            } else {
                if unsafe { (*node).right_child == 0 } {
                    return false;
                }
                path[idx] = node;
                idx += 1;
                node = unsafe { (*node).right };
            }
        }

        idx -= 1;
        let parent = path[idx];
        debug_assert!(
            parent.is_null() || unsafe { (*parent).left == node || (*parent).right == node }
        );
        let left_node = !parent.is_null() && node == unsafe { (*parent).left };
        let mut balance = parent;

        unsafe {
            if (*node).left_child == 0 {
                if (*node).right_child == 0 {
                    if parent.is_null() {
                        root = ptr::null_mut();
                    } else if left_node {
                        (*parent).left_child = 0;
                        (*parent).left = (*node).left;
                        (*parent).balance += 1;
                    } else {
                        (*parent).right_child = 0;
                        (*parent).right = (*node).right;
                        (*parent).balance -= 1;
                    }
                } else {
                    let tmp = self.node_next(node).expect("right child implies successor");
                    (*tmp).left = (*node).left;
                    if parent.is_null() {
                        root = (*node).right;
                    } else if left_node {
                        (*parent).left = (*node).right;
                        (*parent).balance += 1;
                    } else {
                        (*parent).right = (*node).right;
                        (*parent).balance -= 1;
                    }
                }
            } else if (*node).right_child == 0 {
                let tmp = self
                    .node_previous(node)
                    .expect("left child implies predecessor");
                (*tmp).right = (*node).right;
                if parent.is_null() {
                    root = (*node).left;
                } else if left_node {
                    (*parent).left = (*node).left;
                    (*parent).balance += 1;
                } else {
                    (*parent).right = (*node).left;
                    (*parent).balance -= 1;
                }
            } else {
                let mut prev = (*node).left;
                let mut next = (*node).right;
                let mut nextp = node;
                let old_idx = idx + 1;
                idx += 1;

                while (*next).left_child != 0 {
                    idx += 1;
                    nextp = next;
                    path[idx] = nextp;
                    next = (*next).left;
                }

                path[old_idx] = next;
                balance = path[idx];

                if nextp != node {
                    if (*next).right_child != 0 {
                        (*nextp).left = (*next).right;
                    } else {
                        (*nextp).left_child = 0;
                    }
                    (*nextp).balance += 1;
                    (*next).right_child = 1;
                    (*next).right = (*node).right;
                } else {
                    (*node).balance -= 1;
                }

                while (*prev).right_child != 0 {
                    prev = (*prev).right;
                }
                (*prev).right = next;

                (*next).left_child = 1;
                (*next).left = (*node).left;
                (*next).balance = (*node).balance;

                if parent.is_null() {
                    root = next;
                } else if left_node {
                    (*parent).left = next;
                } else {
                    (*parent).right = next;
                }
            }
        }

        if !balance.is_null() {
            loop {
                idx -= 1;
                let bparent = path[idx];
                debug_assert!(
                    bparent.is_null()
                        || unsafe { (*bparent).left == balance || (*bparent).right == balance }
                );
                let balance_left = !bparent.is_null() && balance == unsafe { (*bparent).left };

                if unsafe { (*balance).balance < -1 || (*balance).balance > 1 } {
                    balance = node_balance(balance);
                    if bparent.is_null() {
                        root = balance;
                    } else if balance_left {
                        unsafe {
                            (*bparent).left = balance;
                        }
                    } else {
                        unsafe {
                            (*bparent).right = balance;
                        }
                    }
                }

                if unsafe { (*balance).balance != 0 } || bparent.is_null() {
                    break;
                }

                if balance_left {
                    unsafe {
                        (*bparent).balance += 1;
                    }
                } else {
                    unsafe {
                        (*bparent).balance -= 1;
                    }
                }
                balance = bparent;
            }
        }

        if !steal {
            if let Some(key_destroy) = self.key_destroy_func {
                key_destroy(unsafe { (*node).key });
            }
            if let Some(value_destroy) = self.value_destroy_func {
                value_destroy(unsafe { (*node).value });
            }
        }

        free_tree_node(node);
        *self.root.borrow_mut() = root;
        *self.nnodes.borrow_mut() -= 1;
        true
    }
}

fn release_tree(inner: NonNull<TreeInner>) {
    unsafe {
        let inner = Box::from_raw(inner.as_ptr());
        inner.remove_all_nodes();
    }
}

fn tree_node_new(key: *mut c_void, value: *mut c_void) -> *mut GTreeNode {
    let node = Box::new(GTreeNode {
        key,
        value,
        left: ptr::null_mut(),
        right: ptr::null_mut(),
        balance: 0,
        left_child: 0,
        right_child: 0,
    });
    Box::into_raw(node)
}

fn free_tree_node(node: *mut GTreeNode) {
    if !node.is_null() {
        unsafe {
            drop(Box::from_raw(node));
        }
    }
}

fn node_balance(mut node: *mut GTreeNode) -> *mut GTreeNode {
    unsafe {
        if (*node).balance < -1 {
            if (*(*node).left).balance > 0 {
                (*node).left = node_rotate_left((*node).left);
            }
            node = node_rotate_right(node);
        } else if (*node).balance > 1 {
            if (*(*node).right).balance < 0 {
                (*node).right = node_rotate_right((*node).right);
            }
            node = node_rotate_left(node);
        }
    }
    node
}

fn node_rotate_left(node: *mut GTreeNode) -> *mut GTreeNode {
    unsafe {
        let right = (*node).right;
        if (*right).left_child != 0 {
            (*node).right = (*right).left;
        } else {
            (*node).right_child = 0;
            (*right).left_child = 1;
        }
        (*right).left = node;

        let a_bal = (*node).balance;
        let b_bal = (*right).balance;

        if b_bal <= 0 {
            if a_bal >= 1 {
                (*right).balance = b_bal - 1;
            } else {
                (*right).balance = a_bal + b_bal - 2;
            }
            (*node).balance = a_bal - 1;
        } else if a_bal <= b_bal {
            (*right).balance = a_bal - 2;
        } else {
            (*right).balance = b_bal - 1;
        }
        (*node).balance = a_bal - b_bal - 1;

        right
    }
}

fn node_rotate_right(node: *mut GTreeNode) -> *mut GTreeNode {
    unsafe {
        let left = (*node).left;
        if (*left).right_child != 0 {
            (*node).left = (*left).right;
        } else {
            (*node).left_child = 0;
            (*left).right_child = 1;
        }
        (*left).right = node;

        let a_bal = (*node).balance;
        let b_bal = (*left).balance;

        if b_bal <= 0 {
            if b_bal > a_bal {
                (*left).balance = b_bal + 1;
            } else {
                (*left).balance = a_bal + 2;
            }
            (*node).balance = a_bal - b_bal + 1;
        } else if a_bal <= -1 {
            (*left).balance = b_bal + 1;
        } else {
            (*left).balance = a_bal + b_bal + 2;
        }
        (*node).balance = a_bal + 1;

        left
    }
}

fn node_pre_order(node: *mut GTreeNode, traverse_func: TraverseFn, data: *mut c_void) -> bool {
    unsafe {
        if traverse_func((*node).key, (*node).value, data) {
            return true;
        }
        if (*node).left_child != 0 && node_pre_order((*node).left, traverse_func, data) {
            return true;
        }
        if (*node).right_child != 0 && node_pre_order((*node).right, traverse_func, data) {
            return true;
        }
    }
    false
}

fn node_in_order(node: *mut GTreeNode, traverse_func: TraverseFn, data: *mut c_void) -> bool {
    unsafe {
        if (*node).left_child != 0 && node_in_order((*node).left, traverse_func, data) {
            return true;
        }
        if traverse_func((*node).key, (*node).value, data) {
            return true;
        }
        if (*node).right_child != 0 && node_in_order((*node).right, traverse_func, data) {
            return true;
        }
    }
    false
}

fn node_post_order(node: *mut GTreeNode, traverse_func: TraverseFn, data: *mut c_void) -> bool {
    unsafe {
        if (*node).left_child != 0 && node_post_order((*node).left, traverse_func, data) {
            return true;
        }
        if (*node).right_child != 0 && node_post_order((*node).right, traverse_func, data) {
            return true;
        }
        if traverse_func((*node).key, (*node).value, data) {
            return true;
        }
    }
    false
}

fn node_search(
    mut node: *mut GTreeNode,
    search_func: CompareFn,
    data: *const c_void,
) -> Option<*mut GTreeNode> {
    loop {
        let dir = search_func(unsafe { (*node).key }, data);
        if dir == 0 {
            return Some(node);
        }
        if dir < 0 {
            if unsafe { (*node).left_child == 0 } {
                return None;
            }
            node = unsafe { (*node).left };
        } else if unsafe { (*node).right_child == 0 } {
            return None;
        } else {
            node = unsafe { (*node).right };
        }
    }
}

// ---------------------------------------------------------------------------
// Tests (mirrors `glib/tests/tree.c`)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    static CHARS: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
    static CHARS2: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";

    fn my_compare(a: *const c_void, b: *const c_void) -> i32 {
        let cha = unsafe { *(a as *const u8) };
        let chb = unsafe { *(b as *const u8) };
        i32::from(cha) - i32::from(chb)
    }

    fn my_compare_data(a: *const c_void, b: *const c_void, user_data: *mut c_void) -> i32 {
        assert_eq!(user_data as isize, 123);
        my_compare(a, b)
    }

    fn my_compare_no_data(a: *const c_void, b: *const c_void, _user_data: *mut c_void) -> i32 {
        my_compare(a, b)
    }

    fn char_key(i: usize) -> *mut c_void {
        &CHARS[i] as *const u8 as *mut c_void
    }

    fn my_search(a: *const c_void, b: *const c_void) -> i32 {
        my_compare(b, a)
    }

    thread_local! {
        static DESTROYED_KEY: core::cell::Cell<*mut c_void> =
            core::cell::Cell::new(ptr::null_mut());
        static DESTROYED_VALUE: core::cell::Cell<*mut c_void> =
            core::cell::Cell::new(ptr::null_mut());
        static DESTROYED_KEY_COUNT: core::cell::Cell<u32> = core::cell::Cell::new(0);
        static DESTROYED_VALUE_COUNT: core::cell::Cell<u32> = core::cell::Cell::new(0);
    }

    fn reset_destroy_tracking() {
        DESTROYED_KEY.with(|slot| slot.set(ptr::null_mut()));
        DESTROYED_VALUE.with(|slot| slot.set(ptr::null_mut()));
        DESTROYED_KEY_COUNT.with(|slot| slot.set(0));
        DESTROYED_VALUE_COUNT.with(|slot| slot.set(0));
    }

    fn my_key_destroy(key: *mut c_void) {
        DESTROYED_KEY.with(|slot| slot.set(key));
        DESTROYED_KEY_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    }

    fn my_value_destroy(value: *mut c_void) {
        DESTROYED_VALUE.with(|slot| slot.set(value));
        DESTROYED_VALUE_COUNT.with(|count| count.set(count.get().saturating_add(1)));
    }

    fn my_traverse(key: *mut c_void, _value: *mut c_void, _data: *mut c_void) -> bool {
        let ch = unsafe { *(key as *const u8) };
        assert!(ch > 0);
        ch == b'd'
    }

    struct OrderChecker<'a> {
        expected: &'a [u8],
        pos: usize,
    }

    fn check_order(key: *mut c_void, _value: *mut c_void, data: *mut c_void) -> bool {
        let checker = unsafe { &mut *(data as *mut OrderChecker<'_>) };
        let ch = unsafe { *(key as *const u8) };
        assert_eq!(checker.expected[checker.pos], ch);
        checker.pos += 1;
        false
    }

    #[test]
    fn tree_search() {
        let tree = Tree::new_with_data(my_compare_data, 123 as *mut c_void);

        for i in 0..CHARS.len() {
            let key = char_key(i);
            tree.insert(key, key);
        }

        tree.foreach(my_traverse, ptr::null_mut());
        assert_eq!(tree.nnodes() as usize, CHARS.len());
        assert_eq!(tree.height(), 6);

        let mut checker = OrderChecker {
            expected: CHARS,
            pos: 0,
        };
        tree.foreach(
            check_order,
            (&mut checker as *mut OrderChecker<'_>).cast::<c_void>(),
        );

        for i in 0..26 {
            let key = &CHARS[i + 10] as *const u8 as *const c_void;
            assert!(tree.remove(key));
        }

        let nul = 0u8;
        assert!(!tree.remove((&nul as *const u8).cast()));

        tree.foreach(my_traverse, ptr::null_mut());
        assert_eq!(tree.nnodes() as usize, CHARS2.len());
        assert_eq!(tree.height(), 6);

        let mut checker = OrderChecker {
            expected: CHARS2,
            pos: 0,
        };
        tree.foreach(
            check_order,
            (&mut checker as *mut OrderChecker<'_>).cast::<c_void>(),
        );

        for i in (0..26).rev() {
            let key = &CHARS[i + 10] as *const u8 as *mut c_void;
            tree.insert(key, key);
        }

        let mut checker = OrderChecker {
            expected: CHARS,
            pos: 0,
        };
        tree.foreach(
            check_order,
            (&mut checker as *mut OrderChecker<'_>).cast::<c_void>(),
        );

        for c in [b'0', b'A', b'a', b'z'] {
            let key = &c as *const u8;
            let p = tree.lookup(key.cast());
            assert!(!p.is_null());
            assert_eq!(unsafe { *(p as *const u8) }, c);

            let mut d = ptr::null_mut();
            let mut p2 = ptr::null_mut();
            assert!(tree.lookup_extended(key.cast(), Some(&mut d), Some(&mut p2)));
            assert_eq!(unsafe { *(d as *const u8) }, c);
            assert_eq!(unsafe { *(p2 as *const u8) }, c);
        }

        for c in [b'!', b'=', b'|'] {
            assert!(tree.lookup((&c as *const u8).cast()).is_null());
        }

        for c in [b'0', b'A', b'a', b'z', b'!', b'=', b'|'] {
            let key = &c as *const u8;
            let p = tree.search(my_search, key.cast());
            if c == b'!' || c == b'=' || c == b'|' {
                assert!(p.is_null());
            } else {
                assert!(!p.is_null());
                assert_eq!(unsafe { *(p as *const u8) }, c);
            }
        }

        tree.destroy();
    }

    #[test]
    fn tree_remove() {
        reset_destroy_tracking();

        let tree = Tree::new_full(
            my_compare_no_data,
            ptr::null_mut(),
            Some(my_key_destroy),
            Some(my_value_destroy),
        );

        for i in 0..CHARS.len() {
            let key = char_key(i);
            tree.insert(key, key);
        }

        let mut c = b'0';
        tree.insert(
            (&mut c as *mut u8).cast::<c_void>(),
            (&mut c as *mut u8).cast::<c_void>(),
        );
        assert_eq!(
            DESTROYED_KEY.with(|slot| slot.get()),
            (&mut c as *mut u8).cast::<c_void>()
        );
        assert_eq!(
            DESTROYED_VALUE.with(|slot| slot.get()),
            (&CHARS[0] as *const u8).cast::<c_void>().cast_mut()
        );
        DESTROYED_KEY.with(|slot| slot.set(ptr::null_mut()));
        DESTROYED_VALUE.with(|slot| slot.set(ptr::null_mut()));

        let mut d = b'1';
        tree.replace(
            (&mut d as *mut u8).cast::<c_void>(),
            (&mut d as *mut u8).cast::<c_void>(),
        );
        assert_eq!(
            DESTROYED_KEY.with(|slot| slot.get()),
            (&CHARS[1] as *const u8).cast::<c_void>().cast_mut()
        );
        assert_eq!(
            DESTROYED_VALUE.with(|slot| slot.get()),
            (&CHARS[1] as *const u8).cast::<c_void>().cast_mut()
        );
        DESTROYED_KEY.with(|slot| slot.set(ptr::null_mut()));
        DESTROYED_VALUE.with(|slot| slot.set(ptr::null_mut()));

        let mut e = 0xff_u8;
        let node = tree
            .insert_node(
                (&mut e as *mut u8).cast::<c_void>(),
                (&mut e as *mut u8).cast::<c_void>(),
            )
            .expect("insert_node");
        assert!(!node.is_null());
        assert!(DESTROYED_KEY.with(|slot| slot.get()).is_null());
        assert!(DESTROYED_VALUE.with(|slot| slot.get()).is_null());

        let c2 = b'2';
        assert!(tree.remove((&c2 as *const u8).cast()));
        assert_eq!(
            DESTROYED_KEY.with(|slot| slot.get()),
            (&CHARS[2] as *const u8).cast::<c_void>().cast_mut()
        );
        assert_eq!(
            DESTROYED_VALUE.with(|slot| slot.get()),
            (&CHARS[2] as *const u8).cast::<c_void>().cast_mut()
        );
        DESTROYED_KEY.with(|slot| slot.set(ptr::null_mut()));
        DESTROYED_VALUE.with(|slot| slot.set(ptr::null_mut()));

        let c3 = b'3';
        assert!(tree.steal((&c3 as *const u8).cast()));
        assert!(DESTROYED_KEY.with(|slot| slot.get()).is_null());
        assert!(DESTROYED_VALUE.with(|slot| slot.get()).is_null());

        let mut f = b'4';
        let node = tree
            .replace_node(
                (&mut f as *mut u8).cast::<c_void>(),
                (&mut f as *mut u8).cast::<c_void>(),
            )
            .expect("replace_node");
        assert!(!node.is_null());
        assert_eq!(
            DESTROYED_KEY.with(|slot| slot.get()),
            (&CHARS[4] as *const u8).cast::<c_void>().cast_mut()
        );
        assert_eq!(
            DESTROYED_VALUE.with(|slot| slot.get()),
            (&CHARS[4] as *const u8).cast::<c_void>().cast_mut()
        );

        static REMOVE: &[u8] = b"omkjigfedba";
        for i in 0..REMOVE.len() {
            assert!(tree.remove((&REMOVE[i] as *const u8).cast()));
        }

        tree.destroy();
    }

    #[test]
    fn tree_remove_all() {
        reset_destroy_tracking();

        let tree = Tree::new_full(
            my_compare_no_data,
            ptr::null_mut(),
            Some(my_key_destroy),
            Some(my_value_destroy),
        );

        for i in 0..CHARS.len() {
            let key = char_key(i);
            tree.insert(key, key);
        }

        let destroyed_key_count = DESTROYED_KEY_COUNT.with(|count| count.get());
        let destroyed_value_count = DESTROYED_VALUE_COUNT.with(|count| count.get());

        tree.remove_all();
        assert_eq!(
            DESTROYED_KEY_COUNT.with(|count| count.get()) - destroyed_key_count,
            CHARS.len() as u32
        );
        assert_eq!(
            DESTROYED_VALUE_COUNT.with(|count| count.get()) - destroyed_value_count,
            CHARS.len() as u32
        );
        assert_eq!(tree.height(), 0);
        assert_eq!(tree.nnodes(), 0);

        tree.unref();
    }

    #[test]
    fn tree_destroy_ref() {
        let tree = Tree::new(my_compare);
        for i in 0..CHARS.len() {
            let key = char_key(i);
            tree.insert(key, key);
        }
        assert_eq!(tree.nnodes() as usize, CHARS.len());

        let tree_ref = tree.ref_();
        tree.destroy();
        assert_eq!(tree_ref.nnodes(), 0);
        tree_ref.unref();
    }

    struct TraverseCallbackData {
        buf: String,
        count: i32,
    }

    fn traverse_func(key: *mut c_void, _value: *mut c_void, data: *mut c_void) -> bool {
        let d = unsafe { &mut *(data as *mut TraverseCallbackData) };
        let c = unsafe { *(key as *const u8) } as char;
        d.buf.push(c);
        if d.count >= 0 {
            d.count -= 1;
            d.count == 0
        } else {
            false
        }
    }

    #[test]
    fn tree_traverse_orders() {
        let orders: [(TraverseType, i32, &str); 3] = [
            (
                TraverseType::InOrder,
                -1,
                "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz",
            ),
            (
                TraverseType::PreOrder,
                -1,
                "VF73102546B98ADCENJHGILKMRPOQTSUldZXWYbachfegjiktpnmorqsxvuwyz",
            ),
            (
                TraverseType::PostOrder,
                -1,
                "02146538A9CEDB7GIHKMLJOQPSUTRNFWYXacbZegfikjhdmonqsrpuwvzyxtlV",
            ),
        ];

        let tree = Tree::new(my_compare);
        for i in 0..CHARS.len() {
            let key = char_key(i);
            tree.insert(key, key);
        }

        for (traverse, limit, expected) in orders {
            let mut data = TraverseCallbackData {
                buf: String::new(),
                count: limit,
            };
            tree.traverse(
                traverse_func,
                traverse,
                (&mut data as *mut TraverseCallbackData).cast::<c_void>(),
            );
            assert_eq!(data.buf, expected, "traverse type {:?}", traverse);
        }

        tree.unref();
    }

    #[test]
    fn tree_insert_sorted_order() {
        let tree = Tree::new(my_compare);
        for i in 0..CHARS.len() {
            let key = char_key(i);
            tree.insert(key, key);
        }
        let mut checker = OrderChecker {
            expected: CHARS,
            pos: 0,
        };
        tree.foreach(
            check_order,
            (&mut checker as *mut OrderChecker<'_>).cast::<c_void>(),
        );
        tree.unref();

        let tree = Tree::new(my_compare);
        for i in (0..CHARS.len()).rev() {
            let key = char_key(i);
            tree.insert(key, key);
        }
        let mut checker = OrderChecker {
            expected: CHARS,
            pos: 0,
        };
        tree.foreach(
            check_order,
            (&mut checker as *mut OrderChecker<'_>).cast::<c_void>(),
        );
        tree.unref();
    }

    #[test]
    fn tree_bounds() {
        let mut chars_buf = [0u8; 62];
        let tree = Tree::new(my_compare);

        let mut i = 0usize;
        for j in 0..10 {
            chars_buf[i] = b'0' + j;
            let key = &mut chars_buf[i] as *mut u8 as *mut c_void;
            let node = tree.insert_node(key, key).expect("insert");
            assert_eq!(unsafe { Tree::node_key(node) }, key);
            assert_eq!(unsafe { Tree::node_value(node) }, key);
            i += 1;
        }
        for j in 0..26 {
            chars_buf[i] = b'A' + j;
            let key = &mut chars_buf[i] as *mut u8 as *mut c_void;
            tree.insert_node(key, key);
            i += 1;
        }
        for j in 0..26 {
            chars_buf[i] = b'a' + j;
            let key = &mut chars_buf[i] as *mut u8 as *mut c_void;
            tree.insert_node(key, key);
            i += 1;
        }

        assert_eq!(tree.nnodes(), 62);
        assert!(tree.height() >= 6);
        assert!(tree.height() <= 8);

        assert_bound(&tree, b'a', b'a', true);
        assert_bound(&tree, b'z', b'z', true);
        assert_bound(&tree, b'0' - 1, b'0', true);
        assert_bound(&tree, b'z' + 1, 0, true);

        for idx in 0..10 {
            tree.remove((&chars_buf[idx] as *const u8).cast());
        }
        assert_eq!(tree.nnodes(), 52);
        assert_bound(&tree, b'A', b'A', true);
        assert_bound(&tree, b'9', b'A', false);

        tree.unref();
    }

    fn assert_bound(tree: &Tree, query: u8, expected: u8, lower: bool) {
        let node = if lower {
            tree.lower_bound((&query as *const u8).cast())
        } else {
            tree.upper_bound((&query as *const u8).cast())
        };
        if expected == 0 {
            assert!(node.is_none());
        } else {
            let node = node.expect("bound node");
            assert_eq!(unsafe { *(Tree::node_key(node) as *const u8) }, expected);
        }
    }

    #[test]
    fn tree_new_and_lookup_miss() {
        let tree = Tree::new(my_compare);
        assert_eq!(tree.height(), 0);
        assert_eq!(tree.nnodes(), 0);
        assert!(tree.lookup(b"x".as_ptr().cast()).is_null());
        tree.unref();
    }

    #[test]
    fn tree_node_navigation() {
        let tree = Tree::new(my_compare);
        let mut keys = *b"ace";
        for key in &mut keys {
            let ptr = key as *mut u8 as *mut c_void;
            tree.insert(ptr, ptr);
        }
        let first = tree.node_first().expect("first");
        let last = tree.node_last().expect("last");
        assert_eq!(unsafe { *(Tree::node_key(first) as *const u8) }, b'a');
        assert_eq!(unsafe { *(Tree::node_key(last) as *const u8) }, b'e');
        let mid = tree.node_next(first).expect("next");
        assert_eq!(unsafe { *(Tree::node_key(mid) as *const u8) }, b'c');
        assert_eq!(tree.node_previous(mid), Some(first));
        assert_eq!(tree.node_next(mid), Some(last));
        assert!(tree.node_previous(first).is_none());
        assert!(tree.node_next(last).is_none());
        tree.unref();
    }

    #[test]
    fn tree_foreach_early_stop() {
        let tree = Tree::new(my_compare);
        let mut keys = *b"abcd";
        for key in &mut keys {
            let ptr = key as *mut u8 as *mut c_void;
            tree.insert(ptr, ptr);
        }
        let mut seen = String::new();
        fn collect(key: *mut c_void, _value: *mut c_void, data: *mut c_void) -> bool {
            let s = unsafe { &mut *(data as *mut String) };
            let c = unsafe { *(key as *const u8) } as char;
            s.push(c);
            c == 'b'
        }
        tree.foreach(collect, (&mut seen as *mut String).cast::<c_void>());
        assert_eq!(seen, "ab");
        tree.unref();
    }

    #[test]
    fn tree_replace_vs_insert_key_destroy() {
        reset_destroy_tracking();
        let tree = Tree::new_full(
            my_compare_no_data,
            ptr::null_mut(),
            Some(my_key_destroy),
            None,
        );
        let mut a = b'x';
        let mut b = b'x';
        tree.insert((&mut a as *mut u8).cast::<c_void>(), ptr::null_mut());
        DESTROYED_KEY.with(|slot| slot.set(ptr::null_mut()));
        tree.insert((&mut a as *mut u8).cast::<c_void>(), ptr::null_mut());
        assert_eq!(
            DESTROYED_KEY.with(|slot| slot.get()),
            (&mut a as *mut u8).cast::<c_void>()
        );
        DESTROYED_KEY.with(|slot| slot.set(ptr::null_mut()));
        tree.replace((&mut b as *mut u8).cast::<c_void>(), ptr::null_mut());
        assert_eq!(
            DESTROYED_KEY.with(|slot| slot.get()),
            (&mut a as *mut u8).cast::<c_void>()
        );
        tree.unref();
    }
}
