//! Dynamic module loading matching `gmodule.h` / `gmodule.c`.
//!
//! Provides `GModule` — a reference-counted handle to a dynamically loaded
//! module — plus the `ModulePlatform` trait that supplies the OS-specific
//! `dlopen`/`dlsym`/`dlclose` primitives. On bare metal (no OS dynamic
//! loader) the `NoModulePlatform` stub returns `Unsupported` for every
//! operation, but the registry, ref-counting, error string, resident
//! marking, and `module_build_path` logic all behave like upstream GLib.
//!
//! Fully `no_std` compatible using `alloc` and `spin`.

use crate::prelude::*;
use crate::quark::quark_from_string;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ffi::{c_char, c_void};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::mutex::Mutex;
use spin::rwlock::RwLock;

// ───────────────────────────── flags / errors ─────────────────────────────

/// Module bind flags (`GModuleFlags`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct GModuleFlags(pub u32);

impl GModuleFlags {
    /// Bind symbols lazily — only when first referenced (`G_MODULE_BIND_LAZY`).
    pub const BIND_LAZY: Self = Self(1 << 0);
    /// Bind symbols locally — do not add to the global namespace
    /// (`G_MODULE_BIND_LOCAL`).
    pub const BIND_LOCAL: Self = Self(1 << 1);
    /// Mask covering all valid flag bits (`G_MODULE_BIND_MASK`).
    pub const BIND_MASK: Self = Self(0x03);
    /// Empty flags — bind eagerly and globally.
    pub const NONE: Self = Self(0);

    /// Returns `true` if `other` is set in `self`.
    pub fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
}

impl core::ops::BitOr for GModuleFlags {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        Self(self.0 | rhs.0)
    }
}

/// Module error codes (`GModuleError`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GModuleError {
    /// Generic load/open failure (`G_MODULE_ERROR_FAILED`).
    Failed,
    /// Module's `g_module_check_init` returned an error string
    /// (`G_MODULE_ERROR_CHECK_FAILED`).
    CheckFailed,
}

impl GModuleError {
    /// Numeric error code matching the upstream enum order.
    pub fn to_code(self) -> i32 {
        match self {
            GModuleError::Failed => 0,
            GModuleError::CheckFailed => 1,
        }
    }
}

// ───────────────────────────── callbacks ──────────────────────────────────

/// Module initialization function (`GModuleCheckInit`).
///
/// If a module exports a symbol named `g_module_check_init` it is called
/// automatically when the module is loaded. Returns `None` on success or
/// `Some(error_string)` describing the failure.
pub type GModuleCheckInit = fn(module: &GModule) -> Option<String>;

/// Module unload function (`GModuleUnload`).
///
/// If a module exports a symbol named `g_module_unload` it is called when
/// the module's ref count drops to zero (and it is not resident).
pub type GModuleUnload = fn(module: &GModule);

// ─────────────────────────── platform trait ───────────────────────────────

/// Opaque handle to a platform-loaded module (e.g. `void *` from `dlopen`).
pub type ModuleHandle = *mut c_void;

/// Platform-specific dynamic loader (`dlopen`/`dlsym`/`dlclose` family).
///
/// Implementations map the requested operation onto the host OS's dynamic
/// linker. The trait methods correspond to the static `_g_module_*` helpers
/// in `gmodule.c`; the cross-platform registry logic in `GModule` calls
/// them.
pub trait ModulePlatform {
    /// Returns `true` if dynamic module loading is supported on this
    /// platform (`g_module_supported`).
    fn supported() -> bool;

    /// Open `file_name` and return a platform handle
    /// (`_g_module_open`). `bind_lazy` and `bind_local` correspond to
    /// `RTLD_LAZY` and `RTLD_LOCAL`. On error returns a description
    /// string.
    fn open(file_name: &str, bind_lazy: bool, bind_local: bool) -> Result<ModuleHandle, String>;

    /// Return a handle to the main program itself (`_g_module_self`).
    /// Returns `Err` if not supported.
    fn self_handle() -> Result<ModuleHandle, String>;

    /// Look up `symbol_name` in `handle` (`_g_module_symbol`).
    /// Returns `Ok(ptr)` on success, `Err(description)` on failure.
    fn symbol(handle: ModuleHandle, symbol_name: &str) -> Result<*mut c_void, String>;

    /// Close a previously opened handle (`_g_module_close`).
    fn close(handle: ModuleHandle);

    /// Build a platform-decorated path from a directory and module name
    /// (`_g_module_build_path`). e.g. `/lib` + `mylib` -> `/lib/libmylib.so`
    /// on Linux, `\Windows\mylib.dll` on Windows.
    fn build_path(directory: Option<&str>, module_name: &str) -> String;
}

/// No-op platform implementation for environments without a dynamic loader.
///
/// `supported()` returns `false` and every operation returns an error.
/// Useful on bare-metal kernels (RustOS) so the GLib API surface is
/// linkable even when actual module loading is unavailable.
pub struct NoModulePlatform;

impl ModulePlatform for NoModulePlatform {
    fn supported() -> bool {
        false
    }

    fn open(_file_name: &str, _bind_lazy: bool, _bind_local: bool) -> Result<ModuleHandle, String> {
        Err("dynamic modules are not supported by this system".to_owned())
    }

    fn self_handle() -> Result<ModuleHandle, String> {
        Err("dynamic modules are not supported by this system".to_owned())
    }

    fn symbol(_handle: ModuleHandle, _symbol_name: &str) -> Result<*mut c_void, String> {
        Err("dynamic modules are not supported by this system".to_owned())
    }

    fn close(_handle: ModuleHandle) {}

    fn build_path(directory: Option<&str>, module_name: &str) -> String {
        // Mirror the Linux branch of `_g_module_build_path`.
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

// ─────────────────────── libtool archive parser ─────────────────────────

/// Parsed fields from a libtool `.la` archive file.
///
/// Mirrors the subset of keys read by `parse_libtool_archive` in
/// `gmodule.c` (`dlname`, `libdir`, `installed`) plus the commonly
/// present `library_names` and `dependency_libs` entries.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct LibtoolArchive {
    /// Shared library file name (`dlname='…'`).
    pub dlname: Option<String>,
    /// Space-separated versioned names (`library_names='…'`).
    pub library_names: Option<String>,
    /// Linker flags for dependent libraries (`dependency_libs='…'`).
    pub dependency_libs: Option<String>,
    libdir: Option<String>,
    installed: bool,
}

impl LibtoolArchive {
    /// Build the filesystem path to the shared object described by this
    /// archive, mirroring upstream's `parse_libtool_archive` result.
    pub fn resolve_dlname_path(&self, libtool_path: Option<&str>) -> Result<String, String> {
        let mut libdir = self.libdir.clone();
        if !self.installed {
            let base = libtool_path
                .ok_or_else(|| "installed=no requires libtool archive path".to_owned())?;
            let dir = base
                .rsplit_once('/')
                .or_else(|| base.rsplit_once('\\'))
                .map(|(d, _)| d)
                .unwrap_or(".");
            libdir = Some(format!("{dir}/.libs"));
        }

        let libdir = libdir.ok_or_else(|| "missing libdir in libtool archive".to_owned())?;
        let dlname = self
            .dlname
            .as_ref()
            .ok_or_else(|| "missing dlname in libtool archive".to_owned())?;

        let sep = if libdir.contains('\\') { '\\' } else { '/' };
        if libdir.ends_with(sep) {
            Ok(format!("{libdir}{dlname}"))
        } else {
            Ok(format!("{libdir}{sep}{dlname}"))
        }
    }
}

/// Parse a libtool `.la` file from its text content (pure string parsing).
///
/// `libtool_path` is only required when `installed=no` so the `.libs`
/// sibling directory can be resolved.
pub fn parse_libtool_archive(
    content: &str,
    libtool_path: Option<&str>,
) -> Result<LibtoolArchive, String> {
    let mut archive = LibtoolArchive {
        installed: true,
        ..LibtoolArchive::default()
    };

    for line in content.lines() {
        let line = line.split('#').next().unwrap_or(line).trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, raw_value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = parse_libtool_quoted_value(raw_value.trim())?;

        match key {
            "dlname" => archive.dlname = Some(value),
            "library_names" => archive.library_names = Some(value),
            "dependency_libs" => archive.dependency_libs = Some(value),
            "libdir" => archive.libdir = Some(value),
            "installed" => archive.installed = value == "yes",
            _ => {}
        }
    }

    if archive.dlname.is_none() {
        return Err("missing dlname in libtool archive".to_owned());
    }

    // Validate `installed=no` can resolve before returning success.
    if !archive.installed && libtool_path.is_none() {
        return Err("installed=no requires libtool archive path".to_owned());
    }

    Ok(archive)
}

fn parse_libtool_quoted_value(raw: &str) -> Result<String, String> {
    if raw.is_empty() {
        return Ok(String::new());
    }
    if raw.starts_with('\'') {
        let end = raw
            .rfind('\'')
            .filter(|idx| *idx > 0)
            .ok_or_else(|| format!("unterminated quoted value: {raw}"))?;
        Ok(raw[1..end].to_owned())
    } else {
        Ok(raw.to_owned())
    }
}

// ───────────────────── host platform (tests only) ─────────────────────────

/// Dynamic loader backed by the host OS (`dlopen` / `LoadLibrary`).
///
/// Only compiled for `cargo test` on Linux, macOS, or Windows. Bare-metal
/// / kernel builds keep [`NoModulePlatform`] as the default.
#[cfg(all(
    test,
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
pub struct HostModulePlatform;

#[cfg(all(
    test,
    any(target_os = "linux", target_os = "macos", target_os = "windows")
))]
mod host_platform {
    use super::{HostModulePlatform, ModuleHandle, ModulePlatform};
    use alloc::string::String;
    use core::ffi::c_void;

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    mod dl {
        use core::ffi::{c_char, c_void};

        #[cfg(target_os = "linux")]
        pub(super) const RTLD_LAZY: i32 = 0x1;
        #[cfg(target_os = "linux")]
        pub(super) const RTLD_NOW: i32 = 0x2;
        #[cfg(target_os = "linux")]
        pub(super) const RTLD_LOCAL: i32 = 0;
        #[cfg(target_os = "linux")]
        pub(super) const RTLD_GLOBAL: i32 = 0x100;

        #[cfg(target_os = "macos")]
        pub(super) const RTLD_LAZY: i32 = 0x1;
        #[cfg(target_os = "macos")]
        pub(super) const RTLD_NOW: i32 = 0x2;
        #[cfg(target_os = "macos")]
        pub(super) const RTLD_LOCAL: i32 = 0x4;
        #[cfg(target_os = "macos")]
        pub(super) const RTLD_GLOBAL: i32 = 0x8;

        extern "C" {
            pub fn dlopen(filename: *const c_char, flag: i32) -> *mut c_void;
            pub fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
            pub fn dlclose(handle: *mut c_void) -> i32;
            pub fn dlerror() -> *const c_char;
        }

        pub(super) fn dl_error_message() -> String {
            // SAFETY: `dlerror` returns a thread-local C string or NULL.
            let ptr = unsafe { dlerror() };
            if ptr.is_null() {
                "unknown dl-error".to_owned()
            } else {
                c_str_to_string(ptr)
            }
        }

        pub(super) fn c_str_to_string(ptr: *const c_char) -> String {
            let mut bytes = alloc::vec::Vec::new();
            let mut offset = 0usize;
            loop {
                // SAFETY: reading a NUL-terminated C string.
                let byte = unsafe { *ptr.add(offset) as u8 };
                if byte == 0 {
                    break;
                }
                bytes.push(byte);
                offset += 1;
                if offset > 4096 {
                    break;
                }
            }
            String::from_utf8_lossy(&bytes).into_owned()
        }
    }

    #[cfg(target_os = "windows")]
    mod dl {
        use core::ffi::{c_char, c_void};

        type HMODULE = *mut c_void;
        type DWORD = u32;

        extern "system" {
            fn LoadLibraryA(lp_lib_file_name: *const c_char) -> HMODULE;
            fn GetProcAddress(h_module: HMODULE, lp_proc_name: *const c_char) -> *mut c_void;
            fn FreeLibrary(h_module: HMODULE) -> i32;
            fn GetModuleHandleA(lp_module_name: *const c_char) -> HMODULE;
        }

        pub(super) fn open(
            path: &str,
            _bind_lazy: bool,
            _bind_local: bool,
        ) -> Result<*mut c_void, String> {
            let c_path =
                std::ffi::CString::new(path).map_err(|_| "path contains NUL".to_owned())?;
            // SAFETY: `c_path` is NUL-terminated.
            let handle = unsafe { LoadLibraryA(c_path.as_ptr()) };
            if handle.is_null() {
                return Err(format!("failed to load library '{path}'"));
            }
            Ok(handle)
        }

        pub(super) fn self_handle() -> Result<*mut c_void, String> {
            // SAFETY: NULL requests the executable module.
            let handle = unsafe { GetModuleHandleA(core::ptr::null()) };
            if handle.is_null() {
                return Err("GetModuleHandleA(NULL) failed".to_owned());
            }
            Ok(handle)
        }

        pub(super) fn symbol(handle: *mut c_void, name: &str) -> Result<*mut c_void, String> {
            let c_name =
                std::ffi::CString::new(name).map_err(|_| "symbol contains NUL".to_owned())?;
            // SAFETY: valid module handle from open/self_handle.
            let ptr = unsafe { GetProcAddress(handle, c_name.as_ptr()) };
            if ptr.is_null() {
                return Err(format!("symbol '{name}' not found"));
            }
            Ok(ptr)
        }

        pub(super) fn close(handle: *mut c_void) {
            if !handle.is_null() {
                // SAFETY: handle came from LoadLibraryA.
                unsafe {
                    let _ = FreeLibrary(handle);
                }
            }
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    impl HostModulePlatform {
        fn open_flags(bind_lazy: bool, bind_local: bool) -> i32 {
            let mode = if bind_lazy {
                dl::RTLD_LAZY
            } else {
                dl::RTLD_NOW
            };
            let scope = if bind_local {
                dl::RTLD_LOCAL
            } else {
                dl::RTLD_GLOBAL
            };
            mode | scope
        }
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    impl ModulePlatform for HostModulePlatform {
        fn supported() -> bool {
            true
        }

        fn open(
            file_name: &str,
            bind_lazy: bool,
            bind_local: bool,
        ) -> Result<ModuleHandle, String> {
            let c_path =
                std::ffi::CString::new(file_name).map_err(|_| "path contains NUL".to_owned())?;
            // SAFETY: `c_path` is NUL-terminated; flags match GLib's dlopen usage.
            let handle =
                unsafe { dl::dlopen(c_path.as_ptr(), Self::open_flags(bind_lazy, bind_local)) };
            if handle.is_null() {
                return Err(dl::dl_error_message());
            }
            Ok(handle)
        }

        fn self_handle() -> Result<ModuleHandle, String> {
            // SAFETY: NULL filename opens the main program, matching gmodule-dl.c.
            let handle = unsafe { dl::dlopen(core::ptr::null(), dl::RTLD_GLOBAL | dl::RTLD_LAZY) };
            if handle.is_null() {
                return Err(dl::dl_error_message());
            }
            Ok(handle)
        }

        fn symbol(handle: ModuleHandle, symbol_name: &str) -> Result<*mut c_void, String> {
            let c_name = std::ffi::CString::new(symbol_name)
                .map_err(|_| "symbol contains NUL".to_owned())?;
            // SAFETY: valid handle; clear stale dlerror state first like upstream.
            unsafe {
                let _ = dl::dlerror();
                let ptr = dl::dlsym(handle, c_name.as_ptr());
                let err = dl::dlerror();
                if !err.is_null() {
                    return Err(dl::c_str_to_string(err));
                }
                Ok(ptr)
            }
        }

        fn close(handle: ModuleHandle) {
            if !handle.is_null() {
                // SAFETY: handle came from dlopen.
                unsafe {
                    let _ = dl::dlclose(handle);
                }
            }
        }

        fn build_path(directory: Option<&str>, module_name: &str) -> String {
            let has_lib_prefix = module_name.starts_with("lib");
            #[cfg(target_os = "macos")]
            let suffix = "dylib";
            #[cfg(not(target_os = "macos"))]
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

    #[cfg(target_os = "windows")]
    impl ModulePlatform for HostModulePlatform {
        fn supported() -> bool {
            true
        }

        fn open(
            file_name: &str,
            bind_lazy: bool,
            bind_local: bool,
        ) -> Result<ModuleHandle, String> {
            let _ = (bind_lazy, bind_local);
            dl::open(file_name, bind_lazy, bind_local)
        }

        fn self_handle() -> Result<ModuleHandle, String> {
            dl::self_handle()
        }

        fn symbol(handle: ModuleHandle, symbol_name: &str) -> Result<*mut c_void, String> {
            dl::symbol(handle, symbol_name)
        }

        fn close(handle: ModuleHandle) {
            dl::close(handle)
        }

        fn build_path(directory: Option<&str>, module_name: &str) -> String {
            match directory {
                Some(dir) if !dir.is_empty() => format!("{dir}\\{module_name}.dll"),
                _ => format!("{module_name}.dll"),
            }
        }
    }
}

/// Reset module registry state and enable host-platform tests.
///
/// Call from `#[cfg(test)]` setup before exercising [`HostModulePlatform`].
#[cfg(test)]
pub fn register_host_module_platform_for_tests() {
    reset_module_registry_for_tests();
}

#[cfg(test)]
fn reset_module_registry_for_tests() {
    MODULES.write().clear();
    *MAIN_MODULE.lock() = None;
    *LAST_ERROR.lock() = None;
}

// ───────────────────────────── GModule ────────────────────────────────────

/// A dynamically loaded module (`GModule`).
///
/// Mirrors `struct _GModule` in `gmodule.c`: a ref-counted handle with the
/// originating file name, platform handle, resident flag, and optional
/// unload callback. The kernel-side registry keeps a singly-linked list of
/// opened modules so multiple `module_open` calls for the same path return
/// the same handle (with an incremented ref count).
pub struct GModule {
    file_name: Mutex<Option<String>>,
    handle: Mutex<ModuleHandle>,
    ref_count: AtomicU32,
    is_resident: AtomicBool,
    /// Looked up from the module via `g_module_unload` symbol, if present.
    /// Stored as a raw `fn` pointer rather than `GModuleUnload` so the
    /// `GModule` can be `Send + Sync` (function pointers are `Send + Sync`).
    unload: Mutex<Option<unsafe extern "C" fn(*mut GModule)>>,
    /// Linked-list pointer to the next module in the global registry.
    /// Owned by the registry's `RwLock<Vec<Arc<GModule>>>`.
    next: Mutex<Option<Arc<GModule>>>,
}

// SAFETY: `GModule` owns its state behind `Mutex`/`Atomic*` guards. The
// platform handle is an opaque `*mut c_void` that is only manipulated
// through the `ModulePlatform` trait, whose implementations are
// responsible for thread-safe access. The unload function pointer is a
// plain `fn` pointer (inherently `Send + Sync`). The next pointer is
// behind a `Mutex`. Sharing `Arc<GModule>` across threads is safe.
unsafe impl Send for GModule {}
unsafe impl Sync for GModule {}

impl GModule {
    /// Construct a new `GModule` wrapping `handle` with ref count 1.
    ///
    /// Public for testability — production code should go through
    /// `module_open` / `module_open_full` so the registry and
    /// `g_module_check_init` are handled. This constructor does **not**
    /// insert the module into the global registry and does **not** look
    /// up `g_module_check_init` or `g_module_unload` symbols.
    pub fn new(file_name: Option<String>, handle: ModuleHandle) -> Arc<Self> {
        Arc::new(Self {
            file_name: Mutex::new(file_name),
            handle: Mutex::new(handle),
            ref_count: AtomicU32::new(1),
            is_resident: AtomicBool::new(false),
            unload: Mutex::new(None),
            next: Mutex::new(None),
        })
    }

    /// File name the module was opened with (`g_module_name`).
    ///
    /// Returns `"main"` for the main-program pseudo-module.
    pub fn name(&self) -> String {
        let name = self.file_name.lock().clone();
        match name {
            Some(s) => s,
            None => "main".to_owned(),
        }
    }

    /// Reference count (for diagnostics / smoke checks).
    pub fn ref_count(&self) -> u32 {
        self.ref_count.load(Ordering::SeqCst)
    }

    /// Whether `g_module_make_resident` has been called on this module.
    pub fn is_resident(&self) -> bool {
        self.is_resident.load(Ordering::SeqCst)
    }

    /// Mark a module permanently resident so `module_close` becomes a no-op
    /// (`g_module_make_resident`).
    pub fn make_resident(&self) {
        self.is_resident.store(true, Ordering::SeqCst);
    }

    fn inc_ref(&self) -> u32 {
        self.ref_count.fetch_add(1, Ordering::SeqCst) + 1
    }

    fn dec_ref(&self) -> u32 {
        self.ref_count.fetch_sub(1, Ordering::SeqCst) - 1
    }
}

impl core::fmt::Debug for GModule {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("GModule")
            .field("name", &self.name())
            .field("ref_count", &self.ref_count())
            .field("is_resident", &self.is_resident())
            .finish_non_exhaustive()
    }
}

// ────────────────────────────── registry ──────────────────────────────────

/// Global registry of currently-open modules plus the singleton
/// main-program module. Mirrors the `static GModule *modules` and
/// `static GModule *main_module` in `gmodule.c`.
static MODULES: RwLock<Vec<Arc<GModule>>> = RwLock::new(Vec::new());
static MAIN_MODULE: Mutex<Option<Arc<GModule>>> = Mutex::new(None);

/// Last module error string. Upstream uses `GPrivate` (thread-local); in
/// `no_std` we use a single global slot guarded by a `Mutex`. The kernel
/// is single-threaded at boot so this matches upstream behaviour in
/// practice.
static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

const CHECK_INIT_ERROR_MAX: usize = 4096;

unsafe fn c_char_ptr_to_string(ptr: *const c_char) -> String {
    let mut bytes = Vec::new();
    let mut offset = 0usize;

    while offset < CHECK_INIT_ERROR_MAX {
        // SAFETY: caller provides a C string pointer returned by module init.
        let byte = unsafe { *ptr.add(offset) as u8 };
        if byte == 0 {
            break;
        }
        bytes.push(byte);
        offset += 1;
    }

    String::from_utf8_lossy(&bytes).into_owned()
}

fn set_error(msg: Option<String>) {
    *LAST_ERROR.lock() = msg;
}

fn find_by_handle(handle: ModuleHandle) -> Option<Arc<GModule>> {
    if let Some(main) = MAIN_MODULE.lock().as_ref() {
        if *main.handle.lock() == handle {
            return Some(Arc::clone(main));
        }
    }
    for m in MODULES.read().iter() {
        if *m.handle.lock() == handle {
            return Some(Arc::clone(m));
        }
    }
    None
}

fn find_by_name(name: &str) -> Option<Arc<GModule>> {
    for m in MODULES.read().iter() {
        if let Some(fname) = m.file_name.lock().as_ref() {
            if fname == name {
                return Some(Arc::clone(m));
            }
        }
    }
    None
}

/// Look up `g_module_check_init` and `g_module_unload` in the freshly
/// opened module, run check_init, and store unload on success. Mirrors the
/// post-`_g_module_open` block in `g_module_open_full`.
fn post_open_init<P: ModulePlatform>(module: &Arc<GModule>) -> Result<(), (GModuleError, String)> {
    // SAFETY: We just opened `module`'s handle via `P::open` and no other
    // code has observed it yet. The lookups go through the platform's
    // `symbol` which is responsible for any required synchronization.
    let handle = *module.handle.lock();

    // Look up g_module_check_init (typed as GModuleCheckInit in gmodule.h).
    match P::symbol(handle, "g_module_check_init") {
        Ok(ptr) if !ptr.is_null() => {
            // SAFETY: Caller asserts the symbol has C ABI `fn(*mut GModule)
            // -> *mut c_char`. We treat the returned string as the failure
            // description; NULL means success.
            let check_init: unsafe extern "C" fn(*mut GModule) -> *mut c_char =
                unsafe { core::mem::transmute(ptr) };
            // SAFETY: `module` is a valid `Arc<GModule>`; the C contract is
            // that the check_init function reads only `module->name` (which
            // we provide) and returns either NULL or a freshly allocated
            // string for the caller to free. We do not free because our
            // platform stubs do not actually load modules.
            let err_ptr = unsafe { check_init(Arc::as_ptr(module) as *mut GModule) };
            if !err_ptr.is_null() {
                let err_str = unsafe { c_char_ptr_to_string(err_ptr) };
                return Err((GModuleError::CheckFailed, err_str));
            }
        }
        _ => {}
    }

    // Look up g_module_unload. If present, store it on the module so
    // `module_close` can invoke it later.
    match P::symbol(handle, "g_module_unload") {
        Ok(ptr) if !ptr.is_null() => {
            // SAFETY: Caller asserts C ABI `fn(*mut GModule)`.
            let unload: unsafe extern "C" fn(*mut GModule) = unsafe { core::mem::transmute(ptr) };
            *module.unload.lock() = Some(unload);
        }
        _ => {}
    }

    Ok(())
}

/// Last error string (`g_module_error`).
///
/// Returns `None` if no error is recorded.
pub fn module_error() -> Option<String> {
    LAST_ERROR.lock().clone()
}

/// Error quark for the GModule error domain (`g_module_error_quark`).
pub fn module_error_quark() -> u32 {
    quark_from_string(Some("g-module-error-quark"))
}

/// Whether dynamic module loading is supported on this platform
/// (`g_module_supported`).
pub fn module_supported<P: ModulePlatform>() -> bool {
    P::supported()
}

/// Open a module by file name with the given flags (`g_module_open_full`).
///
/// Mirrors the upstream lookup order:
/// 1. If `file_name` is `None`, return (or create) the main-program module.
/// 2. If a module with this name is already open, bump its ref count.
/// 3. Otherwise call `P::open` and insert the result into the registry.
///
/// On success the new module's ref count is `1`; on a re-open of an
/// existing path it is incremented.
pub fn module_open_full<P: ModulePlatform>(
    file_name: Option<&str>,
    flags: GModuleFlags,
) -> Result<Arc<GModule>, (GModuleError, String)> {
    if !P::supported() {
        set_error(Some(
            "dynamic modules are not supported by this system".to_owned(),
        ));
        return Err((GModuleError::Failed, module_error().unwrap_or_default()));
    }

    // Main program pseudo-module.
    if file_name.is_none() {
        let mut main = MAIN_MODULE.lock();
        if let Some(existing) = main.as_ref() {
            existing.inc_ref();
            set_error(None);
            return Ok(Arc::clone(existing));
        }
        match P::self_handle() {
            Ok(handle) => {
                let module = GModule::new(None, handle);
                module.make_resident();
                *main = Some(Arc::clone(&module));
                set_error(None);
                Ok(module)
            }
            Err(e) => {
                set_error(Some(e.clone()));
                Err((GModuleError::Failed, e))
            }
        }
    } else {
        let name = file_name.unwrap();
        // Re-open of an existing module bumps the ref count.
        if let Some(existing) = find_by_name(name) {
            existing.inc_ref();
            set_error(None);
            return Ok(existing);
        }

        let bind_lazy = flags.contains(GModuleFlags::BIND_LAZY);
        let bind_local = flags.contains(GModuleFlags::BIND_LOCAL);
        match P::open(name, bind_lazy, bind_local) {
            Ok(handle) => {
                // If the same handle is already in the registry (some
                // platforms dedup), close the duplicate and bump the
                // existing ref count.
                if let Some(existing) = find_by_handle(handle) {
                    P::close(handle);
                    existing.inc_ref();
                    set_error(None);
                    return Ok(existing);
                }

                let module = GModule::new(Some(name.to_owned()), handle);
                match post_open_init::<P>(&module) {
                    Ok(()) => {
                        MODULES.write().push(Arc::clone(&module));
                        set_error(None);
                        Ok(module)
                    }
                    Err((code, msg)) => {
                        // Drop the half-constructed module: close its
                        // handle and discard the Arc.
                        P::close(*module.handle.lock());
                        let full_msg =
                            format!("GModule ({name}) initialization check failed: {msg}");
                        set_error(Some(full_msg.clone()));
                        Err((code, full_msg))
                    }
                }
            }
            Err(e) => {
                set_error(Some(e.clone()));
                Err((GModuleError::Failed, e))
            }
        }
    }
}

/// Thin wrapper around `module_open_full` matching `g_module_open`.
pub fn module_open<P: ModulePlatform>(
    file_name: Option<&str>,
    flags: GModuleFlags,
) -> Result<Arc<GModule>, (GModuleError, String)> {
    module_open_full::<P>(file_name, flags)
}

/// Decrement `module`'s ref count, optionally invoking its `g_module_unload`
/// and removing it from the registry when the count hits zero
/// (`g_module_close`).
///
/// Returns `Ok(())` if no error was recorded, `Err` otherwise.
pub fn module_close<P: ModulePlatform>(
    module: &Arc<GModule>,
) -> Result<(), (GModuleError, String)> {
    if !P::supported() {
        set_error(Some(
            "dynamic modules are not supported by this system".to_owned(),
        ));
        return Err((GModuleError::Failed, module_error().unwrap_or_default()));
    }
    if module.ref_count() == 0 {
        set_error(Some("module already closed".to_owned()));
        return Err((GModuleError::Failed, module_error().unwrap_or_default()));
    }

    let new_count = module.dec_ref();
    // Invoke unload if ref count reached zero and module is not resident.
    if new_count == 0 && !module.is_resident() {
        let unload = module.unload.lock().take();
        if let Some(unload_fn) = unload {
            // SAFETY: Caller asserts the function pointer came from a
            // `g_module_unload` symbol exported by the loaded module.
            // We hold an `Arc<GModule>` so the pointer is valid for the
            // duration of the call.
            unsafe { unload_fn(Arc::as_ptr(module) as *mut GModule) };
        }
        // Remove from the global registry.
        let handle = *module.handle.lock();
        let mut registry = MODULES.write();
        if let Some(pos) = registry.iter().position(|m| Arc::ptr_eq(m, module)) {
            registry.remove(pos);
        }
        P::close(handle);
    }
    set_error(None);
    Ok(())
}

/// Look up `symbol_name` in `module` (`g_module_symbol`).
///
/// On success returns the raw symbol pointer; on failure returns an
/// `Err` with a description string. The returned pointer is `null` only
/// if the platform explicitly returns `null` for a valid symbol (per
/// gmodule.h, "a valid symbol can be NULL").
pub fn module_symbol<P: ModulePlatform>(
    module: &Arc<GModule>,
    symbol_name: &str,
) -> Result<*mut c_void, (GModuleError, String)> {
    if !P::supported() {
        set_error(Some(
            "dynamic modules are not supported by this system".to_owned(),
        ));
        return Err((GModuleError::Failed, module_error().unwrap_or_default()));
    }
    let handle = *module.handle.lock();
    match P::symbol(handle, symbol_name) {
        Ok(ptr) => {
            set_error(None);
            Ok(ptr)
        }
        Err(e) => {
            let full = format!("'{symbol_name}': {e}");
            set_error(Some(full.clone()));
            Err((GModuleError::Failed, full))
        }
    }
}

/// Build a platform-decorated module path (`g_module_build_path`).
///
/// Deprecated upstream since 2.76 but still part of the API surface; we
/// keep it for completeness. Delegates to `P::build_path`.
pub fn module_build_path<P: ModulePlatform>(directory: Option<&str>, module_name: &str) -> String {
    P::build_path(directory, module_name)
}

/// Free-function wrapper for `GModule::name` (`g_module_name`).
pub fn module_name(module: &GModule) -> String {
    module.name()
}

/// Free-function wrapper for `GModule::make_resident`
/// (`g_module_make_resident`).
pub fn module_make_resident(module: &GModule) {
    module.make_resident();
}

// ─────────────────────────────── tests ────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flags_bitor_and_contains() {
        let flags = GModuleFlags::BIND_LAZY | GModuleFlags::BIND_LOCAL;
        assert!(flags.contains(GModuleFlags::BIND_LAZY));
        assert!(flags.contains(GModuleFlags::BIND_LOCAL));
        // NONE is 0 so contains(NONE) is trivially true for any flags;
        // verify the mask covers both bits instead.
        assert_eq!(GModuleFlags::BIND_MASK.0, 0x03);
        assert_eq!(
            (flags.0 & GModuleFlags::BIND_MASK.0),
            GModuleFlags::BIND_MASK.0
        );
    }

    #[test]
    fn error_codes_match_upstream_order() {
        assert_eq!(GModuleError::Failed.to_code(), 0);
        assert_eq!(GModuleError::CheckFailed.to_code(), 1);
    }

    #[test]
    fn error_quark_is_nonzero() {
        assert!(module_error_quark() > 0);
    }

    #[test]
    fn no_platform_reports_unsupported() {
        assert!(!NoModulePlatform::supported());
    }

    #[test]
    fn no_platform_open_fails_with_message() {
        let res = NoModulePlatform::open("foo.so", false, false);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("not supported"));
    }

    #[test]
    fn no_platform_self_handle_fails() {
        assert!(NoModulePlatform::self_handle().is_err());
    }

    #[test]
    fn no_platform_symbol_fails() {
        let res = NoModulePlatform::symbol(core::ptr::null_mut(), "main");
        assert!(res.is_err());
    }

    #[test]
    fn build_path_with_directory_adds_lib_prefix_and_so_suffix() {
        let path = NoModulePlatform::build_path(Some("/lib"), "mylib");
        assert_eq!(path, "/lib/libmylib.so");
    }

    #[test]
    fn build_path_with_lib_prefix_does_not_double_prefix() {
        let path = NoModulePlatform::build_path(Some("/lib"), "libfoo");
        assert_eq!(path, "/lib/libfoo");
    }

    #[test]
    fn build_path_without_directory_uses_bare_name() {
        let path = NoModulePlatform::build_path(None, "mylib");
        assert_eq!(path, "libmylib.so");
    }

    #[test]
    fn build_path_without_directory_with_lib_prefix() {
        let path = NoModulePlatform::build_path(None, "libfoo");
        assert_eq!(path, "libfoo");
    }

    #[test]
    fn build_path_with_empty_directory_falls_back_to_bare() {
        let path = NoModulePlatform::build_path(Some(""), "mylib");
        assert_eq!(path, "libmylib.so");
    }

    #[test]
    fn module_open_unsupported_returns_failed() {
        // Clear any prior error.
        set_error(None);
        let res: Result<Arc<GModule>, (GModuleError, String)> =
            module_open_full::<NoModulePlatform>(Some("foo.so"), GModuleFlags::NONE);
        assert!(res.is_err());
        let (code, _msg) = res.unwrap_err();
        assert_eq!(code, GModuleError::Failed);
        assert!(module_error().is_some());
    }

    #[test]
    fn module_open_main_unsupported_returns_failed() {
        set_error(None);
        // Main-module path also requires platform support.
        let res: Result<Arc<GModule>, (GModuleError, String)> =
            module_open_full::<NoModulePlatform>(None, GModuleFlags::NONE);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().0, GModuleError::Failed);
    }

    #[test]
    fn module_symbol_unsupported_returns_failed() {
        set_error(None);
        // Build a dummy GModule just to have an Arc reference.
        let module = GModule::new(Some("dummy".to_owned()), core::ptr::null_mut());
        let res = module_symbol::<NoModulePlatform>(&module, "x");
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().0, GModuleError::Failed);
    }

    #[test]
    fn module_close_unsupported_returns_failed() {
        set_error(None);
        let module = GModule::new(Some("dummy".to_owned()), core::ptr::null_mut());
        // Bump ref count so we don't trip the "already closed" branch.
        module.inc_ref();
        let res = module_close::<NoModulePlatform>(&module);
        assert!(res.is_err());
        assert_eq!(res.unwrap_err().0, GModuleError::Failed);
    }

    #[test]
    fn make_resident_round_trip() {
        let module = GModule::new(Some("dummy".to_owned()), core::ptr::null_mut());
        assert!(!module.is_resident());
        module.make_resident();
        assert!(module.is_resident());
    }

    #[test]
    fn name_returns_main_for_none() {
        let module = GModule::new(None, core::ptr::null_mut());
        assert_eq!(module.name(), "main");
    }

    #[test]
    fn name_returns_file_name_for_some() {
        let module = GModule::new(Some("/lib/libfoo.so".to_owned()), core::ptr::null_mut());
        assert_eq!(module.name(), "/lib/libfoo.so");
    }

    #[test]
    fn ref_count_starts_at_one_and_increments() {
        let module = GModule::new(Some("dummy".to_owned()), core::ptr::null_mut());
        assert_eq!(module.ref_count(), 1);
        assert_eq!(module.inc_ref(), 2);
        assert_eq!(module.ref_count(), 2);
        assert_eq!(module.dec_ref(), 1);
        assert_eq!(module.ref_count(), 1);
    }

    #[test]
    fn parse_libtool_archive_extracts_core_fields() {
        let content = "\
# libfoo.la - a libtool library file
dlname='libfoo.so.0'
library_names='libfoo.so.0.1.2 libfoo.so.0 libfoo.so'
dependency_libs=' -lbar -lz'
libdir='/usr/lib'
installed='yes'
";
        let archive = parse_libtool_archive(content, Some("/usr/lib/libfoo.la")).unwrap();
        assert_eq!(archive.dlname.as_deref(), Some("libfoo.so.0"));
        assert_eq!(
            archive.library_names.as_deref(),
            Some("libfoo.so.0.1.2 libfoo.so.0 libfoo.so")
        );
        assert_eq!(archive.dependency_libs.as_deref(), Some(" -lbar -lz"));
        assert_eq!(
            archive
                .resolve_dlname_path(Some("/usr/lib/libfoo.la"))
                .unwrap(),
            "/usr/lib/libfoo.so.0"
        );
    }

    #[test]
    fn parse_libtool_archive_uninstalled_uses_dot_libs() {
        let content = "\
dlname='libfoo.so.0'
libdir='/build/libfoo'
installed='no'
";
        let archive = parse_libtool_archive(content, Some("/build/libfoo.la")).unwrap();
        assert_eq!(
            archive
                .resolve_dlname_path(Some("/build/libfoo.la"))
                .unwrap(),
            "/build/.libs/libfoo.so.0"
        );
    }

    #[test]
    fn parse_libtool_archive_missing_dlname_fails() {
        let content = "libdir='/usr/lib'\ninstalled='yes'\n";
        let err = parse_libtool_archive(content, Some("/usr/lib/libfoo.la")).unwrap_err();
        assert!(err.contains("dlname"));
    }

    #[cfg(all(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    mod host {
        use super::*;

        #[test]
        fn host_platform_opens_main_module_and_resolves_symbol() {
            register_host_module_platform_for_tests();
            assert!(HostModulePlatform::supported());

            let module = module_open_full::<HostModulePlatform>(None, GModuleFlags::NONE).unwrap();
            assert_eq!(module.name(), "main");
            assert!(module.is_resident());

            #[cfg(any(target_os = "linux", target_os = "macos"))]
            {
                let ptr = module_symbol::<HostModulePlatform>(&module, "malloc").unwrap();
                assert!(!ptr.is_null());
            }
            #[cfg(target_os = "windows")]
            {
                let ptr = module_symbol::<HostModulePlatform>(&module, "GetModuleHandleA").unwrap();
                assert!(!ptr.is_null());
            }
        }
    }
}
