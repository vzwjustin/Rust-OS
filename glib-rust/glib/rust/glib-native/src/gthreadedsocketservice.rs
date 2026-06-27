//! `GThreadedSocketService` — socket service with a bounded handler pool.
//!
//! Models `gio/gthreadedsocketservice.h`.  In this no_std port there are no
//! real threads; the struct tracks concurrency limits and counters so callers
//! can simulate dispatching connections to a bounded pool of handlers.

use spin::Mutex;

/// A socket service that dispatches incoming connections to a bounded pool of
/// handler "threads".
///
/// In this no_std port no actual threads are created.  The struct enforces the
/// pool limit through an atomic counter and records totals for observability.
pub struct ThreadedSocketService {
    /// Maximum number of concurrently active handlers (default 10).
    max_threads: u32,
    /// Number of handlers currently running.
    active_count: Mutex<u32>,
    /// Whether the service is accepting new connections.
    active: Mutex<bool>,
    /// Total number of connections successfully dispatched since creation.
    processed: Mutex<u32>,
}

impl ThreadedSocketService {
    /// Create a new `ThreadedSocketService` with the given pool limit.
    ///
    /// Pass `0` to use the GLib default of `10`.
    pub fn new(max_threads: u32) -> Self {
        let max_threads = if max_threads == 0 { 10 } else { max_threads };
        Self {
            max_threads,
            active_count: Mutex::new(0),
            active: Mutex::new(false),
            processed: Mutex::new(0),
        }
    }

    /// Return the configured maximum number of concurrent handlers.
    pub fn get_max_threads(&self) -> u32 {
        self.max_threads
    }

    /// Start accepting connections.
    pub fn start(&self) {
        *self.active.lock() = true;
    }

    /// Stop accepting connections.  Already-active handlers are unaffected.
    pub fn stop(&self) {
        *self.active.lock() = false;
    }

    /// Return `true` if the service is currently accepting connections.
    pub fn is_active(&self) -> bool {
        *self.active.lock()
    }

    /// Attempt to dispatch a new incoming connection to a handler "thread".
    ///
    /// Returns `true` and increments both `active_count` and `processed` when:
    /// * the service is [`active`](Self::is_active), **and**
    /// * `active_count < max_threads`.
    ///
    /// Returns `false` without changing any counters otherwise (service
    /// stopped, or the pool is full).
    pub fn handle_connection(&self) -> bool {
        if !*self.active.lock() {
            return false;
        }
        let mut count = self.active_count.lock();
        if *count >= self.max_threads {
            return false;
        }
        *count += 1;
        drop(count);
        *self.processed.lock() += 1;
        true
    }

    /// Signal that a handler "thread" has finished processing a connection.
    ///
    /// Decrements `active_count`, saturating at zero so an unbalanced call is
    /// harmless.
    pub fn finish_connection(&self) {
        let mut count = self.active_count.lock();
        *count = count.saturating_sub(1);
    }

    /// Return the number of handlers currently active (connections in flight).
    pub fn active_count(&self) -> u32 {
        *self.active_count.lock()
    }

    /// Return the total number of connections successfully dispatched since
    /// this service was created.
    pub fn total_processed(&self) -> u32 {
        *self.processed.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_defaults() {
        let svc = ThreadedSocketService::new(10);
        assert_eq!(svc.get_max_threads(), 10);
        assert!(!svc.is_active());
        assert_eq!(svc.active_count(), 0);
        assert_eq!(svc.total_processed(), 0);
    }

    #[test]
    fn zero_max_threads_uses_default_ten() {
        let svc = ThreadedSocketService::new(0);
        assert_eq!(svc.get_max_threads(), 10);
    }

    #[test]
    fn start_stop_is_active() {
        let svc = ThreadedSocketService::new(5);
        assert!(!svc.is_active());
        svc.start();
        assert!(svc.is_active());
        svc.stop();
        assert!(!svc.is_active());
    }

    #[test]
    fn handle_connection_requires_active() {
        let svc = ThreadedSocketService::new(5);
        // Service is stopped — must reject.
        assert!(!svc.handle_connection());
        assert_eq!(svc.active_count(), 0);
        assert_eq!(svc.total_processed(), 0);
    }

    #[test]
    fn handle_and_finish_connection() {
        let svc = ThreadedSocketService::new(3);
        svc.start();

        assert!(svc.handle_connection());
        assert!(svc.handle_connection());
        assert_eq!(svc.active_count(), 2);
        assert_eq!(svc.total_processed(), 2);

        svc.finish_connection();
        assert_eq!(svc.active_count(), 1);
        // processed is cumulative — should not decrease.
        assert_eq!(svc.total_processed(), 2);
    }

    #[test]
    fn pool_limit_enforced() {
        let svc = ThreadedSocketService::new(2);
        svc.start();

        assert!(svc.handle_connection()); // slot 1
        assert!(svc.handle_connection()); // slot 2
        assert!(!svc.handle_connection()); // pool full — rejected
        assert_eq!(svc.active_count(), 2);
        assert_eq!(svc.total_processed(), 2); // only 2 succeeded
    }

    #[test]
    fn finish_connection_saturates_at_zero() {
        let svc = ThreadedSocketService::new(5);
        // Unbalanced finish calls must not underflow.
        svc.finish_connection();
        svc.finish_connection();
        assert_eq!(svc.active_count(), 0);
    }

    #[test]
    fn slot_freed_after_finish_allows_new_connection() {
        let svc = ThreadedSocketService::new(1);
        svc.start();

        assert!(svc.handle_connection()); // slot taken
        assert!(!svc.handle_connection()); // pool full

        svc.finish_connection(); // slot released

        assert!(svc.handle_connection()); // should succeed now
        assert_eq!(svc.active_count(), 1);
        assert_eq!(svc.total_processed(), 2); // two successful dispatches total
    }

    #[test]
    fn stop_while_active_prevents_new_connections() {
        let svc = ThreadedSocketService::new(5);
        svc.start();
        assert!(svc.handle_connection());

        svc.stop();
        // Existing handlers are unaffected but new connections are rejected.
        assert!(!svc.handle_connection());
        assert_eq!(svc.active_count(), 1);
        assert_eq!(svc.total_processed(), 1);
    }
}
