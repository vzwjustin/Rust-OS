// SPDX-License-Identifier: MIT
// RustOS I/O vector abstractions (ported from Linux rust/kernel/iov.rs)
// All bindings:: calls replaced with native Rust implementations.

#![allow(dead_code, unused_variables)]

extern crate alloc;
use alloc::vec::Vec;
use core::ptr;

use crate::uaccess::{copy_from_user, UserPtr};

// ---------------------------------------------------------------------------
// IoVec — kernel-side iovec
// ---------------------------------------------------------------------------

/// A single kernel-side I/O buffer descriptor (matches `struct iovec`).
#[repr(C)]
pub struct IoVec {
    /// Pointer to the data buffer.
    pub iov_base: *mut u8,
    /// Length of the buffer in bytes.
    pub iov_len: usize,
}

// SAFETY: raw pointers wrapped in IoVec are handled by the caller.
unsafe impl Send for IoVec {}
unsafe impl Sync for IoVec {}

// ---------------------------------------------------------------------------
// UserIoVec — user-space iovec representation
// ---------------------------------------------------------------------------

/// User-space representation of an iovec (pointer fields are 64-bit on x86_64).
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct UserIoVec {
    /// User-space base address.
    pub iov_base: u64,
    /// Length.
    pub iov_len: u64,
}

// ---------------------------------------------------------------------------
// IovIter — iterator over a kernel iovec array
// ---------------------------------------------------------------------------

/// Sequential iterator over a slice of `IoVec` buffers.
pub struct IovIter<'a> {
    iov: &'a [IoVec],
    /// Current iov index.
    iov_idx: usize,
    /// Byte offset into `iov[iov_idx]`.
    iov_off: usize,
    /// Remaining bytes across all iovecs.
    remaining: usize,
}

impl<'a> IovIter<'a> {
    /// Construct a new `IovIter` covering the first `count` bytes of `iov`.
    pub fn new(iov: &'a [IoVec], count: usize) -> Self {
        // Cap count at total iov capacity.
        let total: usize = iov.iter().map(|v| v.iov_len).sum();
        let remaining = count.min(total);
        IovIter {
            iov,
            iov_idx: 0,
            iov_off: 0,
            remaining,
        }
    }

    /// How many bytes remain in the iterator.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.remaining
    }

    /// Copy bytes from `src` into the iovec sequence.
    /// Returns the number of bytes actually written.
    pub fn copy_to_iter(&mut self, src: &[u8]) -> usize {
        let mut written = 0usize;
        let mut src_off = 0usize;

        while written < src.len() && self.remaining > 0 {
            if self.iov_idx >= self.iov.len() {
                break;
            }
            let seg = &self.iov[self.iov_idx];
            let seg_left = seg.iov_len - self.iov_off;
            let to_copy = (src.len() - src_off).min(seg_left).min(self.remaining);

            if to_copy == 0 {
                self.iov_idx += 1;
                self.iov_off = 0;
                continue;
            }

            unsafe {
                ptr::copy_nonoverlapping(
                    src.as_ptr().add(src_off),
                    seg.iov_base.add(self.iov_off),
                    to_copy,
                );
            }

            self.iov_off += to_copy;
            if self.iov_off >= seg.iov_len {
                self.iov_idx += 1;
                self.iov_off = 0;
            }

            src_off += to_copy;
            written += to_copy;
            self.remaining -= to_copy;
        }

        written
    }

    /// Copy bytes from the iovec sequence into `dst`.
    /// Returns the number of bytes actually read.
    pub fn copy_from_iter(&mut self, dst: &mut [u8]) -> usize {
        let mut read = 0usize;
        let mut dst_off = 0usize;

        while read < dst.len() && self.remaining > 0 {
            if self.iov_idx >= self.iov.len() {
                break;
            }
            let seg = &self.iov[self.iov_idx];
            let seg_left = seg.iov_len - self.iov_off;
            let to_copy = (dst.len() - dst_off).min(seg_left).min(self.remaining);

            if to_copy == 0 {
                self.iov_idx += 1;
                self.iov_off = 0;
                continue;
            }

            unsafe {
                ptr::copy_nonoverlapping(
                    seg.iov_base.add(self.iov_off),
                    dst.as_mut_ptr().add(dst_off),
                    to_copy,
                );
            }

            self.iov_off += to_copy;
            if self.iov_off >= seg.iov_len {
                self.iov_idx += 1;
                self.iov_off = 0;
            }

            dst_off += to_copy;
            read += to_copy;
            self.remaining -= to_copy;
        }

        read
    }

    /// Advance the iterator by `n` bytes (discard data).
    pub fn advance(&mut self, mut n: usize) {
        n = n.min(self.remaining);
        self.remaining -= n;

        while n > 0 {
            if self.iov_idx >= self.iov.len() {
                break;
            }
            let seg = &self.iov[self.iov_idx];
            let seg_left = seg.iov_len - self.iov_off;
            if n >= seg_left {
                n -= seg_left;
                self.iov_idx += 1;
                self.iov_off = 0;
            } else {
                self.iov_off += n;
                n = 0;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// import_iovec — copy iovec array from user space
// ---------------------------------------------------------------------------

/// Maximum number of iovec segments per call (matches Linux UIO_MAXIOV).
pub const UIO_MAXIOV: usize = 1024;

/// Error constant.
const EINVAL: i32 = 22;
const EFAULT: i32 = 14;
const EMSGSIZE: i32 = 90;

/// Copy a user-space iovec array into a kernel-side `Vec<IoVec>`.
///
/// # Safety
/// `user_ptr` must point to an array of `nr_segs` `UserIoVec` structs
/// in user-space memory.
pub unsafe fn import_iovec(
    user_ptr: u64,
    nr_segs: usize,
    max_segs: usize,
) -> Result<Vec<IoVec>, i32> {
    if nr_segs == 0 {
        return Ok(Vec::new());
    }
    if nr_segs > max_segs.min(UIO_MAXIOV) {
        return Err(EINVAL);
    }

    let src = UserPtr::from_addr(user_ptr as usize);
    if !src.range_in_user_space(nr_segs * core::mem::size_of::<UserIoVec>()) {
        return Err(EFAULT);
    }

    let mut result = Vec::with_capacity(nr_segs);

    for i in 0..nr_segs {
        let uiov_ptr = src.add(i * core::mem::size_of::<UserIoVec>());
        let mut uiov = UserIoVec::default();

        let not_copied = unsafe {
            copy_from_user(
                &mut uiov as *mut UserIoVec as *mut u8,
                uiov_ptr,
                core::mem::size_of::<UserIoVec>(),
            )
        };
        if not_copied != 0 {
            return Err(EFAULT);
        }

        // Validate length doesn't overflow isize.
        if uiov.iov_len > isize::MAX as u64 {
            return Err(EINVAL);
        }

        result.push(IoVec {
            iov_base: uiov.iov_base as *mut u8,
            iov_len: uiov.iov_len as usize,
        });
    }

    Ok(result)
}
