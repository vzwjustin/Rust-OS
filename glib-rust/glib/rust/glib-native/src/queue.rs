//! Double-ended queue matching `GQueue`.

use crate::UInt;
use alloc::collections::VecDeque;

/// Double-ended queue (`GQueue`).
///
/// C stores `head`, `tail`, and `length` explicitly; here the front of the
/// internal deque is the head, the back is the tail, and [`get_length`](Self::get_length)
/// reports the element count.
#[derive(Clone, Debug)]
pub struct GQueue<T> {
    deque: VecDeque<T>,
}

impl<T> Default for GQueue<T> {
    fn default() -> Self {
        Self {
            deque: VecDeque::new(),
        }
    }
}

impl<T> GQueue<T> {
    /// Allocate a new queue (`g_queue_new`).
    pub fn new() -> Self {
        Self::default()
    }

    /// Reset a queue in place (`g_queue_init`).
    pub fn init(&mut self) {
        self.deque.clear();
    }

    /// Whether the queue has no elements (`g_queue_is_empty`).
    pub fn is_empty(&self) -> bool {
        self.deque.is_empty()
    }

    /// Number of elements (`g_queue_get_length`).
    pub fn get_length(&self) -> UInt {
        self.deque.len() as UInt
    }

    /// First element without removing it (`g_queue_peek_head`).
    pub fn peek_head(&self) -> Option<&T> {
        self.deque.front()
    }

    /// Last element without removing it (`g_queue_peek_tail`).
    pub fn peek_tail(&self) -> Option<&T> {
        self.deque.back()
    }

    /// Insert at the head (`g_queue_push_head`).
    pub fn push_head(&mut self, data: T) {
        self.deque.push_front(data);
    }

    /// Insert at the tail (`g_queue_push_tail`).
    pub fn push_tail(&mut self, data: T) {
        self.deque.push_back(data);
    }

    /// Remove and return the head (`g_queue_pop_head`).
    pub fn pop_head(&mut self) -> Option<T> {
        self.deque.pop_front()
    }

    /// Remove and return the tail (`g_queue_pop_tail`).
    pub fn pop_tail(&mut self) -> Option<T> {
        self.deque.pop_back()
    }

    /// Remove all elements (`g_queue_clear`).
    pub fn clear(&mut self) {
        self.deque.clear();
    }

    /// Remove all elements, calling `free_func` on each (`g_queue_clear_full`).
    pub fn clear_full<F>(&mut self, mut free_func: F)
    where
        F: FnMut(T),
    {
        while let Some(item) = self.deque.pop_front() {
            free_func(item);
        }
    }

    /// Free the queue after calling `free_func` on each element (`g_queue_free_full`).
    pub fn free_full<F>(mut self, mut free_func: F)
    where
        F: FnMut(T),
    {
        while let Some(item) = self.deque.pop_front() {
            free_func(item);
        }
    }

    /// Reverse element order in place (`g_queue_reverse`).
    pub fn reverse(&mut self) {
        let mut reversed = VecDeque::with_capacity(self.deque.len());
        while let Some(item) = self.deque.pop_back() {
            reversed.push_back(item);
        }
        self.deque = reversed;
    }

    /// Call `func` for each element in head-to-tail order (`g_queue_foreach`).
    pub fn foreach<F>(&self, mut func: F)
    where
        F: FnMut(&T),
    {
        for item in &self.deque {
            func(item);
        }
    }

    /// Find the first element equal to `data` (`g_queue_find`).
    ///
    /// Returns the zero-based index of the match, or `None`.
    pub fn find(&self, data: &T) -> Option<usize>
    where
        T: PartialEq,
    {
        self.deque.iter().position(|item| item == data)
    }

    /// Find the first element matching a custom predicate (`g_queue_find_custom`).
    pub fn find_custom<P>(&self, predicate: P) -> Option<usize>
    where
        P: FnMut(&T) -> bool,
    {
        self.deque.iter().position(predicate)
    }

    /// Consume the queue and drop its storage (`g_queue_free`).
    pub fn free(self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check_integrity(queue: &GQueue<i32>) {
        assert!(queue.get_length() < 4_000_000_000);
        assert_eq!(queue.get_length() as usize, queue.deque.len());
        if queue.is_empty() {
            assert!(queue.peek_head().is_none());
            assert!(queue.peek_tail().is_none());
        } else {
            assert!(queue.peek_head().is_some());
            assert!(queue.peek_tail().is_some());
        }
    }

    #[test]
    fn new_and_static_init_are_empty() {
        let q = GQueue::<i32>::new();
        check_integrity(&q);
        assert!(q.is_empty());

        let q2 = GQueue::<i32>::default();
        check_integrity(&q2);
        assert!(q2.is_empty());
    }

    #[test]
    fn basic_push_pop_peek() {
        let mut q = GQueue::new();

        assert!(q.is_empty());
        q.push_head(2);
        check_integrity(&q);
        assert_eq!(q.peek_head(), Some(&2));
        assert!(!q.is_empty());

        q.push_head(1);
        check_integrity(&q);
        assert_eq!(q.peek_head(), Some(&1));
        assert_eq!(q.peek_tail(), Some(&2));
        assert_eq!(q.get_length(), 2);

        q.push_tail(3);
        q.push_tail(4);
        q.push_tail(5);
        check_integrity(&q);
        assert_eq!(q.get_length(), 5);
        assert_eq!(q.peek_head(), Some(&1));
        assert_eq!(q.peek_tail(), Some(&5));

        assert_eq!(q.pop_head(), Some(1));
        check_integrity(&q);
        assert_eq!(q.get_length(), 4);

        assert_eq!(q.pop_tail(), Some(5));
        check_integrity(&q);
        assert_eq!(q.get_length(), 3);

        assert_eq!(q.pop_head(), Some(2));
        assert_eq!(q.pop_tail(), Some(4));
        assert_eq!(q.pop_head(), Some(3));
        check_integrity(&q);
        assert!(q.is_empty());

        assert_eq!(q.pop_head(), None);
        assert_eq!(q.pop_tail(), None);
        check_integrity(&q);
    }

    #[test]
    fn clear_empties_queue() {
        let mut q = GQueue::new();
        q.push_tail(1234);
        q.push_tail(1);
        q.push_tail(2);
        assert_eq!(q.get_length(), 3);

        q.clear();
        check_integrity(&q);
        assert!(q.is_empty());
    }

    #[test]
    fn reverse_reorders_elements() {
        let mut q = GQueue::new();
        q.push_tail(1);
        q.push_tail(2);
        q.push_tail(3);
        q.reverse();
        check_integrity(&q);
        assert_eq!(q.pop_head(), Some(3));
        assert_eq!(q.pop_head(), Some(2));
        assert_eq!(q.pop_head(), Some(1));
        assert!(q.is_empty());

        q.reverse();
        check_integrity(&q);
        assert!(q.is_empty());
    }

    #[test]
    fn find_existing_and_missing() {
        let mut q = GQueue::new();
        q.push_tail(1234);
        q.push_tail(1);
        q.push_tail(2);

        assert_eq!(q.find(&1), Some(1));
        assert_eq!(q.find(&2), Some(2));
        assert_eq!(q.find(&3), None);

        assert_eq!(q.find_custom(|v| *v == 1234), Some(0));
        assert_eq!(q.find_custom(|v| *v == 99), None);
    }

    #[test]
    fn foreach_visits_all_elements() {
        let mut q = GQueue::new();
        for i in 0..5 {
            q.push_tail(i);
        }

        let mut sum = 0;
        q.foreach(|v| sum += v);
        assert_eq!(sum, 10);
    }

    #[test]
    fn clear_full_runs_callback() {
        let mut q = GQueue::new();
        q.push_tail(1);
        q.push_tail(2);
        q.push_tail(3);
        q.push_tail(4);
        assert_eq!(q.get_length(), 4);

        let mut freed = Vec::new();
        q.clear_full(|x| freed.push(x));

        assert_eq!(freed, vec![1, 2, 3, 4]);
        assert!(q.is_empty());
        check_integrity(&q);
    }

    #[test]
    fn free_full_runs_callback() {
        let mut q = GQueue::new();
        q.push_tail(10);
        q.push_tail(20);
        q.push_tail(30);

        let mut freed = Vec::new();
        q.free_full(|x| freed.push(x));

        assert_eq!(freed, vec![10, 20, 30]);
    }

    #[test]
    fn init_resets_queue() {
        let mut q = GQueue::new();
        q.push_tail(1);
        q.init();
        check_integrity(&q);
        assert!(q.is_empty());
    }

    #[test]
    fn off_by_one_length_bounds() {
        let mut q = GQueue::new();
        q.push_tail(1234);
        check_integrity(&q);
        assert_eq!(q.peek_tail(), Some(&1234));
        assert_eq!(q.get_length(), 1);
        assert_eq!(q.pop_tail(), Some(1234));
        assert_eq!(q.pop_tail(), None);
        check_integrity(&q);
    }
}
