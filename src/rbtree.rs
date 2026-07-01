// SPDX-License-Identifier: GPL-2.0
//! Red-black tree — pure Rust port of Linux kernel rbtree.rs
//!
//! Provides an owned, ordered map from `K: Ord` to `V` backed by a
//! balanced binary search tree with O(log n) insert / remove / lookup.
//!
//! # Key differences from Linux's version
//! - No C bindings; no `bindings::rb_node`
//! - Nodes are heap-allocated via `Box`
//! - `RBTreeNodeReservation` is a pre-allocated node used for lock-safe insertion

#![allow(dead_code, unused_variables, unused_imports)]

extern crate alloc;
use alloc::boxed::Box;
use core::{marker::PhantomData, ptr::NonNull};

// ---------------------------------------------------------------------------
// Color
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Color {
    Red,
    Black,
}

// ---------------------------------------------------------------------------
// RBNode (internal)
// ---------------------------------------------------------------------------

struct RBNode<K, V> {
    color:  Color,
    parent: Option<NonNull<RBNode<K, V>>>,
    left:   Option<NonNull<RBNode<K, V>>>,
    right:  Option<NonNull<RBNode<K, V>>>,
    key:    K,
    value:  V,
}

impl<K, V> RBNode<K, V> {
    fn new(key: K, value: V) -> NonNull<Self> {
        let b = Box::new(RBNode {
            color:  Color::Red,
            parent: None,
            left:   None,
            right:  None,
            key,
            value,
        });
        unsafe { NonNull::new_unchecked(Box::into_raw(b)) }
    }

    /// Is this node a left child of its parent?
    unsafe fn is_left_child(node: NonNull<Self>) -> bool {
        let n = unsafe { node.as_ref() };
        match n.parent {
            None => false,
            Some(p) => unsafe { p.as_ref() }.left == Some(node),
        }
    }

    /// Sibling of `node`.
    unsafe fn sibling(node: NonNull<Self>) -> Option<NonNull<Self>> {
        let n = unsafe { node.as_ref() };
        let parent = n.parent?;
        let p = unsafe { parent.as_ref() };
        if p.left == Some(node) { p.right } else { p.left }
    }

    fn color_of(node: Option<NonNull<Self>>) -> Color {
        match node {
            None => Color::Black,
            Some(n) => unsafe { n.as_ref() }.color,
        }
    }
}

// ---------------------------------------------------------------------------
// RBTree<K, V>
// ---------------------------------------------------------------------------

/// A red-black tree providing an ordered map from `K` to `V`.
///
/// # Examples
///
/// ```
/// let mut tree: RBTree<i32, &str> = RBTree::new();
/// tree.try_insert(10, "ten").unwrap();
/// tree.try_insert(20, "twenty").unwrap();
/// assert_eq!(tree.get(&10), Some(&"ten"));
/// ```
pub struct RBTree<K: Ord, V> {
    root: Option<NonNull<RBNode<K, V>>>,
    len:  usize,
}

unsafe impl<K: Ord + Send, V: Send> Send for RBTree<K, V> {}
unsafe impl<K: Ord + Sync, V: Sync> Sync for RBTree<K, V> {}

impl<K: Ord, V> Default for RBTree<K, V> {
    fn default() -> Self { Self::new() }
}

impl<K: Ord, V> RBTree<K, V> {
    /// Creates an empty `RBTree`.
    pub fn new() -> Self {
        Self { root: None, len: 0 }
    }

    /// Returns the number of entries.
    pub fn len(&self) -> usize { self.len }

    /// Returns `true` if the tree is empty.
    pub fn is_empty(&self) -> bool { self.len == 0 }

    // ------------------------------------------------------------------
    // Lookups
    // ------------------------------------------------------------------

    /// Returns a shared reference to the value associated with `key`,
    /// or `None` if `key` is absent.
    pub fn get(&self, key: &K) -> Option<&V> {
        let node = self.find_node(key)?;
        Some(unsafe { &node.as_ref().value })
    }

    /// Returns a mutable reference to the value associated with `key`,
    /// or `None` if `key` is absent.
    pub fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let mut node = self.find_node(key)?;
        Some(unsafe { &mut node.as_mut().value })
    }

    /// Returns `true` if the tree contains an entry for `key`.
    pub fn contains_key(&self, key: &K) -> bool {
        self.find_node(key).is_some()
    }

    fn find_node(&self, key: &K) -> Option<NonNull<RBNode<K, V>>> {
        let mut cur = self.root;
        while let Some(n) = cur {
            let node = unsafe { n.as_ref() };
            use core::cmp::Ordering::*;
            cur = match key.cmp(&node.key) {
                Less    => node.left,
                Greater => node.right,
                Equal   => return Some(n),
            };
        }
        None
    }

    // ------------------------------------------------------------------
    // Insertion
    // ------------------------------------------------------------------

    /// Inserts `(key, value)` into the tree, replacing any existing value.
    /// Returns the old value if one was replaced.
    ///
    /// Allocates a heap node.
    pub fn try_insert(&mut self, key: K, value: V) -> Result<Option<V>, ()> {
        let node = RBNode::new(key, value);
        Ok(self.insert_node(node))
    }

    /// Inserts `(key, value)`, returning the old value if replaced.
    pub fn insert(&mut self, key: K, value: V) -> Option<V> {
        let node = RBNode::new(key, value);
        self.insert_node(node)
    }

    fn insert_node(&mut self, mut new_node: NonNull<RBNode<K, V>>) -> Option<V> {
        // Standard BST insert
        let mut parent_slot: Option<(NonNull<RBNode<K, V>>, bool)> = None; // (parent, is_left)
        let mut cur = self.root;

        while let Some(mut n) = cur {
            let node = unsafe { n.as_ref() };
            use core::cmp::Ordering::*;
            match unsafe { new_node.as_ref() }.key.cmp(&node.key) {
                Equal => {
                    // Replace: swap values in-place and discard new_node
                    let new_n = unsafe { new_node.as_mut() };
                    let old_n = unsafe { n.as_mut() };
                    // We want to free new_node and return old value
                    core::mem::swap(&mut old_n.value, &mut new_n.value);
                    // new_node now holds the OLD value; drop it
                    let old_val = unsafe { Box::from_raw(new_node.as_ptr()) }.value;
                    return Some(old_val);
                }
                Less => {
                    parent_slot = Some((n, true));
                    cur = node.left;
                }
                Greater => {
                    parent_slot = Some((n, false));
                    cur = node.right;
                }
            }
        }

        // Link new_node into the tree
        {
            let nn = unsafe { new_node.as_mut() };
            nn.color = Color::Red;
            nn.left = None;
            nn.right = None;
            nn.parent = parent_slot.map(|(p, _)| p);
        }

        match parent_slot {
            None => {
                self.root = Some(new_node);
            }
            Some((mut p, is_left)) => {
                let pn = unsafe { p.as_mut() };
                if is_left { pn.left = Some(new_node); } else { pn.right = Some(new_node); }
            }
        }

        self.insert_fixup(new_node);
        self.len += 1;
        None
    }

    // ------------------------------------------------------------------
    // Rotations
    // ------------------------------------------------------------------

    /// Left-rotate around `x`:
    ///
    /// ```text
    ///   x               y
    ///  / \             / \
    /// A   y    =>    x   C
    ///    / \        / \
    ///   B   C      A   B
    /// ```
    unsafe fn rotate_left(&mut self, mut x: NonNull<RBNode<K, V>>) {
        let mut y = unsafe { x.as_ref() }.right.expect("rotate_left: no right child");
        let b = unsafe { y.as_ref() }.left;

        // y's left becomes x's right
        unsafe { x.as_mut() }.right = b;
        if let Some(mut bn) = b {
            unsafe { bn.as_mut() }.parent = Some(x);
        }

        // y replaces x as child of x's parent
        unsafe { y.as_mut() }.parent = unsafe { x.as_ref() }.parent;
        self.replace_child_ptr(x, y);

        // x becomes y's left
        unsafe { y.as_mut() }.left = Some(x);
        unsafe { x.as_mut() }.parent = Some(y);
    }

    /// Right-rotate around `y`.
    unsafe fn rotate_right(&mut self, mut y: NonNull<RBNode<K, V>>) {
        let mut x = unsafe { y.as_ref() }.left.expect("rotate_right: no left child");
        let b = unsafe { x.as_ref() }.right;

        unsafe { y.as_mut() }.left = b;
        if let Some(mut bn) = b {
            unsafe { bn.as_mut() }.parent = Some(y);
        }

        unsafe { x.as_mut() }.parent = unsafe { y.as_ref() }.parent;
        self.replace_child_ptr(y, x);

        unsafe { x.as_mut() }.right = Some(y);
        unsafe { y.as_mut() }.parent = Some(x);
    }

    /// Replaces `old` with `new_child` in old's parent (or as root).
    fn replace_child_ptr(&mut self, old: NonNull<RBNode<K, V>>, new_child: NonNull<RBNode<K, V>>) {
        match unsafe { old.as_ref() }.parent {
            None => { self.root = Some(new_child); }
            Some(mut p) => {
                let pn = unsafe { p.as_mut() };
                if pn.left == Some(old) { pn.left = Some(new_child); }
                else { pn.right = Some(new_child); }
            }
        }
    }

    // ------------------------------------------------------------------
    // Insert fixup
    // ------------------------------------------------------------------

    fn insert_fixup(&mut self, mut z: NonNull<RBNode<K, V>>) {
        loop {
            let parent = match unsafe { z.as_ref() }.parent {
                None => {
                    // z is root; color it black and exit
                    unsafe { z.as_mut() }.color = Color::Black;
                    return;
                }
                Some(p) => p,
            };

            if unsafe { parent.as_ref() }.color == Color::Black {
                return; // tree is valid
            }

            // Parent is red; grandparent must exist (parent can't be root and red)
            let grandparent = match unsafe { parent.as_ref() }.parent {
                None => return,
                Some(g) => g,
            };

            let uncle = if unsafe { grandparent.as_ref() }.left == Some(parent) {
                unsafe { grandparent.as_ref() }.right
            } else {
                unsafe { grandparent.as_ref() }.left
            };

            if RBNode::color_of(uncle) == Color::Red {
                // Case 1: uncle is red — recolor and move up
                unsafe { parent.as_ref() };
                unsafe { (parent.as_ptr() as *mut RBNode<K, V>).as_mut().unwrap() }.color = Color::Black;
                if let Some(mut u) = uncle {
                    unsafe { u.as_mut() }.color = Color::Black;
                }
                unsafe { (grandparent.as_ptr() as *mut RBNode<K, V>).as_mut().unwrap() }.color = Color::Red;
                z = grandparent;
                continue;
            }

            // Uncle is black: cases 2 & 3
            let parent_is_left = unsafe { grandparent.as_ref() }.left == Some(parent);

            if parent_is_left {
                if unsafe { parent.as_ref() }.right == Some(z) {
                    // Case 2: z is right child of left parent — rotate left around parent
                    unsafe { self.rotate_left(parent) };
                    z = parent;
                }
                // Case 3
                let mut p = unsafe { z.as_ref() }.parent.unwrap();
                unsafe { p.as_mut() }.color = Color::Black;
                unsafe { (grandparent.as_ptr() as *mut RBNode<K, V>).as_mut().unwrap() }.color = Color::Red;
                unsafe { self.rotate_right(grandparent) };
            } else {
                if unsafe { parent.as_ref() }.left == Some(z) {
                    unsafe { self.rotate_right(parent) };
                    z = parent;
                }
                let mut p = unsafe { z.as_ref() }.parent.unwrap();
                unsafe { p.as_mut() }.color = Color::Black;
                unsafe { (grandparent.as_ptr() as *mut RBNode<K, V>).as_mut().unwrap() }.color = Color::Red;
                unsafe { self.rotate_left(grandparent) };
            }
            return;
        }
    }

    // ------------------------------------------------------------------
    // Removal
    // ------------------------------------------------------------------

    /// Removes `key` and returns its value, or `None` if absent.
    pub fn remove(&mut self, key: &K) -> Option<V> {
        let node = self.find_node(key)?;
        Some(unsafe { self.remove_node(node) })
    }

    unsafe fn remove_node(&mut self, z: NonNull<RBNode<K, V>>) -> V {
        let zn = unsafe { z.as_ref() };
        let has_left = zn.left.is_some();
        let has_right = zn.right.is_some();

        if has_left && has_right {
            // Replace z with in-order successor (min of right subtree).
            // Swap z's key/value with successor's key/value in-place, then
            // remove the successor node (which now holds z's original data).
            let succ = unsafe { Self::minimum(zn.right.unwrap()) };
            // Swap key and value between z and succ in-place.
            unsafe {
                let z_ptr = z.as_ptr();
                let s_ptr = succ.as_ptr();
                core::mem::swap(&mut z_ptr.key, &mut s_ptr.key);
                core::mem::swap(&mut z_ptr.value, &mut s_ptr.value);
            }
            // Now succ holds z's original key/value; remove it.
            return unsafe { self.remove_node(succ) };
        }

        // z has at most one child
        let child = if has_left { zn.left } else { zn.right };
        let was_black = zn.color == Color::Black;
        let parent = zn.parent;

        // Unlink z
        match child {
            Some(mut c) => {
                unsafe { c.as_mut() }.parent = parent;
            }
            None => {}
        }
        match parent {
            None => { self.root = child; }
            Some(mut p) => {
                let pn = unsafe { p.as_mut() };
                if pn.left == Some(z) { pn.left = child; }
                else { pn.right = child; }
            }
        }

        if was_black {
            match child {
                Some(mut c) if unsafe { c.as_ref() }.color == Color::Red => {
                    unsafe { c.as_mut() }.color = Color::Black;
                }
                _ => {
                    // Double-black fix
                    unsafe { self.delete_fixup(child, parent) };
                }
            }
        }

        self.len -= 1;
        let z_box = unsafe { Box::from_raw(z.as_ptr()) };
        z_box.value
    }

    unsafe fn minimum(mut node: NonNull<RBNode<K, V>>) -> NonNull<RBNode<K, V>> {
        loop {
            match unsafe { node.as_ref() }.left {
                None => return node,
                Some(l) => node = l,
            }
        }
    }

    unsafe fn delete_fixup(
        &mut self,
        mut x: Option<NonNull<RBNode<K, V>>>,
        mut parent: Option<NonNull<RBNode<K, V>>>,
    ) {
        while x != self.root && RBNode::color_of(x) == Color::Black {
            let p = match parent {
                None => break,
                Some(p) => p,
            };
            let pn = unsafe { p.as_ref() };
            if pn.left == x {
                let mut w = pn.right; // sibling
                if RBNode::color_of(w) == Color::Red {
                    // Case 1
                    if let Some(mut wn) = w { unsafe { wn.as_mut() }.color = Color::Black; }
                    unsafe { (p.as_ptr() as *mut RBNode<K, V>).as_mut().unwrap() }.color = Color::Red;
                    unsafe { self.rotate_left(p) };
                    w = unsafe { p.as_ref() }.right;
                }
                let w_left  = w.and_then(|wn| unsafe { wn.as_ref() }.left);
                let w_right = w.and_then(|wn| unsafe { wn.as_ref() }.right);
                if RBNode::color_of(w_left) == Color::Black && RBNode::color_of(w_right) == Color::Black {
                    // Case 2
                    if let Some(mut wn) = w { unsafe { wn.as_mut() }.color = Color::Red; }
                    x = Some(p);
                    parent = unsafe { p.as_ref() }.parent;
                } else {
                    if RBNode::color_of(w_right) == Color::Black {
                        // Case 3
                        if let Some(mut wl) = w_left { unsafe { wl.as_mut() }.color = Color::Black; }
                        if let Some(mut wn) = w { unsafe { wn.as_mut() }.color = Color::Red; }
                        if let Some(wn) = w { unsafe { self.rotate_right(wn) }; }
                        w = unsafe { p.as_ref() }.right;
                    }
                    // Case 4
                    let pc = unsafe { p.as_ref() }.color;
                    if let Some(mut wn) = w { unsafe { wn.as_mut() }.color = pc; }
                    unsafe { (p.as_ptr() as *mut RBNode<K, V>).as_mut().unwrap() }.color = Color::Black;
                    let wr = w.and_then(|wn| unsafe { wn.as_ref() }.right);
                    if let Some(mut wr_n) = wr { unsafe { wr_n.as_mut() }.color = Color::Black; }
                    unsafe { self.rotate_left(p) };
                    x = self.root;
                    break;
                }
            } else {
                // Mirror of above
                let mut w = pn.left;
                if RBNode::color_of(w) == Color::Red {
                    if let Some(mut wn) = w { unsafe { wn.as_mut() }.color = Color::Black; }
                    unsafe { (p.as_ptr() as *mut RBNode<K, V>).as_mut().unwrap() }.color = Color::Red;
                    unsafe { self.rotate_right(p) };
                    w = unsafe { p.as_ref() }.left;
                }
                let w_left  = w.and_then(|wn| unsafe { wn.as_ref() }.left);
                let w_right = w.and_then(|wn| unsafe { wn.as_ref() }.right);
                if RBNode::color_of(w_left) == Color::Black && RBNode::color_of(w_right) == Color::Black {
                    if let Some(mut wn) = w { unsafe { wn.as_mut() }.color = Color::Red; }
                    x = Some(p);
                    parent = unsafe { p.as_ref() }.parent;
                } else {
                    if RBNode::color_of(w_left) == Color::Black {
                        if let Some(mut wr) = w_right { unsafe { wr.as_mut() }.color = Color::Black; }
                        if let Some(mut wn) = w { unsafe { wn.as_mut() }.color = Color::Red; }
                        if let Some(wn) = w { unsafe { self.rotate_left(wn) }; }
                        w = unsafe { p.as_ref() }.left;
                    }
                    let pc = unsafe { p.as_ref() }.color;
                    if let Some(mut wn) = w { unsafe { wn.as_mut() }.color = pc; }
                    unsafe { (p.as_ptr() as *mut RBNode<K, V>).as_mut().unwrap() }.color = Color::Black;
                    let wl = w.and_then(|wn| unsafe { wn.as_ref() }.left);
                    if let Some(mut wl_n) = wl { unsafe { wl_n.as_mut() }.color = Color::Black; }
                    unsafe { self.rotate_right(p) };
                    x = self.root;
                    break;
                }
            }
        }
        if let Some(mut xn) = x {
            unsafe { xn.as_mut() }.color = Color::Black;
        }
    }

    // ------------------------------------------------------------------
    // Iterators
    // ------------------------------------------------------------------

    /// Returns an in-order iterator over `(&K, &V)` pairs.
    pub fn iter(&self) -> RBTreeIter<'_, K, V> {
        RBTreeIter {
            // Start at the leftmost node
            cur: self.root.map(|r| unsafe { RBNode::leftmost(r) }),
            _phantom: PhantomData,
        }
    }

    /// Returns an in-order iterator over `(&K, &mut V)` pairs.
    pub fn iter_mut(&mut self) -> RBTreeIterMut<'_, K, V> {
        RBTreeIterMut {
            cur: self.root.map(|r| unsafe { RBNode::leftmost(r) }),
            _phantom: PhantomData,
        }
    }
}

impl<K: Ord, V> RBNode<K, V> {
    unsafe fn leftmost(mut n: NonNull<Self>) -> NonNull<Self> {
        loop {
            match unsafe { n.as_ref() }.left {
                None => return n,
                Some(l) => n = l,
            }
        }
    }

    unsafe fn in_order_successor(node: NonNull<Self>) -> Option<NonNull<Self>> {
        let n = unsafe { node.as_ref() };
        if let Some(r) = n.right {
            return Some(unsafe { Self::leftmost(r) });
        }
        // Walk up until we come from a left child
        let mut cur = node;
        loop {
            let parent = unsafe { cur.as_ref() }.parent?;
            if unsafe { parent.as_ref() }.left == Some(cur) {
                return Some(parent);
            }
            cur = parent;
        }
    }
}

/// In-order iterator over `(&K, &V)`.
pub struct RBTreeIter<'a, K: Ord, V> {
    cur: Option<NonNull<RBNode<K, V>>>,
    _phantom: PhantomData<&'a (K, V)>,
}

impl<'a, K: Ord, V> Iterator for RBTreeIter<'a, K, V> {
    type Item = (&'a K, &'a V);

    fn next(&mut self) -> Option<Self::Item> {
        let node = self.cur?;
        let n = unsafe { node.as_ref() };
        self.cur = unsafe { RBNode::in_order_successor(node) };
        Some((&n.key, &n.value))
    }
}

/// In-order iterator over `(&K, &mut V)`.
pub struct RBTreeIterMut<'a, K: Ord, V> {
    cur: Option<NonNull<RBNode<K, V>>>,
    _phantom: PhantomData<&'a mut (K, V)>,
}

impl<'a, K: Ord, V> Iterator for RBTreeIterMut<'a, K, V> {
    type Item = (&'a K, &'a mut V);

    fn next(&mut self) -> Option<Self::Item> {
        let mut node = self.cur?;
        let n = unsafe { node.as_mut() };
        self.cur = unsafe { RBNode::in_order_successor(node) };
        Some((&n.key, &mut n.value))
    }
}

impl<'a, K: Ord, V> IntoIterator for &'a RBTree<K, V> {
    type Item = (&'a K, &'a V);
    type IntoIter = RBTreeIter<'a, K, V>;
    fn into_iter(self) -> RBTreeIter<'a, K, V> { self.iter() }
}

// ---------------------------------------------------------------------------
// Drop
// ---------------------------------------------------------------------------

impl<K: Ord, V> Drop for RBTree<K, V> {
    fn drop(&mut self) {
        // Post-order traversal to free all nodes
        fn drop_subtree<K: Ord, V>(node: Option<NonNull<RBNode<K, V>>>) {
            if let Some(n) = node {
                let left  = unsafe { n.as_ref() }.left;
                let right = unsafe { n.as_ref() }.right;
                drop_subtree(left);
                drop_subtree(right);
                drop(unsafe { Box::from_raw(n.as_ptr()) });
            }
        }
        drop_subtree(self.root);
    }
}

// ---------------------------------------------------------------------------
// RBTreeNodeReservation — pre-allocated node for lock-safe insertion
// ---------------------------------------------------------------------------

/// A pre-allocated node that can be inserted into an [`RBTree`] without
/// further heap allocation (useful when holding a lock that forbids alloc).
pub struct RBTreeNodeReservation<K: Ord, V> {
    node: NonNull<RBNode<K, V>>,
}

unsafe impl<K: Ord + Send, V: Send> Send for RBTreeNodeReservation<K, V> {}

impl<K: Ord, V> RBTreeNodeReservation<K, V> {
    /// Pre-allocates a node.  Returns `Err(())` on OOM.
    pub fn new() -> Result<Self, ()> {
        // Allocate a Box with dummy values that we will overwrite later.
        // We use MaybeUninit to avoid constructing K and V.
        let raw = Box::into_raw(Box::new(core::mem::MaybeUninit::<RBNode<K, V>>::uninit()));
        Ok(Self {
            node: unsafe { NonNull::new_unchecked(raw as *mut RBNode<K, V>) },
        })
    }

    /// Consumes the reservation and inserts it into `tree` with the given key and value.
    /// Always succeeds (no allocation).
    pub fn insert(self, tree: &mut RBTree<K, V>, key: K, value: V) -> Option<V> {
        let node = self.node;
        core::mem::forget(self); // don't drop the pre-allocated memory
        // Initialize the node in-place
        unsafe {
            core::ptr::write(node.as_ptr(), RBNode {
                color:  Color::Red,
                parent: None,
                left:   None,
                right:  None,
                key,
                value,
            });
        }
        tree.insert_node(node)
    }
}

impl<K: Ord, V> Drop for RBTreeNodeReservation<K, V> {
    fn drop(&mut self) {
        // Free as MaybeUninit to avoid running destructors on uninit data
        drop(unsafe { Box::from_raw(self.node.as_ptr() as *mut core::mem::MaybeUninit<RBNode<K, V>>) });
    }
}
