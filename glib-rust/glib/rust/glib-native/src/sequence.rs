//! Sorted sequence matching `gsequence.h` / `gsequence.c`.
//!
//! A sorted sequence data structure backed by a `Vec`. Supports insertion,
//! removal, search, and iteration. Fully `no_std` compatible using `alloc`.

use crate::prelude::*;

/// A sorted sequence (`GSequence`).
///
/// Elements are stored in a `Vec` and can be kept sorted or unsorted.
/// Iterators are indices into the internal storage.
pub struct Sequence<T> {
    items: Vec<T>,
}

/// An iterator into a sequence (`GSequenceIter`).
pub type SequenceIter = usize;

impl<T> Sequence<T> {
    /// Create a new sequence (`g_sequence_new`).
    pub fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Get the length (`g_sequence_get_length`).
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Returns `true` if the sequence is empty (`g_sequence_is_empty`).
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Get the begin iterator (`g_sequence_get_begin_iter`).
    pub fn begin(&self) -> SequenceIter {
        0
    }

    /// Get the end iterator (`g_sequence_get_end_iter`).
    pub fn end(&self) -> SequenceIter {
        self.items.len()
    }

    /// Get an iterator at a position (`g_sequence_get_iter_at_pos`).
    pub fn iter_at_pos(&self, pos: i32) -> SequenceIter {
        if pos < 0 {
            self.items.len()
        } else {
            core::cmp::min(pos as usize, self.items.len())
        }
    }

    /// Append an element (`g_sequence_append`).
    pub fn append(&mut self, data: T) -> SequenceIter {
        let idx = self.items.len();
        self.items.push(data);
        idx
    }

    /// Prepend an element (`g_sequence_prepend`).
    pub fn prepend(&mut self, data: T) -> SequenceIter {
        self.items.insert(0, data);
        0
    }

    /// Insert before an iterator (`g_sequence_insert_before`).
    pub fn insert_before(&mut self, iter: SequenceIter, data: T) -> SequenceIter {
        let pos = core::cmp::min(iter, self.items.len());
        self.items.insert(pos, data);
        pos
    }

    /// Remove an element at iter (`g_sequence_remove`).
    pub fn remove(&mut self, iter: SequenceIter) {
        if iter < self.items.len() {
            self.items.remove(iter);
        }
    }

    /// Remove a range (`g_sequence_remove_range`).
    pub fn remove_range(&mut self, begin: SequenceIter, end: SequenceIter) {
        let b = core::cmp::min(begin, self.items.len());
        let e = core::cmp::min(end, self.items.len());
        if b < e {
            self.items.drain(b..e);
        }
    }

    /// Get the element at iter (`g_sequence_get`).
    pub fn get(&self, iter: SequenceIter) -> Option<&T> {
        self.items.get(iter)
    }

    /// Get a mutable reference at iter (`g_sequence_set`).
    pub fn get_mut(&mut self, iter: SequenceIter) -> Option<&mut T> {
        self.items.get_mut(iter)
    }

    /// Set the element at iter (`g_sequence_set`).
    pub fn set(&mut self, iter: SequenceIter, data: T) {
        if iter < self.items.len() {
            self.items[iter] = data;
        }
    }

    /// Check if iter is at begin (`g_sequence_iter_is_begin`).
    pub fn iter_is_begin(&self, iter: SequenceIter) -> bool {
        iter == 0
    }

    /// Check if iter is at end (`g_sequence_iter_is_end`).
    pub fn iter_is_end(&self, iter: SequenceIter) -> bool {
        iter >= self.items.len()
    }

    /// Get the next iterator (`g_sequence_iter_next`).
    pub fn iter_next(&self, iter: SequenceIter) -> SequenceIter {
        core::cmp::min(iter + 1, self.items.len())
    }

    /// Get the previous iterator (`g_sequence_iter_prev`).
    pub fn iter_prev(&self, iter: SequenceIter) -> SequenceIter {
        iter.saturating_sub(1)
    }

    /// Get the position of an iterator (`g_sequence_iter_get_position`).
    pub fn iter_position(&self, iter: SequenceIter) -> i32 {
        if iter >= self.items.len() {
            self.items.len() as i32
        } else {
            iter as i32
        }
    }

    /// Move iterator by delta (`g_sequence_iter_move`).
    pub fn iter_move(&self, iter: SequenceIter, delta: i32) -> SequenceIter {
        let new_pos = iter as i32 + delta;
        if new_pos < 0 {
            0
        } else {
            core::cmp::min(new_pos as usize, self.items.len())
        }
    }

    /// Sort the sequence (`g_sequence_sort`).
    pub fn sort(&mut self, cmp: impl FnMut(&T, &T) -> core::cmp::Ordering) {
        self.items.sort_by(cmp);
    }

    /// Insert sorted (`g_sequence_insert_sorted`).
    pub fn insert_sorted(&mut self, data: T, mut cmp: impl FnMut(&T, &T) -> core::cmp::Ordering) -> SequenceIter {
        let pos = self.items.partition_point(|item| cmp(item, &data) == core::cmp::Ordering::Less);
        self.items.insert(pos, data);
        pos
    }

    /// Search for an element (`g_sequence_search`).
    pub fn search(&self, data: &T, cmp: impl Fn(&T, &T) -> core::cmp::Ordering) -> SequenceIter {
        self.items.partition_point(|item| cmp(item, data) == core::cmp::Ordering::Less)
    }

    /// Lookup an exact match (`g_sequence_lookup`).
    pub fn lookup(&self, data: &T, cmp: impl Fn(&T, &T) -> core::cmp::Ordering) -> Option<SequenceIter> {
        let pos = self.search(data, &cmp);
        if pos < self.items.len() && cmp(&self.items[pos], data) == core::cmp::Ordering::Equal {
            Some(pos)
        } else {
            None
        }
    }

    /// Swap two elements (`g_sequence_swap`).
    pub fn swap(&mut self, a: SequenceIter, b: SequenceIter) {
        if a < self.items.len() && b < self.items.len() {
            self.items.swap(a, b);
        }
    }

    /// Move element from src to dest (`g_sequence_move`).
    pub fn move_item(&mut self, src: SequenceIter, dest: SequenceIter) {
        if src >= self.items.len() {
            return;
        }
        let item = self.items.remove(src);
        let dest = if dest > src { dest - 1 } else { dest };
        self.items.insert(core::cmp::min(dest, self.items.len()), item);
    }

    /// ForEach (`g_sequence_foreach`).
    pub fn foreach(&self, mut f: impl FnMut(&T)) {
        for item in &self.items {
            f(item);
        }
    }

    /// ForEach in range (`g_sequence_foreach_range`).
    pub fn foreach_range(&self, begin: SequenceIter, end: SequenceIter, mut f: impl FnMut(&T)) {
        let e = core::cmp::min(end, self.items.len());
        for i in begin..e {
            f(&self.items[i]);
        }
    }

    /// Get the range midpoint (`g_sequence_range_get_midpoint`).
    pub fn range_midpoint(&self, begin: SequenceIter, end: SequenceIter) -> SequenceIter {
        (begin + end) / 2
    }

    /// Clear all elements.
    pub fn clear(&mut self) {
        self.items.clear();
    }
}

impl<T> Default for Sequence<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::cmp::Ordering;

    fn cmp_i32(a: &i32, b: &i32) -> Ordering {
        a.cmp(b)
    }

    #[test]
    fn append_and_get() {
        let mut seq: Sequence<i32> = Sequence::new();
        let i1 = seq.append(10);
        let i2 = seq.append(20);
        assert_eq!(seq.len(), 2);
        assert_eq!(seq.get(i1), Some(&10));
        assert_eq!(seq.get(i2), Some(&20));
    }

    #[test]
    fn prepend() {
        let mut seq: Sequence<i32> = Sequence::new();
        seq.append(20);
        seq.prepend(10);
        assert_eq!(seq.get(0), Some(&10));
        assert_eq!(seq.get(1), Some(&20));
    }

    #[test]
    fn insert_before() {
        let mut seq: Sequence<i32> = Sequence::new();
        seq.append(10);
        seq.append(30);
        seq.insert_before(1, 20);
        assert_eq!(seq.get(0), Some(&10));
        assert_eq!(seq.get(1), Some(&20));
        assert_eq!(seq.get(2), Some(&30));
    }

    #[test]
    fn remove() {
        let mut seq: Sequence<i32> = Sequence::new();
        seq.append(10);
        seq.append(20);
        seq.append(30);
        seq.remove(1);
        assert_eq!(seq.len(), 2);
        assert_eq!(seq.get(1), Some(&30));
    }

    #[test]
    fn sort() {
        let mut seq: Sequence<i32> = Sequence::new();
        seq.append(30);
        seq.append(10);
        seq.append(20);
        seq.sort(cmp_i32);
        assert_eq!(seq.get(0), Some(&10));
        assert_eq!(seq.get(1), Some(&20));
        assert_eq!(seq.get(2), Some(&30));
    }

    #[test]
    fn insert_sorted() {
        let mut seq: Sequence<i32> = Sequence::new();
        seq.insert_sorted(20, cmp_i32);
        seq.insert_sorted(10, cmp_i32);
        seq.insert_sorted(30, cmp_i32);
        assert_eq!(seq.get(0), Some(&10));
        assert_eq!(seq.get(1), Some(&20));
        assert_eq!(seq.get(2), Some(&30));
    }

    #[test]
    fn search_and_lookup() {
        let mut seq: Sequence<i32> = Sequence::new();
        seq.insert_sorted(10, cmp_i32);
        seq.insert_sorted(30, cmp_i32);
        seq.insert_sorted(20, cmp_i32);

        let found = seq.lookup(&20, cmp_i32);
        assert_eq!(found, Some(1));

        let not_found = seq.lookup(&25, cmp_i32);
        assert_eq!(not_found, None);
    }

    #[test]
    fn iter_navigation() {
        let mut seq: Sequence<i32> = Sequence::new();
        seq.append(10);
        seq.append(20);
        seq.append(30);

        assert!(seq.iter_is_begin(0));
        assert!(!seq.iter_is_begin(1));
        assert!(seq.iter_is_end(3));
        assert_eq!(seq.iter_next(0), 1);
        assert_eq!(seq.iter_prev(2), 1);
        assert_eq!(seq.iter_position(1), 1);
        assert_eq!(seq.iter_move(0, 2), 2);
    }

    #[test]
    fn foreach() {
        let mut seq: Sequence<i32> = Sequence::new();
        seq.append(10);
        seq.append(20);
        seq.append(30);

        let mut sum = 0;
        seq.foreach(|v| sum += v);
        assert_eq!(sum, 60);
    }

    #[test]
    fn remove_range() {
        let mut seq: Sequence<i32> = Sequence::new();
        for i in 0..5 {
            seq.append(i * 10);
        }
        seq.remove_range(1, 3);
        assert_eq!(seq.len(), 3);
        assert_eq!(seq.get(0), Some(&0));
        assert_eq!(seq.get(1), Some(&30));
        assert_eq!(seq.get(2), Some(&40));
    }

    #[test]
    fn swap() {
        let mut seq: Sequence<i32> = Sequence::new();
        seq.append(10);
        seq.append(20);
        seq.swap(0, 1);
        assert_eq!(seq.get(0), Some(&20));
        assert_eq!(seq.get(1), Some(&10));
    }
}
