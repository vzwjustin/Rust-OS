//! Trash stack matching `gtrashstack.h` (deprecated).
//!
//! A simple LIFO stack for managing memory. Deprecated in GLib 2.48.
//! Fully `no_std` compatible using `alloc`.

use alloc::vec::Vec;

/// A trash stack (`GTrashStack`).
///
/// A simple stack of pointers. Deprecated in GLib 2.48.
pub struct TrashStack {
    items: Vec<usize>,
}

impl TrashStack {
    /// Create a new empty trash stack.
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Push data onto the stack (`g_trash_stack_push`).
    pub fn push(&mut self, data: usize) {
        self.items.push(data);
    }

    /// Pop data from the stack (`g_trash_stack_pop`).
    pub fn pop(&mut self) -> Option<usize> {
        self.items.pop()
    }

    /// Peek at the top of the stack (`g_trash_stack_peek`).
    pub fn peek(&self) -> Option<usize> {
        self.items.last().copied()
    }

    /// Get the height of the stack (`g_trash_stack_height`).
    pub fn height(&self) -> u32 {
        self.items.len() as u32
    }

    /// Returns `true` if the stack is empty.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

impl Default for TrashStack {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop() {
        let mut s = TrashStack::new();
        s.push(0x1000);
        s.push(0x2000);
        assert_eq!(s.pop(), Some(0x2000));
        assert_eq!(s.pop(), Some(0x1000));
        assert_eq!(s.pop(), None);
    }

    #[test]
    fn peek() {
        let mut s = TrashStack::new();
        assert_eq!(s.peek(), None);
        s.push(42);
        assert_eq!(s.peek(), Some(42));
        s.push(99);
        assert_eq!(s.peek(), Some(99));
    }

    #[test]
    fn height() {
        let mut s = TrashStack::new();
        assert_eq!(s.height(), 0);
        s.push(1);
        s.push(2);
        s.push(3);
        assert_eq!(s.height(), 3);
        s.pop();
        assert_eq!(s.height(), 2);
    }

    #[test]
    fn is_empty() {
        let mut s = TrashStack::new();
        assert!(s.is_empty());
        s.push(1);
        assert!(!s.is_empty());
        s.pop();
        assert!(s.is_empty());
    }
}
