//! GSimpleAsyncResult matching `gio/gsimpleasyncresult.h`.
//!
//! A simple `GAsyncResult` implementation. In this no_std port we model
//! it with a generic result value and error state.
//!
//! Fully `no_std` compatible using `alloc`.

use alloc::string::{String, ToString};
use spin::Mutex;

/// A simple asynchronous result (`GSimpleAsyncResult`).
pub struct SimpleAsyncResult {
    source_tag: Mutex<String>,
    error: Mutex<Option<String>>,
    done: Mutex<bool>,
    cancelled: Mutex<bool>,
}

impl SimpleAsyncResult {
    /// Creates a new simple async result.
    ///
    /// Mirrors `g_simple_async_result_new`.
    pub fn new(source_tag: &str) -> Self {
        Self {
            source_tag: Mutex::new(source_tag.to_string()),
            error: Mutex::new(None),
            done: Mutex::new(false),
            cancelled: Mutex::new(false),
        }
    }

    /// Gets the source tag.
    pub fn get_source_tag(&self) -> String {
        self.source_tag.lock().clone()
    }

    /// Sets an error.
    pub fn set_error(&self, error: &str) {
        *self.error.lock() = Some(error.to_string());
    }

    /// Gets the error, if any.
    pub fn get_error(&self) -> Option<String> {
        self.error.lock().clone()
    }

    /// Marks the result as complete.
    ///
    /// Mirrors `g_simple_async_result_complete`.
    pub fn complete(&self) {
        *self.done.lock() = true;
    }

    /// Checks if the result is complete.
    pub fn is_complete(&self) -> bool {
        *self.done.lock()
    }

    /// Marks the result as cancelled.
    pub fn set_cancelled(&self) {
        *self.cancelled.lock() = true;
        self.complete();
    }

    /// Checks if the result was cancelled.
    pub fn is_cancelled(&self) -> bool {
        *self.cancelled.lock()
    }

    /// Checks if the result has an error.
    pub fn had_error(&self) -> bool {
        self.error.lock().is_some()
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let r = SimpleAsyncResult::new("my-operation");
        assert_eq!(r.get_source_tag(), "my-operation");
        assert!(!r.is_complete());
        assert!(!r.is_cancelled());
    }

    #[test]
    fn test_complete() {
        let r = SimpleAsyncResult::new("op");
        r.complete();
        assert!(r.is_complete());
    }

    #[test]
    fn test_error() {
        let r = SimpleAsyncResult::new("op");
        r.set_error("something went wrong");
        assert!(r.had_error());
        assert_eq!(r.get_error(), Some("something went wrong".to_string()));
    }

    #[test]
    fn test_cancelled() {
        let r = SimpleAsyncResult::new("op");
        r.set_cancelled();
        assert!(r.is_cancelled());
        assert!(r.is_complete());
    }
}
