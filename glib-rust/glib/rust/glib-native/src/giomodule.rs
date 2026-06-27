//! GIOModule matching `gio/giomodule.h`.
//!
//! Loadable GIO extension modules with a ref-counted [`IoModule`] handle,
//! platform-specific open/symbol/close via [`IoModulePlatform`], and a
//! global `BTreeMap` registry. Mirrors the design of [`crate::gmodule`].
//!
//! Fully `no_std` compatible using `alloc` and `spin`.

use crate::gioerror::IOErrorEnum;
use crate::prelude::*;
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::ffi::c_void;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::{Mutex, RwLock};

/// Opaque handle to a platform-loaded I/O module.
pub type IoModuleHandle = *mut c_void;

/// Platform-specific dynamic loader for GIO modules.
///
/// Mirrors the `_g_io_module_*` helpers in upstream `giomodule.c`.
pub trait IoModulePlatform {
    /// Returns `true` if dynamic I/O module loading is supported.
    fn supported() -> bool;

    /// Open `path` and return a platform handle.
    fn open(path: &str) -> Result<IoModuleHandle, IOErrorEnum>;

    /// Look up `symbol_name` in `handle`.
    fn symbol(handle: IoModuleHandle, symbol_name: &str) -> Result<*mut c_void, IOErrorEnum>;

    /// Close a previously opened handle.
    fn close(handle: IoModuleHandle);

    /// Build a platform-decorated module path from `directory` and `module_name`.
    fn build_path(directory: Option<&str>, module_name: &str) -> String;
}

/// No-op platform for environments without a dynamic loader.
pub struct NoIoModulePlatform;

impl IoModulePlatform for NoIoModulePlatform {
    fn supported() -> bool {
        false
    }

    fn open(_path: &str) -> Result<IoModuleHandle, IOErrorEnum> {
        Err(IOErrorEnum::NotSupported)
    }

    fn symbol(_handle: IoModuleHandle, _symbol_name: &str) -> Result<*mut c_void, IOErrorEnum> {
        Err(IOErrorEnum::NotSupported)
    }

    fn close(_handle: IoModuleHandle) {}

    fn build_path(directory: Option<&str>, module_name: &str) -> String {
        let has_lib_prefix = module_name.starts_with("lib");
        let suffix = "so";
        match directory {
            Some(dir) if !dir.is_empty() => {
                if has_lib_prefix {
                    format!("{dir}/{module_name}")
                } else {
                    format!("{dir}/lib{module_name}.{suffix}")
                }
            }
            _ => {
                if has_lib_prefix {
                    module_name.to_owned()
                } else {
                    format!("lib{module_name}.{suffix}")
                }
            }
        }
    }
}

/// A dynamically loaded GIO module (`GIOModule`).
///
/// Ref-counted via [`Arc`]; the global registry keeps one entry per path.
pub struct IoModule {
    path: String,
    handle: Mutex<IoModuleHandle>,
    ref_count: AtomicU32,
}

impl IoModule {
    fn new(path: String, handle: IoModuleHandle) -> Arc<Self> {
        Arc::new(Self {
            path,
            handle: Mutex::new(handle),
            ref_count: AtomicU32::new(1),
        })
    }

    /// Returns the path this module was opened with.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the current reference count (diagnostics).
    pub fn ref_count(&self) -> u32 {
        self.ref_count.load(Ordering::SeqCst)
    }

    fn inc_ref(&self) -> u32 {
        self.ref_count.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn dec_ref(&self) -> u32 {
        self.ref_count.fetch_sub(1, Ordering::SeqCst) - 1
    }
}

impl core::fmt::Debug for IoModule {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("IoModule")
            .field("path", &self.path)
            .field("ref_count", &self.ref_count())
            .finish_non_exhaustive()
    }
}

// SAFETY: State is behind `Mutex`/`AtomicU32`; the opaque handle is only
// accessed through `IoModulePlatform`.
unsafe impl Send for IoModule {}
unsafe impl Sync for IoModule {}

// ────────────────────────────── registry ──────────────────────────────────

static MODULES: RwLock<BTreeMap<String, Arc<IoModule>>> = RwLock::new(BTreeMap::new());

/// Open a GIO module by path (`g_io_module_open`).
///
/// If the module is already open, its reference count is incremented.
pub fn io_module_open<P: IoModulePlatform>(path: &str) -> Result<Arc<IoModule>, IOErrorEnum> {
    if !P::supported() {
        return Err(IOErrorEnum::NotSupported);
    }

    if let Some(existing) = MODULES.read().get(path) {
        existing.inc_ref();
        return Ok(Arc::clone(existing));
    }

    let handle = P::open(path)?;
    let module = IoModule::new(path.to_owned(), handle);
    MODULES.write().insert(path.to_owned(), Arc::clone(&module));
    Ok(module)
}

/// Decrement a module's reference count and close it when zero
/// (`g_io_module_close`).
pub fn io_module_close<P: IoModulePlatform>(module: &Arc<IoModule>) -> Result<(), IOErrorEnum> {
    if !P::supported() {
        return Err(IOErrorEnum::NotSupported);
    }
    if module.ref_count() == 0 {
        return Err(IOErrorEnum::Failed);
    }

    let new_count = module.dec_ref();
    if new_count == 0 {
        let handle = *module.handle.lock();
        MODULES.write().remove(&module.path);
        P::close(handle);
    }
    Ok(())
}

/// Look up `symbol_name` in `module` (`g_io_module_symbol`).
pub fn io_module_symbol<P: IoModulePlatform>(
    module: &Arc<IoModule>,
    symbol_name: &str,
) -> Result<*mut c_void, IOErrorEnum> {
    if !P::supported() {
        return Err(IOErrorEnum::NotSupported);
    }
    let handle = *module.handle.lock();
    P::symbol(handle, symbol_name)
}

/// Build a platform-decorated module path (`g_io_module_build_path`).
pub fn io_module_build_path<P: IoModulePlatform>(
    directory: Option<&str>,
    module_name: &str,
) -> String {
    P::build_path(directory, module_name)
}

/// Returns the number of currently registered modules (diagnostics / tests).
pub fn io_module_registry_len() -> usize {
    MODULES.read().len()
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

    struct MockIoModulePlatform {
        _private: (),
    }

    static MOCK_OPEN_COUNT: AtomicU32 = AtomicU32::new(0);
    const MOCK_HANDLE: IoModuleHandle = 0xDEAD_BEEF_usize as IoModuleHandle;

    impl IoModulePlatform for MockIoModulePlatform {
        fn supported() -> bool {
            true
        }

        fn open(path: &str) -> Result<IoModuleHandle, IOErrorEnum> {
            if path.contains("missing") {
                return Err(IOErrorEnum::NotFound);
            }
            MOCK_OPEN_COUNT.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(MOCK_HANDLE)
        }

        fn symbol(_handle: IoModuleHandle, symbol_name: &str) -> Result<*mut c_void, IOErrorEnum> {
            if symbol_name == "g_io_extension_point_implement" {
                Ok(0x1000 as *mut c_void)
            } else {
                Err(IOErrorEnum::NotFound)
            }
        }

        fn close(_handle: IoModuleHandle) {}

        fn build_path(directory: Option<&str>, module_name: &str) -> String {
            NoIoModulePlatform::build_path(directory, module_name)
        }
    }

    #[test]
    fn test_no_platform_not_supported() {
        assert!(!NoIoModulePlatform::supported());
        assert!(io_module_open::<NoIoModulePlatform>("/lib/gio/libtest.so").is_err());
    }

    #[test]
    fn test_build_path() {
        assert_eq!(
            io_module_build_path::<NoIoModulePlatform>(Some("/usr/lib/gio/modules"), "giognutls"),
            "/usr/lib/gio/modules/libgiognutls.so"
        );
        assert_eq!(
            io_module_build_path::<NoIoModulePlatform>(None, "libgiotls"),
            "libgiotls"
        );
    }

    #[test]
    fn test_open_close_and_reopen() {
        MOCK_OPEN_COUNT.store(0, AtomicOrdering::SeqCst);
        let path = "/usr/lib/gio/modules/libgio-test.so";

        let m1 = io_module_open::<MockIoModulePlatform>(path).unwrap();
        assert_eq!(m1.path(), path);
        assert_eq!(m1.ref_count(), 1);
        assert_eq!(MOCK_OPEN_COUNT.load(AtomicOrdering::SeqCst), 1);

        let m2 = io_module_open::<MockIoModulePlatform>(path).unwrap();
        assert_eq!(m2.ref_count(), 2);
        assert_eq!(MOCK_OPEN_COUNT.load(AtomicOrdering::SeqCst), 1);

        io_module_close::<MockIoModulePlatform>(&m1).unwrap();
        assert_eq!(m2.ref_count(), 1);
        io_module_close::<MockIoModulePlatform>(&m2).unwrap();
        assert_eq!(io_module_registry_len(), 0);
    }

    #[test]
    fn test_symbol_lookup() {
        let path = "/usr/lib/gio/modules/libgio-symbol.so";
        let module = io_module_open::<MockIoModulePlatform>(path).unwrap();
        let sym =
            io_module_symbol::<MockIoModulePlatform>(&module, "g_io_extension_point_implement")
                .unwrap();
        assert!(!sym.is_null());
        io_module_close::<MockIoModulePlatform>(&module).unwrap();
    }

    #[test]
    fn test_open_missing() {
        let res = io_module_open::<MockIoModulePlatform>("/missing/module.so");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err(), IOErrorEnum::NotFound);
    }
}
