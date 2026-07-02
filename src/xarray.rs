// SPDX-License-Identifier: GPL-2.0
//! XArray — extensible sparse array — pure Rust port of Linux kernel xarray.
//!
//! Maps `u64` indices to values `T` using a multi-level 64-ary radix tree.

#![allow(dead_code, unused_variables, unused_imports)]

extern crate alloc;
use alloc::boxed::Box;

const XA_CHUNK_SHIFT: u32 = 6;
const XA_CHUNK_SIZE: usize = 1 << XA_CHUNK_SHIFT;
const XA_CHUNK_MASK: u64 = (XA_CHUNK_SIZE as u64) - 1;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum XMark { Mark0 = 0, Mark1 = 1, Mark2 = 2 }

enum SlotKind<T> {
    Node(Box<XaNode<T>>),
    Leaf(*mut T),
}

struct XaNode<T> {
    slots: [Option<SlotKind<T>>; XA_CHUNK_SIZE],
    marks: [[u64; 1]; 3],
    count: usize,
}

impl<T> XaNode<T> {
    fn new() -> Box<Self> {
        Box::new(XaNode {
            slots: [const { None }; XA_CHUNK_SIZE],
            marks: [[0u64; 1]; 3],
            count: 0,
        })
    }

    fn slot_index(index: u64, level: u32) -> usize {
        ((index >> (level * XA_CHUNK_SHIFT)) & XA_CHUNK_MASK) as usize
    }

    fn mark_get(&self, mark: XMark, slot: usize) -> bool {
        (self.marks[mark as usize][0] >> slot) & 1 == 1
    }

    fn mark_set(&mut self, mark: XMark, slot: usize) {
        self.marks[mark as usize][0] |= 1u64 << slot;
    }

    fn mark_clear(&mut self, mark: XMark, slot: usize) {
        self.marks[mark as usize][0] &= !(1u64 << slot);
    }
}

/// Sparse array mapping `u64` indices to values of type `T`.
pub struct XArray<T> {
    root: Option<Box<XaNode<T>>>,
    height: u32,
}

unsafe impl<T: Send> Send for XArray<T> {}
unsafe impl<T: Sync> Sync for XArray<T> {}

impl<T> Default for XArray<T> {
    fn default() -> Self { Self::new() }
}

impl<T> XArray<T> {
    pub fn new() -> Self {
        Self { root: None, height: 0 }
    }

    pub fn store(&mut self, index: u64, value: Box<T>) -> Option<Box<T>> {
        let ptr = Box::into_raw(value);
        self.ensure_height(index);
        let height = self.height;
        let root = self.root.as_mut().expect("root after ensure_height");
        Self::store_recursive(root, index, height - 1, ptr)
            .map(|old| unsafe { Box::from_raw(old) })
    }

    fn store_recursive(node: &mut XaNode<T>, index: u64, level: u32, ptr: *mut T) -> Option<*mut T> {
        let slot = XaNode::<T>::slot_index(index, level);
        if level == 0 {
            let old = match node.slots[slot].take() {
                Some(SlotKind::Leaf(p)) => { node.count -= 1; Some(p) }
                _ => None,
            };
            node.slots[slot] = Some(SlotKind::Leaf(ptr));
            node.count += 1;
            old
        } else {
            if node.slots[slot].is_none() {
                node.slots[slot] = Some(SlotKind::Node(XaNode::new()));
                node.count += 1;
            }
            match node.slots[slot].as_mut() {
                Some(SlotKind::Node(child)) => Self::store_recursive(child, index, level - 1, ptr),
                _ => unreachable!(),
            }
        }
    }

    pub fn load(&self, index: u64) -> Option<&T> {
        let root = self.root.as_ref()?;
        if self.height == 0 { return None; }
        Self::load_recursive(root, index, self.height - 1)
    }

    fn load_recursive(node: &XaNode<T>, index: u64, level: u32) -> Option<&T> {
        let slot = XaNode::<T>::slot_index(index, level);
        match node.slots[slot].as_ref()? {
            SlotKind::Leaf(p) => Some(unsafe { &**p }),
            SlotKind::Node(child) => {
                if level == 0 { return None; }
                Self::load_recursive(child, index, level - 1)
            }
        }
    }

    pub fn load_mut(&mut self, index: u64) -> Option<&mut T> {
        let height = self.height;
        let root = self.root.as_mut()?;
        if height == 0 { return None; }
        Self::load_mut_recursive(root, index, height - 1)
    }

    fn load_mut_recursive(node: &mut XaNode<T>, index: u64, level: u32) -> Option<&mut T> {
        let slot = XaNode::<T>::slot_index(index, level);
        match node.slots[slot].as_mut()? {
            SlotKind::Leaf(p) => Some(unsafe { &mut **p }),
            SlotKind::Node(child) => {
                if level == 0 { return None; }
                Self::load_mut_recursive(child, index, level - 1)
            }
        }
    }

    pub fn erase(&mut self, index: u64) -> Option<Box<T>> {
        let height = self.height;
        if height == 0 { return None; }
        let root = self.root.as_mut()?;
        Self::erase_recursive(root, index, height - 1)
            .map(|p| unsafe { Box::from_raw(p) })
    }

    fn erase_recursive(node: &mut XaNode<T>, index: u64, level: u32) -> Option<*mut T> {
        let slot = XaNode::<T>::slot_index(index, level);
        if level == 0 {
            match node.slots[slot].take() {
                Some(SlotKind::Leaf(p)) => { node.count -= 1; Some(p) }
                other => { node.slots[slot] = other; None }
            }
        } else {
            match node.slots[slot].as_mut() {
                Some(SlotKind::Node(child)) => {
                    let result = Self::erase_recursive(child, index, level - 1);
                    if child.count == 0 {
                        node.slots[slot] = None;
                        node.count -= 1;
                    }
                    result
                }
                _ => None,
            }
        }
    }

    pub fn mark_set(&mut self, index: u64, mark: XMark) { self.mark_op(index, mark, true); }
    pub fn mark_clear(&mut self, index: u64, mark: XMark) { self.mark_op(index, mark, false); }

    pub fn mark_get(&self, index: u64, mark: XMark) -> bool {
        let height = self.height;
        if height == 0 { return false; }
        match self.root.as_ref() {
            None => false,
            Some(r) => Self::mark_get_recursive(r, index, height - 1, mark),
        }
    }

    fn mark_get_recursive(node: &XaNode<T>, index: u64, level: u32, mark: XMark) -> bool {
        let slot = XaNode::<T>::slot_index(index, level);
        if level == 0 {
            node.mark_get(mark, slot)
        } else {
            match node.slots[slot].as_ref() {
                Some(SlotKind::Node(child)) => Self::mark_get_recursive(child, index, level - 1, mark),
                _ => false,
            }
        }
    }

    fn mark_op(&mut self, index: u64, mark: XMark, set: bool) {
        let height = self.height;
        if height == 0 { return; }
        match self.root.as_mut() {
            None => {}
            Some(r) => Self::mark_op_recursive(r, index, height - 1, mark, set),
        }
    }

    fn mark_op_recursive(node: &mut XaNode<T>, index: u64, level: u32, mark: XMark, set: bool) {
        let slot = XaNode::<T>::slot_index(index, level);
        if level == 0 {
            if set { node.mark_set(mark, slot); } else { node.mark_clear(mark, slot); }
        } else {
            match node.slots[slot].as_mut() {
                Some(SlotKind::Node(child)) => Self::mark_op_recursive(child, index, level - 1, mark, set),
                _ => {}
            }
        }
    }

    pub fn find_after(&self, start: u64) -> Option<u64> {
        let height = self.height;
        if height == 0 { return None; }
        let root = self.root.as_ref()?;
        Self::find_recursive(root, start, height - 1, 0)
    }

    fn find_recursive(node: &XaNode<T>, start: u64, level: u32, base: u64) -> Option<u64> {
        let chunk_span = 1u64 << ((level + 1) * XA_CHUNK_SHIFT);
        let slot_span = chunk_span / XA_CHUNK_SIZE as u64;
        let start_slot = XaNode::<T>::slot_index(start, level);
        for s in start_slot..XA_CHUNK_SIZE {
            let slot_base = base + (s as u64) * slot_span;
            let effective_start = if s == start_slot { start } else { slot_base };
            match node.slots[s].as_ref() {
                None => continue,
                Some(SlotKind::Leaf(_)) => {
                    if level == 0 { return Some(slot_base); }
                }
                Some(SlotKind::Node(child)) => {
                    if level > 0 {
                        if let Some(idx) = Self::find_recursive(child, effective_start, level - 1, slot_base) {
                            return Some(idx);
                        }
                    }
                }
            }
        }
        None
    }

    pub fn iter(&self) -> XArrayIter<'_, T> {
        XArrayIter { xa: self, next: 0 }
    }

    fn max_index_for_height(height: u32) -> u64 {
        if height == 0 { return 0; }
        let bits = height * XA_CHUNK_SHIFT;
        if bits >= 64 { u64::MAX } else { (1u64 << bits) - 1 }
    }

    fn ensure_height(&mut self, index: u64) {
        if self.root.is_none() {
            self.root = Some(XaNode::new());
            self.height = 1;
        }
        while Self::max_index_for_height(self.height) < index {
            let old_root = self.root.take().unwrap();
            let mut new_root = XaNode::new();
            let had = old_root.count > 0;
            new_root.slots[0] = Some(SlotKind::Node(old_root));
            if had { new_root.count = 1; }
            self.root = Some(new_root);
            self.height += 1;
        }
    }
}

impl<T> Drop for XArray<T> {
    fn drop(&mut self) {
        fn drop_node<T>(node: Box<XaNode<T>>) {
            for slot in node.slots {
                match slot {
                    Some(SlotKind::Leaf(p)) => { drop(unsafe { Box::from_raw(p) }); }
                    Some(SlotKind::Node(child)) => { drop_node(child); }
                    None => {}
                }
            }
        }
        if let Some(root) = self.root.take() { drop_node(root); }
    }
}

pub struct XArrayIter<'a, T> {
    xa: &'a XArray<T>,
    next: u64,
}

impl<'a, T> Iterator for XArrayIter<'a, T> {
    type Item = (u64, &'a T);
    fn next(&mut self) -> Option<(u64, &'a T)> {
        let idx = self.xa.find_after(self.next)?;
        self.next = idx.checked_add(1)?;
        let val = self.xa.load(idx)?;
        Some((idx, val))
    }
}
