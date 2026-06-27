//! GTask matching `gio/gtask.h`.
//!
//! `GTask` is the primary `GAsyncResult` implementation — a single-shot
//! async computation that stores a result or error and notifies a callback.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gasyncresult::AsyncResult;
use alloc::boxed::Box;
use alloc::string::String;
use core::any::Any;
use core::ffi::c_void;
use spin::Mutex;

/// The outcome stored inside a `Task`.
enum TaskOutcome {
    Pending,
    Value(Box<dyn Any + Send>),
    Error(Error),
}

/// A single-shot async task (`GTask`).
pub struct Task {
    name: Option<String>,
    outcome: Mutex<TaskOutcome>,
    source_tag: Option<*mut c_void>,
}

impl Task {
    /// Creates a new pending task.
    ///
    /// Mirrors `g_task_new`.
    pub fn new() -> Self {
        Self {
            name: None,
            outcome: Mutex::new(TaskOutcome::Pending),
            source_tag: None,
        }
    }

    /// Sets the task name for debugging.
    ///
    /// Mirrors `g_task_set_name`.
    pub fn set_name(&mut self, name: &str) {
        self.name = Some(name.into());
    }

    /// Gets the task name.
    pub fn get_name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Sets a source tag (pointer-sized identifier for the operation).
    ///
    /// Mirrors `g_task_set_source_tag`.
    pub fn set_source_tag(&mut self, tag: *mut c_void) {
        self.source_tag = Some(tag);
    }

    /// Gets the source tag.
    ///
    /// Mirrors `g_task_get_source_tag`.
    pub fn get_source_tag(&self) -> Option<*mut c_void> {
        self.source_tag
    }

    /// Stores a successful result.
    ///
    /// Mirrors `g_task_return_value`.
    pub fn return_value<T: Any + Send + 'static>(&self, value: T) {
        *self.outcome.lock() = TaskOutcome::Value(Box::new(value));
    }

    /// Stores an error result.
    ///
    /// Mirrors `g_task_return_error`.
    pub fn return_error(&self, error: Error) {
        *self.outcome.lock() = TaskOutcome::Error(error);
    }

    /// Returns true if the task has completed (success or error).
    ///
    /// Mirrors `g_task_had_error` (negated) / result availability.
    pub fn is_done(&self) -> bool {
        !matches!(*self.outcome.lock(), TaskOutcome::Pending)
    }

    /// Returns true if the task completed with an error.
    ///
    /// Mirrors `g_task_had_error`.
    pub fn had_error(&self) -> bool {
        matches!(*self.outcome.lock(), TaskOutcome::Error(_))
    }

    /// Propagates the result, returning `Ok(T)` or `Err(Error)`.
    ///
    /// Mirrors `g_task_propagate_value` / `g_task_propagate_pointer`.
    ///
    /// # Panics
    /// Panics if the task is still pending or the stored type doesn't match `T`.
    pub fn propagate_value<T: Any + Send + 'static>(&self) -> Result<T, Error> {
        let mut lock = self.outcome.lock();
        match &*lock {
            TaskOutcome::Pending => panic!("task not yet complete"),
            TaskOutcome::Error(_) => {
                let TaskOutcome::Error(e) = core::mem::replace(&mut *lock, TaskOutcome::Pending)
                else {
                    unreachable!()
                };
                Err(e)
            }
            TaskOutcome::Value(_) => {
                let TaskOutcome::Value(boxed) =
                    core::mem::replace(&mut *lock, TaskOutcome::Pending)
                else {
                    unreachable!()
                };
                Ok(*boxed
                    .downcast::<T>()
                    .expect("type mismatch in propagate_value"))
            }
        }
    }
}

impl Default for Task {
    fn default() -> Self {
        Self::new()
    }
}

impl AsyncResult for Task {
    fn get_user_data(&self) -> *mut c_void {
        self.source_tag.unwrap_or(core::ptr::null_mut())
    }

    fn get_source_object(&self) -> *mut c_void {
        core::ptr::null_mut()
    }

    fn is_tagged(&self, source_tag: *mut c_void) -> bool {
        self.source_tag == Some(source_tag)
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quark::quark_from_string;

    fn dummy_quark() -> u32 {
        quark_from_string(Some("gtask-test"))
    }

    #[test]
    fn test_task_new_is_pending() {
        let t = Task::new();
        assert!(!t.is_done());
        assert!(!t.had_error());
    }

    #[test]
    fn test_task_return_value() {
        let t = Task::new();
        t.return_value(42u32);
        assert!(t.is_done());
        assert!(!t.had_error());
        assert_eq!(t.propagate_value::<u32>().unwrap(), 42);
    }

    #[test]
    fn test_task_return_error() {
        let t = Task::new();
        t.return_error(Error::new(dummy_quark(), 1, "oops"));
        assert!(t.is_done());
        assert!(t.had_error());
        assert!(t.propagate_value::<u32>().is_err());
    }

    #[test]
    fn test_task_name() {
        let mut t = Task::new();
        assert!(t.get_name().is_none());
        t.set_name("read-file");
        assert_eq!(t.get_name(), Some("read-file"));
    }

    #[test]
    fn test_task_source_tag() {
        let mut t = Task::new();
        assert!(t.get_source_tag().is_none());
        let tag = 0xDEAD as *mut c_void;
        t.set_source_tag(tag);
        assert_eq!(t.get_source_tag(), Some(tag));
        assert!(t.is_tagged(tag));
        assert!(!t.is_tagged(core::ptr::null_mut()));
    }

    #[test]
    fn test_async_result_trait() {
        let mut t = Task::new();
        let tag = 7 as *mut c_void;
        t.set_source_tag(tag);
        assert_eq!(t.get_user_data(), tag);
        assert!(t.get_source_object().is_null());
    }

    #[test]
    fn test_default() {
        let t = Task::default();
        assert!(!t.is_done());
    }
}
