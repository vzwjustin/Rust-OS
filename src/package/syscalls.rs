//! Package management system call interface
//!
//! This module provides syscall interface for userspace package management operations.

use crate::memory::user_space::UserSpaceMemory;
use crate::package::{PackageManager, PackageManagerType, PackageOperation};
use crate::syscall::SyscallError;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Package management syscall numbers
pub mod syscall_numbers {
    /// Install a package: syscall(SYS_PKG_INSTALL, name_ptr, name_len)
    pub const SYS_PKG_INSTALL: usize = 200;

    /// Remove a package: syscall(SYS_PKG_REMOVE, name_ptr, name_len)
    pub const SYS_PKG_REMOVE: usize = 201;

    /// Search for packages: syscall(SYS_PKG_SEARCH, query_ptr, query_len, result_ptr, result_len)
    pub const SYS_PKG_SEARCH: usize = 202;

    /// Get package info: syscall(SYS_PKG_INFO, name_ptr, name_len, result_ptr, result_len)
    pub const SYS_PKG_INFO: usize = 203;

    /// List installed packages: syscall(SYS_PKG_LIST, result_ptr, result_len)
    pub const SYS_PKG_LIST: usize = 204;

    /// Update package database: syscall(SYS_PKG_UPDATE)
    pub const SYS_PKG_UPDATE: usize = 205;

    /// Upgrade packages: syscall(SYS_PKG_UPGRADE, name_ptr, name_len)
    pub const SYS_PKG_UPGRADE: usize = 206;
}

/// Global package manager instance
static PACKAGE_MANAGER: spin::Mutex<Option<PackageManager>> = spin::Mutex::new(None);

/// Initialize the package manager
pub fn init_package_manager(manager_type: PackageManagerType) {
    *PACKAGE_MANAGER.lock() = Some(PackageManager::new(manager_type));
}

/// Run a closure with the package manager, returning `None` if uninitialized
fn with_package_manager<R>(f: impl FnOnce(&mut PackageManager) -> R) -> Option<R> {
    PACKAGE_MANAGER.lock().as_mut().map(f)
}

/// Handle package management syscalls
pub fn handle_package_syscall(
    syscall_number: usize,
    arg1: usize,
    arg2: usize,
    arg3: usize,
    arg4: usize,
) -> Result<isize, &'static str> {
    use syscall_numbers::*;

    match syscall_number {
        SYS_PKG_INSTALL => {
            let name = unsafe { read_string_from_user(arg1, arg2)? };
            with_package_manager(|pm| pm.execute_operation(PackageOperation::Install, &name))
                .ok_or("Package manager not initialized")?
                .map(|_| 0)
                .map_err(|_| "Package installation failed")
        }

        SYS_PKG_REMOVE => {
            let name = unsafe { read_string_from_user(arg1, arg2)? };
            with_package_manager(|pm| pm.execute_operation(PackageOperation::Remove, &name))
                .ok_or("Package manager not initialized")?
                .map(|_| 0)
                .map_err(|_| "Package removal failed")
        }

        SYS_PKG_SEARCH => {
            let query = unsafe { read_string_from_user(arg1, arg2)? };
            let result =
                with_package_manager(|pm| pm.execute_operation(PackageOperation::Search, &query))
                    .ok_or("Package manager not initialized")?
                    .map_err(|_| "Package search failed")?;
            unsafe {
                write_string_to_user(arg3, arg4, &result)?;
            }
            Ok(result.len() as isize)
        }

        SYS_PKG_INFO => {
            let name = unsafe { read_string_from_user(arg1, arg2)? };
            let result =
                with_package_manager(|pm| pm.execute_operation(PackageOperation::Info, &name))
                    .ok_or("Package manager not initialized")?
                    .map_err(|_| "Package info failed")?;
            unsafe {
                write_string_to_user(arg3, arg4, &result)?;
            }
            Ok(result.len() as isize)
        }

        SYS_PKG_LIST => {
            let result =
                with_package_manager(|pm| pm.execute_operation(PackageOperation::List, ""))
                    .ok_or("Package manager not initialized")?
                    .map_err(|_| "Package list failed")?;
            unsafe {
                write_string_to_user(arg1, arg2, &result)?;
            }
            Ok(result.len() as isize)
        }

        SYS_PKG_UPDATE => {
            with_package_manager(|pm| pm.execute_operation(PackageOperation::Update, ""))
                .ok_or("Package manager not initialized")?
                .map(|_| 0)
                .map_err(|_| "Package update failed")
        }

        SYS_PKG_UPGRADE => {
            let name = unsafe { read_string_from_user(arg1, arg2)? };
            with_package_manager(|pm| pm.execute_operation(PackageOperation::Upgrade, &name))
                .ok_or("Package manager not initialized")?
                .map(|_| 0)
                .map_err(|_| "Package upgrade failed")
        }

        _ => Err("Unknown package management syscall"),
    }
}

/// Read a string from userspace memory
///
/// # Safety
/// This function reads from user-provided memory addresses. The caller must ensure:
/// - The pointer is valid and points to readable memory
/// - The length doesn't exceed the actual allocation
unsafe fn read_string_from_user(ptr: usize, len: usize) -> Result<String, &'static str> {
    if ptr == 0 || len == 0 || len > 4096 {
        return Err("Invalid string parameters");
    }

    UserSpaceMemory::validate_user_ptr(ptr as u64, len as u64, false)
        .map_err(syscall_error_to_str)?;

    let mut buffer = Vec::with_capacity(len);
    buffer.resize(len, 0);
    UserSpaceMemory::copy_from_user(ptr as u64, &mut buffer).map_err(syscall_error_to_str)?;

    core::str::from_utf8(&buffer)
        .map(|s| s.to_string())
        .map_err(|_| "Invalid UTF-8 string")
}

/// Write a string to userspace memory
///
/// # Safety
/// This function writes to user-provided memory addresses. The caller must ensure:
/// - The pointer is valid and points to writable memory
/// - The buffer size is sufficient
unsafe fn write_string_to_user(ptr: usize, max_len: usize, data: &str) -> Result<(), &'static str> {
    if ptr == 0 || max_len == 0 {
        return Err("Invalid buffer parameters");
    }

    let bytes = data.as_bytes();
    let write_len = core::cmp::min(bytes.len(), max_len);

    UserSpaceMemory::validate_user_ptr(ptr as u64, write_len as u64, true)
        .map_err(syscall_error_to_str)?;
    UserSpaceMemory::copy_to_user(ptr as u64, &bytes[..write_len]).map_err(syscall_error_to_str)?;

    Ok(())
}

fn syscall_error_to_str(err: SyscallError) -> &'static str {
    match err {
        SyscallError::InvalidAddress => "Invalid user address",
        SyscallError::InvalidArgument => "Invalid argument",
        SyscallError::PermissionDenied => "Permission denied",
        _ => "Memory access failed",
    }
}
