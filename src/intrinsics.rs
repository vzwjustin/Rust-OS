//! Compiler intrinsics for missing memory symbols.
//!
//! When building with `-Zbuild-std=core,compiler_builtins,alloc` without
//! `compiler-builtins-mem`, the linker needs explicit `memcpy`, `memset`,
//! `memcmp`, and `memmove` definitions.  This module provides them.

#![allow(clippy::missing_safety_doc)]

/// # Safety
/// Classic memcpy semantics — `dest` and `src` must be valid for `n` bytes
/// and must not overlap (use `memmove` for overlapping regions).
#[no_mangle]
pub unsafe extern "C" fn memcpy(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if dest as usize == src as usize || n == 0 {
        return dest;
    }

    // Forward copy when dest < src, backward when dest > src to avoid
    // clobbering source data in overlapping scenarios (defensive).
    if (dest as usize) < (src as usize) {
        let mut i = 0;
        while i < n {
            *dest.add(i) = *src.add(i);
            i += 1;
        }
    } else {
        let mut i = n;
        while i > 0 {
            i -= 1;
            *dest.add(i) = *src.add(i);
        }
    }
    dest
}

/// # Safety
/// `dest` must be valid for `n` bytes.
#[no_mangle]
pub unsafe extern "C" fn memset(dest: *mut u8, c: u8, n: usize) -> *mut u8 {
    let mut i = 0;
    while i < n {
        *dest.add(i) = c;
        i += 1;
    }
    dest
}

/// # Safety
/// `a` and `b` must be valid for `n` bytes.
#[no_mangle]
pub unsafe extern "C" fn memcmp(a: *const u8, b: *const u8, n: usize) -> i32 {
    let mut i = 0;
    while i < n {
        let av = *a.add(i);
        let bv = *b.add(i);
        if av != bv {
            return (av as i32) - (bv as i32);
        }
        i += 1;
    }
    0
}

/// # Safety
/// `dest` and `src` must be valid for `n` bytes.  Handles overlapping regions.
#[no_mangle]
pub unsafe extern "C" fn memmove(dest: *mut u8, src: *const u8, n: usize) -> *mut u8 {
    if dest as usize == src as usize || n == 0 {
        return dest;
    }

    if (dest as usize) < (src as usize) {
        let mut i = 0;
        while i < n {
            *dest.add(i) = *src.add(i);
            i += 1;
        }
    } else {
        let mut i = n;
        while i > 0 {
            i -= 1;
            *dest.add(i) = *src.add(i);
        }
    }
    dest
}
