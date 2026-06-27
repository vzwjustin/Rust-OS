//! GAsyncInitable interface matching `gio/gasyncinitable.h`.
//!
//! Upstream `GAsyncInitable` is a `GInterface` for objects that can be
//! initialized asynchronously, where initialization may fail. We port
//! it as a Rust trait.
//!
//! Fully `no_std` compatible.

use crate::error::Error;
use crate::gcancellable::GCancellable;

/// Trait for asynchronously initializable objects (`GAsyncInitable`).
pub trait AsyncInitable {
    /// Starts asynchronous initialization of the object.
    ///
    /// Mirrors `g_async_initable_init_async`.
    fn init_async(&self, io_priority: i32, cancellable: Option<&GCancellable>);

    /// Finishes asynchronous initialization.
    ///
    /// Mirrors `g_async_initable_init_finish`.
    fn init_finish(&self) -> Result<(), Error>;
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    struct TestAsyncInitable {
        initialized: spin::Mutex<bool>,
        pending: spin::Mutex<bool>,
    }

    impl TestAsyncInitable {
        fn new() -> Self {
            Self {
                initialized: spin::Mutex::new(false),
                pending: spin::Mutex::new(false),
            }
        }
    }

    impl AsyncInitable for TestAsyncInitable {
        fn init_async(&self, _io_priority: i32, _cancellable: Option<&GCancellable>) {
            *self.pending.lock() = true;
            // Simulate immediate completion
            *self.pending.lock() = false;
            *self.initialized.lock() = true;
        }

        fn init_finish(&self) -> Result<(), Error> {
            if *self.initialized.lock() {
                Ok(())
            } else {
                Err(Error::new(
                    crate::gioerror::io_error_quark(),
                    crate::gioerror::IOErrorEnum::Failed.to_code(),
                    "Not yet initialized",
                ))
            }
        }
    }

    struct FailingAsyncInitable;

    impl AsyncInitable for FailingAsyncInitable {
        fn init_async(&self, _io_priority: i32, _cancellable: Option<&GCancellable>) {}

        fn init_finish(&self) -> Result<(), Error> {
            Err(Error::new(
                crate::gioerror::io_error_quark(),
                crate::gioerror::IOErrorEnum::Failed.to_code(),
                "Async initialization failed",
            ))
        }
    }

    #[test]
    fn test_async_initable_success() {
        let obj = TestAsyncInitable::new();
        assert!(!*obj.initialized.lock());
        obj.init_async(0, None);
        obj.init_finish().unwrap();
        assert!(*obj.initialized.lock());
    }

    #[test]
    fn test_async_initable_failure() {
        let obj = FailingAsyncInitable;
        obj.init_async(0, None);
        assert!(obj.init_finish().is_err());
    }
}
