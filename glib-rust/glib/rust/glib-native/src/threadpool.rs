//! Thread pool matching `gthreadpool.h` / `gthreadpool.c`.
//!
//! Defines types and a task queue for thread pools. Actual thread
//! creation requires OS support and is deferred to a platform layer.
//! Fully `no_std` compatible using `alloc` and `spin`.

use crate::prelude::*;
use crate::thread::GMutex;

/// Thread pool error codes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ThreadPoolError {
    Invalid,
    Failed,
}

/// A thread pool (`GThreadPool`).
///
/// In no_std, this maintains a task queue. Actual thread creation
/// and execution requires OS support.
pub struct ThreadPool {
    func: fn(usize),
    max_threads: i32,
    exclusive: bool,
    queue: GMutex<Vec<usize>>,
    num_threads: u32,
    sort_func: Option<fn(&usize, &usize) -> core::cmp::Ordering>,
    unprocessed: u32,
}

impl ThreadPool {
    /// Create a new thread pool (`g_thread_pool_new`).
    pub fn new(func: fn(usize), max_threads: i32, exclusive: bool) -> Result<Self, ThreadPoolError> {
        if max_threads < -1 {
            return Err(ThreadPoolError::Invalid);
        }
        Ok(Self {
            func,
            max_threads,
            exclusive,
            queue: GMutex::new(Vec::new()),
            num_threads: 0,
            sort_func: None,
            unprocessed: 0,
        })
    }

    /// Push a task to the pool (`g_thread_pool_push`).
    pub fn push(&self, data: usize) -> Result<(), ThreadPoolError> {
        self.queue.lock().push(data);
        Ok(())
    }

    /// Get the number of unprocessed items (`g_thread_pool_unprocessed`).
    pub fn unprocessed(&self) -> u32 {
        self.queue.lock().len() as u32
    }

    /// Set the sort function (`g_thread_pool_set_sort_function`).
    pub fn set_sort_function(&mut self, func: fn(&usize, &usize) -> core::cmp::Ordering) {
        self.sort_func = Some(func);
    }

    /// Move a task to the front of the queue (`g_thread_pool_move_to_front`).
    pub fn move_to_front(&self, data: usize) -> bool {
        let mut queue = self.queue.lock();
        if let Some(pos) = queue.iter().position(|&x| x == data) {
            let item = queue.remove(pos);
            queue.insert(0, item);
            true
        } else {
            false
        }
    }

    /// Set max threads (`g_thread_pool_set_max_threads`).
    pub fn set_max_threads(&mut self, max_threads: i32) -> Result<(), ThreadPoolError> {
        if max_threads < -1 {
            return Err(ThreadPoolError::Invalid);
        }
        self.max_threads = max_threads;
        Ok(())
    }

    /// Get max threads (`g_thread_pool_get_max_threads`).
    pub fn get_max_threads(&self) -> i32 {
        self.max_threads
    }

    /// Get number of threads (`g_thread_pool_get_num_threads`).
    pub fn get_num_threads(&self) -> u32 {
        self.num_threads
    }

    /// Get exclusive flag.
    pub fn is_exclusive(&self) -> bool {
        self.exclusive
    }

    /// Process all queued tasks on the current thread.
    ///
    /// In no_std without OS threads, this processes the queue inline.
    pub fn process_all(&mut self) {
        let mut queue = self.queue.lock();
        if let Some(sort_fn) = self.sort_func {
            queue.sort_by(sort_fn);
        }
        while let Some(data) = queue.pop() {
            drop(queue);
            (self.func)(data);
            queue = self.queue.lock();
        }
    }

    /// Free the thread pool (`g_thread_pool_free`).
    ///
    /// If `wait_` is true, processes all remaining tasks first.
    pub fn free(&mut self, _immediate: bool, wait_: bool) {
        if wait_ {
            self.process_all();
        } else {
            self.queue.lock().clear();
        }
    }
}

/// Global unused thread pool settings.
static MAX_UNUSED_THREADS: spin::Mutex<i32> = spin::Mutex::new(0);
static NUM_UNUSED_THREADS: spin::Mutex<u32> = spin::Mutex::new(0);
static MAX_IDLE_TIME: spin::Mutex<u32> = spin::Mutex::new(0);

/// Set max unused threads (`g_thread_pool_set_max_unused_threads`).
pub fn set_max_unused_threads(max_threads: i32) {
    *MAX_UNUSED_THREADS.lock() = max_threads;
}

/// Get max unused threads (`g_thread_pool_get_max_unused_threads`).
pub fn get_max_unused_threads() -> i32 {
    *MAX_UNUSED_THREADS.lock()
}

/// Get num unused threads (`g_thread_pool_get_num_unused_threads`).
pub fn get_num_unused_threads() -> u32 {
    *NUM_UNUSED_THREADS.lock()
}

/// Stop unused threads (`g_thread_pool_stop_unused_threads`).
pub fn stop_unused_threads() {
    *NUM_UNUSED_THREADS.lock() = 0;
}

/// Set max idle time (`g_thread_pool_set_max_idle_time`).
pub fn set_max_idle_time(interval: u32) {
    *MAX_IDLE_TIME.lock() = interval;
}

/// Get max idle time (`g_thread_pool_get_max_idle_time`).
pub fn get_max_idle_time() -> u32 {
    *MAX_IDLE_TIME.lock()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn process_task(data: usize) {
        // Simple task - just consume the data
        let _ = data;
    }

    #[test]
    fn thread_pool_new() {
        let pool = ThreadPool::new(process_task, 4, false);
        assert!(pool.is_ok());
        let pool = pool.unwrap();
        assert_eq!(pool.get_max_threads(), 4);
        assert!(!pool.is_exclusive());
    }

    #[test]
    fn thread_pool_push_and_process() {
        let mut pool = ThreadPool::new(process_task, 1, false).unwrap();
        pool.push(1).unwrap();
        pool.push(2).unwrap();
        pool.push(3).unwrap();
        assert_eq!(pool.unprocessed(), 3);
        pool.process_all();
        assert_eq!(pool.unprocessed(), 0);
    }

    #[test]
    fn thread_pool_move_to_front() {
        let pool = ThreadPool::new(process_task, 1, false).unwrap();
        pool.push(1).unwrap();
        pool.push(2).unwrap();
        pool.push(3).unwrap();
        assert!(pool.move_to_front(3));
        assert!(!pool.move_to_front(99));
    }

    #[test]
    fn thread_pool_invalid() {
        let pool = ThreadPool::new(process_task, -2, false);
        assert!(pool.is_err());
    }

    #[test]
    fn thread_pool_set_max() {
        let mut pool = ThreadPool::new(process_task, 2, false).unwrap();
        pool.set_max_threads(8).unwrap();
        assert_eq!(pool.get_max_threads(), 8);
        assert!(pool.set_max_threads(-2).is_err());
    }

    #[test]
    fn thread_pool_free_wait() {
        let mut pool = ThreadPool::new(process_task, 1, false).unwrap();
        pool.push(42).unwrap();
        pool.free(false, true);
        assert_eq!(pool.unprocessed(), 0);
    }

    #[test]
    fn global_settings() {
        set_max_unused_threads(10);
        assert_eq!(get_max_unused_threads(), 10);
        set_max_idle_time(60);
        assert_eq!(get_max_idle_time(), 60);
        stop_unused_threads();
        assert_eq!(get_num_unused_threads(), 0);
    }
}
