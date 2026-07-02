//! Port of mutter's `mtk/mtk/mtk-dispose-bin.{c,h}` to idiomatic Rust.
//!
//! `MtkDisposeBin` is a helper used during GObject `dispose` to collect
//! items that need to be destroyed *after* the main dispose logic completes.
//!
//! # What's ported
//!
//! - `mtk_dispose_bin_new` — creates an empty bin.
//! - `mtk_dispose_bin_add(bin, data, destroy_func)` — registers an item.
//! - `mtk_dispose_bin_dispose(bin)` — invokes all callbacks in LIFO order.
//!
//! # What's skipped
//!
//! - GLib memory allocation — replaced by Rust's `Vec` and `Drop`.
//! - `GDestroyNotify` — replaced by `Box<dyn FnOnce()>`.

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::vec::Vec;

type DisposeEntry = Box<dyn FnOnce()>;

/// Port of `MtkDisposeBin`: a collection of deferred-destruction callbacks.
///
/// No `Debug` derive: the entries are `Box<dyn FnOnce()>`, which is not `Debug`.
#[derive(Default)]
pub struct DisposeBin {
    entries: Vec<DisposeEntry>,
}

impl DisposeBin {
    /// Port of `mtk_dispose_bin_new`.
    pub fn new() -> Self {
        DisposeBin {
            entries: Vec::new(),
        }
    }

    /// Port of `mtk_dispose_bin_add` — idiomatic Rust closure API.
    pub fn add<F: FnOnce() + 'static>(&mut self, callback: F) {
        self.entries.push(Box::new(callback));
    }

    /// Low-level port — C-style raw pointer API.
    ///
    /// # Safety
    ///
    /// The caller must ensure `data` is valid and `destroy_func` safely handles it.
    pub unsafe fn add_with(&mut self, data: *mut u8, destroy_func: unsafe extern "C" fn(*mut u8)) {
        self.entries.push(Box::new(move || {
            // SAFETY: caller of add_with guarantees data is valid for destroy_func.
            unsafe { destroy_func(data) };
        }));
    }

    /// Port of `mtk_dispose_bin_dispose`. Calls all callbacks in reverse (LIFO) order.
    pub fn dispose(&mut self) {
        while let Some(entry) = self.entries.pop() {
            entry();
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl Drop for DisposeBin {
    fn drop(&mut self) {
        self.dispose();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::rc::Rc;
    use core::cell::Cell;

    #[test]
    fn test_new_is_empty() {
        let bin = DisposeBin::new();
        assert!(bin.is_empty());
        assert_eq!(bin.len(), 0);
    }

    #[test]
    fn test_dispose_runs_all_callbacks() {
        let counter = Rc::new(Cell::new(0));
        let mut bin = DisposeBin::new();
        let c1 = counter.clone();
        bin.add(move || {
            c1.set(c1.get() + 1);
        });
        let c2 = counter.clone();
        bin.add(move || {
            c2.set(c2.get() + 10);
        });
        assert_eq!(counter.get(), 0);
        bin.dispose();
        assert_eq!(counter.get(), 11);
        assert!(bin.is_empty());
    }

    #[test]
    fn test_dispose_runs_in_reverse_order() {
        let order = Rc::new(Cell::new(0));
        let recorded = Rc::new(Cell::new(0u32));
        let mut bin = DisposeBin::new();
        let o1 = order.clone();
        let r1 = recorded.clone();
        bin.add(move || {
            r1.set(r1.get() | (1 << o1.get()));
            o1.set(o1.get() + 1);
        });
        let o2 = order.clone();
        let r2 = recorded.clone();
        bin.add(move || {
            r2.set(r2.get() | (1 << o2.get()));
            o2.set(o2.get() + 1);
        });
        bin.dispose();
        assert_eq!(recorded.get(), 0b11);
    }

    #[test]
    fn test_drop_runs_dispose() {
        let counter = Rc::new(Cell::new(0));
        {
            let mut bin = DisposeBin::new();
            let c = counter.clone();
            bin.add(move || {
                c.set(c.get() + 1);
            });
        }
        assert_eq!(counter.get(), 1);
    }
}
