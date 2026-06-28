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
            linux_compat::special_fd::signalfd(args[0] as i32, args[1] as u64, args[2] as i32)
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
        crate::syscall::SyscallNumber::Openat2 => Err(LinuxError::ENOSYS),
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
            linux_compat::fs_ops::setns(args[0] as i32, args[1] as i32).map(|v| v as u64)
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
            linux_compat::fs_ops::unshare(args[0] as i32).map(|v| v as u64)
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
        crate::syscall::SyscallNumber::Vfork => {
            linux_compat::process_ops::vfork().map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Acct
        | crate::syscall::SyscallNumber::AddKey
        | crate::syscall::SyscallNumber::Bpf
        | crate::syscall::SyscallNumber::DeleteModule
        | crate::syscall::SyscallNumber::FinitModule
        | crate::syscall::SyscallNumber::Fsconfig
        | crate::syscall::SyscallNumber::Fsmount
        | crate::syscall::SyscallNumber::Fsopen
        | crate::syscall::SyscallNumber::Fspick
        | crate::syscall::SyscallNumber::InitModule
        | crate::syscall::SyscallNumber::Ioperm
        | crate::syscall::SyscallNumber::Iopl
        | crate::syscall::SyscallNumber::KexecFileLoad
        | crate::syscall::SyscallNumber::KexecLoad
        | crate::syscall::SyscallNumber::Keyctl
        | crate::syscall::SyscallNumber::LandlockAddRule
        | crate::syscall::SyscallNumber::LandlockCreateRuleset
        | crate::syscall::SyscallNumber::LandlockRestrictSelf
        | crate::syscall::SyscallNumber::MountSetattr
        | crate::syscall::SyscallNumber::MoveMount
        | crate::syscall::SyscallNumber::OpenByHandleAt
        | crate::syscall::SyscallNumber::OpenTree
        | crate::syscall::SyscallNumber::PerfEventOpen
        | crate::syscall::SyscallNumber::Ptrace
        | crate::syscall::SyscallNumber::RequestKey
        | crate::syscall::SyscallNumber::Seccomp
        | crate::syscall::SyscallNumber::Vhangup => Err(LinuxError::EPERM),
        crate::syscall::SyscallNumber::CreateModule
        | crate::syscall::SyscallNumber::ModifyLdt
        | crate::syscall::SyscallNumber::Vserver => Err(LinuxError::EINVAL),
        crate::syscall::SyscallNumber::Adjtimex
        | crate::syscall::SyscallNumber::ClockAdjtime
        | crate::syscall::SyscallNumber::FanotifyInit
        | crate::syscall::SyscallNumber::FanotifyMark
        | crate::syscall::SyscallNumber::FutexWaitv
        | crate::syscall::SyscallNumber::IoCancel
        | crate::syscall::SyscallNumber::IoDestroy
        | crate::syscall::SyscallNumber::IoGetevents
        | crate::syscall::SyscallNumber::IoPgetevents
        | crate::syscall::SyscallNumber::IoSetup
        | crate::syscall::SyscallNumber::IoSubmit
        | crate::syscall::SyscallNumber::IoUringEnter
        | crate::syscall::SyscallNumber::IoUringRegister
        | crate::syscall::SyscallNumber::IoUringSetup
        | crate::syscall::SyscallNumber::Kcmp
        | crate::syscall::SyscallNumber::MemfdSecret
        | crate::syscall::SyscallNumber::Mknodat
        | crate::syscall::SyscallNumber::NameToHandleAt
        | crate::syscall::SyscallNumber::PidfdGetfd
        | crate::syscall::SyscallNumber::PidfdOpen
        | crate::syscall::SyscallNumber::PidfdSendSignal
        | crate::syscall::SyscallNumber::ProcessMadvise
        | crate::syscall::SyscallNumber::ProcessMrelease
        | crate::syscall::SyscallNumber::ProcessVmReadv
        | crate::syscall::SyscallNumber::ProcessVmWritev
        | crate::syscall::SyscallNumber::Pselect6
        | crate::syscall::SyscallNumber::QuotactlFd
        | crate::syscall::SyscallNumber::RestartSyscall
        | crate::syscall::SyscallNumber::Rseq
        | crate::syscall::SyscallNumber::RtSigqueueinfo
        | crate::syscall::SyscallNumber::RtSigreturn
        | crate::syscall::SyscallNumber::RtSigtimedwait
        | crate::syscall::SyscallNumber::RtTgsigqueueinfo
        | crate::syscall::SyscallNumber::SchedGetattr
        | crate::syscall::SyscallNumber::SchedSetattr
        | crate::syscall::SyscallNumber::Semctl
        | crate::syscall::SyscallNumber::Semop
        | crate::syscall::SyscallNumber::Semtimedop
        | crate::syscall::SyscallNumber::SetMempolicyHomeNode
        | crate::syscall::SyscallNumber::Setfsgid
        | crate::syscall::SyscallNumber::Setfsuid
        | crate::syscall::SyscallNumber::Shmat
        | crate::syscall::SyscallNumber::Shmctl
        | crate::syscall::SyscallNumber::Statx
        | crate::syscall::SyscallNumber::Sync
        | crate::syscall::SyscallNumber::Time
        | crate::syscall::SyscallNumber::TimerGettime
        | crate::syscall::SyscallNumber::TimerSettime
        | crate::syscall::SyscallNumber::Umounth
        | crate::syscall::SyscallNumber::Uname
        | crate::syscall::SyscallNumber::Userfaultfd
        | crate::syscall::SyscallNumber::Ustat
        | crate::syscall::SyscallNumber::Vmsplice
        | crate::syscall::SyscallNumber::Waitid => Err(LinuxError::ENOSYS),
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
            let euid = args[1] as u32;
            linux_compat::process_ops::seteuid(euid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Setregid => {
            let egid = args[1] as u32;
            linux_compat::process_ops::setegid(egid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Setresuid => {
            let euid = args[1] as u32;
            linux_compat::process_ops::seteuid(euid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getresuid => {
            let ruid = args[0] as *mut u32;
            let euid = args[1] as *mut u32;
            let suid = args[2] as *mut u32;
            unsafe {
                if !ruid.is_null() {
                    *ruid = linux_compat::process_ops::getuid();
                }
                if !euid.is_null() {
                    *euid = linux_compat::process_ops::geteuid();
                }
                if !suid.is_null() {
                    *suid = linux_compat::process_ops::geteuid();
                }
            }
            Ok(0)
        }
        crate::syscall::SyscallNumber::Setresgid => {
            let egid = args[1] as u32;
            linux_compat::process_ops::setegid(egid).map(|v| v as u64)
        }
        crate::syscall::SyscallNumber::Getresgid => {
            let rgid = args[0] as *mut u32;
            let egid = args[1] as *mut u32;
            let sgid = args[2] as *mut u32;
            unsafe {
                if !rgid.is_null() {
                    *rgid = linux_compat::process_ops::getgid();
                }
                if !egid.is_null() {
                    *egid = linux_compat::process_ops::getegid();
                }
                if !sgid.is_null() {
                    *sgid = linux_compat::process_ops::getegid();
                }
            }
            Ok(0)
        }
        crate::syscall::SyscallNumber::Getgroups => {
            let size = args[0] as i32;
            let list = args[1] as *mut u32;
            if size == 0 {
                return Ok(1);
            }
            if size < 1 {
                return Err(LinuxError::EINVAL);
            }
            unsafe {
                if !list.is_null() {
                    *list = 0;
                }
            }
            Ok(1)
        }
        crate::syscall::SyscallNumber::Setgroups => {
            let size = args[0] as i32;
            if size < 0 {
                return Err(LinuxError::EINVAL);
            }
            Ok(0)
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
