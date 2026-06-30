//! Clean syscall ABI layer types.
//!
//! Typed wrappers for user-space pointers from syscall registers.
//! All pointer/slice types validate user addresses before dereference.

use crate::memory::user_space::UserSpaceMemory;
use crate::syscall::SyscallError;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::marker::PhantomData;

/// Raw syscall arguments from user registers (rdi, rsi, rdx, r10, r8, r9).
#[derive(Debug, Clone, Copy)]
pub struct SyscallArgs {
    pub raw: [u64; 6],
}

impl SyscallArgs {
    #[inline]
    pub fn new(raw: [u64; 6]) -> Self {
        Self { raw }
    }

    #[inline]
    pub fn from_slice(args: &[u64]) -> Self {
        let mut raw = [0u64; 6];
        let n = core::cmp::min(args.len(), 6);
        raw[..n].copy_from_slice(&args[..n]);
        Self { raw }
    }

    #[inline]
    pub fn arg0(&self) -> u64 {
        self.raw[0]
    }
    #[inline]
    pub fn arg1(&self) -> u64 {
        self.raw[1]
    }
    #[inline]
    pub fn arg2(&self) -> u64 {
        self.raw[2]
    }
    #[inline]
    pub fn arg3(&self) -> u64 {
        self.raw[3]
    }
    #[inline]
    pub fn arg4(&self) -> u64 {
        self.raw[4]
    }
    #[inline]
    pub fn arg5(&self) -> u64 {
        self.raw[5]
    }

    #[inline]
    pub fn arg0_ptr<T>(&self) -> UserPtr<T> {
        UserPtr::new(self.raw[0])
    }
    #[inline]
    pub fn arg1_ptr<T>(&self) -> UserPtr<T> {
        UserPtr::new(self.raw[1])
    }
    #[inline]
    pub fn arg2_ptr<T>(&self) -> UserPtr<T> {
        UserPtr::new(self.raw[2])
    }

    pub fn arg0_cstr(&self, max_len: usize) -> Result<UserCStr, SyscallError> {
        UserCStr::from_user(self.raw[0], max_len)
    }
    pub fn arg1_cstr(&self, max_len: usize) -> Result<UserCStr, SyscallError> {
        UserCStr::from_user(self.raw[1], max_len)
    }
    pub fn arg0_slice_in<T: Copy>(&self, count: usize) -> Result<UserSliceIn<T>, SyscallError> {
        UserSliceIn::new(self.raw[0], count)
    }
    pub fn arg1_slice_out<T: Copy>(&self, count: usize) -> Result<UserSliceOut<T>, SyscallError> {
        UserSliceOut::new(self.raw[1], count)
    }
    pub fn arg2_slice_out<T: Copy>(&self, count: usize) -> Result<UserSliceOut<T>, SyscallError> {
        UserSliceOut::new(self.raw[2], count)
    }
}

impl From<&[u64]> for SyscallArgs {
    #[inline]
    fn from(args: &[u64]) -> Self {
        Self::from_slice(args)
    }
}

/// A validated pointer to a single `T` in user space.
#[derive(Debug)]
pub struct UserPtr<T> {
    addr: u64,
    _marker: PhantomData<fn() -> T>,
}

impl<T> UserPtr<T> {
    #[inline]
    pub fn new(addr: u64) -> Self {
        Self {
            addr,
            _marker: PhantomData,
        }
    }

    #[inline]
    pub fn null() -> Self {
        Self::new(0)
    }

    #[inline]
    pub fn is_null(&self) -> bool {
        self.addr == 0
    }

    #[inline]
    pub fn addr(&self) -> u64 {
        self.addr
    }

    pub fn read(&self) -> Result<T, SyscallError>
    where
        T: Copy,
    {
        if self.is_null() {
            return Err(SyscallError::InvalidAddress);
        }
        let size = core::mem::size_of::<T>();
        UserSpaceMemory::validate_user_ptr(self.addr, size as u64, false)?;
        let mut buf = vec![0u8; size];
        UserSpaceMemory::copy_from_user(self.addr, &mut buf)?;
        unsafe {
            let mut aligned = core::mem::MaybeUninit::<T>::uninit();
            core::ptr::copy_nonoverlapping(buf.as_ptr(), aligned.as_mut_ptr() as *mut u8, size);
            Ok(aligned.assume_init())
        }
    }

    pub fn write(&self, value: &T) -> Result<(), SyscallError>
    where
        T: Copy,
    {
        if self.is_null() {
            return Err(SyscallError::InvalidAddress);
        }
        let size = core::mem::size_of::<T>();
        UserSpaceMemory::validate_user_ptr(self.addr, size as u64, true)?;
        let mut buf = vec![0u8; size];
        unsafe {
            core::ptr::copy_nonoverlapping(value as *const T as *const u8, buf.as_mut_ptr(), size);
        }
        UserSpaceMemory::copy_to_user(self.addr, &buf)
    }
}

impl<T> Clone for UserPtr<T> {
    #[inline]
    fn clone(&self) -> Self {
        Self::new(self.addr)
    }
}
impl<T> Copy for UserPtr<T> {}

/// A validated read-only slice of `T` in user space.
#[derive(Debug)]
pub struct UserSliceIn<T> {
    addr: u64,
    count: usize,
    _marker: PhantomData<fn() -> T>,
}

impl<T: Copy> UserSliceIn<T> {
    pub fn new(addr: u64, count: usize) -> Result<Self, SyscallError> {
        if count == 0 {
            return Ok(Self {
                addr,
                count: 0,
                _marker: PhantomData,
            });
        }
        if addr == 0 {
            return Err(SyscallError::InvalidAddress);
        }
        let byte_len = count
            .checked_mul(core::mem::size_of::<T>())
            .ok_or(SyscallError::InvalidArgument)?;
        UserSpaceMemory::validate_user_ptr(addr, byte_len as u64, false)?;
        Ok(Self {
            addr,
            count,
            _marker: PhantomData,
        })
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.count
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn copy_in(&self) -> Result<Vec<T>, SyscallError> {
        if self.count == 0 {
            return Ok(Vec::new());
        }
        let byte_len = self.count * core::mem::size_of::<T>();
        let mut buf = vec![0u8; byte_len];
        UserSpaceMemory::copy_from_user(self.addr, &mut buf)?;
        let mut result: Vec<T> = Vec::with_capacity(self.count);
        unsafe {
            for i in 0..self.count {
                let src = buf.as_ptr().add(i * core::mem::size_of::<T>());
                let dst = result.as_mut_ptr().add(i) as *mut u8;
                core::ptr::copy_nonoverlapping(src, dst, core::mem::size_of::<T>());
            }
            result.set_len(self.count);
        }
        Ok(result)
    }

    pub fn copy_into(&self, dest: &mut [T]) -> Result<usize, SyscallError> {
        let n = core::cmp::min(self.count, dest.len());
        if n == 0 {
            return Ok(0);
        }
        let byte_len = n * core::mem::size_of::<T>();
        let mut buf = vec![0u8; byte_len];
        UserSpaceMemory::copy_from_user(self.addr, &mut buf)?;
        unsafe {
            for i in 0..n {
                let src = buf.as_ptr().add(i * core::mem::size_of::<T>());
                let dst = dest.as_mut_ptr().add(i) as *mut u8;
                core::ptr::copy_nonoverlapping(src, dst, core::mem::size_of::<T>());
            }
        }
        Ok(n)
    }
}

/// A validated writable slice of `T` in user space.
#[derive(Debug)]
pub struct UserSliceOut<T> {
    addr: u64,
    count: usize,
    _marker: PhantomData<fn() -> T>,
}

impl<T: Copy> UserSliceOut<T> {
    pub fn new(addr: u64, count: usize) -> Result<Self, SyscallError> {
        if count == 0 {
            return Ok(Self {
                addr,
                count: 0,
                _marker: PhantomData,
            });
        }
        if addr == 0 {
            return Err(SyscallError::InvalidAddress);
        }
        let byte_len = count
            .checked_mul(core::mem::size_of::<T>())
            .ok_or(SyscallError::InvalidArgument)?;
        UserSpaceMemory::validate_user_ptr(addr, byte_len as u64, true)?;
        Ok(Self {
            addr,
            count,
            _marker: PhantomData,
        })
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.count
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn copy_out(&self, src: &[T]) -> Result<usize, SyscallError> {
        let n = core::cmp::min(self.count, src.len());
        if n == 0 {
            return Ok(0);
        }
        let byte_len = n * core::mem::size_of::<T>();
        let mut buf = vec![0u8; byte_len];
        unsafe {
            for i in 0..n {
                let s = src.as_ptr().add(i) as *const u8;
                let d = buf.as_mut_ptr().add(i * core::mem::size_of::<T>());
                core::ptr::copy_nonoverlapping(s, d, core::mem::size_of::<T>());
            }
        }
        UserSpaceMemory::copy_to_user(self.addr, &buf)?;
        Ok(n)
    }

    /// Write `count` zero-valued `T`s to the user buffer.
    pub fn zero(&self) -> Result<usize, SyscallError> {
        if self.count == 0 {
            return Ok(0);
        }
        let byte_len = self.count * core::mem::size_of::<T>();
        let buf = vec![0u8; byte_len];
        UserSpaceMemory::copy_to_user(self.addr, &buf)?;
        Ok(self.count)
    }
}

/// A NUL-terminated C string copied from user space.
///
/// The string is copied into kernel memory on construction, so it is safe to
/// hold across `yield` points.
#[derive(Debug, Clone)]
pub struct UserCStr {
    inner: String,
}

impl UserCStr {
    /// Copy a NUL-terminated string from user space, up to `max_len` bytes
    /// (not including the NUL terminator).
    pub fn from_user(addr: u64, max_len: usize) -> Result<Self, SyscallError> {
        if addr == 0 {
            return Err(SyscallError::InvalidAddress);
        }
        let s = UserSpaceMemory::copy_string_from_user(addr, max_len)?;
        Ok(Self { inner: s })
    }

    /// The string content (without NUL terminator).
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.inner
    }

    /// Consume into an owned `String`.
    #[inline]
    pub fn into_string(self) -> String {
        self.inner
    }

    /// Length in bytes (excluding NUL).
    #[inline]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
}

impl core::fmt::Display for UserCStr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Display::fmt(&self.inner, f)
    }
}

impl AsRef<str> for UserCStr {
    #[inline]
    fn as_ref(&self) -> &str {
        &self.inner
    }
}
