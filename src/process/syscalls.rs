//! System Calls Interface and Dispatcher
//!
//! This module implements the system call interface for RustOS, providing
//! a standardized way for processes to request kernel services.

use super::{Pid, Priority, ProcessManager, ProcessState};
pub use crate::syscall::SyscallNumber;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// System call return values
#[derive(Debug, Clone, Copy)]
pub enum SyscallResult {
    Success(u64),
    Error(SyscallError),
}

impl SyscallResult {
    pub fn to_u64(self) -> u64 {
        match self {
            SyscallResult::Success(val) => val,
            SyscallResult::Error(err) => err as u64,
        }
    }
}

/// System call error codes
#[derive(Debug, Clone, Copy)]
#[repr(u64)]
pub enum SyscallError {
    InvalidSyscall = 0xFFFFFFFFFFFFFFFF,
    InvalidArgument = 0xFFFFFFFFFFFFFFFE,
    PermissionDenied = 0xFFFFFFFFFFFFFFFD,
    ProcessNotFound = 0xFFFFFFFFFFFFFFFC,
    OutOfMemory = 0xFFFFFFFFFFFFFFFB,
    InvalidFileDescriptor = 0xFFFFFFFFFFFFFFFA,
    FileNotFound = 0xFFFFFFFFFFFFFFF9,
    ResourceBusy = 0xFFFFFFFFFFFFFFF8,
    OperationNotSupported = 0xFFFFFFFFFFFFFFF7,
    NoChildProcess = 0xFFFFFFFFFFFFFFF6,
    InvalidAddress = 0xFFFFFFFFFFFFFFF5,
    IoError = 0xFFFFFFFFFFFFFFF4,
    InvalidExecutable = 0xFFFFFFFFFFFFFFF3,
    FileTooLarge = 0xFFFFFFFFFFFFFFF2,
    NotFound = 0xFFFFFFFFFFFFFFF1,
}

/// Map a `linux_compat` result into the dispatcher return convention (negative errno).
fn linux_result_i32(result: crate::linux_compat::LinuxResult<i32>) -> SyscallResult {
    match result {
        Ok(v) => SyscallResult::Success(v as u64),
        Err(e) => SyscallResult::Success((-(e as i32) as i64) as u64),
    }
}

/// Same as `linux_result_i32` but for isize-returning linux_compat functions.
fn linux_result_isize(result: crate::linux_compat::LinuxResult<isize>) -> SyscallResult {
    match result {
        Ok(v) => SyscallResult::Success(v as u64),
        Err(e) => SyscallResult::Success((-(e as i32) as i64) as u64),
    }
}

/// File open flags
#[derive(Debug, Clone, Copy)]
pub struct OpenFlags {
    pub read: bool,
    pub write: bool,
    pub create: bool,
    pub truncate: bool,
    pub append: bool,
    pub exclusive: bool,
}

impl From<u64> for OpenFlags {
    fn from(flags: u64) -> Self {
        Self {
            read: (flags & 0x01) != 0,
            write: (flags & 0x02) != 0,
            create: (flags & 0x04) != 0,
            truncate: (flags & 0x08) != 0,
            append: (flags & 0x10) != 0,
            exclusive: (flags & 0x20) != 0,
        }
    }
}

/// System call dispatcher
pub struct SyscallDispatcher {
    /// System call statistics
    syscall_count: [u64; 64],
    /// Total system calls handled
    total_syscalls: u64,
}

impl SyscallDispatcher {
    /// Create a new system call dispatcher
    pub const fn new() -> Self {
        Self {
            syscall_count: [0; 64],
            total_syscalls: 0,
        }
    }

    /// Dispatch a system call
    pub fn dispatch(
        &mut self,
        syscall_number: u64,
        args: &[u64],
        process_manager: &ProcessManager,
    ) -> Result<u64, &'static str> {
        self.total_syscalls += 1;

        // ── Seccomp enforcement ─────────────────────────────────────────────
        // Check the current process's seccomp filter before dispatching.
        // SECCOMP_RET_KILL → process was already terminated; return SIGSYS errno.
        // SECCOMP_RET_TRAP / SECCOMP_RET_ERRNO → return the error value.
        // SECCOMP_RET_ALLOW / no filter → proceed normally.
        {
            let mut args6 = [0u64; 6];
            for (i, &v) in args.iter().enumerate().take(6) {
                args6[i] = v;
            }
            if let Err(errno) = crate::seccomp::check_syscall(syscall_number as i32, &args6) {
                // errno is already negative (e.g. -31 for SIGSYS); return as u64.
                return Ok(errno as i64 as u64);
            }
        }

        let syscall = SyscallNumber::from(syscall_number);

        // Update statistics
        if (syscall_number as usize) < self.syscall_count.len() {
            self.syscall_count[syscall_number as usize] += 1;
        }

        let current_pid = process_manager.current_process();

        let result = match syscall {
            SyscallNumber::Exit | SyscallNumber::ExitGroup => {
                self.sys_exit(args, process_manager, current_pid)
            }
            SyscallNumber::Fork => self.sys_fork(args, process_manager, current_pid),
            SyscallNumber::Execve => self.sys_execve(args, process_manager, current_pid),
            SyscallNumber::Wait4 => self.sys_wait(args, process_manager, current_pid),
            SyscallNumber::GetPid => self.sys_getpid(process_manager, current_pid),
            SyscallNumber::GetPpid => self.sys_getppid(process_manager, current_pid),
            SyscallNumber::Nanosleep => self.sys_sleep(args, process_manager, current_pid),
            SyscallNumber::Clone => self.sys_clone(args, process_manager, current_pid),
            SyscallNumber::Waitid => self.sys_waitid(args, process_manager, current_pid),
            SyscallNumber::Open => self.sys_open(args, process_manager, current_pid),
            SyscallNumber::Close => self.sys_close(args, process_manager, current_pid),
            SyscallNumber::Read => self.sys_read(args, process_manager, current_pid),
            SyscallNumber::Write => self.sys_write(args, process_manager, current_pid),
            SyscallNumber::Lseek => self.sys_seek(args, process_manager, current_pid),
            SyscallNumber::Stat | SyscallNumber::Fstat | SyscallNumber::Lstat => {
                self.sys_stat(args, process_manager, current_pid)
            }
            SyscallNumber::Openat => self.sys_openat(args, process_manager, current_pid),
            SyscallNumber::Mkdirat => self.sys_mkdirat(args, process_manager, current_pid),
            SyscallNumber::Unlinkat => self.sys_unlinkat(args, process_manager, current_pid),
            SyscallNumber::Fchmod => self.sys_fchmod(args, process_manager, current_pid),
            SyscallNumber::Mmap => self.sys_mmap(args, process_manager, current_pid),
            SyscallNumber::Munmap => self.sys_munmap(args, process_manager, current_pid),
            SyscallNumber::Brk => self.sys_brk(args, process_manager, current_pid),
            SyscallNumber::Mprotect => self.sys_mprotect(args, process_manager, current_pid),
            SyscallNumber::Madvise => self.sys_madvise(args, process_manager, current_pid),
            SyscallNumber::Pipe => self.sys_pipe(args, process_manager, current_pid),
            SyscallNumber::RtSigaction => self.sys_signal(args, process_manager, current_pid),
            SyscallNumber::Kill => self.sys_kill(args, process_manager, current_pid),
            SyscallNumber::Futex => self.sys_futex(args, process_manager, current_pid),
            SyscallNumber::Socket => self.sys_socket(args, process_manager, current_pid),
            SyscallNumber::Bind => self.sys_bind(args, process_manager, current_pid),
            SyscallNumber::Connect => self.sys_connect(args, process_manager, current_pid),
            SyscallNumber::Listen => self.sys_listen(args, process_manager, current_pid),
            SyscallNumber::Accept => self.sys_accept(args, process_manager, current_pid),
            SyscallNumber::Uname => self.sys_uname(args, process_manager, current_pid),
            SyscallNumber::Gettimeofday | SyscallNumber::ClockGettime => {
                self.sys_gettime(process_manager)
            }
            SyscallNumber::Settimeofday | SyscallNumber::ClockSettime => {
                self.sys_settime(args, process_manager, current_pid)
            }
            SyscallNumber::Setpriority => self.sys_setpriority(args, process_manager, current_pid),
            SyscallNumber::Getpriority => self.sys_getpriority(args, process_manager, current_pid),
            SyscallNumber::SetTidAddress => {
                self.sys_set_tid_address(args, process_manager, current_pid)
            }
            SyscallNumber::Ioctl => self.sys_ioctl(args, process_manager, current_pid),
            SyscallNumber::Fcntl => self.sys_fcntl(args, process_manager, current_pid),
            SyscallNumber::PkgInstall => self.sys_pkg_install(args),
            SyscallNumber::PkgRemove => self.sys_pkg_remove(args),
            SyscallNumber::PkgSearch => self.sys_pkg_search(args),
            SyscallNumber::PkgInfo => self.sys_pkg_info(args),
            SyscallNumber::PkgList => self.sys_pkg_list(args),
            SyscallNumber::PkgUpdate => self.sys_pkg_update(args),
            SyscallNumber::PkgUpgrade => self.sys_pkg_upgrade(args),
            // ── Additional syscalls wired to linux_compat or simple stubs ──
            SyscallNumber::SchedYield => SyscallResult::Success(0),
            SyscallNumber::Getuid => self.sys_getuid(process_manager, current_pid),
            SyscallNumber::Getgid => self.sys_getgid(process_manager, current_pid),
            SyscallNumber::Geteuid => self.sys_getuid(process_manager, current_pid),
            SyscallNumber::Getegid => self.sys_getgid(process_manager, current_pid),
            SyscallNumber::Setuid => self.sys_setuid(args, process_manager, current_pid),
            SyscallNumber::Setgid => self.sys_setgid(args, process_manager, current_pid),
            SyscallNumber::Gettid => self.sys_gettid(process_manager, current_pid),
            SyscallNumber::RtSigprocmask => {
                linux_result_i32(crate::linux_compat::signal_ops::rt_sigprocmask(
                    args.first().copied().unwrap_or(0) as i32,
                    args.get(1).copied().unwrap_or(0) as *const u64,
                    args.get(2).copied().unwrap_or(0) as *mut u64,
                    args.get(3).copied().unwrap_or(8) as usize,
                ))
            }
            SyscallNumber::Tkill => {
                let tid = args.first().copied().unwrap_or(0) as Pid;
                let sig = args.get(1).copied().unwrap_or(0);
                self.sys_kill(&[tid as u64, sig], process_manager, current_pid)
            }
            SyscallNumber::Tgkill => {
                // tgkill(tgid, tid, sig) — we treat tid as the PID
                let tid = args.get(1).copied().unwrap_or(0) as Pid;
                let sig = args.get(2).copied().unwrap_or(0);
                self.sys_kill(&[tid as u64, sig], process_manager, current_pid)
            }
            SyscallNumber::Dup => linux_result_i32(crate::linux_compat::file_ops::dup(
                args.first().copied().unwrap_or(0) as i32,
            )),
            SyscallNumber::Dup2 => linux_result_i32(crate::linux_compat::file_ops::dup2(
                args.first().copied().unwrap_or(0) as i32,
                args.get(1).copied().unwrap_or(0) as i32,
            )),
            SyscallNumber::Sendto => linux_result_isize(crate::linux_compat::socket_ops::sendto(
                args.first().copied().unwrap_or(0) as i32,
                args.get(1).copied().unwrap_or(0) as *const u8,
                args.get(2).copied().unwrap_or(0) as usize,
                args.get(3).copied().unwrap_or(0) as i32,
                args.get(4).copied().unwrap_or(0) as *const crate::linux_compat::SockAddr,
                args.get(5).copied().unwrap_or(0) as u32,
            )),
            SyscallNumber::Recvfrom => {
                linux_result_isize(crate::linux_compat::socket_ops::recvfrom(
                    args.first().copied().unwrap_or(0) as i32,
                    args.get(1).copied().unwrap_or(0) as *mut u8,
                    args.get(2).copied().unwrap_or(0) as usize,
                    args.get(3).copied().unwrap_or(0) as i32,
                    args.get(4).copied().unwrap_or(0) as *mut crate::linux_compat::SockAddr,
                    args.get(5).copied().unwrap_or(0) as *mut u32,
                ))
            }
            SyscallNumber::Shutdown => linux_result_i32(crate::linux_compat::socket_ops::shutdown(
                args.first().copied().unwrap_or(0) as i32,
                args.get(1).copied().unwrap_or(0) as i32,
            )),
            SyscallNumber::SetSockopt => {
                linux_result_i32(crate::linux_compat::socket_ops::setsockopt(
                    args.first().copied().unwrap_or(0) as i32,
                    args.get(1).copied().unwrap_or(0) as i32,
                    args.get(2).copied().unwrap_or(0) as i32,
                    args.get(3).copied().unwrap_or(0) as *const u8,
                    args.get(4).copied().unwrap_or(0) as u32,
                ))
            }
            SyscallNumber::GetSockopt => {
                linux_result_i32(crate::linux_compat::socket_ops::getsockopt(
                    args.first().copied().unwrap_or(0) as i32,
                    args.get(1).copied().unwrap_or(0) as i32,
                    args.get(2).copied().unwrap_or(0) as i32,
                    args.get(3).copied().unwrap_or(0) as *mut u8,
                    args.get(4).copied().unwrap_or(0) as *mut u32,
                ))
            }
            SyscallNumber::Unlink => {
                linux_result_i32(crate::linux_compat::file_ops::unlinkat(
                    -100, // AT_FDCWD
                    args.first().copied().unwrap_or(0) as *const u8,
                    0,
                ))
            }
            SyscallNumber::Mkdir => {
                linux_result_i32(crate::linux_compat::file_ops::mkdirat(
                    -100, // AT_FDCWD
                    args.first().copied().unwrap_or(0) as *const u8,
                    args.get(1).copied().unwrap_or(0o755) as u32,
                ))
            }
            SyscallNumber::Rmdir => {
                linux_result_i32(crate::linux_compat::file_ops::unlinkat(
                    -100, // AT_FDCWD
                    args.first().copied().unwrap_or(0) as *const u8,
                    0x200, // AT_REMOVEDIR
                ))
            }
            SyscallNumber::Chmod => {
                linux_result_i32(crate::linux_compat::file_ops::fchmodat(
                    -100, // AT_FDCWD
                    args.first().copied().unwrap_or(0) as *const u8,
                    args.get(1).copied().unwrap_or(0) as u32,
                    0,
                ))
            }
            SyscallNumber::Seccomp => {
                let ret = crate::seccomp::seccomp_set_mode(
                    args.first().copied().unwrap_or(0) as u32,
                    args.get(1).copied().unwrap_or(0) as u32,
                    args.get(2).copied().unwrap_or(0) as *const u8,
                );
                if ret == 0 {
                    SyscallResult::Success(0)
                } else {
                    SyscallResult::Success((ret as i64) as u64)
                }
            }
            SyscallNumber::Invalid => SyscallResult::Error(SyscallError::InvalidSyscall),
            _ => match crate::linux_integration::route_syscall(syscall_number, args) {
                Ok(value) => SyscallResult::Success(value),
                Err(error) => SyscallResult::Success((-(error as i32) as i64) as u64),
            },
        };

        match result {
            SyscallResult::Success(val) => Ok(val),
            SyscallResult::Error(_) => Err("System call failed"),
        }
    }

    // Process management system calls

    /// sys_exit - Terminate the calling process
    fn sys_exit(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let exit_status = args.get(0).copied().unwrap_or(0) as i32;

        match process_manager.terminate_process(current_pid, exit_status) {
            Ok(()) => SyscallResult::Success(0),
            Err(_) => SyscallResult::Error(SyscallError::ProcessNotFound),
        }
    }

    /// sys_fork - Create a new process with copy-on-write memory
    fn sys_fork(
        &self,
        _args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        use crate::process::integration::get_integration_manager;

        // Validate parent process exists
        let parent_process = match process_manager.get_process(current_pid) {
            Some(pcb) => pcb,
            None => return SyscallResult::Error(SyscallError::ProcessNotFound),
        };

        // Check if process can fork (not in critical state)
        if matches!(parent_process.state, super::ProcessState::Terminated) {
            return SyscallResult::Error(SyscallError::ProcessNotFound);
        }

        // Use production fork implementation with copy-on-write
        let integration_manager = get_integration_manager();
        match integration_manager.fork_process(current_pid) {
            Ok(child_pid) => {
                // Verify child process was created successfully
                if let Some(mut child_process) = process_manager.get_process(child_pid) {
                    // Ensure parent-child relationship is properly set
                    if child_process.parent_pid != Some(current_pid) {
                        // Fix parent-child relationship if not set correctly
                        child_process.parent_pid = Some(current_pid);
                    }

                    // Copy file descriptors from parent to child
                    child_process.file_descriptors = parent_process.file_descriptors.clone();
                    child_process.file_offsets = parent_process.file_offsets.clone();

                    // Copy signal handlers from parent to child
                    child_process.signal_handlers = parent_process.signal_handlers.clone();

                    // Set the child's return value (rax) to 0 so that when
                    // the child process is first scheduled and returns from
                    // the syscall, it sees fork() returning 0. The parent
                    // gets child_pid as the return value (below).
                    child_process.context.rax = 0;

                    // Write the updated child PCB back
                    let _ = process_manager.with_process_mut(child_pid, |pcb| {
                        pcb.parent_pid = Some(current_pid);
                        pcb.file_descriptors = child_process.file_descriptors.clone();
                        pcb.file_offsets = child_process.file_offsets.clone();
                        pcb.signal_handlers = child_process.signal_handlers.clone();
                        pcb.context.rax = 0;
                    });

                    // Parent sees child_pid as the return value
                    SyscallResult::Success(child_pid as u64)
                } else {
                    // Child process creation failed
                    SyscallResult::Error(SyscallError::OutOfMemory)
                }
            }
            Err(_) => SyscallResult::Error(SyscallError::OutOfMemory),
        }
    }

    /// sys_exec - Execute a new program
    fn sys_exec(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        use crate::fs::OpenFlags;
        use crate::process::elf_loader::{ElfLoader, ElfLoaderError};
        use alloc::string::String;
        use alloc::vec::Vec;

        // Step 1: Validate and read program path from user space
        let program_path_ptr = args.get(0).copied().unwrap_or(0);
        if program_path_ptr == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgument);
        }

        // Validate pointer is in user space (0x400000 - 0xFFFFFFFF00000000)
        if program_path_ptr < 0x400000 || program_path_ptr >= 0xFFFFFFFF00000000 {
            return SyscallResult::Error(SyscallError::InvalidAddress);
        }

        // Read null-terminated string from user space (max 256 bytes for path)
        let program_path = unsafe {
            let mut path_bytes = Vec::new();
            let mut ptr = program_path_ptr as *const u8;

            for _ in 0..256 {
                // Validate each byte address
                if (ptr as u64) < 0x400000 || (ptr as u64) >= 0xFFFFFFFF00000000 {
                    return SyscallResult::Error(SyscallError::InvalidAddress);
                }

                let byte = *ptr;
                if byte == 0 {
                    break;
                }
                path_bytes.push(byte);
                ptr = ptr.add(1);
            }

            // Convert to string
            match String::from_utf8(path_bytes) {
                Ok(s) => s,
                Err(_) => return SyscallResult::Error(SyscallError::InvalidArgument),
            }
        };

        // Validate path is not empty
        if program_path.is_empty() {
            return SyscallResult::Error(SyscallError::InvalidArgument);
        }

        // Step 2: Load binary from filesystem
        let vfs = crate::fs::vfs();

        // Open the file for reading
        let fd = match vfs.open(
            &program_path,
            OpenFlags {
                read: true,
                write: false,
                create: false,
                append: false,
                truncate: false,
                exclusive: false,
            },
        ) {
            Ok(fd) => fd,
            Err(_) => return SyscallResult::Error(SyscallError::FileNotFound),
        };

        // Get file metadata to determine size
        let file_size = match vfs.stat(&program_path) {
            Ok(metadata) => metadata.size as usize,
            Err(_) => {
                let _ = vfs.close(fd);
                return SyscallResult::Error(SyscallError::IoError);
            }
        };

        // Validate file size (max 16MB for executable)
        if file_size > 16 * 1024 * 1024 {
            let _ = vfs.close(fd);
            return SyscallResult::Error(SyscallError::FileTooLarge);
        }

        // Read entire binary into memory
        let mut binary_data = Vec::with_capacity(file_size);
        binary_data.resize(file_size, 0);

        match vfs.read(fd, &mut binary_data) {
            Ok(bytes_read) if bytes_read == file_size => {
                // Successfully read entire file
            }
            _ => {
                let _ = vfs.close(fd);
                return SyscallResult::Error(SyscallError::IoError);
            }
        }

        // Close file descriptor
        let _ = vfs.close(fd);

        // Step 3: Initialize ELF loader with security features enabled
        let elf_loader = ElfLoader::new(
            true, // enable_aslr - Address Space Layout Randomization
            true, // enable_nx - No-Execute protection
        );

        // Step 4: Parse and load ELF binary
        let loaded_binary = match elf_loader.load_elf_binary(&binary_data, current_pid) {
            Ok(binary) => binary,
            Err(e) => {
                // Map ELF loader errors to syscall errors
                let syscall_error = match e {
                    ElfLoaderError::InvalidMagic => SyscallError::InvalidExecutable,
                    ElfLoaderError::UnsupportedClass => SyscallError::InvalidExecutable,
                    ElfLoaderError::UnsupportedArchitecture => SyscallError::InvalidExecutable,
                    ElfLoaderError::InvalidFileType => SyscallError::InvalidExecutable,
                    ElfLoaderError::MemoryAllocationFailed => SyscallError::OutOfMemory,
                    ElfLoaderError::InvalidPermissions => SyscallError::PermissionDenied,
                    ElfLoaderError::InvalidEntryPoint => SyscallError::InvalidExecutable,
                    ElfLoaderError::FileTooLarge => SyscallError::FileTooLarge,
                    _ => SyscallError::InvalidExecutable,
                };
                return SyscallResult::Error(syscall_error);
            }
        };

        // Step 5: Update process control block with loaded binary information
        let mut process = match process_manager.get_process(current_pid) {
            Some(p) => p,
            None => return SyscallResult::Error(SyscallError::ProcessNotFound),
        };

        // Update memory layout
        process.memory.code_start = loaded_binary.base_address.as_u64();
        process.memory.code_size = loaded_binary
            .code_regions
            .iter()
            .map(|r| r.size as u64)
            .sum();

        process.memory.data_start = loaded_binary
            .data_regions
            .first()
            .map(|r| r.start.as_u64())
            .unwrap_or(0);
        process.memory.data_size = loaded_binary
            .data_regions
            .iter()
            .map(|r| r.size as u64)
            .sum();

        process.memory.heap_start = loaded_binary.heap_start.as_u64();
        process.memory.heap_size = 8 * 1024; // 8KB initial heap

        process.memory.stack_start = loaded_binary.stack_top.as_u64() - 8 * 1024 * 1024; // Stack base
        process.memory.stack_size = 8 * 1024 * 1024; // 8MB stack

        // Update entry point and reset CPU context
        process.entry_point = loaded_binary.entry_point.as_u64();
        process.context.rip = loaded_binary.entry_point.as_u64(); // Set instruction pointer
        process.context.rsp = loaded_binary.stack_top.as_u64(); // Set stack pointer

        // Reset other registers
        process.context.rax = 0;
        process.context.rbx = 0;
        process.context.rcx = 0;
        process.context.rdx = 0;
        process.context.rsi = 0;
        process.context.rdi = 0;
        process.context.rbp = loaded_binary.stack_top.as_u64();

        // Set process state to ready
        process.state = ProcessState::Ready;

        // Clear file descriptors except stdin/stdout/stderr
        process.file_descriptors.retain(|&fd, _| fd <= 2);
        process.file_offsets.retain(|&fd, _| fd <= 2);

        // Clear signal handlers (reset to default)
        process.signal_handlers.clear();

        // Success - return 0
        SyscallResult::Success(0)
    }

    /// sys_wait - Wait for child process to terminate
    fn sys_wait(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let wait_pid = args.get(0).map(|&p| p as i32).unwrap_or(-1);

        // Get current process
        let _current_process = match process_manager.get_process(current_pid) {
            Some(p) => p,
            None => return SyscallResult::Error(SyscallError::ProcessNotFound),
        };

        // Find child processes
        let children: Vec<Pid> = process_manager
            .processes
            .read()
            .iter()
            .filter_map(|(pid, pcb)| {
                if pcb.parent_pid == Some(current_pid) {
                    if wait_pid == -1 || wait_pid == *pid as i32 {
                        Some(*pid)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();

        if children.is_empty() {
            return SyscallResult::Error(SyscallError::NoChildProcess);
        }

        // Check for any terminated children
        for child_pid in children {
            if let Some(child) = process_manager.get_process(child_pid) {
                if matches!(child.state, ProcessState::Terminated) {
                    // Reap the child process
                    let exit_code = child.exit_code.unwrap_or(0);
                    process_manager.processes.write().remove(&child_pid);
                    return SyscallResult::Success(((child_pid as u64) << 32) | (exit_code as u64));
                }
            }
        }

        // Block current process until a child terminates
        if let Err(_) = process_manager.block_process(current_pid) {
            return SyscallResult::Error(SyscallError::ProcessNotFound);
        }

        // Return would happen after unblocking when child terminates
        SyscallResult::Success(0)
    }

    /// sys_getpid - Get process ID
    fn sys_getpid(&self, process_manager: &ProcessManager, current_pid: Pid) -> SyscallResult {
        // Validate that the current PID is valid and exists
        if current_pid == 0 {
            return SyscallResult::Error(SyscallError::ProcessNotFound);
        }

        // Verify the process actually exists in the process table
        match process_manager.get_process(current_pid) {
            Some(_) => {
                // Return the namespace-local PID so processes inside a PID
                // namespace see their virtual PID (1 for the init of the ns).
                let ns = crate::namespace::get_nsproxy(current_pid);
                let local_pid = ns.pid.lock().local_pid(current_pid);
                SyscallResult::Success(local_pid as u64)
            }
            None => {
                // This should not happen - current PID should always be valid
                SyscallResult::Error(SyscallError::ProcessNotFound)
            }
        }
    }

    /// sys_getppid - Get parent process ID
    fn sys_getppid(&self, process_manager: &ProcessManager, current_pid: Pid) -> SyscallResult {
        match process_manager.get_process(current_pid) {
            Some(pcb) => match pcb.parent_pid {
                Some(ppid) => SyscallResult::Success(ppid as u64),
                None => SyscallResult::Success(0), // No parent (probably kernel process)
            },
            None => SyscallResult::Error(SyscallError::ProcessNotFound),
        }
    }

    /// sys_sleep - Sleep for specified time
    fn sys_sleep(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let sleep_time_ms = args.get(0).copied().unwrap_or(0);

        if sleep_time_ms == 0 {
            return SyscallResult::Success(0);
        }

        // Block the process temporarily
        match process_manager.block_process(current_pid) {
            Ok(()) => {
                // Calculate wake-up time using the time subsystem
                let current_time_ms = crate::time::uptime_ms();
                let wake_time = current_time_ms + sleep_time_ms;

                // Store wake-up time in process control block
                {
                    let mut processes = process_manager.processes.write();
                    if let Some(pcb) = processes.get_mut(&current_pid) {
                        pcb.wake_time = Some(wake_time);
                    }
                }

                // Process is woken when `wake_time` is reached (timer integration).

                SyscallResult::Success(0)
            }
            Err(_) => SyscallResult::Error(SyscallError::ProcessNotFound),
        }
    }

    // File I/O system calls

    /// sys_open - Open a file
    fn sys_open(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let path_ptr = args.get(0).copied().unwrap_or(0);
        let flags = args.get(1).copied().unwrap_or(0) as u32;
        let mode = args.get(2).copied().unwrap_or(0o644) as u32;

        // Copy path from user memory
        let path = match self.copy_string_from_user(path_ptr) {
            Ok(p) => p,
            Err(_) => return SyscallResult::Error(SyscallError::InvalidAddress),
        };

        // Open file through VFS
        use crate::fs::{get_vfs, SyscallOpenFlags};
        let vfs = get_vfs();

        let open_flags = SyscallOpenFlags::from_bits(flags).unwrap_or(SyscallOpenFlags::READ);

        match vfs.open(&path, open_flags, mode) {
            Ok(inode) => {
                // Allocate file descriptor on the live process, not a clone.
                let next_fd = process_manager.with_process_mut(current_pid, |p| {
                    let mut next_fd = 3; // Start after stdin/stdout/stderr
                    while p.file_descriptors.contains_key(&next_fd) {
                        next_fd += 1;
                        if next_fd > 65535 {
                            return None;
                        }
                    }
                    let fd = super::FileDescriptor::from_inode(inode, flags);
                    p.file_descriptors.insert(next_fd, fd);
                    p.file_offsets.insert(next_fd, 0);
                    Some(next_fd)
                });

                match next_fd {
                    Some(Some(fd)) => SyscallResult::Success(fd as u64),
                    _ => SyscallResult::Error(SyscallError::ProcessNotFound),
                }
            }
            Err(_) => SyscallResult::Error(SyscallError::FileNotFound),
        }
    }

    /// sys_close - Close a file descriptor
    fn sys_close(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let fd = args.get(0).copied().unwrap_or(0) as u32;

        let removed = process_manager.with_process_mut(current_pid, |p| {
            let removed = p.file_descriptors.remove(&fd).is_some();
            if removed {
                p.file_offsets.remove(&fd);
            }
            removed
        });

        if removed.unwrap_or(false) {
            SyscallResult::Success(0)
        } else {
            SyscallResult::Error(SyscallError::InvalidFileDescriptor)
        }
    }

    /// sys_read - Read from a file descriptor
    fn sys_read(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let fd = args.get(0).copied().unwrap_or(0) as u32;
        let buffer_ptr = args.get(1).copied().unwrap_or(0);
        let count = args.get(2).copied().unwrap_or(0) as usize;

        let mut buffer = vec![0u8; count];
        let result = process_manager.with_process_mut(current_pid, |p| {
            let fd_entry = p.file_descriptors.get_mut(&fd)?;
            Some(fd_entry.read(&mut buffer).map(|n| (fd_entry.offset(), n)))
        });
        let (new_offset, bytes_read) = match result {
            Some(Some(Ok((offset, n)))) => (offset, n),
            Some(Some(Err(_))) => return SyscallResult::Error(SyscallError::IoError),
            _ => return SyscallResult::Error(SyscallError::InvalidFileDescriptor),
        };

        process_manager.with_process_mut(current_pid, |p| {
            p.file_offsets.insert(fd, new_offset as usize);
        });

        if self.copy_to_user(buffer_ptr, &buffer[..bytes_read]).is_ok() {
            SyscallResult::Success(bytes_read as u64)
        } else {
            SyscallResult::Error(SyscallError::InvalidAddress)
        }
    }

    /// sys_write - Write to a file descriptor
    fn sys_write(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let fd = args.get(0).copied().unwrap_or(0) as u32;
        let buffer_ptr = args.get(1).copied().unwrap_or(0);
        let count = args.get(2).copied().unwrap_or(0) as usize;

        let mut buffer = vec![0u8; count];
        if self.copy_from_user(buffer_ptr, &mut buffer).is_err() {
            return SyscallResult::Error(SyscallError::InvalidAddress);
        }

        let result = process_manager.with_process_mut(current_pid, |p| {
            let fd_entry = p.file_descriptors.get_mut(&fd)?;
            Some(fd_entry.write(&buffer).map(|n| (fd_entry.offset(), n)))
        });
        let (new_offset, bytes_written) = match result {
            Some(Some(Ok((offset, n)))) => (offset, n),
            Some(Some(Err(_))) => return SyscallResult::Error(SyscallError::IoError),
            _ => return SyscallResult::Error(SyscallError::InvalidFileDescriptor),
        };

        process_manager.with_process_mut(current_pid, |p| {
            p.file_offsets.insert(fd, new_offset as usize);
        });

        SyscallResult::Success(bytes_written as u64)
    }

    /// sys_seek - Seek in a file
    fn sys_seek(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let fd = args.get(0).copied().unwrap_or(0) as u32;
        let offset = args.get(1).copied().unwrap_or(0) as i64;
        let whence = args.get(2).copied().unwrap_or(0) as u32;

        if whence > 2 {
            return SyscallResult::Error(SyscallError::InvalidArgument);
        }

        let result = process_manager.with_process_mut(current_pid, |p| {
            let fd_entry = p.file_descriptors.get_mut(&fd)?;
            let current = fd_entry.offset() as i64;
            let file_size = fd_entry.size().ok()? as i64;

            let new_offset = match whence {
                0 => offset,
                1 => current + offset,
                2 => file_size + offset,
                _ => return Some(Err(SyscallError::InvalidArgument)),
            };

            if new_offset < 0 {
                return Some(Err(SyscallError::InvalidArgument));
            }

            fd_entry.set_offset(new_offset as u64);
            Some(Ok(new_offset as u64))
        });

        let new_offset = match result {
            Some(Some(Ok(offset))) => offset,
            Some(Some(Err(e))) => return SyscallResult::Error(e),
            _ => return SyscallResult::Error(SyscallError::InvalidFileDescriptor),
        };

        process_manager.with_process_mut(current_pid, |p| {
            p.file_offsets.insert(fd, new_offset as usize);
        });
        SyscallResult::Success(new_offset)
    }

    /// sys_stat - Get file status
    fn sys_stat(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let path_ptr = args.get(0).copied().unwrap_or(0);
        let stat_buf_ptr = args.get(1).copied().unwrap_or(0);

        // Copy path from user memory
        let path = match self.copy_string_from_user(path_ptr) {
            Ok(p) => p,
            Err(_) => return SyscallResult::Error(SyscallError::InvalidAddress),
        };

        // Get file info through VFS
        use crate::fs::{get_vfs, SyscallOpenFlags};
        let vfs = get_vfs();

        match vfs.open(&path, SyscallOpenFlags::READ, 0) {
            Ok(inode) => {
                // Create stat structure
                #[repr(C)]
                struct Stat {
                    dev: u64,
                    ino: u64,
                    mode: u32,
                    nlink: u32,
                    uid: u32,
                    gid: u32,
                    rdev: u64,
                    size: u64,
                    blksize: u64,
                    blocks: u64,
                    atime: u64,
                    mtime: u64,
                    ctime: u64,
                }

                let stat = Stat {
                    dev: 0,
                    ino: inode.inode_number() as u64,
                    mode: inode.mode(),
                    nlink: 1,
                    uid: 0,
                    gid: 0,
                    rdev: 0,
                    size: inode.size() as u64,
                    blksize: 4096,
                    blocks: (inode.size() + 4095) / 4096,
                    atime: 0,
                    mtime: 0,
                    ctime: 0,
                };

                // Copy to user buffer
                let stat_bytes = unsafe {
                    core::slice::from_raw_parts(
                        &stat as *const _ as *const u8,
                        core::mem::size_of::<Stat>(),
                    )
                };

                if self.copy_to_user(stat_buf_ptr, stat_bytes).is_ok() {
                    SyscallResult::Success(0)
                } else {
                    SyscallResult::Error(SyscallError::InvalidAddress)
                }
            }
            Err(_) => SyscallResult::Error(SyscallError::FileNotFound),
        }
    }

    // Memory management system calls

    /// sys_mmap - Map memory using production memory manager
    fn sys_mmap(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        use crate::memory::{
            allocate_memory, deallocate_memory, MemoryProtection, MemoryRegionType,
        };

        let length = args.get(1).copied().unwrap_or(0);
        let prot = args.get(2).copied().unwrap_or(0);
        let flags = args.get(3).copied().unwrap_or(0);
        let fd = args.get(4).copied().unwrap_or(u64::MAX) as i32;
        let offset = args.get(5).copied().unwrap_or(0);

        if length == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgument);
        }

        let protection = MemoryProtection {
            readable: (prot & 0x1) != 0,
            writable: (prot & 0x2) != 0,
            executable: (prot & 0x4) != 0,
            user_accessible: true,
            cache_disabled: false,
            write_through: false,
            copy_on_write: false,
            guard_page: false,
        };

        let is_anonymous = (flags & 0x20) != 0 || fd < 0;

        if !is_anonymous {
            if (flags & 0x01) != 0 {
                return SyscallResult::Error(SyscallError::OperationNotSupported);
            }

            match crate::vfs::vfs_fd_kind(fd) {
                Ok(crate::vfs::FdKind::Regular) => {}
                Ok(_) => return SyscallResult::Error(SyscallError::InvalidFileDescriptor),
                Err(_) => return SyscallResult::Error(SyscallError::InvalidFileDescriptor),
            }

            let fill_protection = MemoryProtection {
                readable: true,
                writable: true,
                executable: false,
                user_accessible: true,
                cache_disabled: false,
                write_through: false,
                copy_on_write: false,
                guard_page: false,
            };

            let virt_addr =
                match allocate_memory(length as usize, MemoryRegionType::UserHeap, fill_protection)
                {
                    Ok(addr) => addr,
                    Err(_) => return SyscallResult::Error(SyscallError::OutOfMemory),
                };

            let free_and_fail = |err: SyscallError| -> SyscallResult {
                let _ = deallocate_memory(virt_addr);
                SyscallResult::Error(err)
            };

            let file_size = match crate::vfs::vfs_fstat(fd) {
                Ok(stat) => stat.size,
                Err(_) => return free_and_fail(SyscallError::InvalidFileDescriptor),
            };
            let available = file_size.saturating_sub(offset);
            let to_read = core::cmp::min(available, length) as usize;
            let base_ptr = virt_addr.as_u64() as *mut u8;

            let mut copied = 0usize;
            while copied < to_read {
                let chunk_len = core::cmp::min(4096, to_read - copied);
                let dst =
                    unsafe { core::slice::from_raw_parts_mut(base_ptr.add(copied), chunk_len) };
                match crate::vfs::vfs_pread(fd, dst, offset + copied as u64) {
                    Ok(0) => break,
                    Ok(n) => copied += n,
                    Err(_) => return free_and_fail(SyscallError::IoError),
                }
            }
            if copied < length as usize {
                unsafe {
                    core::ptr::write_bytes(base_ptr.add(copied), 0u8, length as usize - copied);
                }
            }

            if crate::memory::protect_memory(virt_addr, length as usize, protection).is_err() {
                return free_and_fail(SyscallError::OutOfMemory);
            }

            return SyscallResult::Success(virt_addr.as_u64());
        }

        let region_type = if protection.executable {
            MemoryRegionType::UserCode
        } else {
            MemoryRegionType::UserData
        };

        match allocate_memory(length as usize, region_type, protection) {
            Ok(virt_addr) => SyscallResult::Success(virt_addr.as_u64()),
            Err(_) => SyscallResult::Error(SyscallError::OutOfMemory),
        }
    }

    /// sys_munmap - Unmap memory using production memory manager
    fn sys_munmap(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        use crate::memory::deallocate_memory;
        use x86_64::VirtAddr;

        let addr = args.get(0).copied().unwrap_or(0);
        let _length = args.get(1).copied().unwrap_or(0);

        if addr == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgument);
        }

        let virt_addr = VirtAddr::new(addr);
        match deallocate_memory(virt_addr) {
            Ok(()) => SyscallResult::Success(0),
            Err(_) => SyscallResult::Error(SyscallError::InvalidArgument),
        }
    }

    /// sys_brk - Change data segment size using production memory manager
    fn sys_brk(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        use crate::memory::{
            allocate_memory, deallocate_memory, MemoryProtection, MemoryRegionType, PAGE_SIZE,
        };

        let new_brk = args.get(0).copied().unwrap_or(0);

        // Get current process
        let mut process = match process_manager.get_process(current_pid) {
            Some(pcb) => pcb,
            None => return SyscallResult::Error(SyscallError::ProcessNotFound),
        };

        let current_heap_end = process.memory.heap_start + process.memory.heap_size;

        if new_brk == 0 {
            // Return current break
            return SyscallResult::Success(current_heap_end);
        }

        // Validate new break address
        if new_brk < process.memory.heap_start {
            return SyscallResult::Error(SyscallError::InvalidArgument);
        }

        if new_brk > current_heap_end {
            // Expand heap
            let expansion_size = new_brk - current_heap_end;

            // Limit heap expansion to prevent abuse (max 1GB heap)
            if process.memory.heap_size + expansion_size > 1024 * 1024 * 1024 {
                return SyscallResult::Error(SyscallError::OutOfMemory);
            }

            let aligned_size =
                ((expansion_size + PAGE_SIZE as u64 - 1) / PAGE_SIZE as u64) * PAGE_SIZE as u64;

            let protection = MemoryProtection {
                readable: true,
                writable: true,
                executable: false,
                user_accessible: true,
                cache_disabled: false,
                write_through: false,
                copy_on_write: false,
                guard_page: false,
            };

            match allocate_memory(
                aligned_size as usize,
                MemoryRegionType::UserHeap,
                protection,
            ) {
                Ok(_) => {
                    // Update process heap size
                    process.memory.heap_size += expansion_size;
                    SyscallResult::Success(new_brk)
                }
                Err(_) => SyscallResult::Error(SyscallError::OutOfMemory),
            }
        } else if new_brk < current_heap_end {
            // Shrink heap
            let shrink_size = current_heap_end - new_brk;
            let aligned_size =
                ((shrink_size + PAGE_SIZE as u64 - 1) / PAGE_SIZE as u64) * PAGE_SIZE as u64;

            // Calculate the address to deallocate from
            let dealloc_start = current_heap_end - aligned_size;

            match deallocate_memory(x86_64::VirtAddr::new(dealloc_start)) {
                Ok(()) => {
                    // Update process heap size
                    process.memory.heap_size -= shrink_size;
                    SyscallResult::Success(new_brk)
                }
                Err(_) => SyscallResult::Error(SyscallError::InvalidArgument),
            }
        } else {
            // No change
            SyscallResult::Success(current_heap_end)
        }
    }

    /// sys_sbrk - Change data segment size incrementally
    fn sys_sbrk(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let increment = args.get(0).copied().unwrap_or(0) as i64;

        // Get current process
        let process = match process_manager.get_process(current_pid) {
            Some(pcb) => pcb,
            None => return SyscallResult::Error(SyscallError::ProcessNotFound),
        };

        let current_brk = process.memory.heap_start + process.memory.heap_size;
        let new_brk = if increment >= 0 {
            current_brk + increment as u64
        } else {
            current_brk.saturating_sub((-increment) as u64)
        };

        // Use brk implementation
        match self.sys_brk(&[new_brk], process_manager, current_pid) {
            SyscallResult::Success(_) => SyscallResult::Success(current_brk),
            SyscallResult::Error(e) => SyscallResult::Error(e),
        }
    }

    // Inter-process communication

    /// sys_pipe - Create a pipe
    fn sys_pipe(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let pipefd_ptr = args.get(0).copied().unwrap_or(0);

        if pipefd_ptr == 0 {
            return SyscallResult::Error(SyscallError::InvalidArgument);
        }

        let (read_fd, write_fd) = match process_manager.with_process_mut(current_pid, |p| {
            if p.file_descriptors.len() + 2 > 1024 {
                return None;
            }

            let (read_pipe_id, write_pipe_id) = process_manager.create_pipe().ok()?;

            let mut read_fd = None;
            let mut write_fd = None;
            let mut next_fd = 3;
            while next_fd <= 65535 {
                if !p.file_descriptors.contains_key(&next_fd) {
                    if read_fd.is_none() {
                        read_fd = Some(next_fd);
                    } else {
                        write_fd = Some(next_fd);
                        break;
                    }
                }
                next_fd += 1;
            }

            let read_fd = read_fd?;
            let write_fd = write_fd?;

            p.file_descriptors.insert(
                read_fd,
                super::FileDescriptor {
                    fd_type: super::FileDescriptorType::Pipe {
                        pipe_id: read_pipe_id,
                    },
                    flags: 0,
                    offset: 0,
                },
            );
            p.file_offsets.insert(read_fd, 0);

            p.file_descriptors.insert(
                write_fd,
                super::FileDescriptor {
                    fd_type: super::FileDescriptorType::Pipe {
                        pipe_id: write_pipe_id,
                    },
                    flags: 0,
                    offset: 0,
                },
            );
            p.file_offsets.insert(write_fd, 0);

            Some((read_fd, write_fd))
        }) {
            Some(Some(fds)) => fds,
            _ => return SyscallResult::Error(SyscallError::OutOfMemory),
        };

        let pipefds = [read_fd as i32, write_fd as i32];
        let mut buf = [0u8; 8];
        buf[0..4].copy_from_slice(&pipefds[0].to_le_bytes());
        buf[4..8].copy_from_slice(&pipefds[1].to_le_bytes());

        if self.copy_to_user(pipefd_ptr, &buf).is_ok() {
            SyscallResult::Success(0)
        } else {
            SyscallResult::Error(SyscallError::InvalidAddress)
        }
    }

    /// sys_signal - Set signal handler
    fn sys_signal(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let signal = args.get(0).copied().unwrap_or(0) as u32;
        let handler = args.get(1).copied().unwrap_or(0);

        // Get process and set signal handler
        if let Some(mut process) = process_manager.get_process(current_pid) {
            // Validate signal number (1-31 are standard signals)
            if signal == 0 || signal > 31 {
                return SyscallResult::Error(SyscallError::InvalidArgument);
            }

            // Store signal handler in process control block
            if !process.signal_handlers.contains_key(&signal) {
                process.signal_handlers = BTreeMap::new();
            }
            process.signal_handlers.insert(signal, handler);

            SyscallResult::Success(0)
        } else {
            SyscallResult::Error(SyscallError::ProcessNotFound)
        }
    }

    /// sys_kill - Send signal to process
    fn sys_kill(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let target_pid = args.get(0).copied().unwrap_or(0) as Pid;
        let signal = args.get(1).copied().unwrap_or(0) as u32;

        // Simple implementation: signal 9 (SIGKILL) terminates process
        if signal == 9 {
            match process_manager.terminate_process(target_pid, -1) {
                Ok(()) => SyscallResult::Success(0),
                Err(_) => SyscallResult::Error(SyscallError::ProcessNotFound),
            }
        } else if signal == 15 {
            // SIGTERM
            // Request process termination
            if let Some(mut target) = process_manager.get_process(target_pid) {
                // Check if process has a signal handler for SIGTERM
                if let Some(&_handler) = target.signal_handlers.get(&15) {
                    // Queue signal for delivery
                    target.pending_signals.push(signal);
                    if matches!(target.state, ProcessState::Sleeping) {
                        // Wake up sleeping process to handle signal
                        process_manager.unblock_process(target_pid).ok();
                    }
                    SyscallResult::Success(0)
                } else {
                    // Default action: terminate process
                    match process_manager.terminate_process(target_pid, 0) {
                        Ok(()) => SyscallResult::Success(0),
                        Err(_) => SyscallResult::Error(SyscallError::ProcessNotFound),
                    }
                }
            } else {
                SyscallResult::Error(SyscallError::ProcessNotFound)
            }
        } else if signal == 2 {
            // SIGINT
            // Interrupt signal (Ctrl+C)
            if let Some(mut target) = process_manager.get_process(target_pid) {
                if let Some(&_handler) = target.signal_handlers.get(&2) {
                    target.pending_signals.push(signal);
                    if matches!(target.state, ProcessState::Sleeping) {
                        process_manager.unblock_process(target_pid).ok();
                    }
                    SyscallResult::Success(0)
                } else {
                    // Default action: terminate
                    match process_manager.terminate_process(target_pid, 130) {
                        // 128 + signal number
                        Ok(()) => SyscallResult::Success(0),
                        Err(_) => SyscallResult::Error(SyscallError::ProcessNotFound),
                    }
                }
            } else {
                SyscallResult::Error(SyscallError::ProcessNotFound)
            }
        } else if signal == 19 {
            // SIGSTOP
            // Stop process
            match process_manager.block_process(target_pid) {
                Ok(()) => SyscallResult::Success(0),
                Err(_) => SyscallResult::Error(SyscallError::ProcessNotFound),
            }
        } else if signal == 18 {
            // SIGCONT
            // Continue process
            match process_manager.unblock_process(target_pid) {
                Ok(()) => SyscallResult::Success(0),
                Err(_) => SyscallResult::Error(SyscallError::ProcessNotFound),
            }
        } else {
            // For other signals, just queue them if handler exists
            if let Some(mut target) = process_manager.get_process(target_pid) {
                if target.signal_handlers.contains_key(&signal) {
                    target.pending_signals.push(signal);
                    SyscallResult::Success(0)
                } else {
                    // No handler, ignore signal
                    SyscallResult::Success(0)
                }
            } else {
                SyscallResult::Error(SyscallError::ProcessNotFound)
            }
        }
    }

    // System information

    /// sys_uname - Get system information
    fn sys_uname(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        use core::mem::size_of;

        let buf_ptr = args.get(0).copied().unwrap_or(0);

        if buf_ptr == 0 {
            return SyscallResult::Error(SyscallError::InvalidAddress);
        }

        // struct utsname definition (POSIX compatible)
        #[repr(C)]
        struct UtsName {
            sysname: [u8; 65],
            nodename: [u8; 65],
            release: [u8; 65],
            version: [u8; 65],
            machine: [u8; 65],
        }

        const UTSNAME_SIZE: usize = size_of::<UtsName>();

        // Create and populate utsname structure
        let mut utsname = UtsName {
            sysname: [0; 65],
            nodename: [0; 65],
            release: [0; 65],
            version: [0; 65],
            machine: [0; 65],
        };

        // Fill in system information — use the calling process's UTS namespace
        // so that processes with an unshared UTS namespace see their own hostname.
        let uts_ns = crate::namespace::get_nsproxy(_current_pid).uts;
        copy_str_to_buf(&mut utsname.sysname, &uts_ns.sysname);
        copy_str_to_buf(&mut utsname.nodename, &uts_ns.nodename);
        copy_str_to_buf(&mut utsname.release, &uts_ns.release);
        copy_str_to_buf(&mut utsname.version, &uts_ns.version);
        copy_str_to_buf(&mut utsname.machine, &uts_ns.machine);

        // Copy to user space
        let utsname_bytes =
            unsafe { core::slice::from_raw_parts(&utsname as *const _ as *const u8, UTSNAME_SIZE) };

        if self.copy_to_user(buf_ptr, utsname_bytes).is_ok() {
            SyscallResult::Success(0)
        } else {
            SyscallResult::Error(SyscallError::InvalidAddress)
        }
    }

    /// sys_gettime - Get current time
    fn sys_gettime(&self, _process_manager: &ProcessManager) -> SyscallResult {
        let current_time = super::get_system_time();
        SyscallResult::Success(current_time)
    }

    /// sys_settime - Set system time
    fn sys_settime(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let new_time = args.get(0).copied().unwrap_or(0);

        // Check for root/admin privileges
        if let Some(ctx) = crate::security::get_context(current_pid) {
            if !ctx.is_root() && !crate::security::check_permission(current_pid, "sys_time") {
                return SyscallResult::Error(SyscallError::PermissionDenied);
            }
        } else {
            return SyscallResult::Error(SyscallError::PermissionDenied);
        }

        // Set system time through time subsystem
        crate::time::set_system_time(new_time);
        SyscallResult::Success(0)
    }

    // Process control

    /// sys_setpriority - Set process priority
    fn sys_setpriority(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let target_pid = args.get(0).copied().unwrap_or(current_pid as u64) as Pid;
        let priority_value = args.get(1).copied().unwrap_or(0) as u8;

        // Convert priority value to Priority enum
        let new_priority = match priority_value {
            0 => Priority::RealTime,
            1 => Priority::High,
            2 => Priority::Normal,
            3 => Priority::Low,
            4 => Priority::Idle,
            _ => return SyscallResult::Error(SyscallError::InvalidArgument),
        };

        // Validate target process exists
        if process_manager.get_process(target_pid).is_none() {
            return SyscallResult::Error(SyscallError::ProcessNotFound);
        }

        // Check permissions - can only change own priority or need privileges for others
        if target_pid != current_pid {
            if let Some(ctx) = crate::security::get_context(current_pid) {
                if !ctx.is_root() && !crate::security::check_permission(current_pid, "sys_nice") {
                    return SyscallResult::Error(SyscallError::PermissionDenied);
                }
            } else {
                return SyscallResult::Error(SyscallError::PermissionDenied);
            }
        }

        // Check privilege requirements for high priorities
        match new_priority {
            Priority::RealTime => {
                if !crate::security::check_permission(current_pid, "sys_admin") {
                    return SyscallResult::Error(SyscallError::PermissionDenied);
                }
            }
            Priority::High => {
                if let Some(ctx) = crate::security::get_context(current_pid) {
                    if ctx.level == crate::security::SecurityLevel::User && !ctx.is_root() {
                        return SyscallResult::Error(SyscallError::PermissionDenied);
                    }
                }
            }
            _ => {} // Normal, Low, Idle available to all
        }

        // Update priority in process control block and scheduler
        {
            let mut processes = process_manager.processes.write();
            if let Some(pcb) = processes.get_mut(&target_pid) {
                pcb.priority = new_priority;
            } else {
                return SyscallResult::Error(SyscallError::ProcessNotFound);
            }
        }

        // Notify scheduler of priority change using process/scheduler module
        match super::scheduler::set_process_priority(target_pid, new_priority) {
            Ok(()) => SyscallResult::Success(0),
            Err(_) => SyscallResult::Error(SyscallError::InvalidArgument),
        }
    }

    /// sys_getpriority - Get process priority
    fn sys_getpriority(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let target_pid = args.get(0).copied().unwrap_or(current_pid as u64) as Pid;

        match process_manager.get_process(target_pid) {
            Some(pcb) => SyscallResult::Success(pcb.priority as u64),
            None => SyscallResult::Error(SyscallError::ProcessNotFound),
        }
    }

    // Extended system calls for Linux application support

    /// sys_clone - Create thread/process (flexible fork)
    fn sys_clone(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let flags = args.first().copied().unwrap_or(0);
        let stack = args.get(1).copied().unwrap_or(0) as *mut u8;
        let parent_tid = args.get(2).copied().unwrap_or(0) as *mut i32;
        let child_tid = args.get(3).copied().unwrap_or(0) as *mut i32;
        let tls = args.get(4).copied().unwrap_or(0);
        linux_result_i32(crate::linux_compat::thread_ops::clone(
            flags, stack, parent_tid, child_tid, tls,
        ))
    }

    /// sys_execve - Execute program via linux_compat ELF loader
    fn sys_execve(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let filename = args.first().copied().unwrap_or(0) as *const u8;
        let argv = args.get(1).copied().unwrap_or(0) as *const *const u8;
        let envp = args.get(2).copied().unwrap_or(0) as *const *const u8;
        linux_result_i32(crate::linux_compat::process_ops::execve(
            filename, argv, envp,
        ))
    }

    /// sys_waitid - Wait for process state change
    fn sys_waitid(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        const P_ALL: i32 = 0;
        const P_PID: i32 = 1;
        const P_PGID: i32 = 2;

        let idtype = args.first().copied().unwrap_or(0) as i32;
        let id = args.get(1).copied().unwrap_or(0) as i32;
        let options = args.get(3).copied().unwrap_or(0) as i32;

        let pid = match idtype {
            P_ALL => -1,
            P_PID => id,
            P_PGID => -id,
            _ => return linux_result_i32(Err(crate::linux_compat::LinuxError::EINVAL)),
        };

        let mut status = 0i32;
        linux_result_i32(crate::linux_compat::process_ops::wait4(
            pid,
            &mut status,
            options,
            core::ptr::null_mut(),
        ))
    }

    /// sys_openat - Open file relative to directory fd
    fn sys_openat(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let dirfd = args.first().copied().unwrap_or(0) as i32;
        let pathname = args.get(1).copied().unwrap_or(0) as *const u8;
        let flags = args.get(2).copied().unwrap_or(0) as i32;
        let mode = args.get(3).copied().unwrap_or(0) as u32;
        linux_result_i32(crate::linux_compat::file_ops::openat(
            dirfd, pathname, flags, mode,
        ))
    }

    /// sys_mkdirat - Create directory at path relative to fd
    fn sys_mkdirat(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let dirfd = args.first().copied().unwrap_or(0) as i32;
        let path = args.get(1).copied().unwrap_or(0) as *const u8;
        let mode = args.get(2).copied().unwrap_or(0) as u32;
        linux_result_i32(crate::linux_compat::file_ops::mkdirat(dirfd, path, mode))
    }

    /// sys_unlinkat - Delete file/directory at path relative to fd
    fn sys_unlinkat(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let dirfd = args.first().copied().unwrap_or(0) as i32;
        let path = args.get(1).copied().unwrap_or(0) as *const u8;
        let flags = args.get(2).copied().unwrap_or(0) as i32;
        linux_result_i32(crate::linux_compat::file_ops::unlinkat(dirfd, path, flags))
    }

    /// sys_fchmod - Change file permissions
    fn sys_fchmod(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let fd = args.first().copied().unwrap_or(0) as i32;
        let mode = args.get(1).copied().unwrap_or(0) as u32;
        linux_result_i32(crate::linux_compat::file_ops::fchmod(fd, mode))
    }

    /// sys_mprotect - Change memory protection
    fn sys_mprotect(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let addr = args.first().copied().unwrap_or(0) as *mut u8;
        let len = args.get(1).copied().unwrap_or(0) as usize;
        let prot = args.get(2).copied().unwrap_or(0) as i32;
        linux_result_i32(crate::linux_compat::memory_ops::mprotect(addr, len, prot))
    }

    /// sys_madvise - Give advice about memory usage
    fn sys_madvise(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let addr = args.first().copied().unwrap_or(0) as *mut u8;
        let len = args.get(1).copied().unwrap_or(0) as usize;
        let advice = args.get(2).copied().unwrap_or(0) as i32;
        linux_result_i32(crate::linux_compat::memory_ops::madvise(addr, len, advice))
    }

    /// sys_futex - Fast userspace mutex
    fn sys_futex(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let uaddr = args.first().copied().unwrap_or(0) as *mut i32;
        let op = args.get(1).copied().unwrap_or(0) as i32;
        let val = args.get(2).copied().unwrap_or(0) as i32;
        let timeout = args.get(3).copied().unwrap_or(0) as *const crate::linux_compat::TimeSpec;
        let uaddr2 = args.get(4).copied().unwrap_or(0) as *mut i32;
        let val3 = args.get(5).copied().unwrap_or(0) as i32;
        linux_result_i32(crate::linux_compat::thread_ops::futex(
            uaddr, op, val, timeout, uaddr2, val3,
        ))
    }

    /// sys_socket - Create socket
    fn sys_socket(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let domain = args.first().copied().unwrap_or(0) as i32;
        let sock_type = args.get(1).copied().unwrap_or(0) as i32;
        let protocol = args.get(2).copied().unwrap_or(0) as i32;
        linux_result_i32(crate::linux_compat::socket_ops::socket(
            domain, sock_type, protocol,
        ))
    }

    /// sys_bind - Bind socket to address
    fn sys_bind(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let sockfd = args.first().copied().unwrap_or(0) as i32;
        let addr = args.get(1).copied().unwrap_or(0) as *const crate::linux_compat::SockAddr;
        let addrlen = args.get(2).copied().unwrap_or(0) as u32;
        linux_result_i32(crate::linux_compat::socket_ops::bind(sockfd, addr, addrlen))
    }

    /// sys_connect - Connect socket
    fn sys_connect(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let sockfd = args.first().copied().unwrap_or(0) as i32;
        let addr = args.get(1).copied().unwrap_or(0) as *const crate::linux_compat::SockAddr;
        let addrlen = args.get(2).copied().unwrap_or(0) as u32;
        linux_result_i32(crate::linux_compat::socket_ops::connect(
            sockfd, addr, addrlen,
        ))
    }

    /// sys_listen - Listen on socket
    fn sys_listen(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let sockfd = args.first().copied().unwrap_or(0) as i32;
        let backlog = args.get(1).copied().unwrap_or(0) as i32;
        linux_result_i32(crate::linux_compat::socket_ops::listen(sockfd, backlog))
    }

    /// sys_accept - Accept socket connection
    fn sys_accept(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let sockfd = args.first().copied().unwrap_or(0) as i32;
        let addr = args.get(1).copied().unwrap_or(0) as *mut crate::linux_compat::SockAddr;
        let addrlen = args.get(2).copied().unwrap_or(0) as *mut u32;
        linux_result_i32(crate::linux_compat::socket_ops::accept(
            sockfd, addr, addrlen,
        ))
    }

    /// sys_set_tid_address - Set thread ID address
    fn sys_set_tid_address(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let tidptr = args.first().copied().unwrap_or(0) as *mut i32;
        let tid = crate::linux_compat::thread_ops::set_tid_address(tidptr);
        SyscallResult::Success(tid as u64)
    }

    /// sys_ioctl - Device-specific I/O control
    fn sys_ioctl(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let fd = args.first().copied().unwrap_or(0) as i32;
        let request = args.get(1).copied().unwrap_or(0);
        let argp = args.get(2).copied().unwrap_or(0);
        linux_result_i32(crate::linux_compat::ioctl_ops::ioctl(fd, request, argp))
    }

    /// sys_fcntl - File control operations
    fn sys_fcntl(
        &self,
        args: &[u64],
        _process_manager: &ProcessManager,
        _current_pid: Pid,
    ) -> SyscallResult {
        let fd = args.first().copied().unwrap_or(0) as i32;
        let cmd = args.get(1).copied().unwrap_or(0) as i32;
        let arg = args.get(2).copied().unwrap_or(0);
        linux_result_i32(crate::linux_compat::ioctl_ops::fcntl(fd, cmd, arg))
    }

    // Package management syscalls (experimental)

    fn sys_pkg_install(&self, args: &[u64]) -> SyscallResult {
        match crate::package::handle_package_syscall(
            200,
            args[0] as usize,
            args[1] as usize,
            args[2] as usize,
            args[3] as usize,
        ) {
            Ok(val) => SyscallResult::Success(val as u64),
            Err(_) => SyscallResult::Error(SyscallError::PermissionDenied),
        }
    }

    fn sys_pkg_remove(&self, args: &[u64]) -> SyscallResult {
        match crate::package::handle_package_syscall(
            201,
            args[0] as usize,
            args[1] as usize,
            args[2] as usize,
            args[3] as usize,
        ) {
            Ok(val) => SyscallResult::Success(val as u64),
            Err(_) => SyscallResult::Error(SyscallError::PermissionDenied),
        }
    }

    fn sys_pkg_search(&self, args: &[u64]) -> SyscallResult {
        match crate::package::handle_package_syscall(
            202,
            args[0] as usize,
            args[1] as usize,
            args[2] as usize,
            args[3] as usize,
        ) {
            Ok(val) => SyscallResult::Success(val as u64),
            Err(_) => SyscallResult::Error(SyscallError::NotFound),
        }
    }

    fn sys_pkg_info(&self, args: &[u64]) -> SyscallResult {
        match crate::package::handle_package_syscall(
            203,
            args[0] as usize,
            args[1] as usize,
            args[2] as usize,
            args[3] as usize,
        ) {
            Ok(val) => SyscallResult::Success(val as u64),
            Err(_) => SyscallResult::Error(SyscallError::NotFound),
        }
    }

    fn sys_pkg_list(&self, args: &[u64]) -> SyscallResult {
        match crate::package::handle_package_syscall(
            204,
            args[0] as usize,
            args[1] as usize,
            args[2] as usize,
            args[3] as usize,
        ) {
            Ok(val) => SyscallResult::Success(val as u64),
            Err(_) => SyscallResult::Error(SyscallError::OperationNotSupported),
        }
    }

    fn sys_pkg_update(&self, args: &[u64]) -> SyscallResult {
        match crate::package::handle_package_syscall(
            205,
            args[0] as usize,
            args[1] as usize,
            args[2] as usize,
            args[3] as usize,
        ) {
            Ok(val) => SyscallResult::Success(val as u64),
            Err(_) => SyscallResult::Error(SyscallError::OperationNotSupported),
        }
    }

    fn sys_pkg_upgrade(&self, args: &[u64]) -> SyscallResult {
        match crate::package::handle_package_syscall(
            206,
            args[0] as usize,
            args[1] as usize,
            args[2] as usize,
            args[3] as usize,
        ) {
            Ok(val) => SyscallResult::Success(val as u64),
            Err(_) => SyscallResult::Error(SyscallError::PermissionDenied),
        }
    }

    /// sys_getuid - Get real user ID
    fn sys_getuid(&self, process_manager: &ProcessManager, current_pid: Pid) -> SyscallResult {
        match process_manager.get_process(current_pid) {
            Some(pcb) => SyscallResult::Success(pcb.uid as u64),
            None => SyscallResult::Success(0),
        }
    }

    /// sys_getgid - Get real group ID
    fn sys_getgid(&self, process_manager: &ProcessManager, current_pid: Pid) -> SyscallResult {
        match process_manager.get_process(current_pid) {
            Some(pcb) => SyscallResult::Success(pcb.gid as u64),
            None => SyscallResult::Success(0),
        }
    }

    /// sys_setuid - Set real user ID (requires privilege)
    fn sys_setuid(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let new_uid = args.first().copied().unwrap_or(0) as u32;
        let allowed = process_manager
            .get_process(current_pid)
            .map(|p| p.uid == 0 || p.uid == new_uid)
            .unwrap_or(false);
        if !allowed {
            return SyscallResult::Success((-1i64) as u64); // EPERM
        }
        let _ = process_manager.with_process_mut(current_pid, |p| {
            p.uid = new_uid;
        });
        SyscallResult::Success(0)
    }

    /// sys_setgid - Set real group ID (requires privilege)
    fn sys_setgid(
        &self,
        args: &[u64],
        process_manager: &ProcessManager,
        current_pid: Pid,
    ) -> SyscallResult {
        let new_gid = args.first().copied().unwrap_or(0) as u32;
        let allowed = process_manager
            .get_process(current_pid)
            .map(|p| p.uid == 0 || p.gid == new_gid)
            .unwrap_or(false);
        if !allowed {
            return SyscallResult::Success((-1i64) as u64); // EPERM
        }
        let _ = process_manager.with_process_mut(current_pid, |p| {
            p.gid = new_gid;
        });
        SyscallResult::Success(0)
    }

    /// sys_gettid - Get thread ID (same as PID in our single-threaded model)
    fn sys_gettid(&self, process_manager: &ProcessManager, current_pid: Pid) -> SyscallResult {
        match process_manager.get_process(current_pid) {
            Some(_) => SyscallResult::Success(current_pid as u64),
            None => SyscallResult::Error(SyscallError::ProcessNotFound),
        }
    }

    /// Get system call statistics
    pub fn get_stats(&self) -> (u64, &[u64; 64]) {
        (self.total_syscalls, &self.syscall_count)
    }

    // Helper methods for user-space memory operations

    /// Copy string from user space with full validation and security checks
    fn copy_string_from_user(&self, user_ptr: u64) -> Result<String, SyscallError> {
        use crate::memory::user_space::UserSpaceMemory;

        // Use production-ready implementation with:
        // - User space pointer validation
        // - Page table walking with permission checks
        // - Safe byte-by-byte copying with fault handling
        // - Null terminator detection
        // - UTF-8 validation
        const PATH_MAX: usize = 4096;
        UserSpaceMemory::copy_string_from_user(user_ptr, PATH_MAX)
            .map_err(|_| SyscallError::InvalidAddress)
    }

    /// Copy data from user space
    fn copy_from_user(&self, user_ptr: u64, buffer: &mut [u8]) -> Result<(), SyscallError> {
        use crate::memory::user_space::UserSpaceMemory;

        UserSpaceMemory::copy_from_user(user_ptr, buffer).map_err(|_| SyscallError::InvalidAddress)
    }

    /// Copy data to user space
    fn copy_to_user(&self, user_ptr: u64, buffer: &[u8]) -> Result<(), SyscallError> {
        use crate::memory::user_space::UserSpaceMemory;

        UserSpaceMemory::copy_to_user(user_ptr, buffer).map_err(|_| SyscallError::InvalidAddress)
    }
}

/// Helper function to copy string to fixed-size buffer
fn copy_str_to_buf(dest: &mut [u8], src: &str) {
    let bytes = src.as_bytes();
    let copy_len = core::cmp::min(bytes.len(), dest.len() - 1);
    dest[..copy_len].copy_from_slice(&bytes[..copy_len]);
    dest[copy_len] = 0; // Null terminator
}

/// System call handler entry point (called from assembly)
#[no_mangle]
pub extern "C" fn syscall_handler(
    syscall_number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> u64 {
    let args = [arg1, arg2, arg3, arg4, arg5, arg6];
    let process_manager = super::get_process_manager();

    match process_manager.handle_syscall(syscall_number, &args) {
        Ok(result) => result,
        Err(_) => SyscallError::InvalidSyscall as u64,
    }
}
