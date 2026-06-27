//! Async queue matching `gasyncqueue.h` / `gasyncqueue.c`.
//!
//! Thread-safe FIFO queue using `spin::Mutex`. In a `no_std` kernel
//! environment, the blocking `pop` spins until data is available.
//! Fully `no_std` compatible using `alloc` and `spin`.

use crate::prelude::*;
use alloc::collections::VecDeque;
use alloc::sync::Arc;
use spin::mutex::Mutex;

/// An async queue (`GAsyncQueue`).
///
/// Thread-safe FIFO queue. `pop` blocks (spins) until data is available.
/// `try_pop` returns `None` immediately if the queue is empty.
pub struct AsyncQueue<T> {
    inner: Mutex<AsyncQueueInner<T>>,
}

struct AsyncQueueInner<T> {
    queue: VecDeque<T>,
}

impl<T> AsyncQueue<T> {
    /// Create a new async queue (`g_async_queue_new`).
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(AsyncQueueInner {
                queue: VecDeque::new(),
            }),
        }
    }

    /// Push data to the back of the queue (`g_async_queue_push`).
    pub fn push(&self, data: T) {
        self.inner.lock().queue.push_back(data);
    }

    /// Push data to the front of the queue (`g_async_queue_push_front`).
    pub fn push_front(&self, data: T) {
        self.inner.lock().queue.push_front(data);
    }

    /// Pop data from the front, blocking until available (`g_async_queue_pop`).
    ///
    /// In a `no_std` environment, this spins until data is available.
    pub fn pop(&self) -> T {
        loop {
            if let Some(data) = self.try_pop() {
                return data;
            }
            core::hint::spin_loop();
        }
    }

    /// Try to pop data without blocking (`g_async_queue_try_pop`).
    pub fn try_pop(&self) -> Option<T> {
        self.inner.lock().queue.pop_front()
    }

    /// Pop data from the back (`g_async_queue_pop_unlocked` variant).
    pub fn try_pop_back(&self) -> Option<T> {
        self.inner.lock().queue.pop_back()
    }

    /// Get the length of the queue (`g_async_queue_length`).
    pub fn len(&self) -> usize {
        self.inner.lock().queue.len()
    }

    /// Returns `true` if the queue is empty.
    pub fn is_empty(&self) -> bool {
        self.inner.lock().queue.is_empty()
    }

    /// Remove a specific item from the queue (`g_async_queue_remove`).
    pub fn remove(&self, item: &T) -> bool
    where
        T: PartialEq,
    {
        let mut inner = self.inner.lock();
        let before = inner.queue.len();
        inner.queue.retain(|x| x != item);
        inner.queue.len() != before
    }

    /// Sort the queue (`g_async_queue_sort`).
    pub fn sort(&self, cmp: impl FnMut(&T, &T) -> core::cmp::Ordering) {
        let mut inner = self.inner.lock();
        let mut items: Vec<T> = inner.queue.drain(..).collect();
        items.sort_by(cmp);
        inner.queue.extend(items);
    }

    /// Push sorted (`g_async_queue_push_sorted`).
    pub fn push_sorted(&self, data: T, mut cmp: impl FnMut(&T, &T) -> core::cmp::Ordering) {
        let mut inner = self.inner.lock();
        let pos = inner
            .queue
            .iter()
            .position(|item| cmp(item, &data) == core::cmp::Ordering::Greater);
        match pos {
            Some(p) => {
                let mut items: Vec<T> = inner.queue.drain(..).collect();
                items.insert(p, data);
                inner.queue.extend(items);
            }
            None => {
                inner.queue.push_back(data);
            }
        }
    }

    /// Clear all items from the queue.
    pub fn clear(&self) {
        self.inner.lock().queue.clear();
    }
}

impl<T> Default for AsyncQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Create a new shared async queue (`g_async_queue_new` + ref).
pub fn async_queue_new<T>() -> Arc<AsyncQueue<T>> {
    Arc::new(AsyncQueue::new())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop() {
        let q = AsyncQueue::new();
        q.push(1);
        q.push(2);
        q.push(3);
        assert_eq!(q.len(), 3);
        assert_eq!(q.try_pop(), Some(1));
        assert_eq!(q.try_pop(), Some(2));
        assert_eq!(q.try_pop(), Some(3));
        assert_eq!(q.try_pop(), None);
    }

    #[test]
    fn push_front() {
        let q = AsyncQueue::new();
        q.push(1);
        q.push_front(0);
        assert_eq!(q.try_pop(), Some(0));
        assert_eq!(q.try_pop(), Some(1));
    }

    #[test]
    fn is_empty() {
        let q = AsyncQueue::new();
        assert!(q.is_empty());
        q.push(1);
        assert!(!q.is_empty());
        q.try_pop();
        assert!(q.is_empty());
    }

    #[test]
    fn remove() {
        let q = AsyncQueue::new();
        q.push(1);
        q.push(2);
        q.push(3);
        assert!(q.remove(&2));
        assert_eq!(q.len(), 2);
        assert_eq!(q.try_pop(), Some(1));
        assert_eq!(q.try_pop(), Some(3));
    }

    #[test]
    fn sort() {
        let q = AsyncQueue::new();
        q.push(30);
        q.push(10);
        q.push(20);
        q.sort(|a, b| a.cmp(b));
        assert_eq!(q.try_pop(), Some(10));
        assert_eq!(q.try_pop(), Some(20));
        assert_eq!(q.try_pop(), Some(30));
    }

    #[test]
    fn push_sorted() {
        let q = AsyncQueue::new();
        q.push_sorted(20, |a, b| a.cmp(b));
        q.push_sorted(10, |a, b| a.cmp(b));
        q.push_sorted(30, |a, b| a.cmp(b));
        assert_eq!(q.try_pop(), Some(10));
        assert_eq!(q.try_pop(), Some(20));
        assert_eq!(q.try_pop(), Some(30));
    }

    #[test]
    fn clear() {
        let q = AsyncQueue::new();
        q.push(1);
        q.push(2);
        q.clear();
        assert_eq!(q.len(), 0);
        assert!(q.is_empty());
    }
}
