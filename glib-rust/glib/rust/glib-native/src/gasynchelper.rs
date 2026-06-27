//! GAsyncHelper matching `gio/gasynchelper.h`.
//! Internal helper utilities for async operations. In this no_std port
//! we model a simple async completion callback queue.
//! Fully `no_std` compatible using `alloc`.

use alloc::boxed::Box;
use alloc::vec::Vec;
use spin::Mutex;

/// A pending async callback.
pub struct AsyncCallback {
    pub id: u64,
    pub callback: Box<dyn Fn() + Send + Sync>,
}

/// An async helper queue (`GAsyncHelper`).
pub struct AsyncHelper {
    pending: Mutex<Vec<AsyncCallback>>,
    next_id: Mutex<u64>,
}

impl AsyncHelper {
    pub fn new() -> Self {
        Self {
            pending: Mutex::new(Vec::new()),
            next_id: Mutex::new(1),
        }
    }

    pub fn queue(&self, callback: Box<dyn Fn() + Send + Sync>) -> u64 {
        let mut id = self.next_id.lock();
        let cb_id = *id;
        *id += 1;
        self.pending.lock().push(AsyncCallback {
            id: cb_id,
            callback,
        });
        cb_id
    }

    pub fn drain(&self) -> usize {
        let mut pending = self.pending.lock();
        let callbacks: Vec<_> = pending.drain(..).collect();
        for cb in &callbacks {
            (cb.callback)();
        }
        callbacks.len()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.lock().len()
    }

    pub fn cancel(&self, id: u64) -> bool {
        let mut pending = self.pending.lock();
        let before = pending.len();
        pending.retain(|cb| cb.id != id);
        pending.len() != before
    }
}

impl Default for AsyncHelper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::sync::Arc;

    #[test]
    fn test_queue_drain() {
        let h = AsyncHelper::new();
        let counter = Arc::new(core::sync::atomic::AtomicU32::new(0));
        let c = counter.clone();
        h.queue(Box::new(move || {
            c.fetch_add(1, core::sync::atomic::Ordering::SeqCst);
        }));
        assert_eq!(h.pending_count(), 1);
        assert_eq!(h.drain(), 1);
        assert_eq!(counter.load(core::sync::atomic::Ordering::SeqCst), 1);
    }

    #[test]
    fn test_cancel() {
        let h = AsyncHelper::new();
        let id = h.queue(Box::new(|| {}));
        assert!(h.cancel(id));
        assert_eq!(h.pending_count(), 0);
    }
}
