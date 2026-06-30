//! FFI bindings for C compression libraries (zstd, bzip2, xz/lzma2).
//!
//! These are compiled from `c_libs/` via `build.rs` using the `cc` crate.
//! The C code uses `kcompat.h` which maps malloc/free/memset to the
//! RustOS kernel allocator via the `rustos_kalloc`/`rustos_kfree` FFI
//! functions defined below.

use alloc::vec;
use alloc::vec::Vec;

// ── Kernel allocator FFI (called by C kcompat.c) ──────────────────

use crate::ALLOCATOR;
use core::alloc::{GlobalAlloc, Layout};
use core::{cmp, mem, ptr};

#[derive(Copy, Clone)]
#[repr(C, align(8))]
struct AllocationHeader {
    magic: usize,
    size: usize,
}

const ALLOCATION_MAGIC: usize = 0x5255_5354_4f53_4b41;
const ALLOCATION_ALIGN: usize = mem::align_of::<AllocationHeader>();
const ALLOCATION_HEADER_SIZE: usize = mem::size_of::<AllocationHeader>();

fn allocation_layout(size: usize) -> Option<Layout> {
    let total_size = ALLOCATION_HEADER_SIZE.checked_add(size.max(1))?;
    Layout::from_size_align(total_size, ALLOCATION_ALIGN).ok()
}

#[no_mangle]
pub extern "C" fn rustos_kalloc(size: usize) -> *mut u8 {
    let layout = match allocation_layout(size) {
        Some(layout) => layout,
        None => return ptr::null_mut(),
    };
    unsafe {
        let raw = ALLOCATOR.alloc(layout);
        if raw.is_null() {
            return ptr::null_mut();
        }

        let header = raw.cast::<AllocationHeader>();
        ptr::write(
            header,
            AllocationHeader {
                magic: ALLOCATION_MAGIC,
                size,
            },
        );
        raw.add(ALLOCATION_HEADER_SIZE)
    }
}

#[no_mangle]
pub extern "C" fn rustos_kfree(ptr: *mut u8, _size: usize) {
    if ptr.is_null() {
        return;
    }
    unsafe {
        let raw = ptr.sub(ALLOCATION_HEADER_SIZE);
        let header = raw.cast::<AllocationHeader>();
        let allocation = ptr::read(header);
        if allocation.magic != ALLOCATION_MAGIC {
            return;
        }

        if let Some(layout) = allocation_layout(allocation.size) {
            ALLOCATOR.dealloc(raw, layout);
        }
    }
}

#[no_mangle]
pub extern "C" fn rustos_krealloc(ptr: *mut u8, _old_size: usize, new_size: usize) -> *mut u8 {
    if ptr.is_null() {
        return rustos_kalloc(new_size);
    }
    if new_size == 0 {
        rustos_kfree(ptr, 0);
        return ptr::null_mut();
    }

    let old_size = unsafe {
        let raw = ptr.sub(ALLOCATION_HEADER_SIZE);
        let header = raw.cast::<AllocationHeader>();
        let allocation = ptr::read(header);
        if allocation.magic != ALLOCATION_MAGIC {
            return ptr::null_mut();
        }
        ptr::write(header, allocation);
        allocation.size
    };

    let new_ptr = rustos_kalloc(new_size);
    if new_ptr.is_null() {
        return ptr::null_mut();
    }
    unsafe {
        ptr::copy_nonoverlapping(ptr, new_ptr, cmp::min(old_size, new_size));
    }
    rustos_kfree(ptr, 0);
    new_ptr
}

// ── C library FFI declarations ────────────────────────────────────

extern "C" {
    fn zstd_decompress(
        src: *const u8,
        src_size: usize,
        dst: *mut u8,
        dst_capacity: usize,
        out_size: *mut usize,
    ) -> i32;

    fn bzip2_decompress(
        src: *const u8,
        src_size: usize,
        dst: *mut u8,
        dst_capacity: usize,
        out_size: *mut usize,
    ) -> i32;

    fn xz_decompress(
        src: *const u8,
        src_size: usize,
        dst: *mut u8,
        dst_capacity: usize,
        out_size: *mut usize,
    ) -> i32;
}

// ── Safe wrappers ─────────────────────────────────────────────────

/// Decompress zstd-compressed data.
pub fn zstd_decompress_safe(input: &[u8]) -> Result<Vec<u8>, &'static str> {
    // Start with 4x the input size, grow if needed
    let mut capacity = input.len() * 4;
    loop {
        let mut output = vec![0u8; capacity];
        let mut out_size: usize = 0;
        let ret = unsafe {
            zstd_decompress(
                input.as_ptr(),
                input.len(),
                output.as_mut_ptr(),
                output.len(),
                &mut out_size,
            )
        };
        if ret == 0 {
            output.truncate(out_size);
            return Ok(output);
        }
        if ret == -1 && capacity < input.len() * 64 {
            // Try larger buffer
            capacity *= 2;
            continue;
        }
        return Err("zstd decompression failed");
    }
}

/// Decompress bzip2-compressed data.
pub fn bzip2_decompress_safe(input: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut capacity = input.len() * 8;
    loop {
        let mut output = vec![0u8; capacity];
        let mut out_size: usize = 0;
        let ret = unsafe {
            bzip2_decompress(
                input.as_ptr(),
                input.len(),
                output.as_mut_ptr(),
                output.len(),
                &mut out_size,
            )
        };
        if ret == 0 {
            output.truncate(out_size);
            return Ok(output);
        }
        if ret == -1 && capacity < input.len() * 128 {
            capacity *= 2;
            continue;
        }
        return Err("bzip2 decompression failed");
    }
}

/// Decompress xz/lzma2-compressed data.
pub fn xz_decompress_safe(input: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut capacity = input.len() * 8;
    loop {
        let mut output = vec![0u8; capacity];
        let mut out_size: usize = 0;
        let ret = unsafe {
            xz_decompress(
                input.as_ptr(),
                input.len(),
                output.as_mut_ptr(),
                output.len(),
                &mut out_size,
            )
        };
        if ret == 0 {
            output.truncate(out_size);
            return Ok(output);
        }
        if ret == -1 && capacity < input.len() * 128 {
            capacity *= 2;
            continue;
        }
        return Err("xz decompression failed");
    }
}
