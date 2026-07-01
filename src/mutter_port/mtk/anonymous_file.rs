//! Port of mutter's `mtk/mtk/mtk-anonymous-file.{c,h}` to idiomatic Rust.
//!
//! `MtkAnonymousFile` wraps a memfd-based anonymous, read-only file used by
//! the compositor to send mid-sized data buffers to Wayland clients via file
//! descriptors.
//!
//! # What's ported
//!
//! - `MtkAnonymousFileMapmode` enum (`PRIVATE` / `SHARED`).
//! - `mtk_anonymous_file_new` — creates the memfd, writes data, seals it.
//! - `mtk_anonymous_file_free` — closes the fd (implemented via `Drop`).
//! - `mtk_anonymous_file_open_fd` — returns the fd for `PRIVATE` or dups for `SHARED`.
//! - `mtk_anonymous_file_close_fd` — closes the fd unless it is the original.
//! - `mtk_anonymous_file_size` — returns the stored size.
//!
//! # What's skipped
//!
//! - The `HAVE_MEMFD_CREATE` fallback path (tmpfile + `O_TMPFILE`).
//! - GLib error reporting (`g_set_error`) replaced by `Result`.

#![allow(dead_code)]

use core::ptr;

const SYS_MEMFD_CREATE: i64 = 319;
const SYS_FTRUNCATE: i64 = 76;
const SYS_MMAP: i64 = 9;
const SYS_MUNMAP: i64 = 11;
const SYS_CLOSE: i64 = 3;
const SYS_FCNTL: i64 = 72;

const MFD_CLOEXEC: u32 = 0x0001;
const MFD_ALLOW_SEALING: u32 = 0x0002;

const F_SEAL_SEAL: i32 = 0x0001;
const F_SEAL_SHRINK: i32 = 0x0002;
const F_SEAL_GROW: i32 = 0x0004;
const F_SEAL_WRITE: i32 = 0x0008;
const READONLY_SEALS: i32 = F_SEAL_SEAL | F_SEAL_SHRINK | F_SEAL_GROW | F_SEAL_WRITE;

const F_ADD_SEALS: i32 = 1033;
const F_DUPFD_CLOEXEC: i32 = 1030;

const PROT_WRITE: i32 = 0x2;
const MAP_SHARED: i32 = 0x01;
const MAP_FAILED: isize = -1;
const INVALID_FD: i32 = -1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnonymousFileError {
    MemfdCreateFailed(i32),
    FtruncateFailed(i32),
    MmapFailed(i32),
    MunmapFailed(i32),
    FcntlFailed(i32),
    UnsupportedArch,
    InvalidFd,
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall2(num: i64, arg1: i64, arg2: i64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "syscall",
        inlateout("rax") num => ret,
        in("rdi") arg1,
        in("rsi") arg2,
        lateout("rcx") _,
        lateout("r11") _,
    );
    ret
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall3(num: i64, arg1: i64, arg2: i64, arg3: i64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "syscall",
        inlateout("rax") num => ret,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        lateout("rcx") _,
        lateout("r11") _,
    );
    ret
}

#[cfg(target_arch = "x86_64")]
#[inline(always)]
unsafe fn syscall6(num: i64, arg1: i64, arg2: i64, arg3: i64, arg4: i64, arg5: i64, arg6: i64) -> i64 {
    let ret: i64;
    core::arch::asm!(
        "syscall",
        inlateout("rax") num => ret,
        in("rdi") arg1,
        in("rsi") arg2,
        in("rdx") arg3,
        in("r10") arg4,
        in("r8") arg5,
        in("r9") arg6,
        lateout("rcx") _,
        lateout("r11") _,
    );
    ret
}

fn syscall_ret(raw: i64) -> Result<i32, i32> {
    if raw >= 0 { Ok(raw as i32) } else { Err(-(raw) as i32) }
}

fn sys_memfd_create(name: &str, flags: u32) -> Result<i32, AnonymousFileError> {
    let mut buf: [u8; 32] = [0u8; 32];
    let bytes = name.as_bytes();
    let len = bytes.len().min(buf.len() - 1);
    buf[..len].copy_from_slice(&bytes[..len]);
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: buf is a valid stack-allocated NUL-terminated array.
        let raw = unsafe { syscall2(SYS_MEMFD_CREATE, buf.as_ptr() as i64, flags as i64) };
        syscall_ret(raw).map_err(AnonymousFileError::MemfdCreateFailed)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (name, flags, &buf);
        Err(AnonymousFileError::UnsupportedArch)
    }
}

fn sys_ftruncate(fd: i32, length: usize) -> Result<i32, AnonymousFileError> {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: fd is a valid open file descriptor, length is non-negative.
        let raw = unsafe { syscall2(SYS_FTRUNCATE, fd as i64, length as i64) };
        syscall_ret(raw).map_err(AnonymousFileError::FtruncateFailed)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (fd, length);
        Err(AnonymousFileError::UnsupportedArch)
    }
}

fn sys_mmap(addr: *mut u8, length: usize, prot: i32, flags: i32, fd: i32, offset: i64) -> Result<*mut u8, AnonymousFileError> {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: addr may be NULL; fd must be valid; offset page-aligned.
        let raw = unsafe { syscall6(SYS_MMAP, addr as i64, length as i64, prot as i64, flags as i64, fd as i64, offset) };
        if raw == MAP_FAILED as i64 { Err(AnonymousFileError::MmapFailed(0)) } else { Ok(raw as *mut u8) }
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (addr, length, prot, flags, fd, offset);
        Err(AnonymousFileError::UnsupportedArch)
    }
}

fn sys_munmap(addr: *mut u8, length: usize) -> Result<i32, AnonymousFileError> {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: addr must be a page-aligned address from mmap, length matches.
        let raw = unsafe { syscall2(SYS_MUNMAP, addr as i64, length as i64) };
        syscall_ret(raw).map_err(AnonymousFileError::MunmapFailed)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (addr, length);
        Err(AnonymousFileError::UnsupportedArch)
    }
}

fn sys_close(fd: i32) -> Result<i32, AnonymousFileError> {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: closing an invalid fd returns EBADF, which is harmless.
        let raw = unsafe { syscall2(SYS_CLOSE, fd as i64, 0) };
        syscall_ret(raw).map_err(|_| AnonymousFileError::InvalidFd)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = fd;
        Err(AnonymousFileError::UnsupportedArch)
    }
}

fn sys_fcntl(fd: i32, cmd: i32, arg: i32) -> Result<i32, AnonymousFileError> {
    #[cfg(target_arch = "x86_64")]
    {
        // SAFETY: fd should be valid; cmd/arg are valid fcntl pair.
        let raw = unsafe { syscall3(SYS_FCNTL, fd as i64, cmd as i64, arg as i64) };
        syscall_ret(raw).map_err(AnonymousFileError::FcntlFailed)
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        let _ = (fd, cmd, arg);
        Err(AnonymousFileError::UnsupportedArch)
    }
}

/// How the file descriptor will be mapped by the receiver.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnonymousFileMapmode {
    Private,
    Shared,
}

/// Port of `MtkAnonymousFile`: an anonymous, sealed, read-only file.
#[derive(Debug)]
pub struct AnonymousFile {
    fd: i32,
    size: usize,
}

impl AnonymousFile {
    /// Port of `mtk_anonymous_file_new`.
    pub fn new(size: usize, data: &[u8]) -> Result<Self, AnonymousFileError> {
        let fd = sys_memfd_create("mtk-anonymous-file", MFD_CLOEXEC | MFD_ALLOW_SEALING)?;
        if size > 0 {
            sys_ftruncate(fd, size)?;
            let mapped = sys_mmap(ptr::null_mut(), size, PROT_WRITE, MAP_SHARED, fd, 0)?;
            // SAFETY: mapped is a valid writable mapping of `size` bytes.
            unsafe { ptr::copy_nonoverlapping(data.as_ptr(), mapped, size); }
            sys_munmap(mapped, size)?;
            sys_fcntl(fd, F_ADD_SEALS, READONLY_SEALS)?;
        }
        Ok(AnonymousFile { fd, size })
    }

    /// Creates an `AnonymousFile` from an already-open file descriptor.
    pub fn from_fd(fd: i32, size: usize) -> Self {
        AnonymousFile { fd, size }
    }

    /// Port of `mtk_anonymous_file_size`.
    pub fn size(&self) -> usize { self.size }

    /// Returns the raw file descriptor, or `-1` if freed.
    pub fn fd(&self) -> i32 { self.fd }

    /// Returns `true` if the file descriptor is still open.
    pub fn is_open(&self) -> bool { self.fd != INVALID_FD }

    /// Port of `mtk_anonymous_file_open_fd`.
    pub fn open_fd(&self, mapmode: AnonymousFileMapmode) -> Result<i32, AnonymousFileError> {
        if self.fd == INVALID_FD { return Err(AnonymousFileError::InvalidFd); }
        match mapmode {
            AnonymousFileMapmode::Private => Ok(self.fd),
            AnonymousFileMapmode::Shared => sys_fcntl(self.fd, F_DUPFD_CLOEXEC, 0),
        }
    }

    /// Port of `mtk_anonymous_file_close_fd`.
    pub fn close_fd(&self, fd: i32) -> Result<i32, AnonymousFileError> {
        if fd == self.fd { Ok(0) } else { sys_close(fd) }
    }

    /// Port of `mtk_anonymous_file_free`.
    pub fn free(&mut self) -> Result<i32, AnonymousFileError> {
        if self.fd != INVALID_FD {
            let result = sys_close(self.fd);
            self.fd = INVALID_FD;
            result
        } else {
            Ok(0)
        }
    }
}

impl Drop for AnonymousFile {
    fn drop(&mut self) { let _ = self.free(); }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_fd_and_size() {
        let file = AnonymousFile::from_fd(42, 1024);
        assert_eq!(file.size(), 1024);
        assert_eq!(file.fd(), 42);
        assert!(file.is_open());
    }

    #[test]
    fn test_open_fd_private_returns_same_fd() {
        let file = AnonymousFile::from_fd(7, 100);
        let fd = file.open_fd(AnonymousFileMapmode::Private).unwrap();
        assert_eq!(fd, 7);
    }

    #[test]
    fn test_close_fd_private_does_not_close() {
        let file = AnonymousFile::from_fd(7, 100);
        let result = file.close_fd(7);
        assert!(result.is_ok());
        assert!(file.is_open());
    }

    #[test]
    fn test_open_fd_on_invalid_fd_returns_error() {
        let mut file = AnonymousFile::from_fd(7, 100);
        file.fd = INVALID_FD;
        assert_eq!(file.open_fd(AnonymousFileMapmode::Private), Err(AnonymousFileError::InvalidFd));
    }

    #[test]
    fn test_mapmode_equality() {
        assert_ne!(AnonymousFileMapmode::Private, AnonymousFileMapmode::Shared);
    }

    #[test]
    fn test_readonly_seals_constant() {
        assert_eq!(READONLY_SEALS, F_SEAL_SEAL | F_SEAL_SHRINK | F_SEAL_GROW | F_SEAL_WRITE);
    }
}
