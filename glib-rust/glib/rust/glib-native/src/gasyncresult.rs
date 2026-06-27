//! GAsyncResult interface matching `gio/gasyncresult.h`.
//!
//! Upstream `GAsyncResult` is a `GInterface` for results of asynchronous
//! operations. We port it as a Rust trait.
//!
//! Fully `no_std` compatible.

/// Trait for asynchronous operation results (`GAsyncResult`).
///
/// Implemented by objects that represent the result of an asynchronous
/// operation.
pub trait AsyncResult {
    /// Gets the user data passed to the asynchronous callback.
    ///
    /// Mirrors `g_async_result_get_user_data`.
    fn get_user_data(&self) -> *mut core::ffi::c_void;

    /// Gets the source object that issued the asynchronous operation.
    ///
    /// Mirrors `g_async_result_get_source_object`.
    /// Returns a raw pointer since the source object type is unspecified.
    fn get_source_object(&self) -> *mut core::ffi::c_void;

    /// Checks if the result is tagged with the given source tag.
    ///
    /// Mirrors `g_async_result_is_tagged`.
    fn is_tagged(&self, source_tag: *mut core::ffi::c_void) -> bool;
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    struct TestAsyncResult {
        user_data: usize,
        source_tag: usize,
    }

    impl AsyncResult for TestAsyncResult {
        fn get_user_data(&self) -> *mut core::ffi::c_void {
            self.user_data as *mut core::ffi::c_void
        }
        fn get_source_object(&self) -> *mut core::ffi::c_void {
            core::ptr::null_mut()
        }
        fn is_tagged(&self, source_tag: *mut core::ffi::c_void) -> bool {
            source_tag == self.source_tag as *mut core::ffi::c_void
        }
    }

    #[test]
    fn test_async_result_basic() {
        let result = TestAsyncResult {
            user_data: 0x1000,
            source_tag: 0x2000,
        };
        assert_eq!(result.get_user_data() as usize, 0x1000);
        assert!(result.get_source_object().is_null());
        assert!(result.is_tagged(0x2000 as *mut core::ffi::c_void));
        assert!(!result.is_tagged(0x3000 as *mut core::ffi::c_void));
    }
}
