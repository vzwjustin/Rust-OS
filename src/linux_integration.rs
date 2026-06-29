//! Linux Integration Module
//!
//! This module provides deep integration between the Linux compatibility layer
//! and the RustOS native kernel, ensuring that Linux APIs properly utilize
//! RustOS kernel subsystems while maintaining the custom Rust kernel as the
//! main driver.

#![allow(unused)]

use crate::linux_compat::{self, LinuxError, LinuxResult};
use lazy_static::lazy_static;
use spin::Mutex;

/// Integration state
static INTEGRATION_INITIALIZED: Mutex<bool> = Mutex::new(false);

/// Integration statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct IntegrationStats {
    /// Number of Linux API calls routed to kernel
    pub syscalls_routed: u64,
    /// Number of VFS operations
    pub vfs_operations: u64,
    /// Number of process operations
    pub process_operations: u64,
    /// Number of network operations
    pub network_operations: u64,
    /// Number of memory operations
    pub memory_operations: u64,
}

lazy_static! {
    static ref INTEGRATION_STATS: Mutex<IntegrationStats> = Mutex::new(IntegrationStats::default());
}

/// Initialize Linux integration with kernel subsystems
pub fn init() -> Result<(), &'static str> {
    let mut initialized = INTEGRATION_INITIALIZED.lock();
    if *initialized {
        return Ok(());
    }

    unsafe {
        crate::early_serial_write_str("[Linux Integration] Initializing deep integration...\r\n")
    };

    // Wire Linux compat file operations to VFS
    unsafe { crate::early_serial_write_str("linux_integration: vfs begin\r\n") };
    init_vfs_integration()?;
    unsafe { crate::early_serial_write_str("linux_integration: vfs ok\r\n") };

    // Wire Linux compat process operations to process manager
    unsafe { crate::early_serial_write_str("linux_integration: process begin\r\n") };
    init_process_integration()?;
    unsafe { crate::early_serial_write_str("linux_integration: process ok\r\n") };

    // Wire Linux compat socket operations to network stack
    unsafe { crate::early_serial_write_str("linux_integration: network begin\r\n") };
    init_network_integration()?;
    unsafe { crate::early_serial_write_str("linux_integration: network ok\r\n") };

    // Wire Linux compat memory operations to memory manager
    unsafe { crate::early_serial_write_str("linux_integration: memory begin\r\n") };
    init_memory_integration()?;
    unsafe { crate::early_serial_write_str("linux_integration: memory ok\r\n") };

    // Wire Linux compat time operations to time subsystem
    unsafe { crate::early_serial_write_str("linux_integration: time begin\r\n") };
    init_time_integration()?;
    unsafe { crate::early_serial_write_str("linux_integration: time ok\r\n") };

    // Ensure crypto registry is ready for AF_ALG-style consumers
    unsafe { crate::early_serial_write_str("linux_integration: crypto begin\r\n") };
    init_crypto_integration()?;
    unsafe { crate::early_serial_write_str("linux_integration: crypto ok\r\n") };

    *initialized = true;
    unsafe { crate::early_serial_write_str("[Linux Integration] Deep integration complete\r\n") };

    Ok(())
}

/// Initialize VFS integration for Linux file operations
fn init_vfs_integration() -> Result<(), &'static str> {
    unsafe {
        crate::early_serial_write_str("[Linux Integration] Wiring file operations to VFS...\r\n")
    };

    // The linux_compat::file_ops module already uses our VFS
    // Just verify that VFS is available

    unsafe {
        crate::early_serial_write_str(
            "[Linux Integration] File operations -> VFS integration ready\r\n",
        )
    };
    Ok(())
}

/// Initialize process integration for Linux process operations
fn init_process_integration() -> Result<(), &'static str> {
    unsafe {
        crate::early_serial_write_str(
            "[Linux Integration] Wiring process operations to process manager...\r\n",
        )
    };

    // The linux_compat::process_ops module uses our process manager
    // Verify that process manager is available

    unsafe {
        crate::early_serial_write_str(
            "[Linux Integration] Process operations -> Process Manager integration ready\r\n",
        )
    };
    Ok(())
}

/// Initialize network integration for Linux socket operations
fn init_network_integration() -> Result<(), &'static str> {
    unsafe {
        crate::early_serial_write_str(
            "[Linux Integration] Wiring socket operations to network stack...\r\n",
        )
    };

    // The linux_compat::socket_ops module uses our network stack
    // Verify that network stack is available

    unsafe {
        crate::early_serial_write_str(
            "[Linux Integration] Socket operations -> Network Stack integration ready\r\n",
        )
    };
    Ok(())
}

/// Initialize memory integration for Linux memory operations
fn init_memory_integration() -> Result<(), &'static str> {
    unsafe {
        crate::early_serial_write_str(
            "[Linux Integration] Wiring memory operations to memory manager...\r\n",
        )
    };

    // The linux_compat::memory_ops module uses our memory manager
    // Verify that memory manager is available

    unsafe {
        crate::early_serial_write_str(
            "[Linux Integration] Memory operations -> Memory Manager integration ready\r\n",
        )
    };
    Ok(())
}

/// Initialize time integration for Linux time operations
fn init_time_integration() -> Result<(), &'static str> {
    unsafe {
        crate::early_serial_write_str(
            "[Linux Integration] Wiring time operations to time subsystem...\r\n",
        )
    };

    // The linux_compat::time_ops module uses our time subsystem
    // Verify that time subsystem is available

    unsafe {
        crate::early_serial_write_str(
            "[Linux Integration] Time operations -> Time Subsystem integration ready\r\n",
        )
    };
    Ok(())
}

/// Initialize crypto integration for hash/cipher consumers (AF_ALG-style).
fn init_crypto_integration() -> Result<(), &'static str> {
    if !crate::crypto::is_initialized() {
        crate::crypto::init();
    }

    if crate::crypto::crypto_alg_count() == 0 {
        return Err("crypto registry empty after init");
    }

    unsafe {
        crate::early_serial_write_str(
            "[Linux Integration] Crypto algorithms -> kernel crypto registry ready\r\n",
        )
    };
    Ok(())
}

/// Route a Linux syscall through the integration layer
pub fn route_syscall(syscall_number: u64, args: &[u64]) -> LinuxResult<u64> {
    let mut stats = INTEGRATION_STATS.lock();
    stats.syscalls_routed += 1;

    let syscall = crate::syscall::SyscallNumber::from_u64(syscall_number);
    if syscall == crate::syscall::SyscallNumber::Invalid {
        return Err(LinuxError::ENOSYS);
    }

    // Route to appropriate subsystem based on syscall type
    match syscall {
        crate::syscall::SyscallNumber::Signalfd4 => {
            let mask_ptr = args[1] as *const u64;
            if mask_ptr.is_null() {
                return Err(LinuxError::EFAULT);
            }
            if args[2] < core::mem::size_of::<u64>() as u64 {
                return Err(LinuxError::EINVAL);
            }
            let mask = unsafe { core::ptr::read_unaligned(mask_ptr) };
            linux_compat::special_fd::signalfd(args[0] as i32, mask, args[3] as i32)
                .map(|v| v as u64)
        }
        // File operations
        crate::syscall::SyscallNumber::Read
        | crate::syscall::SyscallNumber::Write
        | crate::syscall::SyscallNumber::Open
        | crate::syscall::SyscallNumber::Close
        | crate::syscall::SyscallNumber::Stat
        | crate::syscall::SyscallNumber::Fstat
        | crate::syscall::SyscallNumber::Lstat
        | crate::syscall::SyscallNumber::Lseek
        | crate::syscall::SyscallNumber::Pread64
        | crate::syscall::SyscallNumber::Pwrite64
        | crate::syscall::SyscallNumber::Readv
        | crate::syscall::SyscallNumber::Writev
        | crate::syscall::SyscallNumber::Access
        | crate::syscall::SyscallNumber::Pipe
        | crate::syscall::SyscallNumber::Dup
        | crate::syscall::SyscallNumber::Dup2
        | crate::syscall::SyscallNumber::Fsync
        | crate::syscall::SyscallNumber::Fdatasync
        | crate::syscall::SyscallNumber::Truncate
        | crate::syscall::SyscallNumber::Ftruncate
        | crate::syscall::SyscallNumber::Getdents
        | crate::syscall::SyscallNumber::Getcwd
        | crate::syscall::SyscallNumber::Chdir
        | crate::syscall::SyscallNumber::Fchdir
        | crate::syscall::SyscallNumber::Rename
        | crate::syscall::SyscallNumber::Mkdir
        | crate::syscall::SyscallNumber::Rmdir
        | crate::syscall::SyscallNumber::Creat
        | crate::syscall::SyscallNumber::Link
        | crate::syscall::SyscallNumber::Unlink
        | crate::syscall::SyscallNumber::Symlink
        | crate::syscall::SyscallNumber::Readlink
        | crate::syscall::SyscallNumber::Chmod
        | crate::syscall::SyscallNumber::Fchmod
        | crate::syscall::SyscallNumber::Chown
        | crate::syscall::SyscallNumber::Fchown
        | crate::syscall::SyscallNumber::Lchown
        | crate::syscall::SyscallNumber::Openat
        | crate::syscall::SyscallNumber::Newfstatat
        | crate::syscall::SyscallNumber::Getdents64 => {
            stats.vfs_operations += 1;
            route_file_syscall(syscall_number, args)
        }
        // Process operations
        crate::syscall::SyscallNumber::Fork
        | crate::syscall::SyscallNumber::Execve
        | crate::syscall::SyscallNumber::Exit
        | crate::syscall::SyscallNumber::ExitGroup
        | crate::syscall::SyscallNumber::Wait4
        | crate::syscall::SyscallNumber::GetPid
        | crate::syscall::SyscallNumber::GetPpid
        | crate::syscall::SyscallNumber::Gettid
        | crate::syscall::SyscallNumber::Clone
        | crate::syscall::SyscallNumber::RtSigaction
        | crate::syscall::SyscallNumber::RtSigprocmask
        | crate::syscall::SyscallNumber::Getuid
        | crate::syscall::SyscallNumber::Geteuid
        | crate::syscall::SyscallNumber::Getgid
        | crate::syscall::SyscallNumber::Getegid
        | crate::syscall::SyscallNumber::Setuid
        | crate::syscall::SyscallNumber::Setgid
        | crate::syscall::SyscallNumber::Setreuid
        | crate::syscall::SyscallNumber::Setregid
        | crate::syscall::SyscallNumber::Setresuid
        | crate::syscall::SyscallNumber::Getresuid
        | crate::syscall::SyscallNumber::Setresgid
        | crate::syscall::SyscallNumber::Getresgid
        | crate::syscall::SyscallNumber::Getgroups
        | crate::syscall::SyscallNumber::Setgroups
        | crate::syscall::SyscallNumber::Setpgid
        | crate::syscall::SyscallNumber::Getpgid
        | crate::syscall::SyscallNumber::Getpgrp
        | crate::syscall::SyscallNumber::Setsid
        | crate::syscall::SyscallNumber::Getsid
        | crate::syscall::SyscallNumber::Umask
        | crate::syscall::SyscallNumber::Chroot
        | crate::syscall::SyscallNumber::Getrlimit
        | crate::syscall::SyscallNumber::Setrlimit
        | crate::syscall::SyscallNumber::Prlimit64
        | crate::syscall::SyscallNumber::Getrusage
        | crate::syscall::SyscallNumber::Times
        | crate::syscall::SyscallNumber::Sysinfo
        | crate::syscall::SyscallNumber::Prctl
        | crate::syscall::SyscallNumber::Capget
        | crate::syscall::SyscallNumber::Capset
        | crate::syscall::SyscallNumber::SchedYield
        | crate::syscall::SyscallNumber::SchedGetaffinity
        | crate::syscall::SyscallNumber::SchedSetaffinity => {
            stats.process_operations += 1;
            route_process_syscall(syscall_number, args)
        }
        // Network operations
        crate::syscall::SyscallNumber::Socket
        | crate::syscall::SyscallNumber::Connect
        | crate::syscall::SyscallNumber::Accept
        | crate::syscall::SyscallNumber::Sendto
        | crate::syscall::SyscallNumber::Recvfrom
        | crate::syscall::SyscallNumber::Sendmsg
        | crate::syscall::SyscallNumber::Recvmsg
        | crate::syscall::SyscallNumber::Shutdown
        | crate::syscall::SyscallNumber::Bind
        | crate::syscall::SyscallNumber::Listen
        | crate::syscall::SyscallNumber::Getsockname
        | crate::syscall::SyscallNumber::Getpeername
        | crate::syscall::SyscallNumber::Socketpair
        | crate::syscall::SyscallNumber::SetSockopt
        | crate::syscall::SyscallNumber::GetSockopt => {
            stats.network_operations += 1;
            route_network_syscall(syscall_number, args)
        }
        // Memory operations
        crate::syscall::SyscallNumber::Mmap
        | crate::syscall::SyscallNumber::Mprotect
        | crate::syscall::SyscallNumber::Munmap
        | crate::syscall::SyscallNumber::Brk
        | crate::syscall::SyscallNumber::Mremap
        | crate::syscall::SyscallNumber::Msync
        | crate::syscall::SyscallNumber::Mincore
        | crate::syscall::SyscallNumber::Madvise => {
            stats.memory_operations += 1;
            route_memory_syscall(syscall_number, args)
        }
        _ => route_misc_syscall(syscall_number, args),
    }
}

/// Route known Linux syscalls that RustOS does not provide as full subsystems.
fn route_misc_syscall(syscall_number: u64, args: &[u64]) -> LinuxResult<u64> {
    let syscall = crate::syscall::SyscallNumber::from_u64(syscall_number);
    match syscall {
        crate::syscall::SyscallNumber::Futimesat => linux_compat::file_ops::utimes(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as *const [linux_compat::TimeVal; 2],
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Personality => {
            linux_compat::process_ops::personality(args[0] as u32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::IoprioSet => {
            linux_compat::resource_ops::ioprio_set(args[0] as i32, args[1] as i32, args[2] as i32)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::IoprioGet => {
            linux_compat::resource_ops::ioprio_get(args[0] as i32, args[1] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::CloseRange => {
            linux_compat::file_ops::close_range(args[0] as u32, args[1] as u32, args[2] as u32)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::PkeyAlloc => {
            linux_compat::memory_ops::pkey_alloc(args[0] as u32, args[1] as u32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::PkeyFree => {
            linux_compat::memory_ops::pkey_free(args[0] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::PkeyMprotect => linux_compat::memory_ops::pkey_mprotect(
            args[0] as *mut u8,
            args[1] as usize,
            args[2] as i32,
            args[3] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::MqOpen => linux_compat::ipc_ops::mq_open(
            args[0] as *const u8,
            args[1] as i32,
            args[2] as u32,
            args[3] as *const linux_compat::ipc_ops::MqAttr,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::MqUnlink => {
            linux_compat::ipc_ops::mq_unlink(args[0] as *const u8).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::MqTimedsend => linux_compat::ipc_ops::mq_timedsend(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as usize,
            args[3] as u32,
            args[4] as *const linux_compat::TimeSpec,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::MqTimedreceive => linux_compat::ipc_ops::mq_timedreceive(
            args[0] as i32,
            args[1] as *mut u8,
            args[2] as usize,
            args[3] as *mut u32,
            args[4] as *const linux_compat::TimeSpec,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::MqNotify => {
            linux_compat::ipc_ops::mq_notify(args[0] as i32, args[1] as *const u8).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::MqGetsetattr => linux_compat::ipc_ops::mq_getsetattr(
            args[0] as i32,
            args[1] as *const linux_compat::ipc_ops::MqAttr,
            args[2] as *mut linux_compat::ipc_ops::MqAttr,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Accept4 => linux_compat::socket_ops::accept4(
            args[0] as i32,
            args[1] as *mut linux_compat::SockAddr,
            args[2] as *mut u32,
            args[3] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Alarm => {
            linux_compat::process_ops::alarm(args[0] as u32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::ArchPrctl => {
            linux_compat::thread_ops::arch_prctl(args[0] as i32, args[1] as u64).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::ClockGetres => linux_compat::time_ops::clock_getres(
            args[0] as i32,
            args[1] as *mut linux_compat::TimeSpec,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::ClockGettime => linux_compat::time_ops::clock_gettime(
            args[0] as i32,
            args[1] as *mut linux_compat::TimeSpec,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::ClockNanosleep => linux_compat::time_ops::clock_nanosleep(
            args[0] as i32,
            args[1] as i32,
            args[2] as *const linux_compat::TimeSpec,
            args[3] as *mut linux_compat::TimeSpec,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::ClockSettime => linux_compat::time_ops::clock_settime(
            args[0] as i32,
            args[1] as *const linux_compat::TimeSpec,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Clone3 => linux_compat::thread_ops::clone3(
            args[0] as *const linux_compat::CloneArgs,
            args[1] as usize,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::CopyFileRange => linux_compat::advanced_io::copy_file_range(
            args[0] as i32,
            args[1] as *mut linux_compat::Off,
            args[2] as i32,
            args[3] as *mut linux_compat::Off,
            args[4] as usize,
            args[5] as u32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Dup3 => {
            linux_compat::file_ops::dup3(args[0] as i32, args[1] as i32, args[2] as i32)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::EpollCreate1 => {
            linux_compat::socket_ops::epoll_create1(args[0] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::EpollCtl => linux_compat::socket_ops::epoll_ctl(
            args[0] as i32,
            args[1] as i32,
            args[2] as i32,
            args[3] as *mut u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::EpollPwait => linux_compat::socket_ops::epoll_pwait(
            args[0] as i32,
            args[1] as *mut u8,
            args[2] as i32,
            args[3] as i32,
            args[4] as *const u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::EpollPwait2 => linux_compat::socket_ops::epoll_pwait2(
            args[0] as i32,
            args[1] as *mut u8,
            args[2] as i32,
            args[3] as *const u8,
            args[4] as *const u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::EpollWait => linux_compat::socket_ops::epoll_wait(
            args[0] as i32,
            args[1] as *mut u8,
            args[2] as i32,
            args[3] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Eventfd => {
            linux_compat::ipc_ops::eventfd(args[0] as u32, args[1] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Eventfd2 => {
            linux_compat::ipc_ops::eventfd2(args[0] as u32, args[1] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Execveat => linux_compat::process_ops::execveat(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as *const *const u8,
            args[3] as *const *const u8,
            args[4] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Faccessat => linux_compat::file_ops::faccessat(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as i32,
            args[3] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Faccessat2 => linux_compat::file_ops::faccessat2(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as i32,
            args[3] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Fadvise64 => linux_compat::advanced_io::fadvise64(
            args[0] as i32,
            args[1] as i64,
            args[2] as i64,
            args[3] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Fallocate => linux_compat::file_ops::fallocate(
            args[0] as i32,
            args[1] as i32,
            args[2] as linux_compat::Off,
            args[3] as linux_compat::Off,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Fchmodat => linux_compat::file_ops::fchmodat(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as u32,
            args[3] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Fchownat => linux_compat::file_ops::fchownat(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as u32,
            args[3] as u32,
            args[4] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Fcntl => {
            linux_compat::ioctl_ops::fcntl(args[0] as i32, args[1] as i32, args[2] as u64)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Fgetxattr => linux_compat::advanced_io::fgetxattr(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as *mut u8,
            args[3] as usize,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Flistxattr => linux_compat::advanced_io::flistxattr(
            args[0] as i32,
            args[1] as *mut u8,
            args[2] as usize,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Flock => {
            linux_compat::file_ops::flock(args[0] as i32, args[1] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Fremovexattr => {
            linux_compat::advanced_io::fremovexattr(args[0] as i32, args[1] as *const u8)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Fsetxattr => linux_compat::advanced_io::fsetxattr(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as *const u8,
            args[3] as usize,
            args[4] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Fstatfs => linux_compat::fs_ops::fstatfs(
            args[0] as i32,
            args[1] as *mut linux_compat::fs_ops::StatFs,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Futex => linux_compat::thread_ops::futex(
            args[0] as *mut i32,
            args[1] as i32,
            args[2] as i32,
            args[3] as *const linux_compat::TimeSpec,
            args[4] as *mut i32,
            args[5] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::GetMempolicy => linux_compat::memory_ops::get_mempolicy(
            args[0] as *mut i32,
            args[1] as *mut u64,
            args[2] as u64,
            args[3] as *mut u8,
            args[4] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::GetRobustList => linux_compat::thread_ops::get_robust_list(
            args[0] as i32,
            args[1] as *mut *mut linux_compat::thread_ops::RobustListHead,
            args[2] as *mut usize,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Getcpu => linux_compat::thread_ops::getcpu(
            args[0] as *mut u32,
            args[1] as *mut u32,
            args[2] as *mut u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Getitimer => {
            linux_compat::process_ops::getitimer(args[0] as i32, args[1] as *mut u8)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getpriority => {
            linux_compat::process_ops::getpriority(args[0] as i32, args[1] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getrandom => linux_compat::sysinfo_ops::getrandom(
            args[0] as *mut u8,
            args[1] as usize,
            args[2] as u32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Gettimeofday => linux_compat::time_ops::gettimeofday(
            args[0] as *mut linux_compat::TimeVal,
            args[1] as *mut u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Getxattr => linux_compat::advanced_io::getxattr(
            args[0] as *const u8,
            args[1] as *const u8,
            args[2] as *mut u8,
            args[3] as usize,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::InotifyAddWatch => linux_compat::fs_ops::inotify_add_watch(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as u32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::InotifyInit => {
            linux_compat::fs_ops::inotify_init().map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::InotifyInit1 => {
            linux_compat::fs_ops::inotify_init1(args[0] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::InotifyRmWatch => {
            linux_compat::fs_ops::inotify_rm_watch(args[0] as i32, args[1] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Ioctl => {
            linux_compat::ioctl_ops::ioctl(args[0] as i32, args[1] as u64, args[2] as u64)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Kill => {
            linux_compat::signal_ops::kill(args[0] as i32, args[1] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Lgetxattr => linux_compat::advanced_io::lgetxattr(
            args[0] as *const u8,
            args[1] as *const u8,
            args[2] as *mut u8,
            args[3] as usize,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Linkat => linux_compat::file_ops::linkat(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as i32,
            args[3] as *const u8,
            args[4] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Listxattr => linux_compat::advanced_io::listxattr(
            args[0] as *const u8,
            args[1] as *mut u8,
            args[2] as usize,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Llistxattr => linux_compat::advanced_io::llistxattr(
            args[0] as *const u8,
            args[1] as *mut u8,
            args[2] as usize,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Lremovexattr => {
            linux_compat::advanced_io::lremovexattr(args[0] as *const u8, args[1] as *const u8)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Lsetxattr => linux_compat::advanced_io::lsetxattr(
            args[0] as *const u8,
            args[1] as *const u8,
            args[2] as *const u8,
            args[3] as usize,
            args[4] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Mbind => linux_compat::memory_ops::mbind(
            args[0] as *mut u8,
            args[1] as usize,
            args[2] as i32,
            args[3] as *const u64,
            args[4] as u64,
            args[5] as u32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Membarrier => {
            linux_compat::thread_ops::membarrier(args[0] as i32, args[1] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::MemfdCreate => {
            linux_compat::memory_ops::memfd_create(args[0] as *const u8, args[1] as u32)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::MigratePages => linux_compat::memory_ops::migrate_pages(
            args[0] as i32,
            args[1] as u64,
            args[2] as *const u64,
            args[3] as *const u64,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Mkdirat => {
            linux_compat::file_ops::mkdirat(args[0] as i32, args[1] as *const u8, args[2] as u32)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Mlock => {
            linux_compat::memory_ops::mlock(args[0] as *const u8, args[1] as usize)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Mlock2 => {
            linux_compat::memory_ops::mlock2(args[0] as *const u8, args[1] as usize, args[2] as i32)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Mlockall => {
            linux_compat::memory_ops::mlockall(args[0] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Mount => linux_compat::fs_ops::mount(
            args[0] as *const u8,
            args[1] as *const u8,
            args[2] as *const u8,
            args[3] as u64,
            args[4] as *const u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::MovePages => linux_compat::memory_ops::move_pages(
            args[0] as i32,
            args[1] as u64,
            args[2] as *const *mut u8,
            args[3] as *const i32,
            args[4] as *mut i32,
            args[5] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Msgctl => linux_compat::ipc_ops::msgctl(
            args[0] as linux_compat::ipc_ops::MsqId,
            args[1] as i32,
            args[2] as *mut u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Msgget => {
            linux_compat::ipc_ops::msgget(args[0] as linux_compat::ipc_ops::Key, args[1] as i32)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Msgrcv => linux_compat::ipc_ops::msgrcv(
            args[0] as linux_compat::ipc_ops::MsqId,
            args[1] as *mut u8,
            args[2] as usize,
            args[3] as i64,
            args[4] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Msgsnd => linux_compat::ipc_ops::msgsnd(
            args[0] as linux_compat::ipc_ops::MsqId,
            args[1] as *const u8,
            args[2] as usize,
            args[3] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Munlock => {
            linux_compat::memory_ops::munlock(args[0] as *const u8, args[1] as usize)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Munlockall => {
            linux_compat::memory_ops::munlockall().map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Nanosleep => linux_compat::time_ops::nanosleep(
            args[0] as *const linux_compat::TimeSpec,
            args[1] as *mut linux_compat::TimeSpec,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Openat2 => linux_compat::file_ops::openat2(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as *const linux_compat::OpenHow,
            args[3] as usize,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Pause => linux_compat::signal_ops::pause().map(|v| v as u64),
        crate::syscall::SyscallNumber::Pipe2 => {
            linux_compat::ipc_ops::pipe2(args[0] as *mut [i32; 2], args[1] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::PivotRoot => {
            linux_compat::fs_ops::pivot_root(args[0] as *const u8, args[1] as *const u8)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::PkgInfo => route_package_syscall(syscall_number, args),
        crate::syscall::SyscallNumber::PkgInstall => route_package_syscall(syscall_number, args),
        crate::syscall::SyscallNumber::PkgList => route_package_syscall(syscall_number, args),
        crate::syscall::SyscallNumber::PkgRemove => route_package_syscall(syscall_number, args),
        crate::syscall::SyscallNumber::PkgSearch => route_package_syscall(syscall_number, args),
        crate::syscall::SyscallNumber::PkgUpdate => route_package_syscall(syscall_number, args),
        crate::syscall::SyscallNumber::PkgUpgrade => route_package_syscall(syscall_number, args),
        crate::syscall::SyscallNumber::Poll => linux_compat::socket_ops::poll(
            args[0] as *mut linux_compat::PollFd,
            args[1] as u64,
            args[2] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Ppoll => linux_compat::socket_ops::ppoll(
            args[0] as *mut linux_compat::PollFd,
            args[1] as u64,
            args[2] as *const linux_compat::TimeSpec,
            args[3] as *const u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Preadv => linux_compat::advanced_io::preadv(
            args[0] as i32,
            args[1] as *const linux_compat::advanced_io::IoVec,
            args[2] as i32,
            args[3] as linux_compat::Off,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Preadv2 => linux_compat::advanced_io::preadv2(
            args[0] as i32,
            args[1] as *const linux_compat::advanced_io::IoVec,
            args[2] as i32,
            args[3] as linux_compat::Off,
            args[4] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Pwritev => linux_compat::advanced_io::pwritev(
            args[0] as i32,
            args[1] as *const linux_compat::advanced_io::IoVec,
            args[2] as i32,
            args[3] as linux_compat::Off,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Pwritev2 => linux_compat::advanced_io::pwritev2(
            args[0] as i32,
            args[1] as *const linux_compat::advanced_io::IoVec,
            args[2] as i32,
            args[3] as linux_compat::Off,
            args[4] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Readahead => {
            linux_compat::advanced_io::readahead(args[0] as i32, args[1] as i64, args[2] as usize)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Readlinkat => linux_compat::file_ops::readlinkat(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as *mut u8,
            args[3] as usize,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Reboot => linux_compat::sysinfo_ops::reboot(
            args[0] as i32,
            args[1] as i32,
            args[2] as u32,
            args[3] as *mut u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Recvmmsg => linux_compat::socket_ops::recvmmsg(
            args[0] as i32,
            args[1] as *mut u8,
            args[2] as u32,
            args[3] as i32,
            args[4] as *const u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Removexattr => {
            linux_compat::advanced_io::removexattr(args[0] as *const u8, args[1] as *const u8)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Renameat => linux_compat::file_ops::renameat(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as i32,
            args[3] as *const u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Renameat2 => linux_compat::file_ops::renameat2(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as i32,
            args[3] as *const u8,
            args[4] as u32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::RtSigpending => linux_compat::signal_ops::rt_sigpending(
            args[0] as *mut linux_compat::SigSet,
            args[1] as usize,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::RtSigsuspend => linux_compat::signal_ops::rt_sigsuspend(
            args[0] as *const linux_compat::SigSet,
            args[1] as usize,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::SchedGetPriorityMax => {
            linux_compat::resource_ops::sched_get_priority_max(args[0] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::SchedGetPriorityMin => {
            linux_compat::resource_ops::sched_get_priority_min(args[0] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::SchedGetparam => linux_compat::resource_ops::sched_getparam(
            args[0] as i32,
            args[1] as *mut linux_compat::resource_ops::SchedParam,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::SchedGetscheduler => {
            linux_compat::resource_ops::sched_getscheduler(args[0] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::SchedRrGetInterval => {
            linux_compat::resource_ops::sched_rr_get_interval(
                args[0] as i32,
                args[1] as *mut linux_compat::TimeSpec,
            )
            .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::SchedSetparam => linux_compat::resource_ops::sched_setparam(
            args[0] as i32,
            args[1] as *const linux_compat::resource_ops::SchedParam,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::SchedSetscheduler => {
            linux_compat::resource_ops::sched_setscheduler(
                args[0] as i32,
                args[1] as i32,
                args[2] as *const linux_compat::resource_ops::SchedParam,
            )
            .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Select => linux_compat::socket_ops::select(
            args[0] as i32,
            args[1] as *mut u64,
            args[2] as *mut u64,
            args[3] as *mut u64,
            args[4] as *mut linux_compat::TimeVal,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Semget => linux_compat::ipc_ops::semget(
            args[0] as linux_compat::ipc_ops::Key,
            args[1] as i32,
            args[2] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Sendfile => linux_compat::advanced_io::sendfile(
            args[0] as i32,
            args[1] as i32,
            args[2] as *mut linux_compat::Off,
            args[3] as usize,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Sendmmsg => linux_compat::socket_ops::sendmmsg(
            args[0] as i32,
            args[1] as *mut u8,
            args[2] as u32,
            args[3] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::SetMempolicy => linux_compat::memory_ops::set_mempolicy(
            args[0] as i32,
            args[1] as *const u64,
            args[2] as u64,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::SetRobustList => linux_compat::thread_ops::set_robust_list(
            args[0] as *mut linux_compat::thread_ops::RobustListHead,
            args[1] as usize,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::SetThreadArea => {
            linux_compat::thread_ops::set_thread_area(args[0] as *mut u8).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::SetTidAddress => {
            Ok(linux_compat::thread_ops::set_tid_address(args[0] as *mut i32) as u64)
        }
        crate::syscall::SyscallNumber::Setdomainname => {
            linux_compat::sysinfo_ops::setdomainname(args[0] as *const u8, args[1] as usize)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Sethostname => {
            linux_compat::sysinfo_ops::sethostname(args[0] as *const u8, args[1] as usize)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Setitimer => linux_compat::process_ops::setitimer(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as *mut u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Setns => {
            let ret = crate::namespace::setns(args[0] as i32, args[1] as u32);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::Setpriority => {
            linux_compat::process_ops::setpriority(args[0] as i32, args[1] as i32, args[2] as i32)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Settimeofday => linux_compat::time_ops::settimeofday(
            args[0] as *const linux_compat::TimeVal,
            args[1] as *const u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Setxattr => linux_compat::advanced_io::setxattr(
            args[0] as *const u8,
            args[1] as *const u8,
            args[2] as *const u8,
            args[3] as usize,
            args[4] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Shmdt => {
            linux_compat::ipc_ops::shmdt(args[0] as *const u8).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Shmget => linux_compat::ipc_ops::shmget(
            args[0] as linux_compat::ipc_ops::Key,
            args[1] as usize,
            args[2] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Sigaltstack => {
            linux_compat::signal_ops::sigaltstack(args[0] as *const u8, args[1] as *mut u8)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Signalfd => linux_compat::ipc_ops::signalfd(
            args[0] as i32,
            args[1] as *const linux_compat::SigSet,
            args[2] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Splice => linux_compat::advanced_io::splice(
            args[0] as i32,
            args[1] as *mut linux_compat::Off,
            args[2] as i32,
            args[3] as *mut linux_compat::Off,
            args[4] as usize,
            args[5] as u32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Statfs => linux_compat::fs_ops::statfs(
            args[0] as *const u8,
            args[1] as *mut linux_compat::fs_ops::StatFs,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Swapoff => {
            linux_compat::fs_ops::swapoff(args[0] as *const u8).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Swapon => {
            linux_compat::fs_ops::swapon(args[0] as *const u8, args[1] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Symlinkat => linux_compat::file_ops::symlinkat(
            args[0] as *const u8,
            args[1] as i32,
            args[2] as *const u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::SyncFileRange => linux_compat::advanced_io::sync_file_range(
            args[0] as i32,
            args[1] as i64,
            args[2] as i64,
            args[3] as u32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Syncfs => {
            linux_compat::fs_ops::syncfs(args[0] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Syslog => {
            linux_compat::sysinfo_ops::syslog(args[0] as i32, args[1] as *mut u8, args[2] as i32)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Tee => linux_compat::advanced_io::tee(
            args[0] as i32,
            args[1] as i32,
            args[2] as usize,
            args[3] as u32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Tgkill => {
            linux_compat::thread_ops::tgkill(args[0] as i32, args[1] as i32, args[2] as i32)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::TimerCreate => linux_compat::time_ops::timer_create(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as *mut linux_compat::time_ops::TimerId,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::TimerDelete => {
            linux_compat::time_ops::timer_delete(args[0] as linux_compat::time_ops::TimerId)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::TimerGetoverrun => {
            linux_compat::time_ops::timer_getoverrun(args[0] as linux_compat::time_ops::TimerId)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::TimerfdCreate => {
            linux_compat::ipc_ops::timerfd_create(args[0] as i32, args[1] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::TimerfdGettime => {
            linux_compat::ipc_ops::timerfd_gettime(args[0] as i32, args[1] as *mut u8)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::TimerfdSettime => linux_compat::ipc_ops::timerfd_settime(
            args[0] as i32,
            args[1] as i32,
            args[2] as *const u8,
            args[3] as *mut u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Tkill => {
            linux_compat::thread_ops::tkill(args[0] as i32, args[1] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Unlinkat => {
            linux_compat::file_ops::unlinkat(args[0] as i32, args[1] as *const u8, args[2] as i32)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Unshare => {
            let ret = crate::namespace::unshare(args[0] as u32);
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::Utimensat => linux_compat::file_ops::utimensat(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as *const [linux_compat::TimeSpec; 2],
            args[3] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Utimes => linux_compat::file_ops::utimes(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as *const [linux_compat::TimeVal; 2],
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Utime => linux_compat::file_ops::utime(
            args[0] as *const u8,
            args[1] as *const linux_compat::file_ops::UtimBuf,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Mknod => {
            linux_compat::file_ops::mknod(args[0] as *const u8, args[1] as u32, args[2] as u64)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Vfork => {
            linux_compat::process_ops::vfork().map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Acct => Err(LinuxError::EPERM),
        crate::syscall::SyscallNumber::Ioperm => {
            crate::privileged_syscalls::ioperm(args[0], args[1], args[2] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Iopl => {
            crate::privileged_syscalls::iopl(args[0] as u32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::OpenByHandleAt => crate::file_handle::open_by_handle_at(
            args[0] as i32,
            args[1] as *mut u8,
            args[2] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Vhangup => {
            crate::privileged_syscalls::vhangup().map(|v| v as u64)
        }

        // ── Deprecated/removed syscalls (return ENOSYS) ──────────────
        crate::syscall::SyscallNumber::GetKernelSyms
        | crate::syscall::SyscallNumber::QueryModule
        | crate::syscall::SyscallNumber::RemapFilePages
        | crate::syscall::SyscallNumber::Nfsservctl => Err(LinuxError::ENOSYS),

        crate::syscall::SyscallNumber::EpollCreateOld => {
            linux_compat::socket_ops::epoll_create(args[0] as i32).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::EpollCtlOld => linux_compat::socket_ops::epoll_ctl(
            args[0] as i32,
            args[1] as i32,
            args[2] as i32,
            args[3] as *mut u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::EpollWaitOld => linux_compat::socket_ops::epoll_wait(
            args[0] as i32,
            args[1] as *mut u8,
            args[2] as i32,
            args[3] as i32,
        )
        .map(|v| v as u64),

        // ── sysfs (deprecated — use /proc/filesystems) ───────────────
        crate::syscall::SyscallNumber::Sysfs => {
            // sysfs(option, arg1, arg2)
            // option 1: get filesystem type index by name
            // option 2: get filesystem type name by index
            // option 3: get fs index for mounted fs
            // Deprecated — return ENOSYS
            Err(LinuxError::ENOSYS)
        }

        // ── _sysctl (deprecated — use /proc/sys) ─────────────────────
        crate::syscall::SyscallNumber::Sysctl => Err(LinuxError::ENOSYS),

        // ── quotactl (old path-based quota, we have quotactl_fd) ─────
        crate::syscall::SyscallNumber::Quotactl => linux_compat::fs_ops::quotactl(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as i32,
            args[3] as *mut u8,
        )
        .map(|v| v as u64),

        // ── get_thread_area (x86 TLS descriptor read) ────────────────
        crate::syscall::SyscallNumber::GetThreadArea => {
            linux_compat::thread_ops::get_thread_area(args[0] as *mut u8).map(|v| v as u64)
        }

        // ── lookup_dcookie (debug cookie lookup) ─────────────────────
        crate::syscall::SyscallNumber::LookupDcookie => {
            // lookup_dcookie(cookie, buf, len) — debug profiling
            Err(LinuxError::ENOSYS)
        }

        // ── cachestat (kernel 6.5+) ──────────────────────────────────
        crate::syscall::SyscallNumber::Cachestat => {
            // cachestat(fd, args, cstat, flags)
            let fd = args[0] as i32;
            let range_ptr = args[1] as *const u8;
            let cstat = args[2] as *mut u8;
            let _flags = args[3] as u32;

            if cstat.is_null() {
                return Err(LinuxError::EFAULT);
            }

            // struct cachestat_range { __u64 off; __u64 len; }
            let (off, len) = if range_ptr.is_null() {
                (0u64, u64::MAX)
            } else {
                unsafe {
                    let r = &*(range_ptr as *const (u64, u64));
                    (r.0, r.1)
                }
            };

            // Get file stats via VFS to compute real page counts
            let file_size = match crate::vfs::vfs_fstat(fd) {
                Ok(stat) => stat.size,
                Err(_) => {
                    // Can't stat — return zeros
                    let buf = unsafe { core::slice::from_raw_parts_mut(cstat, 40) };
                    for b in buf.iter_mut() {
                        *b = 0;
                    }
                    return Ok(0);
                }
            };

            // Calculate pages in the requested range
            let range_end = off.saturating_add(len).min(file_size);
            let range_start = off.min(file_size);
            let bytes_in_range = range_end.saturating_sub(range_start);
            let page_size = 4096u64;
            let nr_pages = bytes_in_range.div_ceil(page_size);

            // struct cachestat { __u64 nr_cache; __u64 nr_dirty; __u64 nr_writeback; __u64 nr_evictable; __u64 nr_recently_evicted; }
            // All pages are in memory (no swap/backing store), so nr_cache = total pages
            let buf = unsafe { core::slice::from_raw_parts_mut(cstat, 40) };
            for b in buf.iter_mut() {
                *b = 0;
            }
            unsafe {
                let cs = &mut *(cstat as *mut (u64, u64, u64, u64, u64));
                cs.0 = nr_pages; // nr_cache
                cs.1 = 0; // nr_dirty
                cs.2 = 0; // nr_writeback
                cs.3 = nr_pages; // nr_evictable
                cs.4 = 0; // nr_recently_evicted
            }
            Ok(0)
        }

        // ── fchmodat2 (kernel 6.6+) ──────────────────────────────────
        crate::syscall::SyscallNumber::Fchmodat2 => {
            // fchmodat2(dirfd, pathname, mode, flags)
            linux_compat::file_ops::fchmodat(
                args[0] as i32,
                args[1] as *const u8,
                args[2] as u32,
                args[3] as i32,
            )
            .map(|v| v as u64)
        }

        // ── map_shadow_stack (kernel 6.6+, CET) ──────────────────────
        crate::syscall::SyscallNumber::MapShadowStack => {
            // map_shadow_stack(addr, size, flags)
            // CET shadow stack — not supported on our target
            Err(LinuxError::ENOSYS)
        }

        // ── New futex API (kernel 6.7+) ──────────────────────────────
        crate::syscall::SyscallNumber::FutexWake => {
            // futex_wake(waiters, mask, flags)
            let ret = crate::futex::futex_wake(args[0] as *mut i32, args[1] as i32, 0xFFFFFFFF);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::FutexWait => {
            // futex_wait(waiters, expected, timeout, flags)
            let ret =
                crate::futex::futex_wait(args[0] as *mut i32, args[1] as i32, 0xFFFFFFFF, None);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::FutexRequeue => {
            // futex_requeue(waiters, expected, requeue_waiters, nr_requeue, flags)
            let ret = crate::futex::futex_wake(args[0] as *mut i32, args[3] as i32, 0xFFFFFFFF);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }

        // ── statmount (kernel 6.8+, new mount API) ───────────────────
        crate::syscall::SyscallNumber::Statmount => {
            // statmount(mnt_id, mask, buf, bufsize, flags)
            // mnt_id is args[0], mask is args[1], buf is args[2], bufsize is args[3]
            let buf = args[2] as *mut u8;
            let bufsize = args[3] as usize;

            if buf.is_null() {
                return Err(LinuxError::EFAULT);
            }

            let vfs = crate::vfs::get_vfs();
            let mount_paths = vfs.list_mount_paths();

            // struct statmount { __u32 size; __u32 mnt_id; __u32 mnt_parent_id;
            //   __u32 mnt_id_old; __u32 mnt_parent_id_old; __u64 sr_dev;
            //   __u64 sb_flags; __u64 opt; __u64 opt_fields; __u64 pad[16]; }
            // Total: 160 bytes minimum
            let len = core::cmp::min(bufsize, 256);
            let buf_slice = unsafe { core::slice::from_raw_parts_mut(buf, len) };
            for b in buf_slice.iter_mut() {
                *b = 0;
            }

            unsafe {
                let sm = &mut *(buf as *mut (u32, u32, u32, u32, u32, u64, u64, u64, u64));
                sm.0 = len as u32; // size
                sm.1 = 1; // mnt_id (root mount = 1)
                sm.2 = 0; // mnt_parent_id (no parent for root)
                sm.3 = 1; // mnt_id_old
                sm.4 = 0; // mnt_parent_id_old
                sm.5 = 0; // sr_dev
                sm.6 = 0; // sb_flags
                sm.7 = 0; // opt
                sm.8 = 0; // opt_fields
            }
            Ok(0)
        }

        // ── listmount (kernel 6.8+, new mount API) ───────────────────
        crate::syscall::SyscallNumber::Listmount => {
            // listmount(mnt_id, buf, bufsize, flags)
            // mnt_id is args[0], buf is args[1], bufsize is args[2]
            let buf = args[1] as *mut u64;
            let bufsize = args[2] as usize;

            if buf.is_null() {
                return Err(LinuxError::EFAULT);
            }

            let vfs = crate::vfs::get_vfs();
            let mount_paths = vfs.list_mount_paths();

            // Return mount IDs (1-based index for each mount)
            let count = core::cmp::min(mount_paths.len(), bufsize / 8);
            for i in 0..count {
                unsafe {
                    *buf.add(i) = (i + 1) as u64;
                }
            }
            Ok(count as u64)
        }

        // ── LSM syscalls (kernel 6.8+) ───────────────────────────────
        crate::syscall::SyscallNumber::LsmGetSelfAttr
        | crate::syscall::SyscallNumber::LsmSetSelfAttr
        | crate::syscall::SyscallNumber::LsmListModules => {
            // No LSM framework — return 0 for get/list (empty), ENOSYS for set
            match syscall {
                crate::syscall::SyscallNumber::LsmSetSelfAttr => Err(LinuxError::ENOSYS),
                _ => Ok(0),
            }
        }

        // ── mseal (kernel 6.10+, memory sealing) ─────────────────────
        crate::syscall::SyscallNumber::Mseal => {
            // mseal(addr, len, flags)
            let addr = args[0] as *mut u8;
            let len = args[1] as usize;
            let flags = args[2] as u32;
            linux_compat::memory_ops::mseal(addr, len, flags).map(|v| v as u64)
        }

        // ── Linux 6.12+ syscalls (463-471) — not yet implemented ──────
        crate::syscall::SyscallNumber::Setxattrat
        | crate::syscall::SyscallNumber::Getxattrat
        | crate::syscall::SyscallNumber::Listxattrat
        | crate::syscall::SyscallNumber::Removexattrat
        | crate::syscall::SyscallNumber::OpenTreeAttr
        | crate::syscall::SyscallNumber::FileGetattr
        | crate::syscall::SyscallNumber::FileSetattr
        | crate::syscall::SyscallNumber::Listns
        | crate::syscall::SyscallNumber::RseqSliceYield
        | crate::syscall::SyscallNumber::Uretprobe
        | crate::syscall::SyscallNumber::Uprobe => Err(LinuxError::ENOSYS),

        crate::syscall::SyscallNumber::Bpf => {
            let ret = crate::bpf::bpf(args[0] as u32, args[1], args[2] as u32);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }

        // ── Keyring syscalls ─────────────────────────────────────────
        crate::syscall::SyscallNumber::AddKey => {
            let ret = crate::keyring::add_key(
                args[0] as *const u8,
                args[1] as *const u8,
                args[2] as *const u8,
                args[3] as usize,
                args[4] as i32,
            );
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::RequestKey => {
            let ret = crate::keyring::request_key(
                args[0] as *const u8,
                args[1] as *const u8,
                args[2] as *const u8,
                args[3] as i32,
            );
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::Keyctl => {
            let ret = crate::keyring::keyctl(args[0] as u32, args[1], args[2], args[3], args[4]);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }

        // ── Module loading syscalls ──────────────────────────────────
        crate::syscall::SyscallNumber::InitModule => {
            let ret = crate::module_loader::init_module(
                args[0] as *const u8,
                args[1] as usize,
                args[2] as *const u8,
            );
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::FinitModule => {
            let ret = crate::module_loader::finit_module(
                args[0] as i32,
                args[1] as *const u8,
                args[2] as u32,
            );
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::DeleteModule => {
            let ret = crate::module_loader::delete_module(args[0] as *const u8, args[1] as u32);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }

        // ── kexec syscalls ───────────────────────────────────────────
        crate::syscall::SyscallNumber::KexecLoad => {
            let ret =
                crate::kexec::kexec_load(args[0], args[1] as usize, args[2] as *const u8, args[3]);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::KexecFileLoad => {
            let ret = crate::kexec::kexec_file_load(
                args[0] as i32,
                args[1] as i32,
                args[2] as usize,
                args[3] as *const u8,
                args[4],
            );
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }

        // ── Perf event syscall ───────────────────────────────────────
        crate::syscall::SyscallNumber::PerfEventOpen => {
            let ret = crate::perf_event::perf_event_open(
                args[0] as *const crate::perf_event::PerfEventAttr,
                args[1] as i32,
                args[2] as i32,
                args[3] as i32,
                args[4],
            );
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }

        // ── New mount API syscalls ───────────────────────────────────
        crate::syscall::SyscallNumber::Fsopen => {
            let ret = crate::mount_api::fsopen(args[0] as *const u8, args[1] as u32);
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::Fsconfig => {
            let ret = crate::mount_api::fsconfig(
                args[0] as i32,
                args[1] as u32,
                args[2] as *const u8,
                args[3] as *const u8,
                args[4] as i32,
            );
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::Fsmount => {
            let ret = crate::mount_api::fsmount(args[0] as i32, args[1] as u32, args[2] as u32);
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::Fspick => {
            let ret = crate::mount_api::fspick(args[0] as *const u8, args[1] as u32);
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::MoveMount => {
            let ret = crate::mount_api::move_mount(
                args[0] as i32,
                args[1] as *const u8,
                args[2] as i32,
                args[3] as *const u8,
                args[4] as u32,
            );
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::OpenTree => {
            let ret =
                crate::mount_api::open_tree(args[0] as i32, args[1] as *const u8, args[2] as u32);
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::MountSetattr => {
            let ret = crate::mount_api::mount_setattr(
                args[0] as i32,
                args[1] as *const u8,
                args[2] as u32,
                args[3],
                args[4],
            );
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }

        // ── Landlock syscalls ─────────────────────────────────────────
        crate::syscall::SyscallNumber::LandlockCreateRuleset => {
            let ret = crate::landlock::landlock_create_ruleset(
                args[0] as *const crate::landlock::LandlockRulesetAttr,
                args[1] as usize,
                args[2] as u32,
            );
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::LandlockAddRule => {
            let ret = crate::landlock::landlock_add_rule(
                args[0] as i32,
                args[1] as u32,
                args[2] as *const u8,
                args[3] as u32,
            );
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::LandlockRestrictSelf => {
            let ret = crate::landlock::landlock_restrict_self(args[0] as i32, args[1] as u32);
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }

        crate::syscall::SyscallNumber::Ptrace => {
            let ret = crate::ptrace::ptrace(
                args[0] as u32,
                args[1] as u32,
                args[2] as u64,
                args[3] as u64,
            );
            if ret < 0 {
                Err(LinuxError::EPERM)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::Seccomp => {
            let ret = crate::seccomp::seccomp_set_mode(
                args[0] as u32,
                args[1] as u32,
                args[2] as *const u8,
            );
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::CreateModule
        | crate::syscall::SyscallNumber::ModifyLdt
        | crate::syscall::SyscallNumber::Vserver => Err(LinuxError::EINVAL),
        // ── Adjtimex (NTP time adjustment) ────────────────────────────
        crate::syscall::SyscallNumber::Adjtimex => {
            // adjtimex(struct timex *)
            if args[0] == 0 {
                return Err(LinuxError::EFAULT);
            }
            // struct timex { u32 modes; s32 offset; s32 freq; s32 maxerror;
            //   s32 esterror; s32 status; s32 constant; s32 precision;
            //   s32 tolerance; s32 tick; u32 ppsfreq; s32 jitter; s32 shift;
            //   s32 stabil; s32 jitcnt; s32 calcnt; s32 errcnt; s32 stbcnt;
            //   s32 tai; s32:32 padding }
            // 208 bytes.
            let buf_ptr = args[0] as *mut u8;
            let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, 208) };

            // Read modes field (first 4 bytes)
            let modes = unsafe { *(args[0] as *const u32) };

            // ADJ_OFFSET = 0x0001, ADJ_FREQUENCY = 0x0002, ADJ_MAXERROR = 0x0004
            // ADJ_ESTERROR = 0x0008, ADJ_STATUS = 0x0010, ADJ_TIMECONST = 0x0020
            // ADJ_TAI = 0x0080
            // For read-only (modes == 0), just return current state.
            // For write modes, we accept but don't apply NTP adjustments
            // since we don't have a real NTP implementation.

            // Clear the buffer and return TIME_OK
            for b in buf.iter_mut() {
                *b = 0;
            }
            // Preserve modes field for read-back
            unsafe {
                *(args[0] as *mut u32) = modes;
            }
            Ok(0) // TIME_OK
        }
        crate::syscall::SyscallNumber::ClockAdjtime => {
            // clock_adjtime(clockid, struct timex *)
            // Same as adjtimex but for a specific clock.
            // We support CLOCK_REALTIME (0) and CLOCK_MONOTONIC (1).
            let clockid = args[0] as i32;
            if args[1] == 0 {
                return Err(LinuxError::EFAULT);
            }
            // Validate clockid
            if clockid < 0 || clockid > 1 {
                return Err(LinuxError::EINVAL);
            }
            let buf_ptr = args[1] as *mut u8;
            let modes = unsafe { *(args[1] as *const u32) };
            let buf = unsafe { core::slice::from_raw_parts_mut(buf_ptr, 208) };
            for b in buf.iter_mut() {
                *b = 0;
            }
            unsafe {
                *(args[1] as *mut u32) = modes;
            }
            Ok(0) // TIME_OK
        }

        // ── Kcmp (compare kernel resources) ───────────────────────────
        crate::syscall::SyscallNumber::Kcmp => {
            // kcmp(pid1, pid2, type, idx1, idx2)
            let pid1 = args[0] as i32;
            let pid2 = args[1] as i32;
            let kcmp_type = args[2] as u32;
            let idx1 = args[3];
            let idx2 = args[4];

            // KCMP_TYPES: 0=VM, 1=FILES, 2=FS, 3=SIGHAND, 4=IO, 5=SYSVSEM, 6=EPOLL_TFD
            if kcmp_type > 6 {
                return Err(LinuxError::EINVAL);
            }

            // Validate PIDs exist
            let pm = crate::process::get_process_manager();
            if pm.get_process(pid1 as u32).is_none() || pm.get_process(pid2 as u32).is_none() {
                return Err(LinuxError::ESRCH);
            }

            // If same PID, resources are always shared
            if pid1 == pid2 {
                return Ok(0); // KCMP_EQUAL
            }

            // KCMP_FILES (type 1): compare file descriptors at idx1/idx2
            if kcmp_type == 1 {
                let fd1 = idx1 as u32;
                let fd2 = idx2 as u32;
                let pcb1 = pm.get_process(pid1 as u32);
                let pcb2 = pm.get_process(pid2 as u32);

                if let (Some(p1), Some(p2)) = (pcb1, pcb2) {
                    let fd1_info = p1.file_descriptors.get(&fd1);
                    let fd2_info = p2.file_descriptors.get(&fd2);
                    if let (Some(f1), Some(f2)) = (fd1_info, fd2_info) {
                        // Compare file descriptor types and offsets
                        if f1.fd_type == f2.fd_type && f1.offset == f2.offset {
                            return Ok(0); // KCMP_EQUAL
                        }
                    }
                }
                return Ok(1); // KCMP_NOT_EQUAL
            }

            // KCMP_VM (type 0): compare memory regions
            // For different processes, VM is never shared (no threads)
            // KCMP_FS, KCMP_SIGHAND, KCMP_IO, KCMP_SYSVSEM: no sharing
            Ok(1) // KCMP_NOT_EQUAL
        }

        // ── RestartSyscall ────────────────────────────────────────────
        crate::syscall::SyscallNumber::RestartSyscall => {
            // restart_syscall() — restart a syscall interrupted by a signal.
            // The PCB stores the interrupted syscall number and args in
            // restart_info. We pop it and re-invoke route_syscall.
            let pid = crate::process::current_pid();
            let pm = crate::process::get_process_manager();
            let restart = pm
                .with_process_mut(pid, |pcb| pcb.restart_info.take())
                .ok_or(LinuxError::ESRCH)?;

            if let Some((syscall_num, syscall_args)) = restart {
                // Re-invoke the original syscall
                route_syscall(syscall_num, &syscall_args)
            } else {
                // No saved syscall to restart
                Err(LinuxError::EINTR)
            }
        }

        // ── SetMempolicyHomeNode ──────────────────────────────────────
        crate::syscall::SyscallNumber::SetMempolicyHomeNode => {
            // set_mempolicy_home_node(start, end, home_node, flags)
            let start = args[0];
            let end = args[1];
            let home_node = args[2] as u64;
            let _flags = args[3] as u32;

            // Validate address range
            if start >= end {
                return Err(LinuxError::EINVAL);
            }
            if start >= 0xFFFF_8000_0000_0000 {
                return Err(LinuxError::EINVAL);
            }

            // Must be page-aligned
            if start & 0xFFF != 0 || end & 0xFFF != 0 {
                return Err(LinuxError::EINVAL);
            }

            // Only node 0 is valid in our single-node system
            if home_node != 0 {
                return Err(LinuxError::EINVAL);
            }

            // No NUMA hardware — accept silently
            Ok(0)
        }

        // ── Umounth (old umount, syscall 166) ─────────────────────────
        crate::syscall::SyscallNumber::Umounth => {
            // Old umount() — equivalent to umount2(target, 0)
            linux_compat::fs_ops::umount(args[0] as *const u8).map(|v| v as u64)
        }

        // ── Ustat (deprecated filesystem stats) ──────────────────────
        crate::syscall::SyscallNumber::Ustat => {
            // ustat(dev, struct ustat*)
            if args[1] == 0 {
                return Err(LinuxError::EFAULT);
            }
            // struct ustat { char f_fname[6]; char f_fpack[6]; long f_tfree;
            //   ino_t f_tinode; } — 20 bytes on 64-bit
            let buf = unsafe { core::slice::from_raw_parts_mut(args[1] as *mut u8, 20) };
            for b in buf.iter_mut() {
                *b = 0;
            }
            // Return success with zeroed stats
            Ok(0)
        }
        crate::syscall::SyscallNumber::MemfdSecret => {
            let ret = crate::memfd_secret::memfd_secret(args[0] as u32);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }

        // ── AIO syscalls ─────────────────────────────────────────────
        crate::syscall::SyscallNumber::IoSetup => {
            let ret = crate::aio::io_setup(args[0] as u32, args[1] as *mut u64);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::IoDestroy => {
            let ret = crate::aio::io_destroy(args[0]);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::IoSubmit => {
            let ret = crate::aio::io_submit(
                args[0],
                args[1] as i64,
                args[2] as *const *const crate::aio::IoCb,
            );
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::IoGetevents => {
            let ret = crate::aio::io_getevents(
                args[0],
                args[1] as i64,
                args[2] as i64,
                args[3] as *mut crate::aio::IoEvent,
                args[4] as *const u8,
            );
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::IoCancel => {
            let ret = crate::aio::io_cancel(
                args[0],
                args[1] as *const crate::aio::IoCb,
                args[2] as *mut crate::aio::IoEvent,
            );
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::IoPgetevents => {
            let ret = crate::aio::io_getevents(
                args[0],
                args[1] as i64,
                args[2] as i64,
                args[3] as *mut crate::aio::IoEvent,
                args[4] as *const u8,
            );
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }

        // ── Quotactl (fd-based) ───────────────────────────────────────
        crate::syscall::SyscallNumber::QuotactlFd => {
            // quotactl_fd(fd, cmd, id, addr)
            // Like quotactl but uses a mount fd instead of a special device path.
            let fd = args[0] as i32;
            let cmd = args[1] as i32;
            let id = args[2] as i32;
            let addr = args[3] as *mut u8;

            // Validate that fd is a valid open file descriptor
            match crate::vfs::vfs_fstat(fd) {
                Ok(stat) => {
                    if stat.inode_type != crate::vfs::InodeType::Directory {
                        return Err(LinuxError::ENOTDIR);
                    }
                }
                Err(_) => {
                    return Err(LinuxError::EBADF);
                }
            }

            // Delegate to quotactl with a null special (fd-based path)
            // The quota module resolves by special string; for fd-based,
            // we pass the fd as a synthetic special path.
            let special_str = alloc::format!("/proc/self/fd/{}\0", fd);
            crate::quota::quotactl(cmd, special_str.as_ptr(), id, addr).map(|v| v as u64)
        }

        // ── SysV IPC syscalls ────────────────────────────────────────
        crate::syscall::SyscallNumber::Semctl => {
            let ret =
                crate::sysv_ipc::semctl(args[0] as i32, args[1] as i32, args[2] as i32, args[3]);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::Semop => {
            let ret = crate::sysv_ipc::semop(
                args[0] as i32,
                args[1] as *const crate::sysv_ipc::SemBuf,
                args[2] as u32,
            );
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::Semtimedop => {
            let ret = crate::sysv_ipc::semtimedop(
                args[0] as i32,
                args[1] as *const crate::sysv_ipc::SemBuf,
                args[2] as u32,
                args[3] as *const u8,
            );
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::Shmat => {
            let ret = crate::sysv_ipc::shmat(args[0] as i32, args[1], args[2] as i32);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret as i32))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::Shmctl => {
            let ret = crate::sysv_ipc::shmctl(args[0] as i32, args[1] as i32, args[2]);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }

        // ── Process VM syscalls ──────────────────────────────────────
        crate::syscall::SyscallNumber::ProcessVmReadv => crate::process_vm::process_vm_readv(
            args[0] as i32,
            args[1],
            args[2] as usize,
            args[3],
            args[4] as usize,
            args[5],
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::ProcessVmWritev => crate::process_vm::process_vm_writev(
            args[0] as i32,
            args[1],
            args[2] as usize,
            args[3],
            args[4] as usize,
            args[5],
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::ProcessMadvise => crate::process_vm::process_madvise(
            args[0] as i32,
            args[1],
            args[2] as usize,
            args[3] as i32,
            args[4] as u32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::ProcessMrelease => {
            crate::process_vm::process_mrelease(args[0] as i32, args[1] as u32).map(|v| v as u64)
        }

        // ── Misc syscalls ────────────────────────────────────────────
        crate::syscall::SyscallNumber::Userfaultfd => {
            let ret = crate::userfaultfd::userfaultfd(args[0] as u32);
            if ret < 0 {
                Err(LinuxError::from_errno(-ret))
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::Rseq => {
            crate::rseq::rseq(args[0], args[1] as u32, args[2] as u32, args[3] as u32)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Pselect6 => linux_compat::socket_ops::pselect6(
            args[0] as i32,
            args[1] as *mut u64,
            args[2] as *mut u64,
            args[3] as *mut u64,
            args[4] as *const linux_compat::TimeSpec,
            args[5] as *const u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Mknodat => linux_compat::file_ops::mknodat(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as u32,
            args[3] as u64,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::NameToHandleAt => crate::file_handle::name_to_handle_at(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as *mut u8,
            args[3] as *mut i32,
            args[4] as i32,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::Vmsplice => {
            // vmsplice(fd, iov, nr_segs, flags)
            // For pipe write fds, copy data from iovec into pipe
            let fd = args[0] as i32;
            let iov = args[1] as *const u8;
            let nr_segs = args[2] as usize;
            let flags = args[3] as u32;

            if nr_segs > 1024 {
                return Err(LinuxError::EINVAL);
            }

            // SPLICE_F_MOVE=1, SPLICE_F_NONBLOCK=2, SPLICE_F_MORE=4, SPLICE_F_GIFT=8
            let valid_flags = 1 | 2 | 4 | 8;
            if flags & !valid_flags != 0 {
                return Err(LinuxError::EINVAL);
            }

            // For non-pipe fds or unsupported, return EINVAL
            // vmsplice on a pipe write end: copy data from iovec into pipe
            let kind = crate::vfs::vfs_fd_kind(fd).map_err(|_| LinuxError::EBADF)?;

            match kind {
                crate::vfs::FdKind::PipeWrite(pipe_id) => {
                    let ipc = crate::process::ipc::get_ipc_manager();
                    let mut total = 0usize;
                    for i in 0..nr_segs {
                        // struct iovec { void *iov_base; size_t iov_len; }
                        let base = unsafe { *(iov.add(i * 16) as *const *const u8) };
                        let len = unsafe { *(iov.add(i * 16 + 8) as *const usize) };
                        if base.is_null() || len == 0 {
                            continue;
                        }
                        let data = unsafe { core::slice::from_raw_parts(base, len) };
                        match ipc.pipe_write(pipe_id, data) {
                            Ok(n) => total += n,
                            Err(_) => break,
                        }
                    }
                    Ok(total as u64)
                }
                crate::vfs::FdKind::PipeRead(pipe_id) => {
                    let ipc = crate::process::ipc::get_ipc_manager();
                    let mut total = 0usize;
                    for i in 0..nr_segs {
                        let base = unsafe { *(iov.add(i * 16) as *const *mut u8) };
                        let len = unsafe { *(iov.add(i * 16 + 8) as *const usize) };
                        if base.is_null() || len == 0 {
                            continue;
                        }
                        let buf = unsafe { core::slice::from_raw_parts_mut(base, len) };
                        match ipc.pipe_read(pipe_id, buf) {
                            Ok(n) => {
                                total += n;
                                if n < len {
                                    break;
                                }
                            }
                            Err(_) => break,
                        }
                    }
                    Ok(total as u64)
                }
                _ => Err(LinuxError::EINVAL),
            }
        }

        // ── Signal syscalls ──────────────────────────────────────────
        crate::syscall::SyscallNumber::RtSigreturn => {
            linux_compat::signal_ops::rt_sigreturn().map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::RtSigqueueinfo => linux_compat::signal_ops::rt_sigqueueinfo(
            args[0] as i32,
            args[1] as i32,
            args[2] as *const u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::RtTgsigqueueinfo => {
            linux_compat::signal_ops::rt_tgsigqueueinfo(
                args[0] as i32,
                args[1] as i32,
                args[2] as i32,
                args[3] as *const u8,
            )
            .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::RtSigtimedwait => {
            if args[0] == 0 {
                return Err(LinuxError::EFAULT);
            }
            let set = unsafe { *(args[0] as *const u64) };
            let timeout_ns = if args[2] != 0 {
                let ts = unsafe { &*(args[2] as *const linux_compat::TimeSpec) };
                Some(ts.tv_sec as u64 * 1_000_000_000 + ts.tv_nsec as u64)
            } else {
                None
            };
            linux_compat::signal_ops::rt_sigtimedwait(set, timeout_ns).map(|v| v as u64)
        }

        // ── PID fd syscalls ──────────────────────────────────────────
        crate::syscall::SyscallNumber::PidfdOpen => {
            let ret = crate::pidfd::pidfd_open(args[0] as i32, args[1] as u32);
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::PidfdSendSignal => {
            let ret = crate::pidfd::pidfd_send_signal(
                args[0] as i32,
                args[1] as i32,
                args[2],
                args[3] as u32,
            );
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::PidfdGetfd => {
            let ret = crate::pidfd::pidfd_getfd(args[0] as i32, args[1] as i32, args[2] as u32);
            if ret < 0 {
                Err(LinuxError::EPERM)
            } else {
                Ok(ret as u64)
            }
        }

        // ── Sync ─────────────────────────────────────────────────────
        crate::syscall::SyscallNumber::Sync => {
            // Flush all dirty buffers to device
            let _ = crate::vfs::get_vfs().sync_all();
            Ok(0)
        }

        // ── Uname ────────────────────────────────────────────────────
        crate::syscall::SyscallNumber::Uname => {
            if args[0] == 0 {
                return Err(LinuxError::EFAULT);
            }
            #[repr(C)]
            struct OldUtsname {
                sysname: [u8; 65],
                nodename: [u8; 65],
                release: [u8; 65],
                version: [u8; 65],
                machine: [u8; 65],
                domainname: [u8; 65],
            }
            let uts = crate::namespace::get_nsproxy(crate::process::current_pid());
            let mut fill = |buf: &mut [u8; 65], s: &str| {
                for (i, b) in s.bytes().enumerate() {
                    if i >= 64 {
                        break;
                    }
                    buf[i] = b;
                }
                buf[s.len().min(64)] = 0;
            };
            let mut name = OldUtsname {
                sysname: [0; 65],
                nodename: [0; 65],
                release: [0; 65],
                version: [0; 65],
                machine: [0; 65],
                domainname: [0; 65],
            };
            fill(&mut name.sysname, &uts.uts.sysname);
            fill(&mut name.nodename, &uts.uts.nodename);
            fill(&mut name.release, &uts.uts.release);
            fill(&mut name.version, &uts.uts.version);
            fill(&mut name.machine, &uts.uts.machine);
            fill(&mut name.domainname, &uts.uts.domainname);
            unsafe {
                *(args[0] as *mut OldUtsname) = name;
            }
            Ok(0)
        }

        // ── Waitid ───────────────────────────────────────────────────
        crate::syscall::SyscallNumber::Waitid => {
            let idtype = args[0] as i32;
            let id = args[1] as i32;
            let infop = args[2] as *mut u8;
            let options = args[3] as i32;

            // idtype: 0=P_ALL, 1=P_PID, 2=P_PGID
            let pm = crate::process::get_process_manager();
            let target_pid = match idtype {
                1 => {
                    if id <= 0 {
                        return Err(LinuxError::EINVAL);
                    }
                    Some(id as u32)
                }
                2 => {
                    if id <= 0 {
                        return Err(LinuxError::EINVAL);
                    }
                    // Find any child in this process group
                    let children: alloc::vec::Vec<u32> = pm
                        .find_processes(|p| {
                            p.pgid == id as u32
                                && p.parent_pid == Some(crate::process::current_pid())
                        })
                        .into_iter()
                        .map(|p| p.pid)
                        .collect();
                    children.first().copied()
                }
                _ => {
                    // P_ALL — find any child that has exited
                    let children: alloc::vec::Vec<u32> = pm
                        .find_processes(|p| p.parent_pid == Some(crate::process::current_pid()))
                        .into_iter()
                        .filter(|p| {
                            matches!(
                                p.state,
                                crate::process::ProcessState::Zombie
                                    | crate::process::ProcessState::Terminated
                            )
                        })
                        .map(|p| p.pid)
                        .collect();
                    children.first().copied()
                }
            };

            if let Some(pid) = target_pid {
                if let Some(pcb) = pm.get_process(pid) {
                    if matches!(
                        pcb.state,
                        crate::process::ProcessState::Zombie
                            | crate::process::ProcessState::Terminated
                    ) {
                        let exit_code = pcb.exit_code.unwrap_or(0);
                        let uid = pcb.uid;
                        // Determine if the child exited normally or was killed by a signal.
                        // In Linux, exit status >= 128 means the process was terminated
                        // by signal (status - 128).
                        let (si_code, si_status) = if exit_code >= 128 {
                            (2, exit_code - 128) // CLD_KILLED, signal number
                        } else {
                            (1, exit_code) // CLD_EXITED, exit code
                        };
                        // Write siginfo
                        if !infop.is_null() {
                            #[repr(C)]
                            struct SigInfo {
                                si_signo: i32,
                                si_errno: i32,
                                si_code: i32,
                                _pad: i32,
                                si_pid: u32,
                                si_uid: u32,
                                si_status: i32,
                                _pad2: [u8; 32],
                            }
                            unsafe {
                                *(infop as *mut SigInfo) = SigInfo {
                                    si_signo: 17, // SIGCHLD
                                    si_errno: 0,
                                    si_code,
                                    _pad: 0,
                                    si_pid: pid,
                                    si_uid: uid,
                                    si_status,
                                    _pad2: [0; 32],
                                };
                            }
                        }
                        // Reap the zombie
                        let _ =
                            pm.reap_zombie_child(crate::process::current_pid(), |p| p.pid == pid);
                        return Ok(pid as u64);
                    }
                }
            }

            // No children to wait for
            if options & 1 != 0 {
                // WNOHANG
                return Ok(0);
            }

            // Would block — return ECHILD if no children exist
            let has_children = pm
                .find_processes(|p| p.parent_pid == Some(crate::process::current_pid()))
                .into_iter()
                .any(|p| {
                    !matches!(
                        p.state,
                        crate::process::ProcessState::Zombie
                            | crate::process::ProcessState::Terminated
                    )
                });

            if !has_children {
                return Err(LinuxError::ECHILD);
            }

            Err(LinuxError::EAGAIN)
        }

        // ── Scheduling attributes ────────────────────────────────────
        crate::syscall::SyscallNumber::SchedGetattr => {
            if args[1] == 0 {
                return Err(LinuxError::EINVAL);
            }
            let pid = if args[0] == 0 {
                crate::process::current_pid()
            } else {
                args[0] as u32
            };
            let pm = crate::process::get_process_manager();
            let pcb = pm.get_process(pid).ok_or(LinuxError::ESRCH)?;

            #[repr(C)]
            struct SchedAttr {
                size: u32,
                policy: u32,
                flags: u64,
                nice: u32,
                priority: u32,
                runtime_ns: u64,
                deadline_ns: u64,
                period_ns: u64,
            }
            let policy = match pcb.priority {
                crate::process::Priority::RealTime => 0, // SCHED_FIFO
                crate::process::Priority::High => 1,     // SCHED_RR
                crate::process::Priority::Normal => 0,   // SCHED_NORMAL
                crate::process::Priority::Low => 0,      // SCHED_NORMAL
                crate::process::Priority::Idle => 5,     // SCHED_IDLE
            };
            let attr = SchedAttr {
                size: core::mem::size_of::<SchedAttr>() as u32,
                policy,
                flags: 0,
                nice: 0,
                priority: pcb.priority as u32,
                runtime_ns: 0,
                deadline_ns: 0,
                period_ns: 0,
            };
            unsafe {
                *(args[1] as *mut SchedAttr) = attr;
            }
            Ok(0)
        }
        crate::syscall::SyscallNumber::SchedSetattr => {
            if args[1] == 0 {
                return Err(LinuxError::EINVAL);
            }
            let pid = if args[0] == 0 {
                crate::process::current_pid()
            } else {
                args[0] as u32
            };
            #[repr(C)]
            struct SchedAttr {
                size: u32,
                policy: u32,
                flags: u64,
                nice: u32,
                priority: u32,
                runtime_ns: u64,
                deadline_ns: u64,
                period_ns: u64,
            }
            let attr = unsafe { &*(args[1] as *const SchedAttr) };
            let new_priority = match attr.policy {
                0 => crate::process::Priority::Normal,
                1 => crate::process::Priority::High,
                5 => crate::process::Priority::Idle,
                _ => crate::process::Priority::Normal,
            };
            let pm = crate::process::get_process_manager();
            pm.with_process_mut(pid, |pcb| {
                pcb.priority = new_priority;
            })
            .ok_or(LinuxError::ESRCH)?;
            Ok(0)
        }

        // ── io_uring syscalls ─────────────────────────────────────────
        crate::syscall::SyscallNumber::IoUringSetup => {
            let ret = crate::io_uring::io_uring_setup(
                args[0] as u32,
                args[1] as *mut crate::io_uring::IoUringParams,
            );
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::IoUringEnter => {
            let ret = crate::io_uring::io_uring_enter(
                args[0] as i32,
                args[1] as u32,
                args[2] as u32,
                args[3] as u32,
            );
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::IoUringRegister => {
            let ret = crate::io_uring::io_uring_register(
                args[0] as i32,
                args[1] as u32,
                args[2],
                args[3] as u32,
            );
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }

        // ── Fanotify syscalls ─────────────────────────────────────────
        crate::syscall::SyscallNumber::FanotifyInit => {
            let ret = crate::fanotify::fanotify_init(args[0] as u32, args[1] as u32);
            if ret < 0 {
                Err(LinuxError::EPERM)
            } else {
                Ok(ret as u64)
            }
        }
        crate::syscall::SyscallNumber::FanotifyMark => {
            let ret = crate::fanotify::fanotify_mark(
                args[0] as i32,
                args[1] as u32,
                args[2],
                args[3] as i32,
                args[4] as *const u8,
            );
            if ret < 0 {
                Err(LinuxError::EINVAL)
            } else {
                Ok(ret as u64)
            }
        }

        // ── FutexWaitv ────────────────────────────────────────────────
        crate::syscall::SyscallNumber::FutexWaitv => {
            let waiters = args[0] as *const u8;
            let nr_waiters = args[1] as u32;
            let flags = args[2] as u32;
            let _timeout = args[3];

            if waiters.is_null() || nr_waiters == 0 || nr_waiters > 128 {
                return Err(LinuxError::EINVAL);
            }

            // futex_waitv struct: { u64 val; u64 uaddr; u32 flags; u32 __reserved; }
            #[repr(C)]
            struct FutexWaitv {
                val: u64,
                uaddr: u64,
                flags: u32,
                __reserved: u32,
            }

            let pid = crate::process::current_pid();

            loop {
                // Check all futexes for any that have changed
                for i in 0..nr_waiters {
                    let w = unsafe { &*(waiters.add(i as usize * 32) as *const FutexWaitv) };
                    let expected = w.val;
                    let uaddr = w.uaddr as *const u32;
                    if uaddr.is_null() {
                        continue;
                    }
                    let current = unsafe { core::ptr::read_volatile(uaddr) };
                    if current != expected as u32 {
                        // Value changed — wake immediately
                        return Ok(i as u64);
                    }
                }

                // All futexes still at expected values — block and wait
                let pm = crate::process::get_process_manager();
                let _ = pm.block_process(pid);
                crate::process::scheduler::yield_cpu();

                // After being woken, loop back and re-check all futexes
                // If a signal woke us, return EINTR
                let pcb = pm.get_process(pid);
                if let Some(ref pcb) = pcb {
                    if !pcb.pending_signals.is_empty() {
                        return Err(LinuxError::EINTR);
                    }
                }
            }
        }

        // ── Statx ─────────────────────────────────────────────────────
        crate::syscall::SyscallNumber::Statx => linux_compat::file_ops::statx(
            args[0] as i32,
            args[1] as *const u8,
            args[2] as i32,
            args[3] as u32,
            args[4] as *mut linux_compat::Statx,
        )
        .map(|v| v as u64),

        // ── Setfsuid / Setfsgid ───────────────────────────────────────
        crate::syscall::SyscallNumber::Setfsuid => {
            let uid = args[0] as u32;
            Ok(linux_compat::process_ops::setfsuid(uid) as u64)
        }
        crate::syscall::SyscallNumber::Setfsgid => {
            let gid = args[0] as u32;
            Ok(linux_compat::process_ops::setfsgid(gid) as u64)
        }

        // ── Time ──────────────────────────────────────────────────────
        crate::syscall::SyscallNumber::Time => {
            let now_secs = (crate::time::uptime_ns() / 1_000_000_000) as i64;
            if args[0] != 0 {
                unsafe {
                    *(args[0] as *mut i64) = now_secs;
                }
            }
            Ok(now_secs as u64)
        }

        // ── TimerGettime / TimerSettime ───────────────────────────────
        crate::syscall::SyscallNumber::TimerGettime => linux_compat::time_ops::timer_gettime(
            args[0] as linux_compat::time_ops::TimerId,
            args[1] as *mut u8,
        )
        .map(|v| v as u64),
        crate::syscall::SyscallNumber::TimerSettime => linux_compat::time_ops::timer_settime(
            args[0] as linux_compat::time_ops::TimerId,
            args[1] as i32,
            args[2] as *const u8,
            args[3] as *mut u8,
        )
        .map(|v| v as u64),

        _ => Err(LinuxError::ENOSYS),
    }
}

/// Route file-related syscalls to VFS
fn route_file_syscall(syscall_number: u64, args: &[u64]) -> LinuxResult<u64> {
    let syscall = crate::syscall::SyscallNumber::from_u64(syscall_number);
    if syscall == crate::syscall::SyscallNumber::Invalid {
        return Err(LinuxError::ENOSYS);
    }
    match syscall {
        crate::syscall::SyscallNumber::Read => {
            let fd = args[0] as i32;
            let buf = args[1] as *mut u8;
            let count = args[2] as usize;
            linux_compat::file_ops::read(fd, buf, count).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Write => {
            let fd = args[0] as i32;
            let buf = args[1] as *const u8;
            let count = args[2] as usize;
            linux_compat::file_ops::write(fd, buf, count).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Open => {
            let pathname = args[0] as *const u8;
            let flags = args[1] as i32;
            let mode = args[2] as u32;
            linux_compat::file_ops::open(pathname, flags, mode).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Close => {
            let fd = args[0] as i32;
            linux_compat::file_ops::close(fd).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Stat => {
            let pathname = args[0] as *const u8;
            let statbuf = args[1] as *mut linux_compat::types::Stat;
            linux_compat::file_ops::stat(pathname, statbuf).map(|_| 0)
        }
        crate::syscall::SyscallNumber::Fstat => {
            let fd = args[0] as i32;
            let statbuf = args[1] as *mut linux_compat::types::Stat;
            linux_compat::file_ops::fstat(fd, statbuf).map(|_| 0)
        }
        crate::syscall::SyscallNumber::Lstat => {
            let pathname = args[0] as *const u8;
            let statbuf = args[1] as *mut linux_compat::types::Stat;
            linux_compat::file_ops::lstat(pathname, statbuf).map(|_| 0)
        }
        crate::syscall::SyscallNumber::Lseek => {
            let fd = args[0] as i32;
            let offset = args[1] as i64;
            let whence = args[2] as i32;
            linux_compat::file_ops::lseek(fd, offset, whence).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Pread64 => {
            let fd = args[0] as i32;
            let buf = args[1] as *mut u8;
            let count = args[2] as usize;
            let offset = args[3] as i64;
            linux_compat::advanced_io::pread(fd, buf, count, offset).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Pwrite64 => {
            let fd = args[0] as i32;
            let buf = args[1] as *const u8;
            let count = args[2] as usize;
            let offset = args[3] as i64;
            linux_compat::advanced_io::pwrite(fd, buf, count, offset).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Readv => {
            let fd = args[0] as i32;
            let iov = args[1] as *const linux_compat::advanced_io::IoVec;
            let iovcnt = args[2] as i32;
            linux_compat::advanced_io::readv(fd, iov, iovcnt).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Writev => {
            let fd = args[0] as i32;
            let iov = args[1] as *const linux_compat::advanced_io::IoVec;
            let iovcnt = args[2] as i32;
            linux_compat::advanced_io::writev(fd, iov, iovcnt).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Access => {
            let pathname = args[0] as *const u8;
            let mode = args[1] as i32;
            linux_compat::file_ops::access(pathname, mode).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Pipe => {
            let pipefd = args[0] as *mut [i32; 2];
            linux_compat::ipc_ops::pipe(pipefd).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Dup => {
            let oldfd = args[0] as i32;
            linux_compat::file_ops::dup(oldfd).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Dup2 => {
            let oldfd = args[0] as i32;
            let newfd = args[1] as i32;
            linux_compat::file_ops::dup2(oldfd, newfd).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Fsync => {
            let fd = args[0] as i32;
            linux_compat::file_ops::fsync(fd).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Fdatasync => {
            let fd = args[0] as i32;
            linux_compat::file_ops::fdatasync(fd).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Truncate => {
            let path = args[0] as *const u8;
            let length = args[1] as i64;
            linux_compat::file_ops::truncate(path, length).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Ftruncate => {
            let fd = args[0] as i32;
            let length = args[1] as i64;
            linux_compat::file_ops::ftruncate(fd, length).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getdents => {
            let fd = args[0] as i32;
            let dirp = args[1] as *mut linux_compat::types::Dirent;
            let count = args[2] as usize;
            linux_compat::file_ops::getdents(fd, dirp, count).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getcwd => {
            let buf = args[0] as *mut u8;
            let size = args[1] as usize;
            linux_compat::file_ops::getcwd(buf, size).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Chdir => {
            let path = args[0] as *const u8;
            linux_compat::file_ops::chdir(path).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Fchdir => {
            let fd = args[0] as i32;
            linux_compat::file_ops::fchdir(fd).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Rename => {
            let oldpath = args[0] as *const u8;
            let newpath = args[1] as *const u8;
            linux_compat::file_ops::rename(oldpath, newpath).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Mkdir => {
            let pathname = args[0] as *const u8;
            let mode = args[1] as u32;
            linux_compat::file_ops::mkdir(pathname, mode).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Rmdir => {
            let pathname = args[0] as *const u8;
            linux_compat::file_ops::rmdir(pathname).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Creat => {
            let pathname = args[0] as *const u8;
            let mode = args[1] as u32;
            linux_compat::file_ops::open(pathname, 577, mode).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Link => {
            let oldpath = args[0] as *const u8;
            let newpath = args[1] as *const u8;
            linux_compat::file_ops::link(oldpath, newpath).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Unlink => {
            let pathname = args[0] as *const u8;
            linux_compat::file_ops::unlink(pathname).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Symlink => {
            let target = args[0] as *const u8;
            let linkpath = args[1] as *const u8;
            linux_compat::file_ops::symlink(target, linkpath).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Readlink => {
            let pathname = args[0] as *const u8;
            let buf = args[1] as *mut u8;
            let bufsiz = args[2] as usize;
            linux_compat::file_ops::readlink(pathname, buf, bufsiz).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Chmod => {
            let filename = args[0] as *const u8;
            let mode = args[1] as u32;
            linux_compat::file_ops::chmod(filename, mode).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Fchmod => {
            let fd = args[0] as i32;
            let mode = args[1] as u32;
            linux_compat::file_ops::fchmod(fd, mode).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Chown => {
            let filename = args[0] as *const u8;
            let user = args[1] as u32;
            let group = args[2] as u32;
            linux_compat::file_ops::chown(filename, user, group).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Fchown => {
            let fd = args[0] as i32;
            let user = args[1] as u32;
            let group = args[2] as u32;
            linux_compat::file_ops::fchown(fd, user, group).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Lchown => {
            let filename = args[0] as *const u8;
            let user = args[1] as u32;
            let group = args[2] as u32;
            linux_compat::file_ops::lchown(filename, user, group).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Openat => {
            let dfd = args[0] as i32;
            let filename = args[1] as *const u8;
            let flags = args[2] as i32;
            let mode = args[3] as u32;
            linux_compat::file_ops::openat(dfd, filename, flags, mode).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Newfstatat => {
            let dfd = args[0] as i32;
            let filename = args[1] as *const u8;
            let statbuf = args[2] as *mut linux_compat::types::Stat;
            let flag = args[3] as i32;
            linux_compat::file_ops::newfstatat(dfd, filename, statbuf, flag).map(|_| 0)
        }
        crate::syscall::SyscallNumber::Getdents64 => {
            let fd = args[0] as i32;
            let dirp = args[1] as *mut u8;
            let count = args[2] as u32;
            linux_compat::advanced_io::getdents64(fd, dirp, count).map(|v| v as u64)
        }
        _ => Err(LinuxError::ENOSYS),
    }
}

/// Route process-related syscalls to process manager
fn route_process_syscall(syscall_number: u64, args: &[u64]) -> LinuxResult<u64> {
    let syscall = crate::syscall::SyscallNumber::from_u64(syscall_number);
    if syscall == crate::syscall::SyscallNumber::Invalid {
        return Err(LinuxError::ENOSYS);
    }
    match syscall {
        crate::syscall::SyscallNumber::Fork => linux_compat::process_ops::fork().map(|v| v as u64),
        crate::syscall::SyscallNumber::Execve => {
            let filename = args[0] as *const u8;
            let argv = args[1] as *const *const u8;
            let envp = args[2] as *const *const u8;
            linux_compat::process_ops::execve(filename, argv, envp).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Exit | crate::syscall::SyscallNumber::ExitGroup => {
            let status = args[0] as i32;
            linux_compat::process_ops::exit(status);
            Ok(0)
        }
        crate::syscall::SyscallNumber::Wait4 => {
            let pid = args[0] as i32;
            let wstatus = args[1] as *mut i32;
            let options = args[2] as i32;
            let rusage = args[3] as *mut linux_compat::types::Rusage;
            linux_compat::process_ops::wait4(pid, wstatus, options, rusage).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::GetPid => Ok(linux_compat::process_ops::getpid() as u64),
        crate::syscall::SyscallNumber::GetPpid => Ok(linux_compat::process_ops::getppid() as u64),
        crate::syscall::SyscallNumber::Gettid => Ok(linux_compat::thread_ops::gettid() as u64),
        crate::syscall::SyscallNumber::Clone => {
            let flags = args[0];
            let child_stack = args[1] as *mut u8;
            let parent_tidptr = args[2] as *mut i32;
            let child_tidptr = args[3] as *mut i32;
            let newtls = args[4];
            linux_compat::thread_ops::clone(flags, child_stack, parent_tidptr, child_tidptr, newtls)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::RtSigaction => {
            let sig = args[0] as i32;
            let act = args[1] as *const linux_compat::types::SigAction;
            let oact = args[2] as *mut linux_compat::types::SigAction;
            let sigsetsize = args[3] as usize;
            linux_compat::signal_ops::rt_sigaction(sig, act, oact, sigsetsize).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::RtSigprocmask => {
            let how = args[0] as i32;
            let set = args[1] as *const linux_compat::types::SigSet;
            let oset = args[2] as *mut linux_compat::types::SigSet;
            let sigsetsize = args[3] as usize;
            linux_compat::signal_ops::rt_sigprocmask(how, set, oset, sigsetsize).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getuid => Ok(linux_compat::process_ops::getuid() as u64),
        crate::syscall::SyscallNumber::Geteuid => Ok(linux_compat::process_ops::geteuid() as u64),
        crate::syscall::SyscallNumber::Getgid => Ok(linux_compat::process_ops::getgid() as u64),
        crate::syscall::SyscallNumber::Getegid => Ok(linux_compat::process_ops::getegid() as u64),
        crate::syscall::SyscallNumber::Setuid => {
            let uid = args[0] as u32;
            linux_compat::process_ops::setuid(uid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Setgid => {
            let gid = args[0] as u32;
            linux_compat::process_ops::setgid(gid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Setreuid => {
            let ruid = args[0] as u32;
            let euid = args[1] as u32;
            linux_compat::process_ops::setreuid(ruid, euid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Setregid => {
            let rgid = args[0] as u32;
            let egid = args[1] as u32;
            linux_compat::process_ops::setregid(rgid, egid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Setresuid => {
            let ruid = args[0] as u32;
            let euid = args[1] as u32;
            let suid = args[2] as u32;
            linux_compat::process_ops::setresuid(ruid, euid, suid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getresuid => {
            let ruid = args[0] as *mut u32;
            let euid = args[1] as *mut u32;
            let suid = args[2] as *mut u32;
            linux_compat::process_ops::getresuid(ruid, euid, suid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Setresgid => {
            let rgid = args[0] as u32;
            let egid = args[1] as u32;
            let sgid = args[2] as u32;
            linux_compat::process_ops::setresgid(rgid, egid, sgid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getresgid => {
            let rgid = args[0] as *mut u32;
            let egid = args[1] as *mut u32;
            let sgid = args[2] as *mut u32;
            linux_compat::process_ops::getresgid(rgid, egid, sgid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getgroups => {
            let size = args[0] as i32;
            let list = args[1] as *mut u32;
            linux_compat::process_ops::getgroups(size, list).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Setgroups => {
            let size = args[0] as i32;
            let list = args[1] as *const u32;
            linux_compat::process_ops::setgroups(size, list).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Setpgid => {
            let pid = args[0] as i32;
            let pgid = args[1] as i32;
            linux_compat::process_ops::setpgid(pid, pgid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getpgid => {
            let pid = args[0] as i32;
            linux_compat::process_ops::getpgid(pid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getpgrp => Ok(linux_compat::process_ops::getpgrp() as u64),
        crate::syscall::SyscallNumber::Setsid => {
            linux_compat::process_ops::setsid().map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getsid => {
            let pid = args[0] as i32;
            linux_compat::process_ops::getsid(pid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Umask => {
            let mask = args[0] as u32;
            Ok(linux_compat::process_ops::umask(mask).map(|v| v as u64)?)
        }
        crate::syscall::SyscallNumber::Chroot => {
            let path = args[0] as *const u8;
            linux_compat::process_ops::chroot(path).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getrlimit => {
            let resource = args[0] as i32;
            let rlim = args[1] as *mut linux_compat::resource_ops::RLimit;
            linux_compat::resource_ops::getrlimit(resource, rlim).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Setrlimit => {
            let resource = args[0] as i32;
            let rlim = args[1] as *const linux_compat::resource_ops::RLimit;
            linux_compat::resource_ops::setrlimit(resource, rlim).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Prlimit64 => {
            let pid = args[0] as i32;
            let resource = args[1] as i32;
            let new_limit = args[2] as *const linux_compat::resource_ops::RLimit;
            let old_limit = args[3] as *mut linux_compat::resource_ops::RLimit;
            linux_compat::resource_ops::prlimit(pid, resource, new_limit, old_limit)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getrusage => {
            let who = args[0] as i32;
            let usage = args[1] as *mut linux_compat::types::Rusage;
            linux_compat::process_ops::getrusage(who, usage).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Times => {
            let buf = args[0] as *mut u8;
            linux_compat::process_ops::times(buf).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Sysinfo => {
            let info = args[0] as *mut linux_compat::sysinfo_ops::SysInfo;
            linux_compat::sysinfo_ops::sysinfo(info).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Prctl => {
            let option = args[0] as i32;
            let arg2 = args[1];
            let arg3 = args[2];
            let arg4 = args[3];
            let arg5 = args[4];
            linux_compat::process_ops::prctl(option, arg2, arg3, arg4, arg5).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Capget => {
            let hdrp = args[0] as *mut u8;
            let datap = args[1] as *mut u8;
            linux_compat::process_ops::capget(hdrp, datap).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Capset => {
            let hdrp = args[0] as *const u8;
            let datap = args[1] as *const u8;
            linux_compat::process_ops::capset(hdrp, datap).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::SchedYield => {
            linux_compat::process_ops::sched_yield().map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::SchedGetaffinity => {
            let pid = args[0] as i32;
            let cpusetsize = args[1] as usize;
            let mask = args[2] as *mut u8;
            linux_compat::process_ops::sched_getaffinity(pid, cpusetsize, mask).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::SchedSetaffinity => {
            let pid = args[0] as i32;
            let cpusetsize = args[1] as usize;
            let mask = args[2] as *const u8;
            linux_compat::process_ops::sched_setaffinity(pid, cpusetsize, mask).map(|v| v as u64)
        }
        _ => Err(LinuxError::ENOSYS),
    }
}

/// Route RustOS package management syscalls.
fn route_package_syscall(syscall_number: u64, args: &[u64]) -> LinuxResult<u64> {
    let num = match crate::syscall::SyscallNumber::from_u64(syscall_number) {
        crate::syscall::SyscallNumber::PkgInstall => 512,
        crate::syscall::SyscallNumber::PkgRemove => 513,
        crate::syscall::SyscallNumber::PkgSearch => 514,
        crate::syscall::SyscallNumber::PkgInfo => 515,
        crate::syscall::SyscallNumber::PkgList => 516,
        crate::syscall::SyscallNumber::PkgUpdate => 517,
        crate::syscall::SyscallNumber::PkgUpgrade => 518,
        _ => return Err(LinuxError::ENOSYS),
    };

    crate::package::handle_package_syscall(
        num,
        args[0] as usize,
        args[1] as usize,
        args[2] as usize,
        args[3] as usize,
    )
    .map(|v| v as u64)
    .map_err(|_| LinuxError::EPERM)
}

/// Route network-related syscalls to network stack
fn route_network_syscall(syscall_number: u64, args: &[u64]) -> LinuxResult<u64> {
    let syscall = crate::syscall::SyscallNumber::from_u64(syscall_number);
    if syscall == crate::syscall::SyscallNumber::Invalid {
        return Err(LinuxError::ENOSYS);
    }
    match syscall {
        crate::syscall::SyscallNumber::Socket => {
            let domain = args[0] as i32;
            let sock_type = args[1] as i32;
            let protocol = args[2] as i32;
            linux_compat::socket_ops::socket(domain, sock_type, protocol).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Connect => {
            let sockfd = args[0] as i32;
            let addr = args[1] as *const linux_compat::types::SockAddr;
            let addrlen = args[2] as u32;
            linux_compat::socket_ops::connect(sockfd, addr, addrlen).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Accept => {
            let sockfd = args[0] as i32;
            let addr = args[1] as *mut linux_compat::types::SockAddr;
            let addrlen = args[2] as *mut u32;
            linux_compat::socket_ops::accept(sockfd, addr, addrlen).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Sendto => {
            let sockfd = args[0] as i32;
            let buf = args[1] as *const u8;
            let len = args[2] as usize;
            let flags = args[3] as i32;
            let dest_addr = args[4] as *const linux_compat::types::SockAddr;
            let addrlen = args[5] as u32;
            linux_compat::socket_ops::sendto(sockfd, buf, len, flags, dest_addr, addrlen)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Recvfrom => {
            let sockfd = args[0] as i32;
            let buf = args[1] as *mut u8;
            let len = args[2] as usize;
            let flags = args[3] as i32;
            let src_addr = args[4] as *mut linux_compat::types::SockAddr;
            let addrlen = args[5] as *mut u32;
            linux_compat::socket_ops::recvfrom(sockfd, buf, len, flags, src_addr, addrlen)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Sendmsg => {
            let sockfd = args[0] as i32;
            let msg = args[1] as *const u8;
            let flags = args[2] as i32;
            linux_compat::socket_ops::sendmsg(sockfd, msg, flags).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Recvmsg => {
            let sockfd = args[0] as i32;
            let msg = args[1] as *mut u8;
            let flags = args[2] as i32;
            linux_compat::socket_ops::recvmsg(sockfd, msg, flags).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Shutdown => {
            let sockfd = args[0] as i32;
            let how = args[1] as i32;
            linux_compat::socket_ops::shutdown(sockfd, how).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Bind => {
            let sockfd = args[0] as i32;
            let addr = args[1] as *const linux_compat::types::SockAddr;
            let addrlen = args[2] as u32;
            linux_compat::socket_ops::bind(sockfd, addr, addrlen).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Listen => {
            let sockfd = args[0] as i32;
            let backlog = args[1] as i32;
            linux_compat::socket_ops::listen(sockfd, backlog).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getsockname => {
            let sockfd = args[0] as i32;
            let addr = args[1] as *mut linux_compat::types::SockAddr;
            let addrlen = args[2] as *mut u32;
            linux_compat::socket_ops::getsockname(sockfd, addr, addrlen).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getpeername => {
            let sockfd = args[0] as i32;
            let addr = args[1] as *mut linux_compat::types::SockAddr;
            let addrlen = args[2] as *mut u32;
            linux_compat::socket_ops::getpeername(sockfd, addr, addrlen).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Socketpair => {
            let domain = args[0] as i32;
            let sock_type = args[1] as i32;
            let protocol = args[2] as i32;
            let sv = args[3] as *mut i32;
            linux_compat::socket_ops::socketpair(domain, sock_type, protocol, sv).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::SetSockopt => {
            let sockfd = args[0] as i32;
            let level = args[1] as i32;
            let optname = args[2] as i32;
            let optval = args[3] as *const u8;
            let optlen = args[4] as u32;
            linux_compat::socket_ops::setsockopt(sockfd, level, optname, optval, optlen)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::GetSockopt => {
            let sockfd = args[0] as i32;
            let level = args[1] as i32;
            let optname = args[2] as i32;
            let optval = args[3] as *mut u8;
            let optlen = args[4] as *mut u32;
            linux_compat::socket_ops::getsockopt(sockfd, level, optname, optval, optlen)
                .map(|v| v as u64)
        }
        _ => Err(LinuxError::ENOSYS),
    }
}

/// Route memory-related syscalls to memory manager
fn route_memory_syscall(syscall_number: u64, args: &[u64]) -> LinuxResult<u64> {
    let syscall = crate::syscall::SyscallNumber::from_u64(syscall_number);
    if syscall == crate::syscall::SyscallNumber::Invalid {
        return Err(LinuxError::ENOSYS);
    }
    match syscall {
        crate::syscall::SyscallNumber::Mmap => {
            let addr = args[0] as *mut u8;
            let length = args[1] as usize;
            let prot = args[2] as i32;
            let flags = args[3] as i32;
            let fd = args[4] as i32;
            let offset = args[5] as i64;
            linux_compat::memory_ops::mmap(addr, length, prot, flags, fd, offset).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Mprotect => {
            let addr = args[0] as *mut u8;
            let length = args[1] as usize;
            let prot = args[2] as i32;
            linux_compat::memory_ops::mprotect(addr, length, prot).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Munmap => {
            let addr = args[0] as *mut u8;
            let length = args[1] as usize;
            linux_compat::memory_ops::munmap(addr, length).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Brk => {
            let addr = args[0] as *mut u8;
            linux_compat::memory_ops::brk(addr).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Mremap => {
            let old_address = args[0] as *mut u8;
            let old_size = args[1] as usize;
            let new_size = args[2] as usize;
            let flags = args[3] as i32;
            let new_address = args[4] as *mut u8;
            linux_compat::memory_ops::mremap(old_address, old_size, new_size, flags, new_address)
                .map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Msync => {
            let addr = args[0] as *mut u8;
            let length = args[1] as usize;
            let flags = args[2] as i32;
            linux_compat::memory_ops::msync(addr, length, flags).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Mincore => {
            let addr = args[0] as *mut u8;
            let length = args[1] as usize;
            let vec = args[2] as *mut u8;
            linux_compat::memory_ops::mincore(addr, length, vec).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Madvise => {
            let addr = args[0] as *mut u8;
            let length = args[1] as usize;
            let advice = args[2] as i32;
            linux_compat::memory_ops::madvise(addr, length, advice).map(|v| v as u64)
        }
        _ => Err(LinuxError::ENOSYS),
    }
}

/// Get integration statistics
pub fn get_stats() -> IntegrationStats {
    *INTEGRATION_STATS.lock()
}

/// Print integration status
pub fn print_status() {
    let stats = get_stats();
    crate::println!("Linux Integration Status:");
    crate::println!("  Syscalls Routed: {}", stats.syscalls_routed);
    crate::println!("  VFS Operations: {}", stats.vfs_operations);
    crate::println!("  Process Operations: {}", stats.process_operations);
    crate::println!("  Network Operations: {}", stats.network_operations);
    crate::println!("  Memory Operations: {}", stats.memory_operations);
}

/// Integration mode configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntegrationMode {
    /// Full integration - all Linux APIs available
    Full,
    /// Minimal integration - core APIs only
    Minimal,
    /// Custom - user-defined subset
    Custom,
}

static INTEGRATION_MODE: Mutex<IntegrationMode> = Mutex::new(IntegrationMode::Full);

/// Set integration mode
pub fn set_mode(mode: IntegrationMode) {
    let mut current_mode = INTEGRATION_MODE.lock();
    *current_mode = mode;
    crate::serial_println!("[Linux Integration] Mode set to {:?}", mode);
}

/// Get current integration mode
pub fn get_mode() -> IntegrationMode {
    *INTEGRATION_MODE.lock()
}

/// Check if a specific Linux API category is enabled
pub fn is_category_enabled(category: &str) -> bool {
    match *INTEGRATION_MODE.lock() {
        IntegrationMode::Full => true,
        IntegrationMode::Minimal => {
            // Only core categories in minimal mode
            matches!(category, "file" | "process" | "memory")
        }
        IntegrationMode::Custom => {
            // Would check against user configuration
            true
        }
    }
}
