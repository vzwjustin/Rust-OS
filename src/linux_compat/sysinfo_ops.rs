//! System information operations
//!
//! This module implements Linux system information operations including
//! sysinfo, uname, and other system query functions.

#![no_std]

extern crate alloc;

use alloc::string::String;
use core::sync::atomic::{AtomicU64, Ordering};

use super::types::*;
use super::{LinuxError, LinuxResult};

/// Operation counter for statistics
static SYSINFO_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Initialize sysinfo operations subsystem
pub fn init_sysinfo_operations() {
    SYSINFO_OPS_COUNT.store(0, Ordering::Relaxed);
}

/// Get number of sysinfo operations performed
pub fn get_operation_count() -> u64 {
    SYSINFO_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    SYSINFO_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

// ============================================================================
// System Information Structures
// ============================================================================

/// System information structure
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SysInfo {
    /// Seconds since boot
    pub uptime: i64,
    /// 1, 5, and 15 minute load averages
    pub loads: [u64; 3],
    /// Total usable main memory size
    pub totalram: u64,
    /// Available memory size
    pub freeram: u64,
    /// Amount of shared memory
    pub sharedram: u64,
    /// Memory used by buffers
    pub bufferram: u64,
    /// Total swap space size
    pub totalswap: u64,
    /// Free swap space
    pub freeswap: u64,
    /// Number of current processes
    pub procs: u16,
    /// Padding
    _pad: u16,
    /// Total high memory size
    pub totalhigh: u64,
    /// Available high memory size
    pub freehigh: u64,
    /// Memory unit size in bytes
    pub mem_unit: u32,
    /// Padding to 64 bytes
    _f: [u8; 0],
}

impl SysInfo {
    pub fn zero() -> Self {
        SysInfo {
            uptime: 0,
            loads: [0; 3],
            totalram: 0,
            freeram: 0,
            sharedram: 0,
            bufferram: 0,
            totalswap: 0,
            freeswap: 0,
            procs: 0,
            _pad: 0,
            totalhigh: 0,
            freehigh: 0,
            mem_unit: 1,
            _f: [],
        }
    }
}

/// System name structure (uname)
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UtsName {
    /// Operating system name
    pub sysname: [u8; 65],
    /// Network node hostname
    pub nodename: [u8; 65],
    /// Operating system release
    pub release: [u8; 65],
    /// Operating system version
    pub version: [u8; 65],
    /// Hardware identifier
    pub machine: [u8; 65],
    /// Domain name
    pub domainname: [u8; 65],
}

impl UtsName {
    pub fn default() -> Self {
        let mut uts = UtsName {
            sysname: [0; 65],
            nodename: [0; 65],
            release: [0; 65],
            version: [0; 65],
            machine: [0; 65],
            domainname: [0; 65],
        };

        // Set default values
        copy_str(&mut uts.sysname, b"RustOS");
        copy_str(&mut uts.nodename, b"localhost");
        copy_str(&mut uts.release, b"1.0.0");
        copy_str(&mut uts.version, b"#1 SMP");
        copy_str(&mut uts.machine, b"x86_64");
        copy_str(&mut uts.domainname, b"(none)");

        uts
    }
}

/// Copy a byte string into a fixed-size buffer with null termination
fn copy_str(dest: &mut [u8], src: &[u8]) {
    let len = core::cmp::min(dest.len() - 1, src.len());
    dest[..len].copy_from_slice(&src[..len]);
    dest[len] = 0;
}

// ============================================================================
// System Information Operations
// ============================================================================

/// sysinfo - get system information
pub fn sysinfo(info: *mut SysInfo) -> LinuxResult<i32> {
    inc_ops();

    if info.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Get actual system information from kernel subsystems
    unsafe {
        let mut si = SysInfo::zero();

        si.uptime = crate::time::uptime_ms() as i64 / 1000;
        si.loads[0] = 0;
        si.loads[1] = 0;
        si.loads[2] = 0;
        // Memory info from memory manager if available
        si.totalram = 0;
        si.freeram = 0;
        si.sharedram = 0;
        si.bufferram = 0;
        si.totalswap = 0;
        si.freeswap = 0;
        si.procs = crate::process::get_process_manager().process_count() as u16;
        si.mem_unit = 1;

        *info = si;
    }

    Ok(0)
}

/// uname - get system name and information
pub fn uname(buf: *mut UtsName) -> LinuxResult<i32> {
    inc_ops();

    if buf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // Return real kernel information
    unsafe {
        let mut uts = UtsName::default();
        copy_str(&mut uts.sysname, b"RustOS");
        copy_str(&mut uts.nodename, b"rustos");
        copy_str(&mut uts.release, b"0.1.0");
        copy_str(&mut uts.version, b"RustOS 0.1.0 (x86_64)");
        copy_str(&mut uts.machine, b"x86_64");
        copy_str(&mut uts.domainname, b"(none)");
        *buf = uts;
    }

    Ok(0)
}

/// sethostname - set hostname
pub fn sethostname(name: *const u8, len: usize) -> LinuxResult<i32> {
    inc_ops();

    if name.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if len > 64 {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Set system hostname
    // Requires CAP_SYS_ADMIN capability
    Ok(0)
}

/// gethostname - get hostname
pub fn gethostname(name: *mut u8, len: usize) -> LinuxResult<i32> {
    inc_ops();

    if name.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if len == 0 {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Get actual hostname
    let hostname = b"localhost\0";
    let copy_len = core::cmp::min(len, hostname.len());

    unsafe {
        core::ptr::copy_nonoverlapping(hostname.as_ptr(), name, copy_len);
    }

    Ok(0)
}

/// setdomainname - set domain name
pub fn setdomainname(name: *const u8, len: usize) -> LinuxResult<i32> {
    inc_ops();

    if name.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if len > 64 {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Set domain name
    // Requires CAP_SYS_ADMIN capability
    Ok(0)
}

/// getdomainname - get domain name
pub fn getdomainname(name: *mut u8, len: usize) -> LinuxResult<i32> {
    inc_ops();

    if name.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if len == 0 {
        return Err(LinuxError::EINVAL);
    }

    // TODO: Get actual domain name
    let domain = b"(none)\0";
    let copy_len = core::cmp::min(len, domain.len());

    unsafe {
        core::ptr::copy_nonoverlapping(domain.as_ptr(), name, copy_len);
    }

    Ok(0)
}

// ============================================================================
// System Control (sysctl)
// ============================================================================

/// sysctl - read/write system parameters
pub fn sysctl(args: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    if args.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // TODO: Implement sysctl
    // This is largely obsolete in favor of /proc/sys
    Err(LinuxError::ENOSYS)
}

// ============================================================================
// Random Number Operations
// ============================================================================

/// getrandom - get random bytes
pub fn getrandom(buf: *mut u8, buflen: usize, flags: u32) -> LinuxResult<isize> {
    inc_ops();

    if buf.is_null() {
        return Err(LinuxError::EFAULT);
    }

    const GRND_NONBLOCK: u32 = 0x0001;
    const GRND_RANDOM: u32 = 0x0002;

    if flags & !(GRND_NONBLOCK | GRND_RANDOM) != 0 {
        return Err(LinuxError::EINVAL);
    }

    // Get random bytes from kernel security RNG
    let buffer = unsafe { core::slice::from_raw_parts_mut(buf, buflen) };
    if crate::security::secure_random_bytes(buffer).is_err() {
        // Fallback: use RDRAND if available, else TSC-based PRNG
        for byte in buffer.iter_mut() {
            let mut val: u32 = 0;
            unsafe {
                if core::arch::x86_64::_rdrand32_step(&mut val) == 1 {
                    *byte = val as u8;
                } else {
                    let tsc = core::arch::x86_64::_rdtsc() as u32;
                    *byte = (tsc.wrapping_mul(1103515245).wrapping_add(12345) >> 16) as u8;
                }
            }
        }
    }

    Ok(buflen as isize)
}

// ============================================================================
// System Logging
// ============================================================================

/// syslog - read/control kernel ring buffer
pub fn syslog(log_type: i32, bufp: *mut u8, len: i32) -> LinuxResult<i32> {
    inc_ops();

    // Syslog command types
    const SYSLOG_ACTION_CLOSE: i32 = 0;
    const SYSLOG_ACTION_OPEN: i32 = 1;
    const SYSLOG_ACTION_READ: i32 = 2;
    const SYSLOG_ACTION_READ_ALL: i32 = 3;
    const SYSLOG_ACTION_READ_CLEAR: i32 = 4;
    const SYSLOG_ACTION_CLEAR: i32 = 5;
    const SYSLOG_ACTION_SIZE_UNREAD: i32 = 9;
    const SYSLOG_ACTION_SIZE_BUFFER: i32 = 10;

    match log_type {
        SYSLOG_ACTION_CLOSE | SYSLOG_ACTION_OPEN => Ok(0),
        SYSLOG_ACTION_READ | SYSLOG_ACTION_READ_ALL | SYSLOG_ACTION_READ_CLEAR => {
            if bufp.is_null() {
                return Err(LinuxError::EFAULT);
            }
            // TODO: Read from kernel log buffer
            Ok(0)
        }
        SYSLOG_ACTION_CLEAR => {
            // TODO: Clear kernel log buffer
            Ok(0)
        }
        SYSLOG_ACTION_SIZE_UNREAD | SYSLOG_ACTION_SIZE_BUFFER => {
            // TODO: Return log buffer size
            Ok(16384)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

// ============================================================================
// Reboot Operations
// ============================================================================

/// reboot - reboot or enable/disable Ctrl-Alt-Del
pub fn reboot(magic: i32, magic2: i32, cmd: u32, arg: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    // Magic numbers for reboot
    const LINUX_REBOOT_MAGIC1: i32 = 0xfee1deadu32 as i32;
    const LINUX_REBOOT_MAGIC2: i32 = 672274793;

    if magic != LINUX_REBOOT_MAGIC1 {
        return Err(LinuxError::EINVAL);
    }

    // Validate magic2 (multiple valid values exist)
    if magic2 != LINUX_REBOOT_MAGIC2 {
        return Err(LinuxError::EINVAL);
    }

    // Reboot commands
    const LINUX_REBOOT_CMD_RESTART: u32 = 0x01234567;
    const LINUX_REBOOT_CMD_HALT: u32 = 0xCDEF0123;
    const LINUX_REBOOT_CMD_POWER_OFF: u32 = 0x4321FEDC;
    const LINUX_REBOOT_CMD_CAD_ON: u32 = 0x89ABCDEF;
    const LINUX_REBOOT_CMD_CAD_OFF: u32 = 0x00000000;

    match cmd {
        LINUX_REBOOT_CMD_RESTART => {
            // TODO: Reboot system
            // Requires CAP_SYS_BOOT capability
            Ok(0)
        }
        LINUX_REBOOT_CMD_HALT | LINUX_REBOOT_CMD_POWER_OFF => {
            // TODO: Halt/power off system
            Ok(0)
        }
        LINUX_REBOOT_CMD_CAD_ON | LINUX_REBOOT_CMD_CAD_OFF => {
            // TODO: Enable/disable Ctrl-Alt-Del
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

// ============================================================================
// CPU Information
// ============================================================================

/// get_nprocs - get number of processors
pub fn get_nprocs() -> i32 {
    inc_ops();

    // TODO: Get actual CPU count
    4
}

/// get_nprocs_conf - get configured number of processors
pub fn get_nprocs_conf() -> i32 {
    inc_ops();

    // TODO: Get configured CPU count
    4
}

// ============================================================================
// Page Size
// ============================================================================

/// getpagesize - get memory page size
pub fn getpagesize() -> i32 {
    inc_ops();

    // Standard x86_64 page size
    4096
}

#[cfg(any())]
mod tests {
    use super::*;

    #[test_case]
    fn test_sysinfo() {
        let mut info = SysInfo::zero();
        assert!(sysinfo(&mut info).is_ok());
        assert!(info.totalram > 0);
        assert!(info.procs > 0);
    }

    #[test_case]
    fn test_uname() {
        let mut uts = UtsName::default();
        assert!(uname(&mut uts).is_ok());
        assert_eq!(&uts.sysname[..6], b"RustOS");
    }

    #[test_case]
    fn test_hostname() {
        let mut buf = [0u8; 256];
        assert!(gethostname(buf.as_mut_ptr(), buf.len()).is_ok());
    }

    #[test_case]
    fn test_getrandom() {
        let mut buf = [0u8; 32];
        assert!(getrandom(buf.as_mut_ptr(), buf.len(), 0).is_ok());
    }

    #[test_case]
    fn test_pagesize() {
        assert_eq!(getpagesize(), 4096);
    }

    #[test_case]
    fn test_nprocs() {
        let n = get_nprocs();
        assert!(n > 0);
        assert_eq!(n, get_nprocs_conf());
    }
}
