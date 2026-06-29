//! Compiler intrinsics for bare-metal environment
//!
//! Provides missing symbols that the compiler expects

use core::ffi::c_void;

/// Memory copy implementation
#[no_mangle]
pub unsafe extern "C" fn memcpy(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    let dest_bytes = dest as *mut u8;
    let src_bytes = src as *const u8;

    let mut i: usize = 0;
    while i < n {
        let dst_ptr = dest_bytes.wrapping_add(i);
        let src_ptr = src_bytes.wrapping_add(i);
        core::ptr::write_unaligned(dst_ptr, core::ptr::read_unaligned(src_ptr));
        i = i.wrapping_add(1);
    }

    dest
}

/// Memory set implementation
#[no_mangle]
pub unsafe extern "C" fn memset(s: *mut c_void, c: i32, n: usize) -> *mut c_void {
    let bytes = s as *mut u8;
    let byte_val = c as u8;

    let mut i: usize = 0;
    while i < n {
        let ptr = bytes.wrapping_add(i);
        core::ptr::write_unaligned(ptr, byte_val);
        i = i.wrapping_add(1);
    }

    s
}

/// Memory compare implementation
#[no_mangle]
pub unsafe extern "C" fn memcmp(s1: *const c_void, s2: *const c_void, n: usize) -> i32 {
    let bytes1 = s1 as *const u8;
    let bytes2 = s2 as *const u8;

    let mut i: usize = 0;
    while i < n {
        let b1 = core::ptr::read_unaligned(bytes1.wrapping_add(i));
        let b2 = core::ptr::read_unaligned(bytes2.wrapping_add(i));

        if b1 < b2 {
            return -1;
        } else if b1 > b2 {
            return 1;
        }
        i = i.wrapping_add(1);
    }

    0
}

/// Memory move implementation (handles overlapping regions)
#[no_mangle]
pub unsafe extern "C" fn memmove(dest: *mut c_void, src: *const c_void, n: usize) -> *mut c_void {
    let dest_bytes = dest as *mut u8;
    let src_bytes = src as *const u8;

    if (dest_bytes as usize) < (src_bytes as usize) {
        // Copy forward
        let mut i: usize = 0;
        while i < n {
            let dst_ptr = dest_bytes.wrapping_add(i);
            let src_ptr = src_bytes.wrapping_add(i);
            core::ptr::write_unaligned(dst_ptr, core::ptr::read_unaligned(src_ptr));
            i = i.wrapping_add(1);
        }
    } else {
        // Copy backward to handle overlap
        let mut i: usize = n;
        while i > 0 {
            i = i.wrapping_sub(1);
            let dst_ptr = dest_bytes.wrapping_add(i);
            let src_ptr = src_bytes.wrapping_add(i);
            core::ptr::write_unaligned(dst_ptr, core::ptr::read_unaligned(src_ptr));
        }
    }

    dest
}

/// C string length implementation
#[no_mangle]
pub unsafe extern "C" fn strlen(s: *const c_void) -> usize {
    let bytes = s as *const u8;
    let mut len: usize = 0;
    while core::ptr::read_unaligned(bytes.add(len)) != 0 {
        len += 1;
    }
    len
}
