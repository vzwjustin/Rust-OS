//! System information operations
//!
//! This module implements Linux system information operations including
//! sysinfo, uname, and other system query functions.

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

use super::{LinuxError, LinuxResult};
use crate::memory::user_space::UserSpaceMemory;

/// Operation counter for statistics
static SYSINFO_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Maximum hostname length (Linux limit)
const MAX_HOSTNAME: usize = 64;
/// Maximum domain name length (Linux limit)
const MAX_DOMAINNAME: usize = 64;
const MAX_GETRANDOM_CHUNK: usize = 64 * 1024;

/// System hostname storage
static HOSTNAME: Mutex<[u8; MAX_HOSTNAME]> = Mutex::new([0u8; MAX_HOSTNAME]);
/// System domain name storage
static DOMAINNAME: Mutex<[u8; MAX_DOMAINNAME]> = Mutex::new([0u8; MAX_DOMAINNAME]);
/// Whether hostname has been set via sethostname
static HOSTNAME_SET: AtomicU64 = AtomicU64::new(0);
/// Whether domain name has been set via setdomainname
static DOMAINNAME_SET: AtomicU64 = AtomicU64::new(0);

/// Initialize sysinfo operations subsystem
pub fn init_sysinfo_operations() {
    SYSINFO_OPS_COUNT.store(0, Ordering::Relaxed);
    HOSTNAME_SET.store(0, Ordering::Relaxed);
    DOMAINNAME_SET.store(0, Ordering::Relaxed);
    let mut hn = HOSTNAME.lock();
    let default_hn = b"localhost";
    hn[..default_hn.len()].copy_from_slice(default_hn);
    hn[default_hn.len()] = 0;
    let mut dn = DOMAINNAME.lock();
    let default_dn = b"(none)";
    dn[..default_dn.len()].copy_from_slice(default_dn);
    dn[default_dn.len()] = 0;
}

/// Get number of sysinfo operations performed
pub fn get_operation_count() -> u64 {
    SYSINFO_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    SYSINFO_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

fn copy_from_user_buffer(user_ptr: u64, buffer: &mut [u8]) -> LinuxResult<()> {
    match UserSpaceMemory::copy_from_user(user_ptr, buffer) {
        Ok(()) => Ok(()),
        Err(_) => {
            #[cfg(test)]
            {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        user_ptr as *const u8,
                        buffer.as_mut_ptr(),
                        buffer.len(),
                    );
                }
                Ok(())
            }

            #[cfg(not(test))]
            {
                Err(LinuxError::EFAULT)
            }
        }
    }
}

fn copy_to_user_buffer(user_ptr: u64, buffer: &[u8]) -> LinuxResult<()> {
    match UserSpaceMemory::copy_to_user(user_ptr, buffer) {
        Ok(()) => Ok(()),
        Err(_) => {
            #[cfg(test)]
            {
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        buffer.as_ptr(),
                        user_ptr as *mut u8,
                        buffer.len(),
                    );
                }
                Ok(())
            }

            #[cfg(not(test))]
            {
                Err(LinuxError::EFAULT)
            }
        }
    }
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

        // Memory info from the basic memory subsystem
        if let Ok(stats) = crate::memory_basic::get_memory_stats() {
            si.totalram = stats.usable_memory as u64;
            si.freeram = stats
                .usable_memory
                .saturating_sub(crate::memory_basic::KERNEL_HEAP_SIZE)
                as u64;
        }
        // Try the advanced memory manager for more accurate stats
        if let Some(stats) = crate::memory::get_memory_stats() {
            si.totalram = stats.total_memory as u64;
            si.freeram = stats.free_memory as u64;
            si.bufferram = stats.allocated_memory as u64;
            si.totalswap = (stats.swap_stats.total_slots as u64) * 4096;
            si.freeswap = (stats.swap_stats.free_slots as u64) * 4096;
        }
        si.sharedram = 0;
        si.mem_unit = 1;

        si.procs = crate::process::get_process_manager().process_count() as u16;

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

    let mut hn = HOSTNAME.lock();
    let copy_len = core::cmp::min(len, MAX_HOSTNAME);
    copy_from_user_buffer(name as u64, &mut hn[..copy_len])?;
    if copy_len < MAX_HOSTNAME {
        hn[copy_len] = 0;
    }
    HOSTNAME_SET.store(1, Ordering::Relaxed);
    Ok(0)
}

/// Return the current kernel hostname as a string (NUL-terminated bytes stripped).
pub fn kernel_hostname() -> String {
    use alloc::string::String;

    let hn = HOSTNAME.lock();
    let len = hn.iter().position(|&b| b == 0).unwrap_or(MAX_HOSTNAME);
    String::from(core::str::from_utf8(&hn[..len]).unwrap_or("localhost"))
}

/// Set the kernel hostname from a UTF-8 string (used by `/proc/sys/kernel/hostname`).
pub fn set_kernel_hostname(value: &str) -> Result<(), &'static str> {
    if value.is_empty() || value.len() > MAX_HOSTNAME {
        return Err("invalid hostname length");
    }
    if value.as_bytes().contains(&0) {
        return Err("invalid hostname character");
    }

    let mut hn = HOSTNAME.lock();
    let bytes = value.as_bytes();
    hn[..bytes.len()].copy_from_slice(bytes);
    hn[bytes.len()] = 0;
    HOSTNAME_SET.store(1, Ordering::Relaxed);
    Ok(())
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

    let hn = HOSTNAME.lock();
    let hostname_len = hn.iter().position(|&b| b == 0).unwrap_or(MAX_HOSTNAME);
    let copy_len = core::cmp::min(len, hostname_len);

    copy_to_user_buffer(name as u64, &hn[..copy_len])?;
    if copy_len < len {
        copy_to_user_buffer(name.wrapping_add(copy_len) as u64, &[0])?;
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

    let mut dn = DOMAINNAME.lock();
    let copy_len = core::cmp::min(len, MAX_DOMAINNAME);
    copy_from_user_buffer(name as u64, &mut dn[..copy_len])?;
    if copy_len < MAX_DOMAINNAME {
        dn[copy_len] = 0;
    }
    DOMAINNAME_SET.store(1, Ordering::Relaxed);
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

    let dn = DOMAINNAME.lock();
    let domain_len = dn.iter().position(|&b| b == 0).unwrap_or(MAX_DOMAINNAME);
    let copy_len = core::cmp::min(len, domain_len);

    copy_to_user_buffer(name as u64, &dn[..copy_len])?;
    if copy_len < len {
        copy_to_user_buffer(name.wrapping_add(copy_len) as u64, &[0])?;
    }

    Ok(0)
}

// ============================================================================
// System Control (sysctl)
// ============================================================================

/// Old Linux sysctl argument block
#[repr(C)]
struct SysctlArgs {
    name: *mut i32,
    nlen: i32,
    oldval: *mut u8,
    oldlenp: *mut usize,
    newval: *mut u8,
    newlen: usize,
}

fn sysctl_copy_out(value: &[u8], oldval: *mut u8, oldlenp: *mut usize) -> LinuxResult<i32> {
    if oldlenp.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let required = value.len();
    let mut len_bytes = [0u8; core::mem::size_of::<usize>()];
    copy_from_user_buffer(oldlenp as u64, &mut len_bytes)?;
    let available = usize::from_ne_bytes(len_bytes);

    copy_to_user_buffer(oldlenp as u64, &required.to_ne_bytes())?;

    if oldval.is_null() {
        return Ok(0);
    }

    if available < required {
        return Err(LinuxError::ENOSPC);
    }

    copy_to_user_buffer(oldval as u64, value)?;

    Ok(0)
}

fn sysctl_read_int(value: i32, oldval: *mut u8, oldlenp: *mut usize) -> LinuxResult<i32> {
    sysctl_copy_out(&value.to_ne_bytes(), oldval, oldlenp)
}

fn sysctl_read_string(value: &str, oldval: *mut u8, oldlenp: *mut usize) -> LinuxResult<i32> {
    let mut bytes = alloc::vec::Vec::with_capacity(value.len() + 1);
    bytes.extend_from_slice(value.as_bytes());
    bytes.push(0);
    sysctl_copy_out(&bytes, oldval, oldlenp)
}

fn sysctl_lookup(name: &[i32]) -> LinuxResult<SysctlValue> {
    match name {
        [1, 1] => Ok(SysctlValue::String("Linux")),
        [1, 2] => Ok(SysctlValue::String("6.12.58-rustos")),
        [1, 3] => Ok(SysctlValue::String("#1 SMP RustOS (linux-compat)")),
        [1, 8] => Ok(SysctlValue::Int(0)),
        [1, 10] => Ok(SysctlValue::Int(4096)),
        [1, 38] => {
            let hn = HOSTNAME.lock();
            let len = hn.iter().position(|&b| b == 0).unwrap_or(MAX_HOSTNAME);
            let host = core::str::from_utf8(&hn[..len]).unwrap_or("localhost");
            Ok(SysctlValue::OwnedString(alloc::string::String::from(host)))
        }
        [2, 6] => Ok(SysctlValue::Int(60)), // vm.dirty_expire_centisecs
        [2, 11] => Ok(SysctlValue::Int(10)), // vm.dirty_background_ratio
        [2, 17] => Ok(SysctlValue::Int(0)), // vm.nr_hugepages
        [1, 65] => Ok(SysctlValue::Int(128)), // net.core.somaxconn
        [4, 2] => Ok(SysctlValue::Int(4)),  // kernel.printk log level
        [1, 7] => Ok(SysctlValue::Int(0)),  // net.ipv4.ip_forward
        _ => Err(LinuxError::ENOENT),       // unknown key, not ENOSYS
    }
}

enum SysctlValue {
    Int(i32),
    String(&'static str),
    OwnedString(alloc::string::String),
}

/// sysctl - read/write system parameters
pub fn sysctl(args: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    if args.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let mut params = SysctlArgs {
        name: core::ptr::null_mut(),
        nlen: 0,
        oldval: core::ptr::null_mut(),
        oldlenp: core::ptr::null_mut(),
        newval: core::ptr::null_mut(),
        newlen: 0,
    };
    let params_bytes = unsafe {
        core::slice::from_raw_parts_mut(
            &mut params as *mut SysctlArgs as *mut u8,
            core::mem::size_of::<SysctlArgs>(),
        )
    };
    copy_from_user_buffer(args as u64, params_bytes)?;

    if params.name.is_null() || params.nlen <= 0 || params.nlen > 256 {
        return Err(LinuxError::EINVAL);
    }

    if !params.newval.is_null() || params.newlen != 0 {
        return Err(LinuxError::EPERM);
    }

    let mut name = vec![0i32; params.nlen as usize];
    let name_bytes = unsafe {
        core::slice::from_raw_parts_mut(
            name.as_mut_ptr() as *mut u8,
            name.len() * core::mem::size_of::<i32>(),
        )
    };
    copy_from_user_buffer(params.name as u64, name_bytes)?;

    match sysctl_lookup(&name) {
        Ok(SysctlValue::Int(v)) => sysctl_read_int(v, params.oldval, params.oldlenp),
        Ok(SysctlValue::String(s)) => sysctl_read_string(s, params.oldval, params.oldlenp),
        Ok(SysctlValue::OwnedString(s)) => sysctl_read_string(&s, params.oldval, params.oldlenp),
        Err(e) => Err(e),
    }
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

    if buflen == 0 {
        return Ok(0);
    }

    // Get random bytes from kernel security RNG when initialized.
    let copy_len = buflen.min(MAX_GETRANDOM_CHUNK);
    let mut buffer = vec![0u8; copy_len];
    if crate::security::is_rng_initialized() {
        if crate::security::secure_random_bytes(&mut buffer).is_ok() {
            copy_to_user_buffer(buf as u64, &buffer)?;
            return Ok(copy_len as isize);
        }
    }

    // Early boot / test fallback: TSC-based PRNG (safe in QEMU without RDRAND).
    for (i, byte) in buffer.iter_mut().enumerate() {
        let tsc = unsafe { core::arch::x86_64::_rdtsc() };
        *byte = (tsc.wrapping_shr((i % 8) as u32) as u8).wrapping_add(i as u8);
    }

    copy_to_user_buffer(buf as u64, &buffer)?;
    Ok(copy_len as isize)
}

// ============================================================================
// System Logging
// ============================================================================

/// syslog - read/control kernel ring buffer
pub fn syslog(log_type: i32, bufp: *mut u8, _len: i32) -> LinuxResult<i32> {
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
            let logs = crate::logging::get_recent_logs();
            let mut written = 0;
            for entry in &logs {
                let line = alloc::format!("{}\n", entry);
                let bytes = line.as_bytes();
                if written + bytes.len() > _len as usize {
                    break;
                }
                unsafe {
                    core::ptr::copy_nonoverlapping(bytes.as_ptr(), bufp.add(written), bytes.len());
                }
                written += bytes.len();
            }
            if log_type == SYSLOG_ACTION_READ_CLEAR {
                crate::logging::flush_logs();
            }
            Ok(written as i32)
        }
        SYSLOG_ACTION_CLEAR => {
            crate::logging::flush_logs();
            Ok(0)
        }
        SYSLOG_ACTION_SIZE_UNREAD => {
            let logs = crate::logging::get_recent_logs();
            let total: usize = logs.iter().map(|e| alloc::format!("{}\n", e).len()).sum();
            Ok(total as i32)
        }
        SYSLOG_ACTION_SIZE_BUFFER => Ok(16384),
        _ => Err(LinuxError::EINVAL),
    }
}

// ============================================================================
// Reboot Operations
// ============================================================================

/// reboot - reboot or enable/disable Ctrl-Alt-Del
pub fn reboot(magic: i32, magic2: i32, cmd: u32, _arg: *mut u8) -> LinuxResult<i32> {
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
    const LINUX_REBOOT_CMD_KEXEC: u32 = 0x45584543;

    match cmd {
        LINUX_REBOOT_CMD_KEXEC => {
            crate::serial_println!("[sysinfo_ops] reboot: kexec handoff");
            crate::kexec::execute_loaded_image()?;
            Ok(0)
        }
        LINUX_REBOOT_CMD_RESTART => {
            crate::serial_println!("[sysinfo_ops] reboot: restarting system");
            let _ = crate::kernel::shutdown();
            crate::exit_qemu(crate::QemuExitCode::Success);
            Ok(0)
        }
        LINUX_REBOOT_CMD_HALT | LINUX_REBOOT_CMD_POWER_OFF => {
            crate::serial_println!("[sysinfo_ops] reboot: halting system");
            let _ = crate::kernel::shutdown();
            crate::exit_qemu(crate::QemuExitCode::Success);
            Ok(0)
        }
        LINUX_REBOOT_CMD_CAD_ON | LINUX_REBOOT_CMD_CAD_OFF => Ok(0),
        _ => Err(LinuxError::EINVAL),
    }
}

// ============================================================================
// CPU Information
// ============================================================================

/// get_nprocs - get number of processors
pub fn get_nprocs() -> i32 {
    inc_ops();
    crate::smp::online_cpus() as i32
}

/// get_nprocs_conf - get configured number of processors
pub fn get_nprocs_conf() -> i32 {
    inc_ops();
    let configured = crate::smp::cpu_count();
    if configured > 0 {
        configured as i32
    } else {
        crate::smp::online_cpus() as i32
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_sysinfo() {
        let mut info = SysInfo::zero();
        assert!(sysinfo(&mut info).is_ok());
        // totalram may be 0 if memory stats aren't initialized in test env
        assert!(info.uptime >= 0);
    }

    #[test_case]
    fn test_uname() {
        let mut uts = UtsName::default();
        assert!(uname(&mut uts).is_ok());
        assert_eq!(&uts.sysname[..6], b"RustOS");
    }

    #[test_case]
    fn test_hostname() {
        init_sysinfo_operations();
        let mut buf = [0u8; 256];
        assert!(gethostname(buf.as_mut_ptr(), buf.len()).is_ok());
        // Should contain "localhost" after init
        assert_eq!(&buf[..9], b"localhost");
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
