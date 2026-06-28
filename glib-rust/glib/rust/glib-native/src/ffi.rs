//! C ABI compatibility layer (Phase 9/13).
//!
//! Thin `extern "C"` wrappers over the native Rust GObject/GLib APIs. Callers
//! must uphold the documented pointer/lifetime contracts; all exported
//! functions are `unsafe` at the Rust boundary because C cannot enforce them.

use crate::bytes::Bytes;
use crate::error::{set_error_literal, Error};
use crate::gobject::{closure_new, object_new, Closure, GObject};
use crate::gparamspec::ParamSpec;
use crate::gsignal::{
    signal_connect_by_name, signal_emit as rust_signal_emit,
    signal_emit_by_name as rust_signal_emit_by_name,
    signal_handler_disconnect as rust_signal_handler_disconnect,
    signal_lookup as rust_signal_lookup, signal_name as rust_signal_name,
    signal_new as rust_signal_new, ConnectFlags, SignalCallback, SignalFlags,
};
use crate::gtype::{
    type_from_name as rust_type_from_name, type_init, type_is_a, type_name as rust_type_name,
    type_register_static as rust_type_register_static,
    type_register_static_simple as rust_type_register_static_simple, GTypeFlags, GTypeInfo,
    ParamFlags, G_TYPE_BOOLEAN, G_TYPE_CHAR, G_TYPE_DOUBLE, G_TYPE_ENUM, G_TYPE_FLAGS,
    G_TYPE_FLOAT, G_TYPE_INT, G_TYPE_INT64, G_TYPE_INVALID, G_TYPE_LONG, G_TYPE_POINTER,
    G_TYPE_STRING, G_TYPE_UCHAR, G_TYPE_UINT, G_TYPE_UINT64, G_TYPE_ULONG,
};
use crate::gvalue::GValue as RustGValue;
use crate::hash::str_hash as rust_str_hash;
use crate::prelude::*;
pub use crate::quark::Quark;
use crate::quark::{
    quark_from_static_string, quark_from_string, quark_to_string, quark_try_string,
};
use crate::strfuncs::{str_has_prefix, str_has_suffix, strcmp as rust_strcmp};
use alloc::alloc::{alloc, alloc_zeroed, dealloc, Layout};
use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use core::ffi::{c_char, c_void};
use core::mem::{align_of, size_of, MaybeUninit};
use core::ptr;
use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

/// Opaque pointer (`gpointer` / `gconstpointer`).
pub type gpointer = *mut c_void;

/// C layout for [`GType`](crate::gtype::GType).
pub type CGType = usize;

/// C-compatible [`GValue`](crate::gvalue::GValue) storage.
///
/// Opaque byte buffer with the same size and alignment as the native Rust
/// `GValue`, suitable for stack allocation from C.
#[repr(C, align(8))]
pub struct GValue {
    _storage: [u8; GVALUE_STORAGE_SIZE],
}

const GVALUE_STORAGE_SIZE: usize = size_of::<RustGValue>();

const _: () = assert!(GVALUE_STORAGE_SIZE > 0);
const _: () = assert!(align_of::<RustGValue>() <= align_of::<GValue>());

/// C layout for [`GError`](crate::error::Error).
#[repr(C)]
pub struct GError {
    /// Error domain (`GQuark`).
    pub domain: Quark,
    /// Domain-specific error code.
    pub code: i32,
    /// NUL-terminated message; owned by this struct, freed by [`g_clear_error`].
    pub message: *mut c_char,
}

// ── GValue helpers ─────────────────────────────────────────────────────

/// # Safety
///
/// `value` must point to a valid `GValue` created through this FFI layer.
unsafe fn value_as_rust_mut(value: *mut GValue) -> &'static mut RustGValue {
    debug_assert!(!value.is_null());
    // SAFETY: `GValue` is layout-compatible with `RustGValue`.
    unsafe { &mut *value.cast::<RustGValue>() }
}

/// # Safety
///
/// `value` must point to a valid `GValue` created through this FFI layer.
unsafe fn value_as_rust(value: *const GValue) -> &'static RustGValue {
    debug_assert!(!value.is_null());
    // SAFETY: `GValue` is layout-compatible with `RustGValue`.
    unsafe { &*value.cast::<RustGValue>() }
}

// ── Type system ────────────────────────────────────────────────────────

/// Look up a type ID by name (`g_type_from_name`).
///
/// # Safety
///
/// `name` must be a valid NUL-terminated C string or null (returns
/// [`G_TYPE_INVALID`]).
#[no_mangle]
pub unsafe extern "C" fn g_type_from_name(name: *const c_char) -> CGType {
    type_init();
    if name.is_null() {
        return G_TYPE_INVALID;
    }
    let s = unsafe { core::ffi::CStr::from_ptr(name).to_str() };
    let Ok(s) = s else {
        return G_TYPE_INVALID;
    };
    rust_type_from_name(s)
}

/// Return the name for `type_` (`g_type_name`).
///
/// # Ownership
///
/// Returns a pointer to a process-lifetime string. For registered types the
/// underlying bytes are leaked via [`CString::into_raw`] on first request and
/// must not be freed by the caller (matches upstream GLib semantics).
///
/// Returns null when `type_` is unknown.
///
/// # Safety
///
/// The returned pointer is valid for the `'static` lifetime of the process.
#[no_mangle]
pub unsafe extern "C" fn g_type_name(type_: CGType) -> *const c_char {
    type_init();
    match rust_type_name(type_) {
        Some(name) => leak_type_name(&name),
        None => ptr::null(),
    }
}

/// Raw pointer stored inside a [`RustGValue`] for [`G_TYPE_POINTER`].
#[derive(Debug)]
struct FfiPointer(pub usize);

fn leak_type_name(name: &str) -> *const c_char {
    use alloc::collections::BTreeMap;
    use alloc::ffi::CString;
    use spin::Mutex;

    static CACHE: Mutex<BTreeMap<String, usize>> = Mutex::new(BTreeMap::new());
    let mut cache = CACHE.lock();
    if let Some(&ptr_usize) = cache.get(name) {
        return ptr_usize as *const c_char;
    }
    let c = CString::new(name).expect("type name contains interior NUL");
    let ptr = c.into_raw();
    cache.insert(name.to_owned(), ptr as usize);
    ptr
}

fn cstr_to_owned(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return Some(String::new());
    }
    let s = unsafe { core::ffi::CStr::from_ptr(ptr).to_str() };
    s.ok().map(str::to_owned)
}

fn quark_key(quark: Quark) -> Option<&'static str> {
    quark_to_string(quark)
}

// ── Memory (`gmem.h`) ──────────────────────────────────────────────────

const ALLOC_HEADER_SIZE: usize = size_of::<usize>();

/// # Safety
///
/// Returned pointers must be released only with [`g_free`].
unsafe fn malloc_raw(n_bytes: usize, zeroed: bool) -> Option<gpointer> {
    if n_bytes == 0 {
        return Some(ptr::null_mut());
    }
    let total = n_bytes.checked_add(ALLOC_HEADER_SIZE)?;
    let layout = Layout::array::<u8>(total).ok()?;
    let base = if zeroed {
        unsafe { alloc_zeroed(layout) }
    } else {
        unsafe { alloc(layout) }
    };
    if base.is_null() {
        return None;
    }
    // SAFETY: `base` points to `total` writable bytes from `alloc`.
    unsafe {
        ptr::write(base.cast::<usize>(), n_bytes);
    }
    Some(unsafe { base.add(ALLOC_HEADER_SIZE).cast() })
}

unsafe fn allocated_size(mem: gpointer) -> Option<usize> {
    if mem.is_null() {
        return None;
    }
    let base = unsafe { mem.cast::<u8>().sub(ALLOC_HEADER_SIZE) };
    Some(unsafe { ptr::read(base.cast::<usize>()) })
}

unsafe fn realloc_raw(mem: gpointer, n_bytes: usize) -> Option<gpointer> {
    if mem.is_null() {
        return unsafe { malloc_raw(n_bytes, false) };
    }
    if n_bytes == 0 {
        unsafe { g_free(mem) };
        return Some(ptr::null_mut());
    }

    let old_size = unsafe { allocated_size(mem) }.unwrap_or(0);
    let new_mem = unsafe { malloc_raw(n_bytes, false) }?;
    unsafe {
        ptr::copy_nonoverlapping(
            mem.cast::<u8>(),
            new_mem.cast::<u8>(),
            core::cmp::min(old_size, n_bytes),
        );
        g_free(mem);
    }
    Some(new_mem)
}

fn checked_n_blocks(n_blocks: usize, n_block_bytes: usize) -> Option<usize> {
    n_blocks.checked_mul(n_block_bytes)
}

unsafe fn dup_bytes_raw(src: *const u8, n_bytes: usize) -> gpointer {
    if src.is_null() || n_bytes == 0 {
        return ptr::null_mut();
    }
    let dst = match unsafe { malloc_raw(n_bytes, false) } {
        Some(dst) => dst,
        None => unsafe { core::hint::unreachable_unchecked() },
    };
    unsafe {
        ptr::copy_nonoverlapping(src, dst.cast::<u8>(), n_bytes);
    }
    dst
}

/// Allocate `n_bytes` uninitialized bytes (`g_malloc`).
///
/// # Safety
///
/// Returns a pointer that must be freed with [`g_free`], or null when
/// `n_bytes` is 0.
#[no_mangle]
pub unsafe extern "C" fn g_malloc(n_bytes: usize) -> gpointer {
    match unsafe { malloc_raw(n_bytes, false) } {
        Some(ptr) => ptr,
        None => unsafe { core::hint::unreachable_unchecked() },
    }
}

/// Allocate `n_bytes` zero-filled bytes (`g_malloc0`).
///
/// # Safety
///
/// Returns a pointer that must be freed with [`g_free`], or null when
/// `n_bytes` is 0. Panics on allocation failure (matches GLib).
#[no_mangle]
pub unsafe extern "C" fn g_malloc0(n_bytes: usize) -> gpointer {
    match unsafe { malloc_raw(n_bytes, true) } {
        Some(ptr) => ptr,
        None => unsafe { core::hint::unreachable_unchecked() },
    }
}

/// Fallible allocation (`g_try_malloc`).
///
/// # Safety
///
/// Returns null on failure or when `n_bytes` is 0.
#[no_mangle]
pub unsafe extern "C" fn g_try_malloc(n_bytes: usize) -> gpointer {
    unsafe { malloc_raw(n_bytes, false) }.unwrap_or(ptr::null_mut())
}

/// Fallible zero-filled allocation (`g_try_malloc0`).
#[no_mangle]
pub unsafe extern "C" fn g_try_malloc0(n_bytes: usize) -> gpointer {
    unsafe { malloc_raw(n_bytes, true) }.unwrap_or(ptr::null_mut())
}

/// Reallocate memory allocated by this ABI (`g_realloc`).
#[no_mangle]
pub unsafe extern "C" fn g_realloc(mem: gpointer, n_bytes: usize) -> gpointer {
    match unsafe { realloc_raw(mem, n_bytes) } {
        Some(ptr) => ptr,
        None => unsafe { core::hint::unreachable_unchecked() },
    }
}

/// Fallible reallocation (`g_try_realloc`).
#[no_mangle]
pub unsafe extern "C" fn g_try_realloc(mem: gpointer, n_bytes: usize) -> gpointer {
    unsafe { realloc_raw(mem, n_bytes) }.unwrap_or(ptr::null_mut())
}

/// Allocate `n_blocks * n_block_bytes` bytes (`g_malloc_n`).
#[no_mangle]
pub unsafe extern "C" fn g_malloc_n(n_blocks: usize, n_block_bytes: usize) -> gpointer {
    let Some(n_bytes) = checked_n_blocks(n_blocks, n_block_bytes) else {
        unsafe { core::hint::unreachable_unchecked() };
    };
    unsafe { g_malloc(n_bytes) }
}

/// Allocate zeroed `n_blocks * n_block_bytes` bytes (`g_malloc0_n`).
#[no_mangle]
pub unsafe extern "C" fn g_malloc0_n(n_blocks: usize, n_block_bytes: usize) -> gpointer {
    let Some(n_bytes) = checked_n_blocks(n_blocks, n_block_bytes) else {
        unsafe { core::hint::unreachable_unchecked() };
    };
    unsafe { g_malloc0(n_bytes) }
}

/// Fallible `n_blocks * n_block_bytes` allocation (`g_try_malloc_n`).
#[no_mangle]
pub unsafe extern "C" fn g_try_malloc_n(n_blocks: usize, n_block_bytes: usize) -> gpointer {
    let Some(n_bytes) = checked_n_blocks(n_blocks, n_block_bytes) else {
        return ptr::null_mut();
    };
    unsafe { g_try_malloc(n_bytes) }
}

/// Fallible zeroed `n_blocks * n_block_bytes` allocation (`g_try_malloc0_n`).
#[no_mangle]
pub unsafe extern "C" fn g_try_malloc0_n(n_blocks: usize, n_block_bytes: usize) -> gpointer {
    let Some(n_bytes) = checked_n_blocks(n_blocks, n_block_bytes) else {
        return ptr::null_mut();
    };
    unsafe { g_try_malloc0(n_bytes) }
}

/// Reallocate to `n_blocks * n_block_bytes` bytes (`g_realloc_n`).
#[no_mangle]
pub unsafe extern "C" fn g_realloc_n(
    mem: gpointer,
    n_blocks: usize,
    n_block_bytes: usize,
) -> gpointer {
    let Some(n_bytes) = checked_n_blocks(n_blocks, n_block_bytes) else {
        unsafe { core::hint::unreachable_unchecked() };
    };
    unsafe { g_realloc(mem, n_bytes) }
}

/// Fallible reallocate to `n_blocks * n_block_bytes` bytes (`g_try_realloc_n`).
#[no_mangle]
pub unsafe extern "C" fn g_try_realloc_n(
    mem: gpointer,
    n_blocks: usize,
    n_block_bytes: usize,
) -> gpointer {
    let Some(n_bytes) = checked_n_blocks(n_blocks, n_block_bytes) else {
        return ptr::null_mut();
    };
    unsafe { g_try_realloc(mem, n_bytes) }
}

/// Release memory from [`g_malloc`] / [`g_malloc0`] / [`g_try_malloc`].
///
/// # Safety
///
/// `mem` must be null or a pointer returned by this module's allocators and not
/// already freed.
#[no_mangle]
pub unsafe extern "C" fn g_free(mem: gpointer) {
    if mem.is_null() {
        return;
    }
    // SAFETY: `mem` was produced by `malloc_raw` with a leading size header.
    unsafe {
        let base = mem.cast::<u8>().sub(ALLOC_HEADER_SIZE);
        let n_bytes = ptr::read(base.cast::<usize>());
        let total = n_bytes
            .checked_add(ALLOC_HEADER_SIZE)
            .expect("corrupt g_malloc header");
        let layout = Layout::array::<u8>(total).expect("corrupt g_malloc layout");
        dealloc(base, layout);
    }
}

/// Initialize the type system (`g_type_init`).
#[no_mangle]
pub unsafe extern "C" fn g_type_init() {
    type_init();
}

/// Check whether `type_` is-a `is_a_type` (`g_type_is_a`).
#[no_mangle]
pub unsafe extern "C" fn g_type_is_a(type_: CGType, is_a_type: CGType) -> i32 {
    i32::from(type_is_a(type_, is_a_type))
}

// ── GValue ─────────────────────────────────────────────────────────────

/// Initialize `value` for `type_` (`g_value_init`).
///
/// # Safety
///
/// `value` must be a valid, writable pointer to uninitialized or reset
/// `GValue` storage.
#[no_mangle]
pub unsafe extern "C" fn g_value_init(value: *mut GValue, type_: CGType) {
    if value.is_null() {
        return;
    }
    // SAFETY: caller guarantees writable `GValue` storage.
    unsafe {
        ptr::write(value.cast::<RustGValue>(), RustGValue::for_type(type_));
    }
}

/// Release the contents of `value` without changing its type (`g_value_unset`).
///
/// # Safety
///
/// `value` must point to an initialized `GValue`.
#[no_mangle]
pub unsafe extern "C" fn g_value_unset(value: *mut GValue) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).reset();
    }
}

/// Read a boolean from `value` (`g_value_get_boolean`).
///
/// # Safety
///
/// `value` must point to an initialized `GValue` holding [`G_TYPE_BOOLEAN`].
#[no_mangle]
pub unsafe extern "C" fn g_value_get_boolean(value: *const GValue) -> i32 {
    if value.is_null() {
        return 0;
    }
    i32::from(unsafe { value_as_rust(value).get_boolean() })
}

/// Write a boolean into `value` (`g_value_set_boolean`).
///
/// # Safety
///
/// `value` must point to an initialized `GValue` holding [`G_TYPE_BOOLEAN`].
#[no_mangle]
pub unsafe extern "C" fn g_value_set_boolean(value: *mut GValue, v: i32) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).set_boolean(v != 0);
    }
}

/// Read a 64-bit integer from `value` (`g_value_get_int64`).
///
/// # Safety
///
/// `value` must point to an initialized `GValue` holding [`G_TYPE_INT64`].
#[no_mangle]
pub unsafe extern "C" fn g_value_get_int64(value: *const GValue) -> i64 {
    if value.is_null() {
        return 0;
    }
    unsafe { value_as_rust(value).get_int64() }
}

/// Write a 64-bit integer into `value` (`g_value_set_int64`).
///
/// # Safety
///
/// `value` must point to an initialized `GValue` holding [`G_TYPE_INT64`].
#[no_mangle]
pub unsafe extern "C" fn g_value_set_int64(value: *mut GValue, v: i64) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).set_int64(v);
    }
}

/// Read a string from `value` (`g_value_get_string`).
///
/// # Ownership
///
/// Returns a pointer to a process-lifetime copy of the string, or null when
/// the value does not hold a string. Callers must not free the result.
///
/// # Safety
///
/// `value` must point to an initialized `GValue`.
#[no_mangle]
pub unsafe extern "C" fn g_value_get_string(value: *const GValue) -> *const c_char {
    if value.is_null() {
        return ptr::null();
    }
    match unsafe { value_as_rust(value).get_string() } {
        Some(s) => leak_type_name(s),
        None => ptr::null(),
    }
}

/// Write a string into `value` (`g_value_set_string`).
///
/// # Safety
///
/// `value` must point to an initialized `GValue` holding [`G_TYPE_STRING`].
/// `v` must be a valid NUL-terminated C string or null (clears to empty).
#[no_mangle]
pub unsafe extern "C" fn g_value_set_string(value: *mut GValue, v: *const c_char) {
    if value.is_null() {
        return;
    }
    let s = if v.is_null() {
        ""
    } else {
        let c = unsafe { core::ffi::CStr::from_ptr(v).to_str() };
        let Ok(c) = c else {
            return;
        };
        c
    };
    unsafe {
        value_as_rust_mut(value).set_string(s);
    }
}

/// Set a static string (`g_value_set_static_string`).
#[no_mangle]
pub unsafe extern "C" fn g_value_set_static_string(value: *mut GValue, v: *const c_char) {
    unsafe { g_value_set_string(value, v) };
}

/// Take ownership of a string (`g_value_take_string`).
#[no_mangle]
pub unsafe extern "C" fn g_value_take_string(value: *mut GValue, v: *mut c_char) {
    unsafe { g_value_set_string(value, v as *const c_char) };
    if !v.is_null() {
        unsafe { g_free(v.cast()) };
    }
}

/// Set a string and consume the caller's allocation (`g_value_set_string_take_ownership`).
#[no_mangle]
pub unsafe extern "C" fn g_value_set_string_take_ownership(value: *mut GValue, v: *mut c_char) {
    unsafe { g_value_take_string(value, v) };
}

/// Duplicate the string stored in `value` (`g_value_dup_string`).
#[no_mangle]
pub unsafe extern "C" fn g_value_dup_string(value: *const GValue) -> *mut c_char {
    if value.is_null() {
        return ptr::null_mut();
    }
    match unsafe { value_as_rust(value).get_string() } {
        Some(s) => {
            let Some(total) = s.len().checked_add(1) else {
                unsafe { core::hint::unreachable_unchecked() };
            };
            let dst = match unsafe { malloc_raw(total, false) } {
                Some(dst) => dst.cast::<u8>(),
                None => unsafe { core::hint::unreachable_unchecked() },
            };
            unsafe {
                ptr::copy_nonoverlapping(s.as_ptr(), dst, s.len());
                ptr::write(dst.add(s.len()), 0);
            }
            dst.cast::<c_char>()
        }
        None => ptr::null_mut(),
    }
}

/// Copy `src_value` into `dest_value` (`g_value_copy`).
///
/// # Safety
///
/// Both pointers must reference initialized `GValue` storage.
#[no_mangle]
pub unsafe extern "C" fn g_value_copy(src_value: *const GValue, dest_value: *mut GValue) {
    if src_value.is_null() || dest_value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(dest_value).copy_from(value_as_rust(src_value));
    }
}

/// Reset `value` to its type's default (`g_value_reset`).
///
/// # Safety
///
/// `value` must point to an initialized `GValue`.
#[no_mangle]
pub unsafe extern "C" fn g_value_reset(value: *mut GValue) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).reset();
    }
}

/// Return the type held by `value` (`G_VALUE_TYPE` / `g_value_get_type`).
///
/// # Safety
///
/// `value` must point to an initialized `GValue`.
#[no_mangle]
pub unsafe extern "C" fn g_value_get_type(value: *const GValue) -> CGType {
    if value.is_null() {
        return G_TYPE_INVALID;
    }
    unsafe { value_as_rust(value).value_type() }
}

/// Read an unsigned integer from `value` (`g_value_get_uint`).
///
/// # Safety
///
/// `value` must point to an initialized `GValue` holding [`G_TYPE_UINT`].
#[no_mangle]
pub unsafe extern "C" fn g_value_get_uint(value: *const GValue) -> u32 {
    if value.is_null() {
        return 0;
    }
    unsafe { value_as_rust(value).get_uint() }
}

/// Write an unsigned integer into `value` (`g_value_set_uint`).
///
/// # Safety
///
/// `value` must point to an initialized `GValue` holding [`G_TYPE_UINT`].
#[no_mangle]
pub unsafe extern "C" fn g_value_set_uint(value: *mut GValue, v: u32) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).set_uint(v);
    }
}

/// Read a double from `value` (`g_value_get_double`).
///
/// # Safety
///
/// `value` must point to an initialized `GValue` holding [`G_TYPE_DOUBLE`].
#[no_mangle]
pub unsafe extern "C" fn g_value_get_double(value: *const GValue) -> f64 {
    if value.is_null() {
        return 0.0;
    }
    unsafe { value_as_rust(value).get_double() }
}

/// Write a double into `value` (`g_value_set_double`).
///
/// # Safety
///
/// `value` must point to an initialized `GValue` holding [`G_TYPE_DOUBLE`].
#[no_mangle]
pub unsafe extern "C" fn g_value_set_double(value: *mut GValue, v: f64) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).set_double(v);
    }
}

/// Read a pointer from `value` (`g_value_get_pointer`).
///
/// # Safety
///
/// `value` must point to an initialized `GValue` holding [`G_TYPE_POINTER`].
#[no_mangle]
pub unsafe extern "C" fn g_value_get_pointer(value: *const GValue) -> gpointer {
    if value.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        value_as_rust(value)
            .get_pointer()
            .and_then(|p| p.downcast_ref::<FfiPointer>())
            .map(|fp| fp.0 as gpointer)
            .unwrap_or(ptr::null_mut())
    }
}

/// Write a pointer into `value` (`g_value_set_pointer`).
///
/// # Safety
///
/// `value` must point to an initialized `GValue` holding [`G_TYPE_POINTER`].
#[no_mangle]
pub unsafe extern "C" fn g_value_set_pointer(value: *mut GValue, v: gpointer) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).set_pointer(Arc::new(FfiPointer(v as usize)));
    }
}

// ── GObject reference counting ─────────────────────────────────────────

/// Increment the reference count of `object` (`g_object_ref`).
///
/// # Safety
///
/// `object` must point to a live [`GObject`] allocated by this library.
#[no_mangle]
pub unsafe extern "C" fn g_object_ref(object: gpointer) -> gpointer {
    if object.is_null() {
        return ptr::null_mut();
    }
    let obj = unsafe { &*(object.cast::<GObject>()) };
    obj.ref_();
    object
}

/// Decrement the reference count of `object` (`g_object_unref`).
///
/// # Safety
///
/// `object` must point to a live [`GObject`] allocated by this library.
#[no_mangle]
pub unsafe extern "C" fn g_object_unref(object: gpointer) {
    if object.is_null() {
        return;
    }
    let obj = unsafe { &*(object.cast::<GObject>()) };
    obj.unref();
}

/// Sink a floating reference (`g_object_ref_sink`).
///
/// # Safety
///
/// `object` must point to a live [`GObject`].
#[no_mangle]
pub unsafe extern "C" fn g_object_ref_sink(object: gpointer) -> gpointer {
    if object.is_null() {
        return ptr::null_mut();
    }
    let obj = unsafe { &*(object.cast::<GObject>()) };
    obj.ref_sink();
    object
}

/// Get quark-keyed user data (`g_object_get_qdata`).
///
/// # Safety
///
/// `object` must point to a live [`GObject`].
#[no_mangle]
pub unsafe extern "C" fn g_object_get_qdata(object: gpointer, quark: Quark) -> gpointer {
    if object.is_null() {
        return ptr::null_mut();
    }
    let key = match quark_key(quark) {
        Some(k) => k,
        None => return ptr::null_mut(),
    };
    let obj = unsafe { &*(object.cast::<GObject>()) };
    match obj.get_data(key) {
        Some(val) => val
            .get_pointer()
            .and_then(|p| p.downcast_ref::<FfiPointer>())
            .map(|fp| fp.0 as gpointer)
            .unwrap_or(ptr::null_mut()),
        None => ptr::null_mut(),
    }
}

/// Set quark-keyed user data (`g_object_set_qdata`).
///
/// # Safety
///
/// `object` must point to a live [`GObject`].
#[no_mangle]
pub unsafe extern "C" fn g_object_set_qdata(object: gpointer, quark: Quark, data: gpointer) {
    if object.is_null() {
        return;
    }
    let key = match quark_key(quark) {
        Some(k) => k,
        None => return,
    };
    let obj = unsafe { &*(object.cast::<GObject>()) };
    let mut val = RustGValue::for_type(G_TYPE_POINTER);
    val.set_pointer(Arc::new(FfiPointer(data as usize)));
    obj.set_data(key, val);
}

/// Weak-notify callback (`GWeakNotify`).
pub type GWeakNotify = Option<extern "C" fn(gpointer, gpointer)>;

static WEAK_NOTIFY_STUBS: Mutex<BTreeMap<usize, Arc<dyn Fn() + Send + Sync>>> =
    Mutex::new(BTreeMap::new());
static NEXT_WEAK_STUB_ID: AtomicUsize = AtomicUsize::new(1);

/// Register a weak reference notify (`g_object_weak_ref`).
///
/// This stub stores the notify closure in a process-lifetime map so the
/// function pointer remains reachable; the notify is invoked via
/// [`GObject::add_weak_ref`].
///
/// # Safety
///
/// `object` must point to a live [`GObject`]. `notify` must be a valid function
/// pointer when non-null.
#[no_mangle]
pub unsafe extern "C" fn g_object_weak_ref(object: gpointer, notify: GWeakNotify, data: gpointer) {
    if object.is_null() {
        return;
    }
    let Some(notify) = notify else {
        return;
    };
    let obj = unsafe { &*(object.cast::<GObject>()) };
    let data_addr = data as usize;
    let object_addr = object as usize;
    let stub_id = NEXT_WEAK_STUB_ID.fetch_add(1, Ordering::SeqCst);
    let stub: Arc<dyn Fn() + Send + Sync> = Arc::new(move || {
        notify(data_addr as gpointer, object_addr as gpointer);
    });
    WEAK_NOTIFY_STUBS.lock().insert(stub_id, stub.clone());
    obj.add_weak_ref(stub);
}

/// C signal handler: `(instance, user_data)`.
pub type GSignalCMarshaller = Option<extern "C" fn(gpointer, gpointer)>;

/// Connect a C callback to a signal (`g_signal_connect_data`).
///
/// Minimal implementation: `c_handler` is invoked as `c_handler(instance, data)`
/// when the signal fires. `destroy_data` is ignored.
///
/// # Safety
///
/// `instance` must point to a live [`GObject`]. `detailed_signal` must be a
/// valid NUL-terminated signal name. `c_handler` must be a valid function
/// pointer when non-null.
#[no_mangle]
pub unsafe extern "C" fn g_signal_connect_data(
    instance: gpointer,
    detailed_signal: *const c_char,
    c_handler: GSignalCMarshaller,
    data: gpointer,
    _destroy_data: GWeakNotify,
    connect_flags: u32,
) -> u64 {
    if instance.is_null() {
        return 0;
    }
    let Some(signal_name) = cstr_to_owned(detailed_signal) else {
        return 0;
    };
    let Some(handler) = c_handler else {
        return 0;
    };
    let obj = unsafe { &*(instance.cast::<GObject>()) };
    let inst_addr = instance as usize;
    let user_data_addr = data as usize;
    let cb: SignalCallback = Arc::new(move |_args| {
        handler(inst_addr as gpointer, user_data_addr as gpointer);
        None
    });
    signal_connect_by_name(obj.type_id(), &signal_name, cb, ConnectFlags(connect_flags)) as u64
}

static PARAM_SPEC_LEAKS: Mutex<BTreeMap<usize, Arc<ParamSpec>>> = Mutex::new(BTreeMap::new());
static NEXT_PARAM_SPEC_KEY: AtomicUsize = AtomicUsize::new(1);

fn leak_param_spec(spec: ParamSpec) -> gpointer {
    let key = NEXT_PARAM_SPEC_KEY.fetch_add(1, Ordering::SeqCst);
    PARAM_SPEC_LEAKS.lock().insert(key, Arc::new(spec));
    key as gpointer
}

/// Create an int [`ParamSpec`] (`g_param_spec_int`).
///
/// # Ownership
///
/// Returns an opaque pointer retained for the process lifetime (stored in an
/// internal map). Callers must not free it.
///
/// # Safety
///
/// `name`, `nick`, and `blurb` must be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn g_param_spec_int(
    name: *const c_char,
    nick: *const c_char,
    blurb: *const c_char,
    minimum: i32,
    maximum: i32,
    default_value: i32,
    flags: u32,
) -> gpointer {
    let Some(name) = cstr_to_owned(name) else {
        return ptr::null_mut();
    };
    let Some(nick) = cstr_to_owned(nick) else {
        return ptr::null_mut();
    };
    let Some(blurb) = cstr_to_owned(blurb) else {
        return ptr::null_mut();
    };
    leak_param_spec(ParamSpec::int(
        &name,
        &nick,
        &blurb,
        minimum,
        maximum,
        default_value,
        ParamFlags(flags),
    ))
}

// ── GError ─────────────────────────────────────────────────────────────

/// Set `*err` when it is null (`g_set_error`).
///
/// This stub accepts a literal message pointer (no `printf` varargs). When
/// `*err` is already set, the call is ignored after emitting a warning, matching
/// upstream GLib behaviour.
///
/// # Safety
///
/// `err` must be a valid pointer to a `GError*` slot, or null to ignore.
/// `message` must be a valid NUL-terminated C string when non-null.
#[no_mangle]
pub unsafe extern "C" fn g_set_error(
    err: *mut *mut GError,
    domain: Quark,
    code: i32,
    message: *const c_char,
) {
    if err.is_null() {
        return;
    }
    let Ok(msg) = (if message.is_null() {
        Ok("")
    } else {
        unsafe { core::ffi::CStr::from_ptr(message).to_str() }
    }) else {
        return;
    };

    let slot = unsafe { &mut *err };
    if !slot.is_null() {
        let gerr = unsafe { &**slot };
        let existing_msg = if gerr.message.is_null() {
            String::new()
        } else {
            unsafe { core::ffi::CStr::from_ptr(gerr.message) }
                .to_string_lossy()
                .into_owned()
        };
        let mut existing = Some(Error::new_literal(gerr.domain, gerr.code, existing_msg));
        set_error_literal(Some(&mut existing), domain, code, msg);
        return;
    }
    unsafe {
        *slot = error_to_gerror(Error::new_literal(domain, code, msg));
    }
}

/// Clear and free `*err` (`g_clear_error`).
///
/// # Safety
///
/// `err` must be a valid pointer to a `GError*` slot, or null to ignore.
#[no_mangle]
pub unsafe extern "C" fn g_clear_error(err: *mut *mut GError) {
    if err.is_null() {
        return;
    }
    let slot = unsafe { &mut *err };
    if slot.is_null() {
        return;
    }
    let gerr = unsafe { Box::from_raw(*slot) };
    free_gerror(*gerr);
    unsafe {
        *slot = ptr::null_mut();
    }
}

/// Free a single `GError` (`g_error_free`).
///
/// # Safety
///
/// `error` must be a valid `GError` allocated by this FFI layer, or null.
#[no_mangle]
pub unsafe extern "C" fn g_error_free(error: *mut GError) {
    if error.is_null() {
        return;
    }
    let gerr = unsafe { Box::from_raw(error) };
    free_gerror(*gerr);
}

/// Move `src` into `*dest` (`g_propagate_error`).
///
/// # Safety
///
/// `dest` must be a valid `GError**` slot or null (drops `src`). `src` must be
/// a valid `GError*` allocated by this layer; ownership is taken.
#[no_mangle]
pub unsafe extern "C" fn g_propagate_error(dest: *mut *mut GError, src: *mut GError) {
    if src.is_null() {
        return;
    }
    let gerr = unsafe { Box::from_raw(src) };
    let domain = gerr.domain;
    let code = gerr.code;
    let message = if gerr.message.is_null() {
        String::new()
    } else {
        unsafe { core::ffi::CStr::from_ptr(gerr.message) }
            .to_string_lossy()
            .into_owned()
    };
    free_gerror(*gerr);

    if dest.is_null() {
        return;
    }
    let slot = unsafe { &mut *dest };
    if slot.is_null() {
        unsafe {
            *slot = error_to_gerror(Error::new_literal(domain, code, message));
        }
    } else {
        let mut scratch = Some(Error::new_literal(domain, code, message));
        set_error_literal(Some(&mut scratch), domain, code, "ignored overwrite");
    }
}

fn error_to_gerror(err: Error) -> *mut GError {
    use alloc::ffi::CString;
    let message = CString::new(err.message()).expect("error message contains interior NUL");
    Box::into_raw(Box::new(GError {
        domain: err.domain(),
        code: err.code(),
        message: message.into_raw(),
    }))
}

fn free_gerror(err: GError) {
    use alloc::ffi::CString;
    if !err.message.is_null() {
        // SAFETY: message was allocated by CString::into_raw in error_to_gerror.
        unsafe {
            drop(CString::from_raw(err.message));
        }
    }
}

// ── GValue: int / uint / uint64 / float / double ──────────────────────

/// Read an int from `value` (`g_value_get_int`).
#[no_mangle]
pub unsafe extern "C" fn g_value_get_int(value: *const GValue) -> i32 {
    if value.is_null() {
        return 0;
    }
    unsafe { value_as_rust(value).get_int() }
}

/// Write an int into `value` (`g_value_set_int`).
#[no_mangle]
pub unsafe extern "C" fn g_value_set_int(value: *mut GValue, v: i32) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).set_int(v);
    }
}

/// Read a uint64 from `value` (`g_value_get_uint64`).
#[no_mangle]
pub unsafe extern "C" fn g_value_get_uint64(value: *const GValue) -> u64 {
    if value.is_null() {
        return 0;
    }
    unsafe { value_as_rust(value).get_uint64() }
}

/// Write a uint64 into `value` (`g_value_set_uint64`).
#[no_mangle]
pub unsafe extern "C" fn g_value_set_uint64(value: *mut GValue, v: u64) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).set_uint64(v);
    }
}

/// Read a float from `value` (`g_value_get_float`).
#[no_mangle]
pub unsafe extern "C" fn g_value_get_float(value: *const GValue) -> f32 {
    if value.is_null() {
        return 0.0;
    }
    unsafe { value_as_rust(value).get_float() }
}

/// Write a float into `value` (`g_value_set_float`).
#[no_mangle]
pub unsafe extern "C" fn g_value_set_float(value: *mut GValue, v: f32) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).set_float(v);
    }
}

// ── GValue: char / uchar / long / ulong ───────────────────────────────

/// Read a char from `value` (`g_value_get_char`).
#[no_mangle]
pub unsafe extern "C" fn g_value_get_char(value: *const GValue) -> i8 {
    if value.is_null() {
        return 0;
    }
    unsafe { value_as_rust(value).get_char() }
}

/// Write a char into `value` (`g_value_set_char`).
#[no_mangle]
pub unsafe extern "C" fn g_value_set_char(value: *mut GValue, v: i8) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).set_char(v);
    }
}

/// Read a uchar from `value` (`g_value_get_uchar`).
#[no_mangle]
pub unsafe extern "C" fn g_value_get_uchar(value: *const GValue) -> u8 {
    if value.is_null() {
        return 0;
    }
    unsafe { value_as_rust(value).get_uchar() }
}

/// Write a uchar into `value` (`g_value_set_uchar`).
#[no_mangle]
pub unsafe extern "C" fn g_value_set_uchar(value: *mut GValue, v: u8) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).set_uchar(v);
    }
}

/// Read a long from `value` (`g_value_get_long`).
#[no_mangle]
pub unsafe extern "C" fn g_value_get_long(value: *const GValue) -> i64 {
    if value.is_null() {
        return 0;
    }
    unsafe { value_as_rust(value).get_long() }
}

/// Write a long into `value` (`g_value_set_long`).
#[no_mangle]
pub unsafe extern "C" fn g_value_set_long(value: *mut GValue, v: i64) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).set_long(v);
    }
}

/// Read a ulong from `value` (`g_value_get_ulong`).
#[no_mangle]
pub unsafe extern "C" fn g_value_get_ulong(value: *const GValue) -> u64 {
    if value.is_null() {
        return 0;
    }
    unsafe { value_as_rust(value).get_ulong() }
}

/// Write a ulong into `value` (`g_value_set_ulong`).
#[no_mangle]
pub unsafe extern "C" fn g_value_set_ulong(value: *mut GValue, v: u64) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).set_ulong(v);
    }
}

// ── GValue: enum / flags ──────────────────────────────────────────────

/// Read an enum from `value` (`g_value_get_enum`).
#[no_mangle]
pub unsafe extern "C" fn g_value_get_enum(value: *const GValue) -> i32 {
    if value.is_null() {
        return 0;
    }
    unsafe { value_as_rust(value).get_enum() }
}

/// Write an enum into `value` (`g_value_set_enum`).
#[no_mangle]
pub unsafe extern "C" fn g_value_set_enum(value: *mut GValue, v: i32) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).set_enum(v);
    }
}

/// Read flags from `value` (`g_value_get_flags`).
#[no_mangle]
pub unsafe extern "C" fn g_value_get_flags(value: *const GValue) -> u32 {
    if value.is_null() {
        return 0;
    }
    unsafe { value_as_rust(value).get_flags() }
}

/// Write flags into `value` (`g_value_set_flags`).
#[no_mangle]
pub unsafe extern "C" fn g_value_set_flags(value: *mut GValue, v: u32) {
    if value.is_null() {
        return;
    }
    unsafe {
        value_as_rust_mut(value).set_flags(v);
    }
}

// ── GQuark ────────────────────────────────────────────────────────────

/// Get (or create) a quark for a process-lifetime `string`
/// (`g_quark_from_static_string`).
///
/// # Safety
///
/// `string` must be a valid NUL-terminated C string that outlives the process.
#[no_mangle]
pub unsafe extern "C" fn g_quark_from_static_string(string: *const c_char) -> Quark {
    if string.is_null() {
        return 0;
    }
    let Ok(s) = (unsafe { core::ffi::CStr::from_ptr(string).to_str() }) else {
        return 0;
    };
    // SAFETY: C caller guarantees `string` outlives the process (static storage).
    quark_from_static_string(Some(unsafe {
        core::mem::transmute::<&str, &'static str>(s)
    }))
}

/// Get (or create) a quark for `string` (`g_quark_from_string`).
#[no_mangle]
pub unsafe extern "C" fn g_quark_from_string(string: *const c_char) -> Quark {
    if string.is_null() {
        return 0;
    }
    let Ok(s) = (unsafe { core::ffi::CStr::from_ptr(string).to_str() }) else {
        return 0;
    };
    quark_from_string(Some(s))
}

/// Try to find a quark for `string` without creating (`g_quark_try_string`).
#[no_mangle]
pub unsafe extern "C" fn g_quark_try_string(string: *const c_char) -> Quark {
    if string.is_null() {
        return 0;
    }
    let Ok(s) = (unsafe { core::ffi::CStr::from_ptr(string).to_str() }) else {
        return 0;
    };
    quark_try_string(Some(s))
}

/// Get the string for `quark` (`g_quark_to_string`).
#[no_mangle]
pub unsafe extern "C" fn g_quark_to_string(quark: Quark) -> *const c_char {
    match quark_to_string(quark) {
        Some(s) => leak_type_name(s),
        None => ptr::null(),
    }
}

// ── Memory allocation ─────────────────────────────────────────────────
// (g_malloc, g_malloc0, g_free are defined earlier in this file)

/// Free a `NULL`-terminated array of strings (`g_strfreev`).
///
/// # Safety
///
/// `str_array` must be null or a pointer to a `NULL`-terminated array of
/// strings allocated by this module's allocators; the array itself is also
/// freed.
#[no_mangle]
pub unsafe extern "C" fn g_strfreev(str_array: *mut *mut c_char) {
    if str_array.is_null() {
        return;
    }
    let mut i = 0usize;
    loop {
        let elem = unsafe { *str_array.add(i) };
        if elem.is_null() {
            break;
        }
        unsafe { g_free(elem.cast()) };
        i += 1;
    }
    unsafe { g_free(str_array.cast()) };
}

/// Duplicate a C string (`g_strdup`).
#[no_mangle]
pub unsafe extern "C" fn g_strdup(str: *const c_char) -> *mut c_char {
    if str.is_null() {
        return ptr::null_mut();
    }
    let len = unsafe { core::ffi::CStr::from_ptr(str).to_bytes_with_nul().len() };
    unsafe { dup_bytes_raw(str.cast::<u8>(), len).cast::<c_char>() }
}

/// Duplicate at most `n` bytes from a C string (`g_strndup`).
#[no_mangle]
pub unsafe extern "C" fn g_strndup(str: *const c_char, n: usize) -> *mut c_char {
    if str.is_null() {
        return ptr::null_mut();
    }

    let bytes = unsafe { core::ffi::CStr::from_ptr(str).to_bytes() };
    let len = core::cmp::min(bytes.len(), n);
    let Some(total) = len.checked_add(1) else {
        unsafe { core::hint::unreachable_unchecked() };
    };
    let dst = match unsafe { malloc_raw(total, false) } {
        Some(dst) => dst.cast::<u8>(),
        None => unsafe { core::hint::unreachable_unchecked() },
    };
    unsafe {
        ptr::copy_nonoverlapping(str.cast::<u8>(), dst, len);
        ptr::write(dst.add(len), 0);
    }
    dst.cast::<c_char>()
}

/// Duplicate a byte buffer (`g_memdup2`).
#[no_mangle]
pub unsafe extern "C" fn g_memdup2(mem: *const c_void, byte_size: usize) -> gpointer {
    unsafe { dup_bytes_raw(mem.cast::<u8>(), byte_size) }
}

/// Duplicate a byte buffer (`g_memdup`, deprecated upstream).
#[no_mangle]
pub unsafe extern "C" fn g_memdup(mem: *const c_void, byte_size: u32) -> gpointer {
    unsafe { g_memdup2(mem, byte_size as usize) }
}

// ── Type system extras ────────────────────────────────────────────────
// (g_type_is_a is defined earlier in this file)

/// Get the fundamental type ID (`g_type_fundamental`).
#[no_mangle]
pub unsafe extern "C" fn g_type_fundamental(type_id: CGType) -> CGType {
    type_init();
    use crate::gtype::G_TYPE_FUNDAMENTAL_SHIFT;
    type_id >> G_TYPE_FUNDAMENTAL_SHIFT
}

/// Get the type registration serial (`g_type_get_type_registration_serial`).
#[no_mangle]
pub unsafe extern "C" fn g_type_get_type_registration_serial() -> u32 {
    crate::gtype::type_get_type_registration_serial()
}

// ── GObject: construction + properties ─────────────────────────────────

/// Create a new GObject of `type_` (`g_object_new`).
///
/// Returns a pointer that must be released with [`g_object_unref`].
///
/// # Safety
///
/// `type_` must be a valid registered GType (typically `G_TYPE_OBJECT` or a
/// subclass).
#[no_mangle]
pub unsafe extern "C" fn g_object_new(type_: CGType) -> gpointer {
    type_init();
    let obj = object_new(type_);
    Arc::into_raw(obj) as gpointer
}

/// Get a property value from `object` (`g_object_get_property`).
///
/// Writes the property value into `value`. The caller must initialise `value`
/// with [`g_value_init`] for the correct type before calling.
///
/// # Safety
///
/// `object` must point to a live [`GObject`]. `property_name` must be a valid
/// NUL-terminated C string. `value` must point to initialised `GValue` storage.
#[no_mangle]
pub unsafe extern "C" fn g_object_get_property(
    object: gpointer,
    property_name: *const c_char,
    value: *mut GValue,
) {
    if object.is_null() || property_name.is_null() || value.is_null() {
        return;
    }
    let name = match unsafe { core::ffi::CStr::from_ptr(property_name).to_str() } {
        Ok(s) => s,
        Err(_) => return,
    };
    let obj = unsafe { &*(object.cast::<GObject>()) };
    if let Some(val) = obj.get_property(name) {
        unsafe {
            value_as_rust_mut(value).copy_from(&val);
        }
    }
}

/// Set a property value on `object` (`g_object_set_property`).
///
/// # Safety
///
/// `object` must point to a live [`GObject`]. `property_name` must be a valid
/// NUL-terminated C string. `value` must point to an initialised `GValue`.
#[no_mangle]
pub unsafe extern "C" fn g_object_set_property(
    object: gpointer,
    property_name: *const c_char,
    value: *const GValue,
) {
    if object.is_null() || property_name.is_null() || value.is_null() {
        return;
    }
    let name = match unsafe { core::ffi::CStr::from_ptr(property_name).to_str() } {
        Ok(s) => s,
        Err(_) => return,
    };
    let obj = unsafe { &*(object.cast::<GObject>()) };
    let val = unsafe { value_as_rust(value) };
    obj.set_property(name, val.clone());
}

/// Check whether `object` has a floating reference (`g_object_is_floating`).
///
/// # Safety
///
/// `object` must point to a live [`GObject`] or be null (returns 0).
#[no_mangle]
pub unsafe extern "C" fn g_object_is_floating(object: gpointer) -> i32 {
    if object.is_null() {
        return 0;
    }
    let obj = unsafe { &*(object.cast::<GObject>()) };
    i32::from(obj.is_floating())
}

/// Force `object` into the floating state (`g_object_force_floating`).
///
/// # Safety
///
/// `object` must point to a live [`GObject`].
#[no_mangle]
pub unsafe extern "C" fn g_object_force_floating(object: gpointer) {
    if object.is_null() {
        return;
    }
    let obj = unsafe { &*(object.cast::<GObject>()) };
    obj.force_floating();
}

// ── GSignal ────────────────────────────────────────────────────────────

/// Register a new signal (`g_signal_new`).
///
/// Returns the signal ID (0 on failure).
///
/// # Safety
///
/// `signal_name` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn g_signal_new(
    signal_name: *const c_char,
    owner_type: CGType,
    flags: u32,
    return_type: CGType,
    n_params: u32,
    param_types: *const CGType,
) -> u32 {
    if signal_name.is_null() {
        return 0;
    }
    let name = match unsafe { core::ffi::CStr::from_ptr(signal_name).to_str() } {
        Ok(s) => s,
        Err(_) => return 0,
    };
    let params: Vec<CGType> = if n_params == 0 || param_types.is_null() {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(param_types, n_params as usize) }.to_vec()
    };
    rust_signal_new(name, owner_type, SignalFlags(flags), return_type, &params)
}

/// Look up a signal ID by name and type (`g_signal_lookup`).
///
/// # Safety
///
/// `name` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn g_signal_lookup(name: *const c_char, owner_type: CGType) -> u32 {
    if name.is_null() {
        return 0;
    }
    let s = match unsafe { core::ffi::CStr::from_ptr(name).to_str() } {
        Ok(s) => s,
        Err(_) => return 0,
    };
    rust_signal_lookup(s, owner_type)
}

/// Get the name of `signal_id` (`g_signal_name`).
///
/// Returns a pointer to a process-lifetime string, or null.
///
/// # Safety
///
/// The returned pointer is valid for the process lifetime and must not be
/// freed.
#[no_mangle]
pub unsafe extern "C" fn g_signal_name(signal_id: u32) -> *const c_char {
    match rust_signal_name(signal_id) {
        Some(name) => leak_type_name(&name),
        None => ptr::null(),
    }
}

/// Emit a signal by ID (`g_signal_emit`).
///
/// # Safety
///
/// `instance` must point to a live [`GObject`]. `args` must point to an array
/// of `n_args` initialised `GValue` slots, or be null when `n_args` is 0.
#[no_mangle]
pub unsafe extern "C" fn g_signal_emit(
    instance: gpointer,
    signal_id: u32,
    _detail: u32,
    args: *const GValue,
    n_args: u32,
) {
    if instance.is_null() || signal_id == 0 {
        return;
    }
    let obj = unsafe { &*(instance.cast::<GObject>()) };
    let arg_vec: Vec<RustGValue> = if n_args == 0 || args.is_null() {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(args.cast::<RustGValue>(), n_args as usize) }
            .iter()
            .cloned()
            .collect()
    };
    rust_signal_emit(obj.type_id(), signal_id, &arg_vec);
}

/// Emit a signal by name (`g_signal_emit_by_name`).
///
/// # Safety
///
/// `instance` must point to a live [`GObject`]. `detailed_signal` must be a
/// valid NUL-terminated C string. `args` must point to an array of `n_args`
/// initialised `GValue` slots, or be null when `n_args` is 0.
#[no_mangle]
pub unsafe extern "C" fn g_signal_emit_by_name(
    instance: gpointer,
    detailed_signal: *const c_char,
    args: *const GValue,
    n_args: u32,
) {
    if instance.is_null() || detailed_signal.is_null() {
        return;
    }
    let sig_name = match unsafe { core::ffi::CStr::from_ptr(detailed_signal).to_str() } {
        Ok(s) => s,
        Err(_) => return,
    };
    let obj = unsafe { &*(instance.cast::<GObject>()) };
    let arg_vec: Vec<RustGValue> = if n_args == 0 || args.is_null() {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(args.cast::<RustGValue>(), n_args as usize) }
            .iter()
            .cloned()
            .collect()
    };
    rust_signal_emit_by_name(obj.type_id(), sig_name, &arg_vec);
}

/// Disconnect a signal handler (`g_signal_handler_disconnect`).
///
/// # Safety
///
/// `handler_id` must be a valid handler ID returned by
/// [`g_signal_connect_data`].
#[no_mangle]
pub unsafe extern "C" fn g_signal_handler_disconnect(handler_id: u64) -> i32 {
    i32::from(rust_signal_handler_disconnect(handler_id as u32))
}

// ── GBytes ─────────────────────────────────────────────────────────────

/// Opaque `GBytes*` pointer type.
pub type GBytesPtr = *mut Bytes;

/// Allocate a new `GBytes` from `data` (`g_bytes_new`).
///
/// # Safety
///
/// `data` must point to at least `size` readable bytes, or be null when `size`
/// is 0. The returned pointer must be released with [`g_bytes_unref`].
#[no_mangle]
pub unsafe extern "C" fn g_bytes_new(data: *const c_void, size: usize) -> GBytesPtr {
    if data.is_null() || size == 0 {
        return Box::into_raw(Box::new(Bytes::new(&[] as &[u8])));
    }
    let slice = unsafe { core::slice::from_raw_parts(data.cast::<u8>(), size) };
    Box::into_raw(Box::new(Bytes::new(slice)))
}

/// Create a `GBytes` that takes ownership of `data` (`g_bytes_new_take`).
///
/// The data is copied into an owned buffer and `data` is released via
/// [`g_free`].
///
/// # Safety
///
/// `data` must be a pointer from [`g_malloc`] / [`g_malloc0`] (or null when
/// `size` is 0). Ownership is taken.
#[no_mangle]
#[no_mangle]
pub unsafe extern "C" fn g_bytes_new_take(data: *mut c_void, size: usize) -> GBytesPtr {
    if data.is_null() || size == 0 {
        if !data.is_null() {
            unsafe { g_free(data) };
        }
        return Box::into_raw(Box::new(Bytes::new(&[] as &[u8])));
    }
    let slice = unsafe { core::slice::from_raw_parts(data.cast::<u8>(), size) };
    let bytes = Bytes::new(slice);
    unsafe { g_free(data) };
    Box::into_raw(Box::new(bytes))
}

/// Get a pointer to the byte data (`g_bytes_get_data`).
///
/// The returned pointer is valid as long as `bytes` is alive.
///
/// # Safety
///
/// `bytes` must be a valid `GBytes*` from [`g_bytes_new`].
#[no_mangle]
pub unsafe extern "C" fn g_bytes_get_data(bytes: GBytesPtr, size: *mut usize) -> *const c_void {
    if bytes.is_null() {
        return ptr::null();
    }
    let b = unsafe { &*bytes };
    if !size.is_null() {
        unsafe { *size = b.len() };
    }
    b.data().as_ptr() as *const c_void
}

/// Get the size of `bytes` (`g_bytes_get_size`).
///
/// # Safety
///
/// `bytes` must be a valid `GBytes*`.
#[no_mangle]
pub unsafe extern "C" fn g_bytes_get_size(bytes: GBytesPtr) -> usize {
    if bytes.is_null() {
        return 0;
    }
    unsafe { (&*bytes).len() }
}

/// Increment the reference count of `bytes` (`g_bytes_ref`).
///
/// # Safety
///
/// `bytes` must be a valid `GBytes*`. The returned pointer must be released
/// with [`g_bytes_unref`].
#[no_mangle]
pub unsafe extern "C" fn g_bytes_ref(bytes: GBytesPtr) -> GBytesPtr {
    if bytes.is_null() {
        return ptr::null_mut();
    }
    let b = unsafe { &*bytes };
    Box::into_raw(Box::new(b.clone()))
}

/// Decrement the reference count of `bytes` (`g_bytes_unref`).
///
/// # Safety
///
/// `bytes` must be a valid `GBytes*` from [`g_bytes_new`] / [`g_bytes_ref`],
/// or null (no-op).
#[no_mangle]
pub unsafe extern "C" fn g_bytes_unref(bytes: GBytesPtr) {
    if bytes.is_null() {
        return;
    }
    unsafe { drop(Box::from_raw(bytes)) };
}

/// Compare two `GBytes` for equality (`g_bytes_equal`).
///
/// # Safety
///
/// Both pointers must be valid `GBytes*` or null (null != non-null).
#[no_mangle]
pub unsafe extern "C" fn g_bytes_equal(bytes1: GBytesPtr, bytes2: GBytesPtr) -> i32 {
    if bytes1.is_null() || bytes2.is_null() {
        return i32::from(bytes1 == bytes2);
    }
    let b1 = unsafe { &*bytes1 };
    let b2 = unsafe { &*bytes2 };
    i32::from(b1.equal(b2))
}

/// Compute a hash for `bytes` (`g_bytes_hash`).
///
/// # Safety
///
/// `bytes` must be a valid `GBytes*`.
#[no_mangle]
pub unsafe extern "C" fn g_bytes_hash(bytes: GBytesPtr) -> u32 {
    if bytes.is_null() {
        return 0;
    }
    unsafe { (&*bytes).hash() }
}

// ── GError: construction + inspection ──────────────────────────────────

/// Allocate a new `GError` (`g_error_new`).
///
/// This stub accepts a literal message pointer (no `printf` varargs).
///
/// # Safety
///
/// `message` must be a valid NUL-terminated C string. The returned pointer
/// must be released with [`g_error_free`] or [`g_clear_error`].
#[no_mangle]
pub unsafe extern "C" fn g_error_new(
    domain: Quark,
    code: i32,
    message: *const c_char,
) -> *mut GError {
    if message.is_null() {
        return error_to_gerror(Error::new_literal(domain, code, ""));
    }
    let msg = unsafe {
        core::ffi::CStr::from_ptr(message)
            .to_string_lossy()
            .into_owned()
    };
    error_to_gerror(Error::new_literal(domain, code, &msg))
}

/// Allocate a new `GError` with a literal message (`g_error_new_literal`).
///
/// # Safety
///
/// `message` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn g_error_new_literal(
    domain: Quark,
    code: i32,
    message: *const c_char,
) -> *mut GError {
    if message.is_null() {
        return error_to_gerror(Error::new_literal(domain, code, ""));
    }
    let msg = unsafe {
        core::ffi::CStr::from_ptr(message)
            .to_string_lossy()
            .into_owned()
    };
    error_to_gerror(Error::new_literal(domain, code, &msg))
}

/// Copy a `GError` (`g_error_copy`).
///
/// # Safety
///
/// `error` must be a valid `GError*` allocated by this FFI layer.
/// The returned pointer must be released with [`g_error_free`] or
/// [`g_clear_error`].
#[no_mangle]
pub unsafe extern "C" fn g_error_copy(error: *const GError) -> *mut GError {
    if error.is_null() {
        return ptr::null_mut();
    }
    let gerr = unsafe { &*error };
    let msg = if gerr.message.is_null() {
        String::new()
    } else {
        unsafe { core::ffi::CStr::from_ptr(gerr.message) }
            .to_string_lossy()
            .into_owned()
    };
    error_to_gerror(Error::new_literal(gerr.domain, gerr.code, &msg))
}

/// Check whether `error` matches `domain` and `code` (`g_error_matches`).
///
/// # Safety
///
/// `error` must be a valid `GError*` or null (returns 0).
#[no_mangle]
pub unsafe extern "C" fn g_error_matches(error: *const GError, domain: Quark, code: i32) -> i32 {
    if error.is_null() {
        return 0;
    }
    let gerr = unsafe { &*error };
    i32::from(gerr.domain == domain && gerr.code == code)
}

// ── GParamSpec: boolean / string / uint ────────────────────────────────

/// Create a boolean [`ParamSpec`] (`g_param_spec_boolean`).
///
/// # Ownership
///
/// Returns an opaque pointer retained for the process lifetime.
///
/// # Safety
///
/// `name`, `nick`, and `blurb` must be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn g_param_spec_boolean(
    name: *const c_char,
    nick: *const c_char,
    blurb: *const c_char,
    default_value: i32,
    flags: u32,
) -> gpointer {
    let Some(name) = cstr_to_owned(name) else {
        return ptr::null_mut();
    };
    let Some(nick) = cstr_to_owned(nick) else {
        return ptr::null_mut();
    };
    let Some(blurb) = cstr_to_owned(blurb) else {
        return ptr::null_mut();
    };
    leak_param_spec(ParamSpec::boolean(
        &name,
        &nick,
        &blurb,
        default_value != 0,
        ParamFlags(flags),
    ))
}

/// Create a string [`ParamSpec`] (`g_param_spec_string`).
///
/// # Safety
///
/// `name`, `nick`, `blurb`, and `default_value` must be valid NUL-terminated C
/// strings (or null for `default_value`).
#[no_mangle]
pub unsafe extern "C" fn g_param_spec_string(
    name: *const c_char,
    nick: *const c_char,
    blurb: *const c_char,
    default_value: *const c_char,
    flags: u32,
) -> gpointer {
    let Some(name) = cstr_to_owned(name) else {
        return ptr::null_mut();
    };
    let Some(nick) = cstr_to_owned(nick) else {
        return ptr::null_mut();
    };
    let Some(blurb) = cstr_to_owned(blurb) else {
        return ptr::null_mut();
    };
    let default = if default_value.is_null() {
        String::new()
    } else {
        unsafe {
            core::ffi::CStr::from_ptr(default_value)
                .to_string_lossy()
                .into_owned()
        }
    };
    leak_param_spec(ParamSpec::string(
        &name,
        &nick,
        &blurb,
        &default,
        ParamFlags(flags),
    ))
}

/// Create a uint [`ParamSpec`] (`g_param_spec_uint`).
///
/// # Safety
///
/// `name`, `nick`, and `blurb` must be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn g_param_spec_uint(
    name: *const c_char,
    nick: *const c_char,
    blurb: *const c_char,
    minimum: u32,
    maximum: u32,
    default_value: u32,
    flags: u32,
) -> gpointer {
    let Some(name) = cstr_to_owned(name) else {
        return ptr::null_mut();
    };
    let Some(nick) = cstr_to_owned(nick) else {
        return ptr::null_mut();
    };
    let Some(blurb) = cstr_to_owned(blurb) else {
        return ptr::null_mut();
    };
    leak_param_spec(ParamSpec::uint(
        &name,
        &nick,
        &blurb,
        minimum,
        maximum,
        default_value,
        ParamFlags(flags),
    ))
}

// ── GType: registration ────────────────────────────────────────────────

/// C-compatible `GTypeInfo` struct for FFI callers.
///
/// Function-pointer fields are stubbed — real class/instance init must be done
/// from Rust code via `type_register_static_simple` with `fn` pointers.
#[repr(C)]
pub struct CGTypeInfo {
    pub class_size: u16,
    pub instance_size: u16,
    pub class_init: Option<extern "C" fn(*mut c_void)>,
    pub instance_init: Option<extern "C" fn(*mut c_void)>,
}

/// Register a static derived type (`g_type_register_static`).
///
/// # Safety
///
/// `type_name` must be a valid NUL-terminated C string. `info` must point to a
/// valid `CGTypeInfo` struct or null.
#[no_mangle]
pub unsafe extern "C" fn g_type_register_static(
    parent_type: CGType,
    type_name: *const c_char,
    info: *const CGTypeInfo,
    flags: u32,
) -> CGType {
    if type_name.is_null() {
        return G_TYPE_INVALID;
    }
    let name = match unsafe { core::ffi::CStr::from_ptr(type_name).to_str() } {
        Ok(s) => s,
        Err(_) => return G_TYPE_INVALID,
    };
    let rust_info = GTypeInfo {
        class_size: if info.is_null() {
            0
        } else {
            unsafe { (*info).class_size }
        },
        instance_size: if info.is_null() {
            0
        } else {
            unsafe { (*info).instance_size }
        },
        class_init: None,
        instance_init: None,
        value_table: None,
    };
    rust_type_register_static(parent_type, name, &rust_info, GTypeFlags(flags))
}

/// Register a static type with simplified parameters
/// (`g_type_register_static_simple`).
///
/// # Safety
///
/// `type_name` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn g_type_register_static_simple(
    parent_type: CGType,
    type_name: *const c_char,
    class_size: u16,
    _class_init: Option<extern "C" fn(*mut c_void)>,
    instance_size: u16,
    _instance_init: Option<extern "C" fn(*mut c_void)>,
    flags: u32,
) -> CGType {
    if type_name.is_null() {
        return G_TYPE_INVALID;
    }
    let name = match unsafe { core::ffi::CStr::from_ptr(type_name).to_str() } {
        Ok(s) => s,
        Err(_) => return G_TYPE_INVALID,
    };
    rust_type_register_static_simple(
        parent_type,
        name,
        class_size,
        None,
        instance_size,
        None,
        GTypeFlags(flags),
    )
}

// ── String helpers ─────────────────────────────────────────────────────

/// Compare two C strings, null-safe (`g_strcmp0`).
///
/// Returns -1, 0, or 1. Null is treated as less than any non-null string.
///
/// # Safety
///
/// Both pointers may be null or valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn g_strcmp0(str1: *const c_char, str2: *const c_char) -> i32 {
    if str1.is_null() && str2.is_null() {
        return 0;
    }
    if str1.is_null() {
        return -1;
    }
    if str2.is_null() {
        return 1;
    }
    let s1 = unsafe { core::ffi::CStr::from_ptr(str1).to_string_lossy() };
    let s2 = unsafe { core::ffi::CStr::from_ptr(str2).to_string_lossy() };
    rust_strcmp(&s1, &s2)
}

/// Check whether `str` begins with `prefix` (`g_str_has_prefix`).
///
/// # Safety
///
/// Both pointers must be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn g_str_has_prefix(str: *const c_char, prefix: *const c_char) -> i32 {
    if str.is_null() || prefix.is_null() {
        return 0;
    }
    let s = unsafe { core::ffi::CStr::from_ptr(str).to_string_lossy() };
    let p = unsafe { core::ffi::CStr::from_ptr(prefix).to_string_lossy() };
    i32::from(str_has_prefix(&s, &p))
}

/// Check whether `str` ends with `suffix` (`g_str_has_suffix`).
///
/// # Safety
///
/// Both pointers must be valid NUL-terminated C strings.
#[no_mangle]
pub unsafe extern "C" fn g_str_has_suffix(str: *const c_char, suffix: *const c_char) -> i32 {
    if str.is_null() || suffix.is_null() {
        return 0;
    }
    let s = unsafe { core::ffi::CStr::from_ptr(str).to_string_lossy() };
    let sf = unsafe { core::ffi::CStr::from_ptr(suffix).to_string_lossy() };
    i32::from(str_has_suffix(&s, &sf))
}

/// Compute a hash for `str` (`g_str_hash`).
///
/// # Safety
///
/// `str` must be a valid NUL-terminated C string.
#[no_mangle]
pub unsafe extern "C" fn g_str_hash(str: *const c_char) -> u32 {
    rust_str_hash(str as *const ())
}

// ── GClosure ───────────────────────────────────────────────────────────

/// Opaque `GClosure*` pointer type.
pub type GClosurePtr = *mut Closure;

/// Create a C closure (`g_cclosure_new`).
///
/// The `callback` is invoked as `callback(data, null)` when the closure is
/// invoked. The `destroy_data` notify is stored but not yet dispatched.
///
/// # Safety
///
/// `callback` must be a valid function pointer when non-null. The returned
/// pointer must be released with [`g_closure_unref`].
#[no_mangle]
pub unsafe extern "C" fn g_cclosure_new(
    callback: GSignalCMarshaller,
    data: gpointer,
    _destroy_data: GWeakNotify,
) -> GClosurePtr {
    let Some(cb) = callback else {
        return ptr::null_mut();
    };
    let data_addr = data as usize;
    let closure = closure_new(move |_args| {
        cb(data_addr as gpointer, ptr::null_mut());
        None
    });
    Arc::into_raw(closure) as GClosurePtr
}

/// Invoke a closure (`g_closure_invoke`).
///
/// # Safety
///
/// `closure` must be a valid `GClosure*` from [`g_cclosure_new`]. `args` must
/// point to an array of `n_args` initialised `GValue` slots, or be null when
/// `n_args` is 0.
#[no_mangle]
pub unsafe extern "C" fn g_closure_invoke(closure: GClosurePtr, args: *const GValue, n_args: u32) {
    if closure.is_null() {
        return;
    }
    let c = unsafe { &*closure };
    let arg_vec: Vec<RustGValue> = if n_args == 0 || args.is_null() {
        Vec::new()
    } else {
        unsafe { core::slice::from_raw_parts(args.cast::<RustGValue>(), n_args as usize) }
            .iter()
            .cloned()
            .collect()
    };
    let _ = c.invoke(&arg_vec);
}

/// Increment the reference count of `closure` (`g_closure_ref`).
///
/// # Safety
///
/// `closure` must be a valid `GClosure*`.
#[no_mangle]
pub unsafe extern "C" fn g_closure_ref(closure: GClosurePtr) -> GClosurePtr {
    if closure.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        Arc::increment_strong_count(closure);
    }
    closure
}

/// Sink a floating closure reference (`g_closure_sink`).
///
/// # Safety
///
/// `closure` must be a valid `GClosure*`.
#[no_mangle]
pub unsafe extern "C" fn g_closure_sink(closure: GClosurePtr) {
    if closure.is_null() {
        return;
    }
    let c = unsafe { &*closure };
    c.sink();
}

/// Decrement the reference count of `closure` (`g_closure_unref`).
///
/// # Safety
///
/// `closure` must be a valid `GClosure*` from [`g_cclosure_new`] or
/// [`g_closure_ref`], or null (no-op).
#[no_mangle]
pub unsafe extern "C" fn g_closure_unref(closure: GClosurePtr) {
    if closure.is_null() {
        return;
    }
    unsafe { drop(Arc::from_raw(closure)) };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gobject::object_new;
    use crate::gsignal::{signal_emit_by_name, signal_new, ConnectFlags, SignalFlags};
    use crate::gtype::{ParamFlags, G_TYPE_NONE, G_TYPE_OBJECT, G_TYPE_UINT};
    use crate::quark::quark_from_static_string;
    use alloc::sync::Arc;
    use std::ffi::CString;
    use std::sync::atomic::{AtomicI32, AtomicUsize, Ordering};

    extern "C" {
        fn g_type_from_name(name: *const c_char) -> CGType;
        fn g_type_name(type_: CGType) -> *const c_char;
        fn g_type_init();
        fn g_value_init(value: *mut GValue, type_: CGType);
        fn g_value_unset(value: *mut GValue);
        fn g_value_reset(value: *mut GValue);
        fn g_value_copy(src: *const GValue, dest: *mut GValue);
        fn g_value_get_type(value: *const GValue) -> CGType;
        fn g_value_get_boolean(value: *const GValue) -> i32;
        fn g_value_set_boolean(value: *mut GValue, v: i32);
        fn g_value_get_int64(value: *const GValue) -> i64;
        fn g_value_set_int64(value: *mut GValue, v: i64);
        fn g_value_get_int(value: *const GValue) -> i32;
        fn g_value_set_int(value: *mut GValue, v: i32);
        fn g_value_get_uint(value: *const GValue) -> u32;
        fn g_value_set_uint(value: *mut GValue, v: u32);
        fn g_value_get_float(value: *const GValue) -> f32;
        fn g_value_set_float(value: *mut GValue, v: f32);
        fn g_value_get_double(value: *const GValue) -> f64;
        fn g_value_set_double(value: *mut GValue, v: f64);
        fn g_value_get_pointer(value: *const GValue) -> gpointer;
        fn g_value_set_pointer(value: *mut GValue, v: gpointer);
        fn g_value_get_enum(value: *const GValue) -> i32;
        fn g_value_set_enum(value: *mut GValue, v: i32);
        fn g_value_get_flags(value: *const GValue) -> u32;
        fn g_value_set_flags(value: *mut GValue, v: u32);
        fn g_value_get_string(value: *const GValue) -> *const c_char;
        fn g_value_set_string(value: *mut GValue, v: *const c_char);
        fn g_object_ref(object: gpointer) -> gpointer;
        fn g_object_unref(object: gpointer);
        fn g_object_ref_sink(object: gpointer) -> gpointer;
        fn g_object_get_qdata(object: gpointer, quark: Quark) -> gpointer;
        fn g_object_set_qdata(object: gpointer, quark: Quark, data: gpointer);
        fn g_object_weak_ref(object: gpointer, notify: GWeakNotify, data: gpointer);
        fn g_signal_connect_data(
            instance: gpointer,
            detailed_signal: *const c_char,
            c_handler: GSignalCMarshaller,
            data: gpointer,
            destroy_data: GWeakNotify,
            connect_flags: u32,
        ) -> u64;
        fn g_param_spec_int(
            name: *const c_char,
            nick: *const c_char,
            blurb: *const c_char,
            minimum: i32,
            maximum: i32,
            default_value: i32,
            flags: u32,
        ) -> gpointer;
        fn g_set_error(err: *mut *mut GError, domain: Quark, code: i32, message: *const c_char);
        fn g_clear_error(err: *mut *mut GError);
        fn g_error_free(error: *mut GError);
        fn g_propagate_error(dest: *mut *mut GError, src: *mut GError);
        fn g_quark_from_string(string: *const c_char) -> Quark;
        fn g_quark_try_string(string: *const c_char) -> Quark;
        fn g_quark_to_string(quark: Quark) -> *const c_char;
        fn g_malloc(n_bytes: usize) -> gpointer;
        fn g_malloc0(n_bytes: usize) -> gpointer;
        fn g_try_malloc(n_bytes: usize) -> gpointer;
        fn g_free(mem: gpointer);
        fn g_strdup(str: *const c_char) -> *mut c_char;
        fn g_type_is_a(type_: CGType, is_a_type: CGType) -> i32;
        fn g_type_fundamental(type_id: CGType) -> CGType;
    }

    #[test]
    fn c_type_from_name_and_type_name() {
        type_init();
        let name = CString::new("gboolean").unwrap();
        let type_ = unsafe { g_type_from_name(name.as_ptr()) };
        assert_eq!(type_, G_TYPE_BOOLEAN);

        let c_name = unsafe { g_type_name(type_) };
        assert!(!c_name.is_null());
        let roundtrip = unsafe { core::ffi::CStr::from_ptr(c_name) };
        assert_eq!(roundtrip.to_str().unwrap(), "gboolean");
    }

    #[test]
    fn c_value_boolean_roundtrip() {
        let mut value: MaybeUninit<GValue> = MaybeUninit::uninit();
        unsafe {
            g_value_init(value.as_mut_ptr(), G_TYPE_BOOLEAN);
            g_value_set_boolean(value.as_mut_ptr(), 1);
            assert_eq!(g_value_get_boolean(value.as_ptr()), 1);
            g_value_unset(value.as_mut_ptr());
            assert_eq!(g_value_get_boolean(value.as_ptr()), 0);
        }
    }

    #[test]
    fn c_value_int64_and_string() {
        let mut value: MaybeUninit<GValue> = MaybeUninit::uninit();
        unsafe {
            g_value_init(value.as_mut_ptr(), G_TYPE_INT64);
            g_value_set_int64(value.as_mut_ptr(), -42);
            assert_eq!(g_value_get_int64(value.as_ptr()), -42);

            g_value_init(value.as_mut_ptr(), G_TYPE_STRING);
            let msg = CString::new("ffi").unwrap();
            g_value_set_string(value.as_mut_ptr(), msg.as_ptr());
            let out = g_value_get_string(value.as_ptr());
            assert!(!out.is_null());
            assert_eq!(core::ffi::CStr::from_ptr(out).to_str().unwrap(), "ffi");

            let dup = g_value_dup_string(value.as_ptr());
            assert!(!dup.is_null());
            assert_eq!(core::ffi::CStr::from_ptr(dup).to_str().unwrap(), "ffi");
            g_free(dup.cast());

            let owned = g_strdup(msg.as_ptr());
            g_value_take_string(value.as_mut_ptr(), owned);
            let out = g_value_get_string(value.as_ptr());
            assert!(!out.is_null());
            assert_eq!(core::ffi::CStr::from_ptr(out).to_str().unwrap(), "ffi");

            let static_msg = CString::new("static ffi").unwrap();
            g_value_set_static_string(value.as_mut_ptr(), static_msg.as_ptr());
            let out = g_value_get_string(value.as_ptr());
            assert_eq!(
                core::ffi::CStr::from_ptr(out).to_str().unwrap(),
                "static ffi"
            );
        }
    }

    #[test]
    fn c_object_ref_unref() {
        type_init();
        let obj: Arc<GObject> = object_new(G_TYPE_OBJECT);
        assert_eq!(obj.ref_count(), 1);
        let raw = Arc::into_raw(obj) as gpointer;
        unsafe {
            g_object_ref(raw);
            assert_eq!((&*raw.cast::<GObject>()).ref_count(), 2);
            g_object_unref(raw);
            assert_eq!((&*raw.cast::<GObject>()).ref_count(), 1);
            g_object_unref(raw);
        }
    }

    #[test]
    fn c_set_and_clear_error() {
        let domain = quark_from_static_string(Some("ffi-test-quark"));
        let mut err: *mut GError = ptr::null_mut();
        let msg = CString::new("boom").unwrap();
        unsafe {
            g_set_error(&mut err, domain, 7, msg.as_ptr());
            assert!(!err.is_null());
            assert_eq!((*err).domain, domain);
            assert_eq!((*err).code, 7);
            let text = core::ffi::CStr::from_ptr((*err).message);
            assert_eq!(text.to_str().unwrap(), "boom");
            g_clear_error(&mut err);
            assert!(err.is_null());
        }
    }

    #[test]
    fn c_value_int_uint_float_double() {
        let mut value: MaybeUninit<GValue> = MaybeUninit::uninit();
        unsafe {
            g_value_init(value.as_mut_ptr(), G_TYPE_INT);
            g_value_set_int(value.as_mut_ptr(), 12345);
            assert_eq!(g_value_get_int(value.as_ptr()), 12345);

            g_value_init(value.as_mut_ptr(), G_TYPE_UINT);
            g_value_set_uint(value.as_mut_ptr(), 99999);
            assert_eq!(g_value_get_uint(value.as_ptr()), 99999);

            g_value_init(value.as_mut_ptr(), G_TYPE_FLOAT);
            g_value_set_float(value.as_mut_ptr(), 3.14);
            assert!((g_value_get_float(value.as_ptr()) - 3.14).abs() < 0.001);

            g_value_init(value.as_mut_ptr(), G_TYPE_DOUBLE);
            g_value_set_double(value.as_mut_ptr(), 2.71828);
            assert!((g_value_get_double(value.as_ptr()) - 2.71828).abs() < 0.00001);
        }
    }

    #[test]
    fn c_value_enum_flags() {
        let mut value: MaybeUninit<GValue> = MaybeUninit::uninit();
        unsafe {
            g_value_init(value.as_mut_ptr(), G_TYPE_ENUM);
            g_value_set_enum(value.as_mut_ptr(), 42);
            assert_eq!(g_value_get_enum(value.as_ptr()), 42);

            g_value_init(value.as_mut_ptr(), G_TYPE_FLAGS);
            g_value_set_flags(value.as_mut_ptr(), 0x3);
            assert_eq!(g_value_get_flags(value.as_ptr()), 0x3);
        }
    }

    #[test]
    fn c_quark_from_and_to_string() {
        let s = CString::new("ffi-quark-test").unwrap();
        let q = unsafe { g_quark_from_string(s.as_ptr()) };
        assert!(q != 0);
        let q2 = unsafe { g_quark_try_string(s.as_ptr()) };
        assert_eq!(q, q2);
        let c_str = unsafe { g_quark_to_string(q) };
        assert!(!c_str.is_null());
        assert_eq!(
            unsafe { core::ffi::CStr::from_ptr(c_str).to_str().unwrap() },
            "ffi-quark-test"
        );
    }

    #[test]
    fn c_malloc_and_free() {
        unsafe {
            let p = g_malloc(64);
            assert!(!p.is_null());
            g_free(p);
            let p0 = g_malloc0(32);
            assert!(!p0.is_null());
            g_free(p0);
            assert!(g_malloc(0).is_null());

            let pn = g_malloc_n(4, 8);
            assert!(!pn.is_null());
            let pn = g_realloc_n(pn, 8, 8);
            assert!(!pn.is_null());
            g_free(pn);

            let ptn = g_try_malloc0_n(2, 16);
            assert!(!ptn.is_null());
            let ptn = g_try_realloc_n(ptn, 4, 16);
            assert!(!ptn.is_null());
            g_free(ptn);

            assert!(g_try_malloc_n(usize::MAX, 2).is_null());
        }
    }

    #[test]
    fn c_strdup() {
        let s = CString::new("hello ffi").unwrap();
        unsafe {
            let dup = g_strdup(s.as_ptr());
            assert!(!dup.is_null());
            assert_eq!(
                core::ffi::CStr::from_ptr(dup).to_str().unwrap(),
                "hello ffi"
            );
            g_free(dup.cast());

            let short = g_strndup(s.as_ptr(), 5);
            assert!(!short.is_null());
            assert_eq!(core::ffi::CStr::from_ptr(short).to_str().unwrap(), "hello");
            g_free(short.cast());

            let bytes = [1u8, 2, 3, 4];
            let copy = g_memdup2(bytes.as_ptr().cast(), bytes.len());
            assert!(!copy.is_null());
            assert_eq!(
                core::slice::from_raw_parts(copy.cast::<u8>(), bytes.len()),
                bytes
            );
            g_free(copy);
        }
    }

    #[test]
    fn c_type_is_a_and_fundamental() {
        type_init();
        unsafe {
            assert_eq!(g_type_is_a(G_TYPE_INT, G_TYPE_INT), 1);
            assert_eq!(g_type_is_a(G_TYPE_INT, G_TYPE_STRING), 0);
            assert_eq!(g_type_fundamental(G_TYPE_INT), G_TYPE_INT >> 2);
        }
    }

    #[test]
    fn c_type_init_bootstraps_registry() {
        unsafe { g_type_init() };
        let name = CString::new("gboolean").unwrap();
        let type_ = unsafe { g_type_from_name(name.as_ptr()) };
        assert_eq!(type_, G_TYPE_BOOLEAN);
    }

    #[test]
    fn c_try_malloc_zero_and_roundtrip() {
        unsafe {
            assert!(g_try_malloc(0).is_null());
            let p = g_try_malloc(16);
            assert!(!p.is_null());
            g_free(p);
        }
    }

    #[test]
    fn c_value_copy_reset_and_get_type() {
        let mut src: MaybeUninit<GValue> = MaybeUninit::uninit();
        let mut dest: MaybeUninit<GValue> = MaybeUninit::uninit();
        unsafe {
            g_value_init(src.as_mut_ptr(), G_TYPE_UINT);
            g_value_set_uint(src.as_mut_ptr(), 77);
            g_value_init(dest.as_mut_ptr(), G_TYPE_UINT);
            g_value_copy(src.as_ptr(), dest.as_mut_ptr());
            assert_eq!(g_value_get_uint(dest.as_ptr()), 77);
            assert_eq!(g_value_get_type(dest.as_ptr()), G_TYPE_UINT);
            g_value_reset(dest.as_mut_ptr());
            assert_eq!(g_value_get_uint(dest.as_ptr()), 0);
        }
    }

    #[test]
    fn c_value_pointer_roundtrip() {
        let mut value: MaybeUninit<GValue> = MaybeUninit::uninit();
        let marker = 0xdeadbeef_usize as gpointer;
        unsafe {
            g_value_init(value.as_mut_ptr(), G_TYPE_POINTER);
            g_value_set_pointer(value.as_mut_ptr(), marker);
            assert_eq!(g_value_get_pointer(value.as_ptr()), marker);
        }
    }

    #[test]
    fn c_object_ref_sink_clears_floating() {
        type_init();
        let obj: Arc<GObject> = object_new(G_TYPE_OBJECT);
        let raw = Arc::into_raw(obj) as gpointer;
        unsafe {
            (&*raw.cast::<GObject>()).force_floating();
            assert!((&*raw.cast::<GObject>()).is_floating());
            g_object_ref_sink(raw);
            assert!(!(&*raw.cast::<GObject>()).is_floating());
            g_object_unref(raw);
        }
    }

    #[test]
    fn c_object_qdata_roundtrip() {
        type_init();
        let obj: Arc<GObject> = object_new(G_TYPE_OBJECT);
        let raw = Arc::into_raw(obj) as gpointer;
        let q = quark_from_static_string(Some("ffi-qdata-key"));
        let payload = 0x1234_usize as gpointer;
        unsafe {
            g_object_set_qdata(raw, q, payload);
            assert_eq!(g_object_get_qdata(raw, q), payload);
            g_object_unref(raw);
        }
    }

    #[test]
    fn c_object_weak_ref_notify_stub() {
        type_init();
        static HITS: AtomicUsize = AtomicUsize::new(0);
        extern "C" fn weak_notify(_data: gpointer, _where: gpointer) {
            HITS.fetch_add(1, Ordering::SeqCst);
        }
        let obj: Arc<GObject> = object_new(G_TYPE_OBJECT);
        let raw = Arc::into_raw(obj) as gpointer;
        unsafe {
            g_object_weak_ref(raw, Some(weak_notify), ptr::null_mut());
            g_object_unref(raw);
        }
        assert_eq!(HITS.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn c_signal_connect_data_invokes_handler() {
        type_init();
        static HITS: AtomicI32 = AtomicI32::new(0);
        extern "C" fn on_sig(_inst: gpointer, _data: gpointer) {
            HITS.fetch_add(1, Ordering::SeqCst);
        }
        signal_new(
            "ffi-c-signal",
            G_TYPE_OBJECT,
            SignalFlags::RUN_LAST,
            G_TYPE_NONE,
            &[],
        );
        let obj: Arc<GObject> = object_new(G_TYPE_OBJECT);
        let raw = Arc::into_raw(obj) as gpointer;
        let sig = CString::new("ffi-c-signal").unwrap();
        unsafe {
            let id = g_signal_connect_data(
                raw,
                sig.as_ptr(),
                Some(on_sig),
                ptr::null_mut(),
                None,
                ConnectFlags::NONE.0,
            );
            assert!(id > 0);
            signal_emit_by_name(G_TYPE_OBJECT, "ffi-c-signal", &[]);
            assert_eq!(HITS.load(Ordering::SeqCst), 1);
            g_object_unref(raw);
        }
    }

    #[test]
    fn c_param_spec_int_returns_opaque_pointer() {
        let name = CString::new("count").unwrap();
        let nick = CString::new("Count").unwrap();
        let blurb = CString::new("item count").unwrap();
        unsafe {
            let spec = g_param_spec_int(
                name.as_ptr(),
                nick.as_ptr(),
                blurb.as_ptr(),
                0,
                100,
                5,
                ParamFlags::READWRITE.0,
            );
            assert!(!spec.is_null());
        }
    }

    #[test]
    fn c_error_free_releases_single_error() {
        let domain = quark_from_static_string(Some("ffi-free-quark"));
        let mut err: *mut GError = ptr::null_mut();
        let msg = CString::new("free-me").unwrap();
        unsafe {
            g_set_error(&mut err, domain, 3, msg.as_ptr());
            assert!(!err.is_null());
            g_error_free(err);
        }
    }

    #[test]
    fn c_propagate_error_moves_into_dest() {
        let domain = quark_from_static_string(Some("ffi-prop-quark"));
        let mut src: *mut GError = ptr::null_mut();
        let mut dest: *mut GError = ptr::null_mut();
        let msg = CString::new("propagated").unwrap();
        unsafe {
            g_set_error(&mut src, domain, 9, msg.as_ptr());
            g_propagate_error(&mut dest, src);
            assert!(!dest.is_null());
            assert_eq!((*dest).code, 9);
            let text = core::ffi::CStr::from_ptr((*dest).message);
            assert_eq!(text.to_str().unwrap(), "propagated");
            g_clear_error(&mut dest);
        }
    }
}
