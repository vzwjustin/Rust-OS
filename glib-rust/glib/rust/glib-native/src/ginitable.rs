//! GInitable interface matching `gio/ginitable.h`.
//!
//! Upstream `GInitable` is a `GInterface` for objects that can be
//! initialized, where initialization may fail. We port it as a Rust trait.
//!
//! Fully `no_std` compatible.

use crate::error::Error;
use crate::gcancellable::GCancellable;

/// Trait for initializable objects (`GInitable`).
///
/// Objects implementing this trait can be initialized with a fallible
/// `init` method.
pub trait Initable {
    /// Initializes the object.
    ///
    /// Mirrors `g_initable_init`.
    fn init(&self, cancellable: Option<&GCancellable>) -> Result<(), Error>;
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    struct TestInitable {
        initialized: spin::Mutex<bool>,
    }

    impl TestInitable {
        fn new() -> Self {
            Self {
                initialized: spin::Mutex::new(false),
            }
        }
    }

    impl Initable for TestInitable {
        fn init(&self, _cancellable: Option<&GCancellable>) -> Result<(), Error> {
            *self.initialized.lock() = true;
            Ok(())
        }
    }

    struct FailingInitable;

    impl Initable for FailingInitable {
        fn init(&self, _cancellable: Option<&GCancellable>) -> Result<(), Error> {
            Err(Error::new(
                crate::gioerror::io_error_quark(),
                crate::gioerror::IOErrorEnum::Failed.to_code(),
                "Initialization failed",
            ))
        }
    }

    #[test]
    fn test_initable_success() {
        let obj = TestInitable::new();
        assert!(!*obj.initialized.lock());
        obj.init(None).unwrap();
        assert!(*obj.initialized.lock());
    }

    #[test]
    fn test_initable_failure() {
        let obj = FailingInitable;
        assert!(obj.init(None).is_err());
    }
}
