//! Package management system call interface
//!
//! This module provides syscall interface for userspace package management operations.

use crate::package::{PackageManager, PackageManagerType, PackageOperation, PackageResult};
use alloc::string::{String, ToString};

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
static mut PACKAGE_MANAGER: Option<PackageManager> = None;

/// Initialize the package manager
pub fn init_package_manager(manager_type: PackageManagerType) {
    unsafe {
        PACKAGE_MANAGER = Some(PackageManager::new(manager_type));
    }
}

/// Get a reference to the package manager
fn get_package_manager() -> PackageResult<&'static mut PackageManager> {
    unsafe {
        PACKAGE_MANAGER.as_mut().ok_or_else(|| {
            crate::package::PackageError::InvalidOperation("Package manager not initialized".into())
        })
    }
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
            let pm = get_package_manager().map_err(|_| "Package manager not initialized")?;

            pm.execute_operation(PackageOperation::Install, &name)
                .map(|_| 0)
                .map_err(|_| "Package installation failed")
        }

        SYS_PKG_REMOVE => {
            let name = unsafe { read_string_from_user(arg1, arg2)? };
            let pm = get_package_manager().map_err(|_| "Package manager not initialized")?;

            pm.execute_operation(PackageOperation::Remove, &name)
                .map(|_| 0)
                .map_err(|_| "Package removal failed")
        }

        SYS_PKG_SEARCH => {
            let query = unsafe { read_string_from_user(arg1, arg2)? };
            let pm = get_package_manager().map_err(|_| "Package manager not initialized")?;

            match pm.execute_operation(PackageOperation::Search, &query) {
                Ok(result) => {
                    unsafe {
                        write_string_to_user(arg3, arg4, &result)?;
                    }
                    Ok(result.len() as isize)
                }
                Err(_) => Err("Package search failed"),
            }
        }

        SYS_PKG_INFO => {
            let name = unsafe { read_string_from_user(arg1, arg2)? };
            let pm = get_package_manager().map_err(|_| "Package manager not initialized")?;

            match pm.execute_operation(PackageOperation::Info, &name) {
                Ok(result) => {
                    unsafe {
                        write_string_to_user(arg3, arg4, &result)?;
                    }
                    Ok(result.len() as isize)
                }
                Err(_) => Err("Package info failed"),
            }
        }

        SYS_PKG_LIST => {
            let pm = get_package_manager().map_err(|_| "Package manager not initialized")?;

            match pm.execute_operation(PackageOperation::List, "") {
                Ok(result) => {
                    unsafe {
                        write_string_to_user(arg1, arg2, &result)?;
                    }
                    Ok(result.len() as isize)
                }
                Err(_) => Err("Package list failed"),
            }
        }

        SYS_PKG_UPDATE => {
            let pm = get_package_manager().map_err(|_| "Package manager not initialized")?;

            pm.execute_operation(PackageOperation::Update, "")
                .map(|_| 0)
                .map_err(|_| "Package update failed")
        }

        SYS_PKG_UPGRADE => {
            let name = unsafe { read_string_from_user(arg1, arg2)? };
            let pm = get_package_manager().map_err(|_| "Package manager not initialized")?;

            pm.execute_operation(PackageOperation::Upgrade, &name)
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

    // TODO: Validate that ptr is in userspace memory range
    // TODO: Check for page faults

    let slice = core::slice::from_raw_parts(ptr as *const u8, len);

    core::str::from_utf8(slice)
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

    // TODO: Validate that ptr is in userspace memory range
    // TODO: Check for page faults

    let dest = core::slice::from_raw_parts_mut(ptr as *mut u8, write_len);
    dest.copy_from_slice(&bytes[..write_len]);

    Ok(())
}
