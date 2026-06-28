//! Advanced I/O operations
//!
//! This module implements advanced Linux I/O operations including
//! vectored I/O, positional I/O, zero-copy operations, and extended attributes.

extern crate alloc;

use core::sync::atomic::{AtomicU64, Ordering};

use super::types::*;
use super::{LinuxError, LinuxResult};
use crate::vfs;

fn vfs_error_to_linux(err: crate::vfs::VfsError) -> LinuxError {
    match err {
        crate::vfs::VfsError::NotFound => LinuxError::ENOENT,
        crate::vfs::VfsError::PermissionDenied => LinuxError::EACCES,
        crate::vfs::VfsError::AlreadyExists => LinuxError::EEXIST,
        crate::vfs::VfsError::NotDirectory => LinuxError::ENOTDIR,
        crate::vfs::VfsError::IsDirectory => LinuxError::EISDIR,
        crate::vfs::VfsError::InvalidArgument => LinuxError::EINVAL,
        crate::vfs::VfsError::IoError => LinuxError::EIO,
        crate::vfs::VfsError::NoSpace => LinuxError::ENOSPC,
        crate::vfs::VfsError::TooManyFiles => LinuxError::EMFILE,
        crate::vfs::VfsError::BadFileDescriptor => LinuxError::EBADF,
        crate::vfs::VfsError::InvalidSeek => LinuxError::EINVAL,
        crate::vfs::VfsError::NameTooLong => LinuxError::ENAMETOOLONG,
        crate::vfs::VfsError::CrossDevice => LinuxError::EXDEV,
        crate::vfs::VfsError::ReadOnly => LinuxError::EROFS,
        crate::vfs::VfsError::NotSupported => LinuxError::ENOSYS,
        crate::vfs::VfsError::DirectoryNotEmpty => LinuxError::ENOTEMPTY,
    }
}

/// Helper to convert null-terminated C string to Rust string
unsafe fn c_str_to_string(ptr: *const u8) -> Result<alloc::string::String, LinuxError> {
    if ptr.is_null() {
        return Err(LinuxError::EFAULT);
    }

    const MAX_C_STRING_LEN: usize = 4096;
    let mut bytes = alloc::vec::Vec::new();
    for offset in 0..MAX_C_STRING_LEN {
        let mut byte = [0u8; 1];
        crate::memory::user_space::UserSpaceMemory::copy_from_user(
            ptr as u64 + offset as u64,
            &mut byte,
        )
        .map_err(|_| LinuxError::EFAULT)?;

        if byte[0] == 0 {
            return alloc::string::String::from_utf8(bytes).map_err(|_| LinuxError::EINVAL);
        }

        bytes.push(byte[0]);
    }

    Err(LinuxError::ENAMETOOLONG)
}

/// Operation counter for statistics
static ADVANCED_IO_COUNT: AtomicU64 = AtomicU64::new(0);

/// Initialize advanced I/O subsystem
pub fn init_advanced_io() {
    ADVANCED_IO_COUNT.store(0, Ordering::Relaxed);
}

/// Get number of advanced I/O operations performed
pub fn get_operation_count() -> u64 {
    ADVANCED_IO_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    ADVANCED_IO_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// I/O vector for vectored I/O operations
#[repr(C)]
pub struct IoVec {
    pub iov_base: *mut u8,
    pub iov_len: usize,
}

fn copy_iov_from_user(iov: *const IoVec, index: i32) -> LinuxResult<IoVec> {
    let offset = (index as usize)
        .checked_mul(core::mem::size_of::<IoVec>())
        .ok_or(LinuxError::EFAULT)?;
    let user_ptr = (iov as u64)
        .checked_add(offset as u64)
        .ok_or(LinuxError::EFAULT)?;
    let mut entry = IoVec {
        iov_base: core::ptr::null_mut(),
        iov_len: 0,
    };
    let entry_bytes = unsafe {
        core::slice::from_raw_parts_mut(
            (&mut entry as *mut IoVec).cast::<u8>(),
            core::mem::size_of::<IoVec>(),
        )
    };

    crate::memory::user_space::UserSpaceMemory::copy_from_user(user_ptr, entry_bytes)
        .map_err(|_| LinuxError::EFAULT)?;
    Ok(entry)
}

fn copy_buffer_from_user(ptr: *const u8, len: usize) -> LinuxResult<alloc::vec::Vec<u8>> {
    let mut data = alloc::vec::Vec::new();
    data.resize(len, 0);
    crate::memory::user_space::UserSpaceMemory::copy_from_user(ptr as u64, &mut data)
        .map_err(|_| LinuxError::EFAULT)?;
    Ok(data)
}

fn copy_buffer_to_user(ptr: *mut u8, data: &[u8]) -> LinuxResult<()> {
    crate::memory::user_space::UserSpaceMemory::copy_to_user(ptr as u64, data)
        .map_err(|_| LinuxError::EFAULT)
}

// ============================================================================
// Positional I/O (pread/pwrite)
// ============================================================================

/// pread - read from file at given offset without changing file position
pub fn pread(fd: Fd, buf: *mut u8, count: usize, offset: Off) -> LinuxResult<isize> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if buf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if offset < 0 {
        return Err(LinuxError::EINVAL);
    }

    let mut buffer = alloc::vec::Vec::new();
    buffer.resize(count, 0);

    let bytes_read = vfs::vfs_pread(fd, &mut buffer, offset as u64).map_err(vfs_error_to_linux)?;
    crate::memory::user_space::UserSpaceMemory::copy_to_user(buf as u64, &buffer[..bytes_read])
        .map_err(|_| LinuxError::EFAULT)?;

    Ok(bytes_read as isize)
}

/// pwrite - write to file at given offset without changing file position
pub fn pwrite(fd: Fd, buf: *const u8, count: usize, offset: Off) -> LinuxResult<isize> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if buf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if offset < 0 {
        return Err(LinuxError::EINVAL);
    }

    let mut data = alloc::vec::Vec::new();
    data.resize(count, 0);
    crate::memory::user_space::UserSpaceMemory::copy_from_user(buf as u64, &mut data)
        .map_err(|_| LinuxError::EFAULT)?;

    vfs::vfs_pwrite(fd, &data, offset as u64)
        .map(|n| n as isize)
        .map_err(vfs_error_to_linux)
}

/// preadv - read data into multiple buffers from given offset
pub fn preadv(fd: Fd, iov: *const IoVec, iovcnt: i32, offset: Off) -> LinuxResult<isize> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if iov.is_null() || iovcnt <= 0 {
        return Err(LinuxError::EINVAL);
    }

    if offset < 0 {
        return Err(LinuxError::EINVAL);
    }

    let mut total = 0isize;
    let mut cur_offset = offset as u64;
    for i in 0..iovcnt as isize {
        let iov = copy_iov_from_user(iov, i as i32)?;
        if iov.iov_len == 0 {
            continue;
        }
        let mut buf = alloc::vec::Vec::new();
        buf.resize(iov.iov_len, 0);
        let n = vfs::vfs_pread(fd, &mut buf, cur_offset).map_err(vfs_error_to_linux)?;
        copy_buffer_to_user(iov.iov_base, &buf[..n])?;
        total += n as isize;
        cur_offset += n as u64;
        if n < iov.iov_len {
            break;
        }
    }
    Ok(total)
}

/// pwritev - write data from multiple buffers to given offset
pub fn pwritev(fd: Fd, iov: *const IoVec, iovcnt: i32, offset: Off) -> LinuxResult<isize> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if iov.is_null() || iovcnt <= 0 {
        return Err(LinuxError::EINVAL);
    }

    if offset < 0 {
        return Err(LinuxError::EINVAL);
    }

    let mut total = 0isize;
    let mut cur_offset = offset as u64;
    for i in 0..iovcnt as isize {
        let iov = copy_iov_from_user(iov, i as i32)?;
        if iov.iov_len == 0 {
            continue;
        }
        let data = copy_buffer_from_user(iov.iov_base, iov.iov_len)?;
        let n = vfs::vfs_pwrite(fd, &data, cur_offset).map_err(vfs_error_to_linux)?;
        total += n as isize;
        cur_offset += n as u64;
        if n < iov.iov_len {
            break;
        }
    }
    Ok(total)
}

// ============================================================================
// Vectored I/O (readv/writev)
// ============================================================================

/// readv - read data into multiple buffers
pub fn readv(fd: Fd, iov: *const IoVec, iovcnt: i32) -> LinuxResult<isize> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if iov.is_null() || iovcnt <= 0 {
        return Err(LinuxError::EINVAL);
    }

    let mut total = 0isize;
    for i in 0..iovcnt as isize {
        let iov = copy_iov_from_user(iov, i as i32)?;
        if iov.iov_len == 0 {
            continue;
        }
        let mut buf = alloc::vec::Vec::new();
        buf.resize(iov.iov_len, 0);
        let n = vfs::vfs_read(fd, &mut buf).map_err(vfs_error_to_linux)?;
        copy_buffer_to_user(iov.iov_base, &buf[..n])?;
        total += n as isize;
        if n < iov.iov_len {
            break;
        }
    }
    Ok(total)
}

/// writev - write data from multiple buffers
pub fn writev(fd: Fd, iov: *const IoVec, iovcnt: i32) -> LinuxResult<isize> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if iov.is_null() || iovcnt <= 0 {
        return Err(LinuxError::EINVAL);
    }

    let mut total: isize = 0;
    for i in 0..iovcnt as isize {
        let iov = copy_iov_from_user(iov, i as i32)?;
        if iov.iov_len == 0 {
            continue;
        }
        let data = copy_buffer_from_user(iov.iov_base, iov.iov_len)?;
        let n = vfs::vfs_write(fd, &data).map_err(vfs_error_to_linux)?;
        total += n as isize;
        if n < iov.iov_len {
            break;
        }
    }
    Ok(total)
}

// ============================================================================
// Zero-copy I/O
// ============================================================================

/// sendfile - copy data between file descriptors
pub fn sendfile(out_fd: Fd, in_fd: Fd, offset: *mut Off, count: usize) -> LinuxResult<isize> {
    inc_ops();

    if out_fd < 0 || in_fd < 0 {
        return Err(LinuxError::EBADF);
    }

    // Read from in_fd at offset, write to out_fd
    let mut buf = [0u8; 4096];
    let mut total = 0usize;
    let mut cur_offset = if offset.is_null() {
        0u64
    } else {
        let o: Off = unsafe { *offset };
        o as u64
    };

    while total < count {
        let to_read = core::cmp::min(buf.len(), count - total);
        let n = if offset.is_null() {
            // Use current file position
            vfs::vfs_read(in_fd, &mut buf[..to_read]).map_err(vfs_error_to_linux)?
        } else {
            vfs::vfs_pread(in_fd, &mut buf[..to_read], cur_offset).map_err(vfs_error_to_linux)?
        };
        if n == 0 {
            break;
        }
        let written = vfs::vfs_write(out_fd, &buf[..n]).map_err(vfs_error_to_linux)?;
        total += written;
        if !offset.is_null() {
            cur_offset += n as u64;
        }
        if n < to_read {
            break;
        }
    }

    if !offset.is_null() {
        unsafe {
            *offset = cur_offset as Off;
        }
    }
    Ok(total as isize)
}

/// splice - splice data to/from a pipe
pub fn splice(
    fd_in: Fd,
    off_in: *mut Off,
    fd_out: Fd,
    off_out: *mut Off,
    len: usize,
    flags: u32,
) -> LinuxResult<isize> {
    inc_ops();

    if fd_in < 0 || fd_out < 0 {
        return Err(LinuxError::EBADF);
    }

    // Splice flags
    const SPLICE_F_MOVE: u32 = 1;
    const SPLICE_F_NONBLOCK: u32 = 2;
    const SPLICE_F_MORE: u32 = 4;
    const SPLICE_F_GIFT: u32 = 8;

    let valid_flags = SPLICE_F_MOVE | SPLICE_F_NONBLOCK | SPLICE_F_MORE | SPLICE_F_GIFT;
    if flags & !valid_flags != 0 {
        return Err(LinuxError::EINVAL);
    }

    // Splice: read from fd_in, write to fd_out
    let mut buf = [0u8; 4096];
    let mut total = 0usize;
    let mut in_off = if off_in.is_null() {
        None
    } else {
        Some(unsafe { *off_in } as u64)
    };
    let mut out_off = if off_out.is_null() {
        None
    } else {
        Some(unsafe { *off_out } as u64)
    };

    while total < len {
        let to_read = core::cmp::min(buf.len(), len - total);
        let n = match in_off {
            Some(o) => vfs::vfs_pread(fd_in, &mut buf[..to_read], o).map_err(vfs_error_to_linux)?,
            None => vfs::vfs_read(fd_in, &mut buf[..to_read]).map_err(vfs_error_to_linux)?,
        };
        if n == 0 {
            break;
        }
        let written = match out_off {
            Some(o) => vfs::vfs_pwrite(fd_out, &buf[..n], o).map_err(vfs_error_to_linux)?,
            None => vfs::vfs_write(fd_out, &buf[..n]).map_err(vfs_error_to_linux)?,
        };
        total += written;
        if let Some(ref mut o) = in_off {
            *o += n as u64;
        }
        if let Some(ref mut o) = out_off {
            *o += written as u64;
        }
        if n < to_read {
            break;
        }
    }

    if !off_in.is_null() {
        unsafe {
            *off_in = in_off.unwrap_or(0) as Off;
        }
    }
    if !off_out.is_null() {
        unsafe {
            *off_out = out_off.unwrap_or(0) as Off;
        }
    }
    Ok(total as isize)
}

/// tee - duplicate pipe content
pub fn tee(fd_in: Fd, fd_out: Fd, len: usize, _flags: u32) -> LinuxResult<isize> {
    inc_ops();

    if fd_in < 0 || fd_out < 0 {
        return Err(LinuxError::EBADF);
    }

    // tee: read from fd_in without consuming, write to fd_out
    // Since we don't have peek semantics, do a read+write
    let mut buf = [0u8; 4096];
    let mut total = 0usize;
    while total < len {
        let to_read = core::cmp::min(buf.len(), len - total);
        let n = vfs::vfs_read(fd_in, &mut buf[..to_read]).map_err(vfs_error_to_linux)?;
        if n == 0 {
            break;
        }
        let written = vfs::vfs_write(fd_out, &buf[..n]).map_err(vfs_error_to_linux)?;
        total += written;
        if n < to_read {
            break;
        }
    }
    Ok(total as isize)
}

/// copy_file_range - copy range of data from one file to another
pub fn copy_file_range(
    fd_in: Fd,
    off_in: *mut Off,
    fd_out: Fd,
    off_out: *mut Off,
    len: usize,
    flags: u32,
) -> LinuxResult<isize> {
    inc_ops();

    if fd_in < 0 || fd_out < 0 {
        return Err(LinuxError::EBADF);
    }

    if flags != 0 {
        return Err(LinuxError::EINVAL);
    }

    // copy_file_range: read from fd_in, write to fd_out
    let mut buf = [0u8; 4096];
    let mut total = 0usize;
    let mut in_off = if off_in.is_null() {
        None
    } else {
        Some(unsafe { *off_in } as u64)
    };
    let mut out_off = if off_out.is_null() {
        None
    } else {
        Some(unsafe { *off_out } as u64)
    };

    while total < len {
        let to_read = core::cmp::min(buf.len(), len - total);
        let n = match in_off {
            Some(o) => vfs::vfs_pread(fd_in, &mut buf[..to_read], o).map_err(vfs_error_to_linux)?,
            None => vfs::vfs_read(fd_in, &mut buf[..to_read]).map_err(vfs_error_to_linux)?,
        };
        if n == 0 {
            break;
        }
        let written = match out_off {
            Some(o) => vfs::vfs_pwrite(fd_out, &buf[..n], o).map_err(vfs_error_to_linux)?,
            None => vfs::vfs_write(fd_out, &buf[..n]).map_err(vfs_error_to_linux)?,
        };
        total += written;
        if let Some(ref mut o) = in_off {
            *o += n as u64;
        }
        if let Some(ref mut o) = out_off {
            *o += written as u64;
        }
        if n < to_read {
            break;
        }
    }

    if !off_in.is_null() {
        unsafe {
            *off_in = in_off.unwrap_or(0) as Off;
        }
    }
    if !off_out.is_null() {
        unsafe {
            *off_out = out_off.unwrap_or(0) as Off;
        }
    }
    Ok(total as isize)
}

// ============================================================================
// Extended Attributes
// ============================================================================

fn validate_xattr_name(name: &str) -> LinuxResult<()> {
    if name.is_empty() || name.len() > 255 {
        return Err(LinuxError::EINVAL);
    }
    if name.starts_with('.') {
        return Err(LinuxError::EINVAL);
    }
    if name.contains('\0') {
        return Err(LinuxError::EINVAL);
    }
    Ok(())
}

unsafe fn xattr_name_from_ptr(name: *const u8) -> LinuxResult<alloc::string::String> {
    if name.is_null() {
        return Err(LinuxError::EFAULT);
    }
    let name_str = c_str_to_string(name)?;
    validate_xattr_name(&name_str)?;
    Ok(name_str)
}

fn xattr_vfs_error(err: crate::vfs::VfsError) -> LinuxError {
    match err {
        crate::vfs::VfsError::NotFound => LinuxError::ENODATA,
        crate::vfs::VfsError::NotSupported => LinuxError::ENOTSUP,
        crate::vfs::VfsError::AlreadyExists => LinuxError::EEXIST,
        crate::vfs::VfsError::InvalidArgument => LinuxError::EINVAL,
        _ => vfs_error_to_linux(err),
    }
}

fn copy_xattr_value(value: &[u8], out: *mut u8, size: usize) -> LinuxResult<isize> {
    if size == 0 {
        return Ok(value.len() as isize);
    }
    if value.len() > size {
        return Err(LinuxError::ERANGE);
    }
    if out.is_null() {
        return Err(LinuxError::EFAULT);
    }
    unsafe {
        core::ptr::copy_nonoverlapping(value.as_ptr(), out, value.len());
    }
    Ok(value.len() as isize)
}

/// getxattr - get an extended attribute value
pub fn getxattr(
    path: *const u8,
    name: *const u8,
    value: *mut u8,
    size: usize,
) -> LinuxResult<isize> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path = unsafe { c_str_to_string(path)? };
    let name = unsafe { xattr_name_from_ptr(name)? };

    match vfs::vfs_getxattr(&path, &name) {
        Ok(data) => copy_xattr_value(&data, value, size),
        Err(e) => Err(xattr_vfs_error(e)),
    }
}

/// lgetxattr - get extended attribute (don't follow symlinks)
pub fn lgetxattr(
    path: *const u8,
    name: *const u8,
    value: *mut u8,
    size: usize,
) -> LinuxResult<isize> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    getxattr(path, name, value, size)
}

/// fgetxattr - get extended attribute by file descriptor
pub fn fgetxattr(fd: Fd, name: *const u8, value: *mut u8, size: usize) -> LinuxResult<isize> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    let name = unsafe { xattr_name_from_ptr(name)? };

    match vfs::vfs_fgetxattr(fd, &name) {
        Ok(data) => copy_xattr_value(&data, value, size),
        Err(e) => Err(xattr_vfs_error(e)),
    }
}

/// setxattr - set an extended attribute value
pub fn setxattr(
    path: *const u8,
    name: *const u8,
    value: *const u8,
    size: usize,
    flags: i32,
) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() || value.is_null() {
        return Err(LinuxError::EFAULT);
    }

    const XATTR_CREATE: i32 = 1;
    const XATTR_REPLACE: i32 = 2;

    if flags & !(XATTR_CREATE | XATTR_REPLACE) != 0 {
        return Err(LinuxError::EINVAL);
    }

    let path = unsafe { c_str_to_string(path)? };
    let name = unsafe { xattr_name_from_ptr(name)? };
    let data = copy_buffer_from_user(value, size)?;

    let create = flags & XATTR_CREATE != 0;
    let replace = flags & XATTR_REPLACE != 0;
    if create && replace {
        return Err(LinuxError::EINVAL);
    }

    vfs::vfs_setxattr(&path, &name, &data, create)
        .map(|_| 0)
        .map_err(xattr_vfs_error)
}

/// lsetxattr - set extended attribute (don't follow symlinks)
pub fn lsetxattr(
    path: *const u8,
    name: *const u8,
    value: *const u8,
    size: usize,
    flags: i32,
) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() || value.is_null() {
        return Err(LinuxError::EFAULT);
    }

    setxattr(path, name, value, size, flags)
}

/// fsetxattr - set extended attribute by file descriptor
pub fn fsetxattr(
    fd: Fd,
    name: *const u8,
    value: *const u8,
    size: usize,
    flags: i32,
) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if value.is_null() {
        return Err(LinuxError::EFAULT);
    }

    const XATTR_CREATE: i32 = 1;
    const XATTR_REPLACE: i32 = 2;

    if flags & !(XATTR_CREATE | XATTR_REPLACE) != 0 {
        return Err(LinuxError::EINVAL);
    }

    let name = unsafe { xattr_name_from_ptr(name)? };
    let data = copy_buffer_from_user(value, size)?;
    let create = flags & XATTR_CREATE != 0;

    vfs::vfs_fsetxattr(fd, &name, &data, create)
        .map(|_| 0)
        .map_err(xattr_vfs_error)
}

/// listxattr - list extended attribute names
pub fn listxattr(path: *const u8, list: *mut u8, size: usize) -> LinuxResult<isize> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path = unsafe { c_str_to_string(path)? };

    match vfs::vfs_listxattr(&path) {
        Ok(data) => copy_xattr_value(&data, list, size),
        Err(e) => Err(xattr_vfs_error(e)),
    }
}

/// llistxattr - list extended attributes (don't follow symlinks)
pub fn llistxattr(path: *const u8, list: *mut u8, size: usize) -> LinuxResult<isize> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    listxattr(path, list, size)
}

/// flistxattr - list extended attributes by file descriptor
pub fn flistxattr(fd: Fd, list: *mut u8, size: usize) -> LinuxResult<isize> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    match vfs::vfs_flistxattr(fd) {
        Ok(data) => copy_xattr_value(&data, list, size),
        Err(e) => Err(xattr_vfs_error(e)),
    }
}

/// removexattr - remove an extended attribute
pub fn removexattr(path: *const u8, name: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path = unsafe { c_str_to_string(path)? };
    let name = unsafe { xattr_name_from_ptr(name)? };

    vfs::vfs_removexattr(&path, &name)
        .map(|_| 0)
        .map_err(xattr_vfs_error)
}

/// lremovexattr - remove extended attribute (don't follow symlinks)
pub fn lremovexattr(path: *const u8, name: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    removexattr(path, name)
}

/// fremovexattr - remove extended attribute by file descriptor
pub fn fremovexattr(fd: Fd, name: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    let name = unsafe { xattr_name_from_ptr(name)? };

    vfs::vfs_fremovexattr(fd, &name)
        .map(|_| 0)
        .map_err(xattr_vfs_error)
}

// ============================================================================
// Directory operations
// ============================================================================

/// mkdir - create directory
pub fn mkdir(path: *const u8, mode: Mode) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path = unsafe { c_str_to_string(path)? };
    vfs::vfs_mkdir(&path, mode).map_err(vfs_error_to_linux)?;
    Ok(0)
}

/// rmdir - remove empty directory
pub fn rmdir(path: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if path.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let path = unsafe { c_str_to_string(path)? };
    vfs::vfs_rmdir(&path).map_err(vfs_error_to_linux)?;
    Ok(0)
}

/// getdents64 - get directory entries (64-bit version)
pub fn getdents64(fd: Fd, dirp: *mut u8, count: u32) -> LinuxResult<i32> {
    inc_ops();

    if fd < 0 {
        return Err(LinuxError::EBADF);
    }

    if dirp.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if count < 24 {
        return Ok(0);
    }

    let (entries, cookie) = vfs::vfs_readdir_fd(fd).map_err(vfs_error_to_linux)?;

    let mut written = 0u32;
    let mut index = cookie as usize;

    while index < entries.len() {
        let entry = &entries[index];
        let name_bytes = entry.name.as_bytes();
        let reclen = ((24 + name_bytes.len() + 1 + 7) & !7) as u16;
        if written + reclen as u32 > count {
            break;
        }

        let d_type = inode_type_to_d_type(entry.inode_type);
        unsafe {
            let base = dirp.add(written as usize);
            *(base as *mut u64) = entry.ino;
            *(base.add(8) as *mut i64) = (index + 1) as i64;
            *(base.add(16) as *mut u16) = reclen;
            *base.add(18) = d_type;
            let name_ptr = base.add(19);
            for (i, &b) in name_bytes.iter().enumerate() {
                *name_ptr.add(i) = b;
            }
            *name_ptr.add(name_bytes.len()) = 0;
        }

        written += reclen as u32;
        index += 1;
    }

    let _ = vfs::vfs_set_dir_cookie(fd, index as u64);
    Ok(written as i32)
}

fn inode_type_to_d_type(inode_type: vfs::InodeType) -> u8 {
    match inode_type {
        vfs::InodeType::Directory => 4,  // DT_DIR
        vfs::InodeType::File => 8,       // DT_REG
        vfs::InodeType::Symlink => 10,   // DT_LNK
        vfs::InodeType::CharDevice => 2, // DT_CHR
        vfs::InodeType::BlockDevice => 6,
        vfs::InodeType::Fifo => 1,
        vfs::InodeType::Socket => 12,
    }
}

#[cfg(any())]
mod tests {
    use super::*;

    #[test_case]
    fn test_pread_pwrite() {
        let buf = [0u8; 100];
        assert!(pread(3, buf.as_ptr() as *mut u8, 100, 0).is_ok());
        assert!(pwrite(3, buf.as_ptr(), 100, 0).is_ok());
    }

    #[test_case]
    fn test_vectored_io() {
        let buf1 = [0u8; 50];
        let buf2 = [0u8; 50];
        let iov = [
            IoVec {
                iov_base: buf1.as_ptr() as *mut u8,
                iov_len: 50,
            },
            IoVec {
                iov_base: buf2.as_ptr() as *mut u8,
                iov_len: 50,
            },
        ];

        assert!(readv(3, iov.as_ptr(), 2).is_ok());
        assert!(writev(3, iov.as_ptr(), 2).is_ok());
    }

    #[test_case]
    fn test_sendfile() {
        assert!(sendfile(4, 3, core::ptr::null_mut(), 1024).is_ok());
    }

    #[test_case]
    fn test_xattr() {
        let path = b"/test\0".as_ptr();
        let name = b"user.test\0".as_ptr();
        let value = b"value\0".as_ptr();

        assert!(setxattr(path, name, value, 5, 0).is_ok());
        assert_eq!(
            getxattr(path, name, core::ptr::null_mut(), 0),
            Err(LinuxError::ENODATA)
        );
    }
}
