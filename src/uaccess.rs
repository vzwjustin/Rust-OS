// SPDX-License-Identifier: MIT
// RustOS user-space memory access (ported from Linux rust/kernel/uaccess.rs)
// All bindings:: calls replaced with native x86_64 implementations.

#![allow(dead_code, unused_variables)]

use core::arch::asm;

// ---------------------------------------------------------------------------
// Address-space constants (x86_64 canonical addresses)
// ---------------------------------------------------------------------------

/// Highest valid user-space virtual address on x86_64 (47-bit user space).
pub const USER_SPACE_TOP: usize = 0x0000_7FFF_FFFF_FFFF;

/// First kernel-space virtual address on x86_64 (canonical hole boundary).
pub const KERNEL_SPACE_START: usize = 0xFFFF_8000_0000_0000;

// ---------------------------------------------------------------------------
// UserPtr
// ---------------------------------------------------------------------------

/// A pointer into user-space memory.
#[repr(transparent)]
#[derive(Clone, Copy)]
pub struct UserPtr(*mut u8);

impl UserPtr {
    /// Construct from a raw address.
    #[inline]
    pub fn from_addr(addr: usize) -> Self {
        UserPtr(addr as *mut u8)
    }

    /// Raw address value.
    #[inline]
    pub fn addr(&self) -> usize {
        self.0 as usize
    }

    /// Advance the pointer by `offset` bytes.
    #[inline]
    pub fn add(&self, offset: usize) -> Self {
        UserPtr(self.0.wrapping_add(offset))
    }

    /// Return `true` if the pointer is null.
    #[inline]
    pub fn is_null(&self) -> bool {
        self.0.is_null()
    }

    /// Return `true` if the address falls within the user-space range.
    #[inline]
    pub fn is_valid_user_addr(&self) -> bool {
        let addr = self.0 as usize;
        addr <= USER_SPACE_TOP
    }

    /// Return `true` if `[self, self+len)` lies entirely in user space.
    #[inline]
    pub fn range_in_user_space(&self, len: usize) -> bool {
        let start = self.0 as usize;
        let end = start.saturating_add(len);
        end > 0 && end <= USER_SPACE_TOP + 1
    }
}

// ---------------------------------------------------------------------------
// x86_64 user-copy primitives
// ---------------------------------------------------------------------------
// These use `rep movsb` / `rep stosb` with x86 exception-table style error
// handling stubbed for now (a full implementation would use .fixup sections).
// The return value is the number of bytes NOT copied (0 = success, Linux convention).

/// Copy `len` bytes from user address `src` into kernel buffer `dst`.
/// Returns the number of bytes that could NOT be copied (0 on full success).
///
/// # Safety
/// - `dst` must be valid for `len` bytes of writes.
/// - `src` must be a valid user-space address range for `len` bytes.
pub unsafe fn copy_from_user(dst: *mut u8, src: UserPtr, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    if !src.range_in_user_space(len) {
        return len;
    }
    unsafe {
        let mut remaining: usize = 0;
        asm!(
            "rep movsb",
            inout("rcx") len => remaining,
            inout("rdi") dst => _,
            inout("rsi") src.0 => _,
            options(nostack, preserves_flags),
        );
        remaining
    }
}

/// Copy `len` bytes from kernel buffer `src` into user address `dst`.
/// Returns the number of bytes that could NOT be copied (0 on full success).
///
/// # Safety
/// - `src` must be valid for `len` bytes of reads.
/// - `dst` must be a valid user-space address range for `len` bytes.
pub unsafe fn copy_to_user(dst: UserPtr, src: *const u8, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    if !dst.range_in_user_space(len) {
        return len;
    }
    unsafe {
        let mut remaining: usize = 0;
        asm!(
            "rep movsb",
            inout("rcx") len => remaining,
            inout("rdi") dst.0 => _,
            inout("rsi") src => _,
            options(nostack, preserves_flags),
        );
        remaining
    }
}

/// Zero `len` bytes at user address `dst`.
/// Returns the number of bytes that could NOT be zeroed (0 on full success).
///
/// # Safety
/// `dst` must be a valid user-space address range for `len` bytes.
pub unsafe fn clear_user(dst: UserPtr, len: usize) -> usize {
    if len == 0 {
        return 0;
    }
    if !dst.range_in_user_space(len) {
        return len;
    }
    unsafe {
        let mut remaining: usize = 0;
        asm!(
            "xor eax, eax",
            "rep stosb",
            inout("rcx") len => remaining,
            inout("rdi") dst.0 => _,
            out("rax") _,
            options(nostack, preserves_flags),
        );
        remaining
    }
}

// ---------------------------------------------------------------------------
// UserSlice
// ---------------------------------------------------------------------------

/// A readable/writable region of user-space memory.
pub struct UserSlice {
    ptr: UserPtr,
    len: usize,
}

impl UserSlice {
    /// Construct a new `UserSlice`.
    pub fn new(ptr: UserPtr, len: usize) -> Self {
        UserSlice { ptr, len }
    }

    /// Turn into a reader.
    pub fn reader(self) -> UserSliceReader {
        UserSliceReader {
            ptr: self.ptr,
            remaining: self.len,
        }
    }

    /// Turn into a writer.
    pub fn writer(self) -> UserSliceWriter {
        UserSliceWriter {
            ptr: self.ptr,
            remaining: self.len,
        }
    }

    /// Split into a reader and a writer sharing the same region.
    pub fn reader_writer(self) -> (UserSliceReader, UserSliceWriter) {
        let r = UserSliceReader {
            ptr: self.ptr,
            remaining: self.len,
        };
        let w = UserSliceWriter {
            ptr: self.ptr,
            remaining: self.len,
        };
        (r, w)
    }

    /// Length of the slice in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether the slice is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
}

// ---------------------------------------------------------------------------
// UserSliceReader
// ---------------------------------------------------------------------------

/// Sequential reader for a user-space memory region.
pub struct UserSliceReader {
    ptr: UserPtr,
    remaining: usize,
}

impl UserSliceReader {
    /// How many bytes remain to be read.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.remaining
    }

    /// Copy bytes from user memory into `dst`.
    ///
    /// # Safety
    /// The user-space range must be valid and readable.
    pub unsafe fn read_raw(&mut self, dst: &mut [u8]) -> Result<(), i32> {
        let len = dst.len();
        if len > self.remaining {
            return Err(EFAULT);
        }
        // SAFETY: caller guarantees validity
        let not_copied = unsafe { copy_from_user(dst.as_mut_ptr(), self.ptr, len) };
        if not_copied != 0 {
            return Err(EFAULT);
        }
        self.ptr = self.ptr.add(len);
        self.remaining -= len;
        Ok(())
    }

    /// Read a value of type `T` from user memory.
    ///
    /// # Safety
    /// The user-space range must be valid and contain a readable `T`.
    pub unsafe fn read<T: Copy>(&mut self) -> Result<T, i32> {
        let size = core::mem::size_of::<T>();
        if size > self.remaining {
            return Err(EFAULT);
        }
        let mut val = core::mem::MaybeUninit::<T>::uninit();
        // SAFETY: size matches T; caller ensures validity
        let not_copied = unsafe { copy_from_user(val.as_mut_ptr() as *mut u8, self.ptr, size) };
        if not_copied != 0 {
            return Err(EFAULT);
        }
        self.ptr = self.ptr.add(size);
        self.remaining -= size;
        Ok(unsafe { val.assume_init() })
    }

    /// Advance the reader by `n` bytes without copying.
    pub fn skip(&mut self, n: usize) -> Result<(), i32> {
        if n > self.remaining {
            return Err(EFAULT);
        }
        self.ptr = self.ptr.add(n);
        self.remaining -= n;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// UserSliceWriter
// ---------------------------------------------------------------------------

/// Sequential writer for a user-space memory region.
pub struct UserSliceWriter {
    ptr: UserPtr,
    remaining: usize,
}

impl UserSliceWriter {
    /// How many bytes remain to be written.
    #[inline]
    pub fn remaining(&self) -> usize {
        self.remaining
    }

    /// Copy bytes from `src` into user memory.
    ///
    /// # Safety
    /// The user-space range must be valid and writable.
    pub unsafe fn write_raw(&mut self, src: &[u8]) -> Result<(), i32> {
        let len = src.len();
        if len > self.remaining {
            return Err(EFAULT);
        }
        let not_copied = unsafe { copy_to_user(self.ptr, src.as_ptr(), len) };
        if not_copied != 0 {
            return Err(EFAULT);
        }
        self.ptr = self.ptr.add(len);
        self.remaining -= len;
        Ok(())
    }

    /// Write a value of type `T` into user memory.
    ///
    /// # Safety
    /// The user-space range must be valid and writable.
    pub unsafe fn write<T: Copy>(&mut self, val: &T) -> Result<(), i32> {
        let size = core::mem::size_of::<T>();
        if size > self.remaining {
            return Err(EFAULT);
        }
        let not_copied = unsafe { copy_to_user(self.ptr, val as *const T as *const u8, size) };
        if not_copied != 0 {
            return Err(EFAULT);
        }
        self.ptr = self.ptr.add(size);
        self.remaining -= size;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Error constants
// ---------------------------------------------------------------------------

const EFAULT: i32 = 14;
const EINVAL: i32 = 22;
