//! GIO cancellable operation matching `gio/gcancellable.h` / `gio/gcancellable.c`.
//!
//! Upstream `GCancellable` is a `GObject` subclass that allows operations to be cancelled.
//! We port it as a plain `pub struct` wrapping thread-safe cancellation state, supporting:
//! - Thread-safe cancellation (`cancel`)
//! - Resetting (`reset`)
//! - Callback connection and disconnection (`connect`, `disconnect`)
//! - Integration with `GError` (`set_error_if_cancelled`)
//! - Global stack of current cancellables (`get_current`, `push_current`, `pop_current`)
//! - Main loop `Source` integration stub (`cancellable_source_new`)

use crate::error::Error;
use crate::gioerror::{io_error_quark, IOErrorEnum};
use crate::poll::PollFD;
use crate::prelude::*;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::mutex::Mutex;

/// A thread-safe cancellation object (`GCancellable`).
pub struct GCancellable {
    state: Mutex<CancellableState>,
}

struct CancellableState {
    cancelled: bool,
    cancelled_running: bool,
    next_handler_id: u32,
    handlers: Vec<CancellableHandler>,
}

struct CancellableHandler {
    id: u32,
    callback: Arc<dyn Fn() + Send + Sync>,
}

// Global stack of current cancellables.
// In `no_std`, we use a single thread-local-like global stack protected by a Mutex.
static CURRENT_CANCELLABLE_STACK: Mutex<Vec<Arc<GCancellable>>> = Mutex::new(Vec::new());

impl GCancellable {
    /// Creates a new `GCancellable` (`g_cancellable_new`).
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(CancellableState {
                cancelled: false,
                cancelled_running: false,
                next_handler_id: 1,
                handlers: Vec::new(),
            }),
        })
    }

    /// Checks if the operation has been cancelled (`g_cancellable_is_cancelled`).
    pub fn is_cancelled(&self) -> bool {
        self.state.lock().cancelled
    }

    /// If `self` is cancelled, sets `error` and returns `Err(Error)` (`g_cancellable_set_error_if_cancelled`).
    pub fn set_error_if_cancelled(&self) -> Result<(), Error> {
        if self.is_cancelled() {
            Err(Error::new(
                io_error_quark(),
                IOErrorEnum::Cancelled.to_code(),
                "Operation was cancelled",
            ))
        } else {
            Ok(())
        }
    }

    /// Reset the cancellable (`g_cancellable_reset`).
    ///
    /// Resets the cancellable to its uncancelled state. Any connected handlers remain connected.
    pub fn reset(&self) {
        let mut state = self.state.lock();
        state.cancelled = false;
    }

    /// Connect a callback to be called when the cancellable is cancelled (`g_cancellable_connect`).
    ///
    /// If the cancellable is already cancelled, the callback is called immediately inline.
    /// Returns the handler ID, or 0 if the callback was run immediately.
    pub fn connect<F>(&self, callback: F) -> u32
    where
        F: Fn() + Send + Sync + 'static,
    {
        let mut state = self.state.lock();
        if state.cancelled {
            drop(state);
            callback();
            0
        } else {
            let id = state.next_handler_id;
            state.next_handler_id += 1;
            state.handlers.push(CancellableHandler {
                id,
                callback: Arc::new(callback),
            });
            id
        }
    }

    /// Disconnects a callback handler (`g_cancellable_disconnect`).
    ///
    /// If the handler is currently running in another thread, this blocks until it finishes.
    pub fn disconnect(&self, handler_id: u32) {
        if handler_id == 0 {
            return;
        }

        // Loop and check if callbacks are currently running. If so, spin-wait to avoid races.
        loop {
            let mut state = self.state.lock();
            if !state.cancelled_running {
                state.handlers.retain(|h| h.id != handler_id);
                break;
            }
            drop(state);
            core::hint::spin_loop();
        }
    }

    /// Cancel the operation (`g_cancellable_cancel`).
    ///
    /// Triggers all connected handlers.
    pub fn cancel(&self) {
        let mut state = self.state.lock();
        if state.cancelled {
            return;
        }

        state.cancelled = true;
        state.cancelled_running = true;

        // Retrieve and execute handlers. We copy/extract the callbacks to avoid executing
        // them while holding the lock (which would cause a deadlock if a callback calls connect/disconnect).
        let callbacks: Vec<Arc<dyn Fn() + Send + Sync>> =
            state.handlers.iter().map(|h| h.callback.clone()).collect();
        drop(state);

        for cb in callbacks {
            cb();
        }

        self.state.lock().cancelled_running = false;
    }

    /// Gets the file descriptor for the cancellable (`g_cancellable_get_fd`).
    ///
    /// In `no_std`, this always returns -1 indicating file descriptor polling is not supported.
    pub fn get_fd(&self) -> i32 {
        -1
    }

    /// Prepares a `PollFD` for polling (`g_cancellable_make_pollfd`).
    ///
    /// In `no_std`, this always returns false indicating polling is not supported.
    pub fn make_pollfd(&self, _pollfd: &mut PollFD) -> bool {
        false
    }

    /// Releases resources allocated for the `PollFD` (`g_cancellable_release_fd`).
    ///
    /// No-op in `no_std`.
    pub fn release_fd(&self) {}
}

// Global Stack API

/// Gets the top of the current cancellable stack (`g_cancellable_get_current`).
pub fn cancellable_get_current() -> Option<Arc<GCancellable>> {
    CURRENT_CANCELLABLE_STACK.lock().last().cloned()
}

/// Pushes a cancellable onto the current stack (`g_cancellable_push_current`).
pub fn cancellable_push_current(cancellable: &Arc<GCancellable>) {
    CURRENT_CANCELLABLE_STACK.lock().push(cancellable.clone());
}

/// Pops a cancellable off the current stack (`g_cancellable_pop_current`).
pub fn cancellable_pop_current(cancellable: &Arc<GCancellable>) {
    let mut stack = CURRENT_CANCELLABLE_STACK.lock();
    if let Some(top) = stack.last() {
        if Arc::ptr_eq(top, cancellable) {
            stack.pop();
        }
    }
}

/// Creates a new `GSource` that triggers when the cancellable is cancelled (`g_cancellable_source_new`).
pub fn cancellable_source_new(cancellable: &Arc<GCancellable>) -> crate::mainloop::Source {
    let mut source = crate::mainloop::Source::new(
        0,
        crate::mainloop::SourceFuncs {
            prepare: Some(|_s| (false, 0)),
            check: Some(|_s| true),
            dispatch: None,
            finalize: None,
        },
    );
    source.set_name("GCancellable");
    if cancellable.is_cancelled() {
        source.set_ready_time(0);
    }
    source
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn test_cancellable_basic() {
        let c = GCancellable::new();
        assert!(!c.is_cancelled());
        assert!(c.set_error_if_cancelled().is_ok());

        c.cancel();
        assert!(c.is_cancelled());

        let err = c.set_error_if_cancelled().unwrap_err();
        assert_eq!(err.domain(), io_error_quark());
        assert_eq!(err.code(), IOErrorEnum::Cancelled.to_code());

        c.reset();
        assert!(!c.is_cancelled());
        assert!(c.set_error_if_cancelled().is_ok());
    }

    #[test]
    fn test_cancellable_connect_immediate() {
        let c = GCancellable::new();
        c.cancel();

        let called = Arc::new(AtomicU32::new(0));
        let called_clone = called.clone();
        let id = c.connect(move || {
            called_clone.fetch_add(1, Ordering::SeqCst);
        });

        assert_eq!(id, 0);
        assert_eq!(called.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_cancellable_connect_deferred() {
        let c = GCancellable::new();
        let called = Arc::new(AtomicU32::new(0));
        let called_clone = called.clone();
        let id = c.connect(move || {
            called_clone.fetch_add(1, Ordering::SeqCst);
        });

        assert!(id > 0);
        assert_eq!(called.load(Ordering::SeqCst), 0);

        c.cancel();
        assert_eq!(called.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_cancellable_disconnect() {
        let c = GCancellable::new();
        let called = Arc::new(AtomicU32::new(0));
        let called_clone = called.clone();
        let id = c.connect(move || {
            called_clone.fetch_add(1, Ordering::SeqCst);
        });

        c.disconnect(id);
        c.cancel();
        assert_eq!(called.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_cancellable_stack() {
        assert!(cancellable_get_current().is_none());

        let c1 = GCancellable::new();
        let c2 = GCancellable::new();

        cancellable_push_current(&c1);
        assert!(Arc::ptr_eq(&cancellable_get_current().unwrap(), &c1));

        cancellable_push_current(&c2);
        assert!(Arc::ptr_eq(&cancellable_get_current().unwrap(), &c2));

        cancellable_pop_current(&c2);
        assert!(Arc::ptr_eq(&cancellable_get_current().unwrap(), &c1));

        cancellable_pop_current(&c1);
        assert!(cancellable_get_current().is_none());
    }

    #[test]
    fn test_cancellable_stubs() {
        let c = GCancellable::new();
        assert_eq!(c.get_fd(), -1);

        let mut pfd = PollFD::new(0, 0);
        assert!(!c.make_pollfd(&mut pfd));
        c.release_fd();

        let source = cancellable_source_new(&c);
        assert_eq!(source.get_name(), "GCancellable");
        assert_eq!(source.get_ready_time(), None);

        c.cancel();
        let source_cancelled = cancellable_source_new(&c);
        assert_eq!(source_cancelled.get_ready_time(), Some(0));
    }
}
