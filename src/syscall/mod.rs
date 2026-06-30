//! System Call Interface for RustOS
//!
//! This module implements the system call interface that allows user-space
//! programs to request services from the kernel. It includes:
//! - System call dispatch mechanism
//! - User/kernel mode switching
//! - Parameter validation and copying
//! - Security checks and capabilities

use crate::scheduler::Pid;
use alloc::string::String;
use alloc::{vec, vec::Vec};
use core::arch::asm;

pub mod abi;
mod linux;

/// Linux x86_64 syscall numbers (see `syscall/linux.rs`).
pub use linux::SyscallNumber;

/// System call result type
pub type SyscallResult = Result<u64, SyscallError>;

/// System call error codes (POSIX-compatible)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum SyscallError {
    /// Invalid system call number
    InvalidSyscall = 1,
    /// Invalid argument (EINVAL)
    InvalidArgument = 22,
    /// Permission denied (EACCES)
    PermissionDenied = 13,
    /// No such file or directory (ENOENT)
    NotFound = 2,
    /// File exists (EEXIST)
    AlreadyExists = 17,
    /// Operation not supported (ENOSYS)
    NotSupported = 38,
    /// Out of memory (ENOMEM)
    OutOfMemory = 12,
    /// I/O error (EIO)
    IoError = 5,
    /// Operation would block (EAGAIN)
    WouldBlock = 11,
    /// Operation interrupted (EINTR)
    Interrupted = 4,
    /// Bad file descriptor (EBADF)
    BadFileDescriptor = 9,
    /// No child processes (ECHILD)
    NoChild = 10,
    /// Resource busy (EBUSY)
    Busy = 16,
    /// Cross-device link (EXDEV)
    CrossDevice = 18,
    /// Directory not empty (ENOTEMPTY)
    DirectoryNotEmpty = 39,
    /// Read-only file system (EROFS)
    ReadOnly = 30,
    /// Too many open files (EMFILE)
    TooManyOpenFiles = 24,
    /// File too large (EFBIG)
    FileTooLarge = 27,
    /// No space left on device (ENOSPC)
    NoSpace = 28,
    /// Is a directory (EISDIR)
    IsDirectory = 21,
    /// Not a directory (ENOTDIR)
    NotDirectory = 20,
    /// Operation not permitted (EPERM)
    NotPermitted = 32,
    /// Invalid address (EFAULT)
    InvalidAddress = 14,
    /// Internal error
    InternalError = 255,
}

/// System call context passed to handlers
#[derive(Debug)]
pub struct SyscallContext {
    /// Process ID making the system call
    pub pid: Pid,
    /// System call number
    pub syscall_num: SyscallNumber,
    /// System call arguments (up to 6 arguments)
    pub args: [u64; 6],
    /// User stack pointer
    pub user_sp: u64,
    /// User instruction pointer
    pub user_ip: u64,
    /// User privilege level (0 = kernel, 3 = user)
    pub privilege_level: u8,
    /// Current working directory
    pub cwd: Option<String>,
}

/// Security validation utilities
pub struct SecurityValidator;

impl SecurityValidator {
    /// Validate user pointer and length
    pub fn validate_user_ptr(ptr: u64, len: u64, write_access: bool) -> Result<(), SyscallError> {
        use crate::memory::user_space::UserSpaceMemory;

        UserSpaceMemory::validate_user_ptr(ptr, len, write_access)
    }

    /// Validate file descriptor
    pub fn validate_fd(fd: i32) -> Result<(), SyscallError> {
        if fd < 0 {
            return Err(SyscallError::BadFileDescriptor);
        }
        Ok(())
    }

    /// Validate process ID
    pub fn validate_pid(pid: Pid) -> Result<(), SyscallError> {
        if pid == 0 {
            return Err(SyscallError::InvalidArgument);
        }
        Ok(())
    }

    /// Copy string from user space
    pub fn copy_string_from_user(ptr: u64, max_len: usize) -> Result<String, SyscallError> {
        use crate::memory::user_space::UserSpaceMemory;

        if ptr == 0 {
            return Err(SyscallError::InvalidArgument);
        }

        Self::validate_user_ptr(ptr, max_len as u64, false)?;

        // Use production user space memory implementation
        UserSpaceMemory::copy_string_from_user(ptr, max_len)
    }

    /// Copy data from user space
    pub fn copy_from_user(ptr: u64, len: usize) -> Result<Vec<u8>, SyscallError> {
        use crate::memory::user_space::UserSpaceMemory;

        if len == 0 {
            return Ok(Vec::new());
        }

        let mut buffer = vec![0u8; len];
        UserSpaceMemory::copy_from_user(ptr, &mut buffer)?;
        Ok(buffer)
    }

    /// Copy data to user space
    pub fn copy_to_user(ptr: u64, data: &[u8]) -> Result<(), SyscallError> {
        use crate::memory::user_space::UserSpaceMemory;

        UserSpaceMemory::copy_to_user(ptr, data)
    }
}

/// System call statistics
#[derive(Debug, Clone)]
pub struct SyscallStats {
    pub total_calls: u64,
    pub successful_calls: u64,
    pub failed_calls: u64,
    pub calls_by_type: [u64; 64], // Track first 64 syscall types
}

impl Default for SyscallStats {
    fn default() -> Self {
        Self {
            total_calls: 0,
            successful_calls: 0,
            failed_calls: 0,
            calls_by_type: [0; 64],
        }
    }
}

static mut SYSCALL_STATS: SyscallStats = SyscallStats {
    total_calls: 0,
    successful_calls: 0,
    failed_calls: 0,
    calls_by_type: [0; 64],
};

/// Initialize the system call interface
pub fn init() -> Result<(), &'static str> {
    // Set up system call interrupt handler (interrupt 0x80)
    setup_syscall_interrupt();

    // Production: syscall interface initialized
    Ok(())
}

/// Set up the system call interrupt handler.
///
/// The live INT 0x80 path is `crate::syscall_handler::syscall_0x80_handler`
/// (a naked assembly stub that correctly preserves user registers). The
/// real syscall entry point is installed by the main interrupt subsystem
/// during boot, so this function does not need to register a separate IDT.
fn setup_syscall_interrupt() {
    // The syscall gate (INT 0x80) is installed by the interrupt subsystem
    // in `crate::interrupts::init()` which loads the real IDT. No local
    // IDT construction is needed here.
}

/// Dispatch a system call to the appropriate handler
pub fn dispatch_syscall(context: &SyscallContext) -> SyscallResult {
    // Validate privilege level for the syscall
    if let Err(_error_msg) =
        crate::security::validate_syscall_privilege(context.syscall_num as u64, context.pid)
    {
        // Log security violation
        return Err(SyscallError::PermissionDenied);
    }

    // Validate process isolation if needed
    if context.syscall_num == SyscallNumber::Kill {
        let target_pid = context.args[0] as Pid;
        if let Err(_) =
            crate::security::validate_process_isolation(context.pid, target_pid, "signal")
        {
            return Err(SyscallError::PermissionDenied);
        }
    }

    if context.syscall_num == SyscallNumber::Invalid {
        return Err(SyscallError::InvalidSyscall);
    }

    crate::linux_integration::route_syscall(context.syscall_num as u64, &context.args)
        .map_err(linux_error_to_syscall_error)
}

fn linux_error_to_syscall_error(error: crate::linux_compat::LinuxError) -> SyscallError {
    match error {
        crate::linux_compat::LinuxError::EPERM => SyscallError::NotPermitted,
        crate::linux_compat::LinuxError::ENOENT => SyscallError::NotFound,
        crate::linux_compat::LinuxError::EINTR => SyscallError::Interrupted,
        crate::linux_compat::LinuxError::EIO => SyscallError::IoError,
        crate::linux_compat::LinuxError::EAGAIN => SyscallError::WouldBlock,
        crate::linux_compat::LinuxError::ENOMEM => SyscallError::OutOfMemory,
        crate::linux_compat::LinuxError::EACCES => SyscallError::PermissionDenied,
        crate::linux_compat::LinuxError::EFAULT => SyscallError::InvalidAddress,
        crate::linux_compat::LinuxError::EBUSY => SyscallError::Busy,
        crate::linux_compat::LinuxError::EEXIST => SyscallError::AlreadyExists,
        crate::linux_compat::LinuxError::ENOTDIR => SyscallError::NotDirectory,
        crate::linux_compat::LinuxError::EISDIR => SyscallError::IsDirectory,
        crate::linux_compat::LinuxError::EINVAL => SyscallError::InvalidArgument,
        crate::linux_compat::LinuxError::EMFILE => SyscallError::TooManyOpenFiles,
        crate::linux_compat::LinuxError::EFBIG => SyscallError::FileTooLarge,
        crate::linux_compat::LinuxError::ENOSPC => SyscallError::NoSpace,
        crate::linux_compat::LinuxError::EXDEV => SyscallError::CrossDevice,
        crate::linux_compat::LinuxError::EROFS => SyscallError::ReadOnly,
        crate::linux_compat::LinuxError::ENOTEMPTY => SyscallError::DirectoryNotEmpty,
        crate::linux_compat::LinuxError::ENOSYS => SyscallError::NotSupported,
        _ => SyscallError::InvalidArgument,
    }
}

// System call implementations

/// Exit the current process
fn sys_exit(exit_code: i32) -> SyscallResult {
    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    // Terminate the current process
    match process_manager.terminate_process(current_pid, exit_code) {
        Ok(()) => {
            // Select the next process to run.
            crate::scheduler::schedule();

            // ponytail: `schedule()` only *selects* the next PID; it does not
            // perform the register-level context switch. Returning here would keep
            // executing in the now-Zombie process. There is no exposed "switch away
            // and never return" entry point to call from here, so park the dead
            // task: halt with interrupts enabled and let the timer interrupt drive
            // the actual context switch to the next task. This loop never returns,
            // which is the correct exit(2) behaviour.
            loop {
                unsafe {
                    asm!("sti; hlt", options(nomem, nostack, preserves_flags));
                }
            }
        }
        Err(_) => Err(SyscallError::InvalidArgument),
    }
}

/// Fork the current process
fn sys_fork() -> SyscallResult {
    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    // Validate current process exists
    if current_pid == 0 {
        return Err(SyscallError::InvalidSyscall);
    }

    // Verify parent process exists
    if process_manager.get_process(current_pid).is_none() {
        return Err(SyscallError::InvalidSyscall);
    }

    // Use integration manager to fork process with copy-on-write
    use crate::process::integration::get_integration_manager;
    let integration_manager = get_integration_manager();

    match integration_manager.fork_process(current_pid) {
        Ok(child_pid) => {
            // fork_process already sets child.context.rax = 0 so the child
            // sees fork() returning 0 when it is first scheduled.  The parent
            // (the caller) sees child_pid as the return value.
            Ok(child_pid as u64)
        }
        Err(_) => Err(SyscallError::OutOfMemory),
    }
}

/// Execute a new program in the current process
fn sys_exec(program_path_ptr: u64, argv_ptr: u64) -> SyscallResult {
    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    // Validate current process exists
    if current_pid == 0 {
        return Err(SyscallError::InvalidSyscall);
    }

    // Validate program path pointer
    if program_path_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    // Copy program path from user space
    let program_path = match SecurityValidator::copy_string_from_user(program_path_ptr, 4096) {
        Ok(path) => path,
        Err(_) => return Err(SyscallError::InvalidArgument),
    };

    // Copy argv from user space.  argv is a NULL-terminated array of char*
    // pointers.  Each pointer points to a NUL-terminated string.
    let argv: Vec<String> = if argv_ptr != 0 {
        match copy_argv_from_user(argv_ptr) {
            Ok(args) => args,
            Err(_) => return Err(SyscallError::InvalidArgument),
        }
    } else {
        vec![program_path.clone()]
    };

    // Load program from filesystem
    let program_data = match load_program_from_filesystem(&program_path) {
        Ok(data) => data,
        Err(_) => return Err(SyscallError::NotFound),
    };

    // Validate ELF format and security
    if let Err(_) = validate_elf_program(&program_data) {
        return Err(SyscallError::InvalidArgument);
    }

    // Use integration manager to execute program
    use crate::process::integration::get_integration_manager;
    let integration_manager = get_integration_manager();

    match integration_manager.exec_process(current_pid, &program_path, &program_data, &argv) {
        Ok(()) => {
            // exec() does not return on success - the process image is replaced
            // This should not be reached in normal execution
            Ok(0)
        }
        Err(_) => Err(SyscallError::InvalidArgument),
    }
}

/// Copy a NULL-terminated array of string pointers from user space.
/// Each entry is a pointer to a NUL-terminated C string.
fn copy_argv_from_user(argv_ptr: u64) -> Result<Vec<String>, SyscallError> {
    use crate::memory::user_space::UserSpaceMemory;

    let mut result = Vec::new();
    let mut offset = 0u64;

    loop {
        // Read one pointer (8 bytes on x86_64) from the argv array
        let ptr_addr = argv_ptr + offset;
        match SecurityValidator::validate_user_ptr(ptr_addr, 8, false) {
            Ok(()) => {}
            Err(_) => return Err(SyscallError::InvalidArgument),
        }

        let mut ptr_bytes = [0u8; 8];
        match UserSpaceMemory::copy_from_user(ptr_addr, &mut ptr_bytes) {
            Ok(()) => {}
            Err(_) => return Err(SyscallError::InvalidArgument),
        }

        let str_ptr = u64::from_le_bytes(ptr_bytes);
        if str_ptr == 0 {
            break; // NULL terminator
        }

        let s = SecurityValidator::copy_string_from_user(str_ptr, 4096)?;
        result.push(s);
        offset += 8;
    }

    Ok(result)
}

/// Load program from filesystem
fn load_program_from_filesystem(path: &str) -> Result<Vec<u8>, &'static str> {
    // Get file metadata first to determine size
    let metadata = match crate::fs::vfs().stat(path) {
        Ok(meta) => meta,
        Err(_) => return Err("Failed to get file metadata"),
    };

    // Open file through VFS
    match crate::fs::vfs().open(path, crate::fs::OpenFlags::read_only()) {
        Ok(fd) => {
            // Read entire file
            let file_size = metadata.size as usize;
            let mut buffer = vec![0u8; file_size];

            match crate::fs::vfs().read(fd, &mut buffer) {
                Ok(bytes_read) => {
                    // Close file
                    let _ = crate::fs::vfs().close(fd);
                    if bytes_read == file_size {
                        Ok(buffer)
                    } else {
                        buffer.truncate(bytes_read);
                        Ok(buffer)
                    }
                }
                Err(_) => {
                    let _ = crate::fs::vfs().close(fd);
                    Err("Failed to read program file")
                }
            }
        }
        Err(_) => Err("Failed to open program file"),
    }
}

/// Validate ELF program format and security
fn validate_elf_program(program_data: &[u8]) -> Result<(), &'static str> {
    // Check minimum size for ELF header
    if program_data.len() < 64 {
        return Err("Program too small to be valid ELF");
    }

    // Check ELF magic number
    if &program_data[0..4] != b"\x7FELF" {
        return Err("Invalid ELF magic number");
    }

    // Check ELF class (32-bit or 64-bit)
    let elf_class = program_data[4];
    if elf_class != 1 && elf_class != 2 {
        return Err("Invalid ELF class");
    }

    // Check data encoding (little-endian or big-endian)
    let data_encoding = program_data[5];
    if data_encoding != 1 && data_encoding != 2 {
        return Err("Invalid ELF data encoding");
    }

    // Check ELF version
    let elf_version = program_data[6];
    if elf_version != 1 {
        return Err("Unsupported ELF version");
    }

    // Check file type (executable)
    let file_type = u16::from_le_bytes([program_data[16], program_data[17]]);
    if file_type != 2 {
        return Err("ELF file is not executable");
    }

    // Check machine architecture (x86_64)
    let machine = u16::from_le_bytes([program_data[18], program_data[19]]);
    if machine != 0x3E {
        return Err("ELF file is not for x86_64 architecture");
    }

    // Basic security checks
    // Check entry point is in valid range
    let entry_point = if elf_class == 2 {
        // 64-bit ELF
        u64::from_le_bytes([
            program_data[24],
            program_data[25],
            program_data[26],
            program_data[27],
            program_data[28],
            program_data[29],
            program_data[30],
            program_data[31],
        ])
    } else {
        // 32-bit ELF
        u32::from_le_bytes([
            program_data[24],
            program_data[25],
            program_data[26],
            program_data[27],
        ]) as u64
    };

    // Validate entry point is in user space
    if entry_point < 0x400000 || entry_point >= 0x800000000000 {
        return Err("Invalid entry point address");
    }

    Ok(())
}

/// Get current process ID
fn sys_getpid() -> SyscallResult {
    let current_pid = get_current_pid();

    // Validate that we have a valid process ID
    if current_pid == 0 {
        // This should not happen in normal user-space system calls
        // Return error if called from invalid context
        Err(SyscallError::InvalidSyscall)
    } else {
        Ok(current_pid as u64)
    }
}

/// Get parent process ID
fn sys_getppid() -> SyscallResult {
    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    // Validate that we have a valid process ID
    if current_pid == 0 {
        return Err(SyscallError::InvalidSyscall);
    }

    // Get parent PID from process control block
    match process_manager.get_process(current_pid) {
        Some(process) => {
            match process.parent_pid {
                Some(ppid) => Ok(ppid as u64),
                None => Ok(0), // No parent (init process or kernel process)
            }
        }
        None => Err(SyscallError::InvalidSyscall),
    }
}

/// SchedYield - Yield CPU
fn sys_yield() -> SyscallResult {
    crate::scheduler::yield_cpu();
    Ok(0)
}

/// Send signal to process with enhanced privilege checking
fn sys_kill(pid: Pid, signal: i32) -> SyscallResult {
    SecurityValidator::validate_pid(pid)?;

    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    if process_manager.get_process(pid).is_none() {
        return Err(SyscallError::NotFound);
    }

    if !crate::security::check_permission(current_pid, "kill") {
        return Err(SyscallError::PermissionDenied);
    }

    match signal {
        9 => {
            if pid == current_pid {
                return Err(SyscallError::InvalidArgument);
            }

            if let Some(current_ctx) = crate::security::get_context(current_pid) {
                if let Some(target_ctx) = crate::security::get_context(pid) {
                    if !current_ctx.is_root() && current_ctx.uid != target_ctx.uid {
                        return Err(SyscallError::PermissionDenied);
                    }
                    if target_ctx.level < current_ctx.level {
                        return Err(SyscallError::PermissionDenied);
                    }
                }
            }

            match process_manager.terminate_process(pid, -9) {
                Ok(()) => Ok(0),
                Err(_) => Err(SyscallError::NotPermitted),
            }
        }
        0 => {
            if let Some(current_ctx) = crate::security::get_context(current_pid) {
                if let Some(target_ctx) = crate::security::get_context(pid) {
                    if !current_ctx.is_root() && current_ctx.uid != target_ctx.uid {
                        return Err(SyscallError::PermissionDenied);
                    }
                }
            }
            Ok(0)
        }
        _ => {
            use crate::process::ipc::{self, Signal};
            let signal = match signal {
                1 => Signal::SIGHUP,
                2 => Signal::SIGINT,
                3 => Signal::SIGQUIT,
                4 => Signal::SIGILL,
                5 => Signal::SIGTRAP,
                6 => Signal::SIGABRT,
                7 => Signal::SIGBUS,
                8 => Signal::SIGFPE,
                10 => Signal::SIGUSR1,
                11 => Signal::SIGSEGV,
                12 => Signal::SIGUSR2,
                13 => Signal::SIGPIPE,
                14 => Signal::SIGALRM,
                15 => Signal::SIGTERM,
                17 => Signal::SIGCHLD,
                18 => Signal::SIGCONT,
                19 => Signal::SIGSTOP,
                20 => Signal::SIGTSTP,
                _ => return Err(SyscallError::InvalidArgument),
            };

            match signal {
                Signal::SIGSTOP => {
                    process_manager.with_process_mut(pid, |pcb| {
                        pcb.set_state(crate::process::ProcessState::Blocked);
                    });
                }
                Signal::SIGCONT => {
                    process_manager.with_process_mut(pid, |pcb| {
                        if pcb.state == crate::process::ProcessState::Blocked {
                            pcb.set_state(crate::process::ProcessState::Ready);
                        }
                    });
                }
                Signal::SIGTERM | Signal::SIGINT | Signal::SIGHUP | Signal::SIGQUIT => {
                    return match process_manager.terminate_process(pid, signal as i32) {
                        Ok(()) => Ok(0),
                        Err(_) => Err(SyscallError::NotPermitted),
                    };
                }
                _ => {}
            }

            ipc::send_signal(pid, signal, current_pid).map_err(|_| SyscallError::NotFound)?;
            Ok(0)
        }
    }
}

/// Open file with enhanced security checks
fn sys_open(pathname: u64, flags: u32) -> SyscallResult {
    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    // Validate current process exists
    if current_pid == 0 {
        return Err(SyscallError::InvalidSyscall);
    }

    // Read-only snapshot used for the uid/gid permission check below. The fd-table
    // mutation itself is applied to the live process via `with_process_mut`.
    let process = match process_manager.get_process(current_pid) {
        Some(p) => p,
        None => return Err(SyscallError::InvalidSyscall),
    };

    // Security validation
    let path = SecurityValidator::copy_string_from_user(pathname, 4096)
        .map_err(|_| SyscallError::InvalidArgument)?;

    // Validate path length and characters
    if path.is_empty() || path.len() > 4095 {
        return Err(SyscallError::InvalidArgument);
    }

    // Check for null bytes in path (security)
    if path.contains('\0') {
        return Err(SyscallError::InvalidArgument);
    }

    // Convert flags to VFS open flags
    let open_flags = crate::fs::OpenFlags::from_posix(flags);

    // Check file permissions before opening
    if let Ok(metadata) = crate::fs::vfs().stat(&path) {
        if !check_file_permissions(&metadata, &open_flags, process.uid, process.gid) {
            return Err(SyscallError::PermissionDenied);
        }
    } else if !open_flags.create {
        return Err(SyscallError::NotFound);
    }

    // Open through VFS
    match crate::fs::vfs().open(&path, open_flags) {
        Ok(fd) => {
            // Allocate the next free fd and persist it on the REAL process. The
            // previous code mutated a clone (discarded on drop), so every open
            // returned fd 3. Compute next_fd from the live table to avoid TOCTOU.
            //
            // ponytail: the returned number is a process-local descriptor; the fd
            // record carries the VFS handle so reads/writes can be serviced.
            let alloc = process_manager.with_process_mut(current_pid, |p| {
                if p.file_descriptors.len() >= 1024 {
                    return Err(SyscallError::TooManyOpenFiles);
                }

                let mut next_fd: u32 = 3; // Start after stdin/stdout/stderr
                while p.file_descriptors.contains_key(&next_fd) {
                    next_fd += 1;
                    if next_fd > 65535 {
                        return Err(SyscallError::TooManyOpenFiles);
                    }
                }

                p.file_descriptors.insert(
                    next_fd,
                    crate::process::FileDescriptor::from_vfs_fd(fd, flags),
                );
                p.file_offsets.insert(next_fd, 0);
                Ok(next_fd)
            });

            match alloc {
                Some(Ok(next_fd)) => Ok(next_fd as u64),
                Some(Err(e)) => {
                    let _ = crate::fs::vfs().close(fd);
                    Err(e)
                }
                None => {
                    let _ = crate::fs::vfs().close(fd);
                    Err(SyscallError::InvalidSyscall)
                }
            }
        }
        Err(fs_error) => {
            // Convert filesystem error to syscall error
            let syscall_error = match fs_error {
                crate::fs::FsError::NotFound => SyscallError::NotFound,
                crate::fs::FsError::PermissionDenied => SyscallError::PermissionDenied,
                crate::fs::FsError::AlreadyExists => SyscallError::AlreadyExists,
                crate::fs::FsError::NotADirectory => SyscallError::NotDirectory,
                crate::fs::FsError::IsADirectory => SyscallError::IsDirectory,
                crate::fs::FsError::InvalidArgument => SyscallError::InvalidArgument,
                crate::fs::FsError::NoSpaceLeft => SyscallError::NoSpace,
                crate::fs::FsError::ReadOnly => SyscallError::ReadOnly,
                crate::fs::FsError::BadFileDescriptor => SyscallError::BadFileDescriptor,
                _ => SyscallError::IoError,
            };
            Err(syscall_error)
        }
    }
}

/// Convert POSIX open flags to VFS open flags
fn convert_posix_flags_to_vfs(flags: u32) -> crate::fs::SyscallOpenFlags {
    use crate::fs::SyscallOpenFlags;

    let mut open_flags = SyscallOpenFlags::empty();

    // Access mode (O_RDONLY=0, O_WRONLY=1, O_RDWR=2)
    let access_mode = flags & 0x3;
    match access_mode {
        0 => open_flags.insert(SyscallOpenFlags::READ), // O_RDONLY
        1 => open_flags.insert(SyscallOpenFlags::WRITE), // O_WRONLY
        2 => open_flags.insert(SyscallOpenFlags::RDWR), // O_RDWR
        _ => open_flags.insert(SyscallOpenFlags::READ), // Default to read-only
    }

    // Other flags
    if (flags & 0x40) != 0 {
        open_flags.insert(SyscallOpenFlags::CREAT);
    } // O_CREAT
    if (flags & 0x80) != 0 {
        open_flags.insert(SyscallOpenFlags::EXCL);
    } // O_EXCL
    if (flags & 0x200) != 0 {
        open_flags.insert(SyscallOpenFlags::TRUNC);
    } // O_TRUNC
    if (flags & 0x400) != 0 {
        open_flags.insert(SyscallOpenFlags::APPEND);
    } // O_APPEND

    open_flags
}

/// Check file permissions for access
fn check_file_permissions(
    metadata: &crate::fs::FileMetadata,
    open_flags: &crate::fs::OpenFlags,
    uid: u32,
    gid: u32,
) -> bool {
    let permissions = &metadata.permissions;

    // Root user (uid 0) can access everything
    if uid == 0 {
        return true;
    }

    // Determine which permission bits to check
    let (read_perm, write_perm, exec_perm) = if uid == metadata.uid {
        // Owner permissions
        (
            permissions.owner_read,
            permissions.owner_write,
            permissions.owner_execute,
        )
    } else if gid == metadata.gid {
        // Group permissions
        (
            permissions.group_read,
            permissions.group_write,
            permissions.group_execute,
        )
    } else {
        // Other permissions
        (
            permissions.other_read,
            permissions.other_write,
            permissions.other_execute,
        )
    };

    // Check read permission
    if open_flags.read && !read_perm {
        return false;
    }

    // Check write permission
    if open_flags.write && !write_perm {
        return false;
    }

    // Check execute permission for directories
    if metadata.file_type == crate::fs::FileType::Directory && !exec_perm {
        return false;
    }

    true
}

/// Close a file descriptor
///
/// Closes the underlying VFS handle for regular files, removes pipe entries, and
/// drops the process-local descriptor.
fn sys_close(fd: i32) -> SyscallResult {
    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    if current_pid == 0 {
        return Err(SyscallError::InvalidSyscall);
    }

    SecurityValidator::validate_fd(fd)?;
    if fd <= 2 {
        return Err(SyscallError::InvalidArgument);
    }

    let fd_record =
        process_manager.with_process_mut(current_pid, |p| p.file_descriptors.remove(&(fd as u32)));

    let fd_record = fd_record.flatten().ok_or(SyscallError::BadFileDescriptor)?;

    // Close underlying VFS handle if this is a regular file.
    if let crate::process::FileDescriptorType::VfsHandle { vfs_fd } = &fd_record.fd_type {
        let _ = crate::fs::vfs().close(*vfs_fd);
    }

    process_manager.with_process_mut(current_pid, |p| {
        p.file_offsets.remove(&(fd as u32));
    });

    Ok(0)
}

/// Convert a filesystem error to a syscall error.
fn fs_error_to_syscall_error(fs_error: crate::fs::FsError) -> SyscallError {
    match fs_error {
        crate::fs::FsError::NotFound => SyscallError::NotFound,
        crate::fs::FsError::PermissionDenied => SyscallError::PermissionDenied,
        crate::fs::FsError::AlreadyExists => SyscallError::AlreadyExists,
        crate::fs::FsError::NotADirectory => SyscallError::NotDirectory,
        crate::fs::FsError::IsADirectory => SyscallError::IsDirectory,
        crate::fs::FsError::InvalidArgument => SyscallError::InvalidArgument,
        crate::fs::FsError::NoSpaceLeft => SyscallError::NoSpace,
        crate::fs::FsError::ReadOnly => SyscallError::ReadOnly,
        crate::fs::FsError::BadFileDescriptor => SyscallError::BadFileDescriptor,
        _ => SyscallError::IoError,
    }
}

/// Read from file descriptor
///
/// Reads through the process-local `FileDescriptor` so that stdin, VFS inodes,
/// and VFS handles are all handled correctly. The per-process offset is kept in
/// sync with the legacy `file_offsets` map.
fn sys_read(fd: i32, buf: u64, count: u64) -> SyscallResult {
    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    if current_pid == 0 {
        return Err(SyscallError::InvalidSyscall);
    }

    SecurityValidator::validate_fd(fd)?;
    SecurityValidator::validate_user_ptr(buf, count, true)?;

    let read_count = core::cmp::min(count, 1024 * 1024) as usize; // Max 1MB
    let mut buffer = vec![0u8; read_count];

    let result = process_manager.with_process_mut(current_pid, |p| {
        let fd_entry = p.file_descriptors.get_mut(&(fd as u32))?;
        Some(fd_entry.read(&mut buffer).map(|n| (fd_entry.offset(), n)))
    });
    let (new_offset, bytes_read) = result
        .flatten()
        .ok_or(SyscallError::BadFileDescriptor)?
        .map_err(fs_error_to_syscall_error)?;

    process_manager.with_process_mut(current_pid, |p| {
        p.file_offsets.insert(fd as u32, new_offset as usize);
    });

    if bytes_read > 0 {
        SecurityValidator::copy_to_user(buf, &buffer[..bytes_read])?;
    }
    Ok(bytes_read as u64)
}

/// Write to file descriptor
///
/// Writes through the process-local `FileDescriptor` so that stdout/stderr,
/// VFS inodes, and VFS handles are handled correctly.
fn sys_write(fd: i32, buf: u64, count: u64) -> SyscallResult {
    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    if current_pid == 0 {
        return Err(SyscallError::InvalidSyscall);
    }

    SecurityValidator::validate_fd(fd)?;
    SecurityValidator::validate_user_ptr(buf, count, false)?;

    let write_count = core::cmp::min(count, 1024 * 1024) as usize; // Max 1MB
    let data = SecurityValidator::copy_from_user(buf, write_count)?;

    let result = process_manager.with_process_mut(current_pid, |p| {
        let fd_entry = p.file_descriptors.get_mut(&(fd as u32))?;
        Some(fd_entry.write(&data).map(|n| (fd_entry.offset(), n)))
    });
    let (new_offset, bytes_written) = result
        .flatten()
        .ok_or(SyscallError::BadFileDescriptor)?
        .map_err(fs_error_to_syscall_error)?;

    process_manager.with_process_mut(current_pid, |p| {
        p.file_offsets.insert(fd as u32, new_offset as usize);
    });

    Ok(bytes_written as u64)
}

/// pipe(pipefd) - Create a pipe and return two file descriptors.
///
/// Writes `[read_fd, write_fd]` as two `i32` values into the user-supplied
/// buffer. The pipe is created through the process IPC manager.
fn sys_pipe(pipefd_ptr: u64) -> SyscallResult {
    if pipefd_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    if current_pid == 0 {
        return Err(SyscallError::InvalidSyscall);
    }

    let (read_fd, write_fd) = match process_manager.with_process_mut(current_pid, |p| {
        if p.file_descriptors.len() + 2 > 1024 {
            return None;
        }

        let (read_pipe_id, write_pipe_id) = match process_manager.create_pipe() {
            Ok(ids) => ids,
            Err(_) => return None,
        };

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
            crate::process::FileDescriptor {
                fd_type: crate::process::FileDescriptorType::Pipe {
                    pipe_id: read_pipe_id,
                },
                flags: 0,
                offset: 0,
            },
        );
        p.file_offsets.insert(read_fd, 0);

        p.file_descriptors.insert(
            write_fd,
            crate::process::FileDescriptor {
                fd_type: crate::process::FileDescriptorType::Pipe {
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
        _ => return Err(SyscallError::OutOfMemory),
    };

    let pipefds = [read_fd as i32, write_fd as i32];
    let mut buf = [0u8; 8];
    buf[0..4].copy_from_slice(&pipefds[0].to_le_bytes());
    buf[4..8].copy_from_slice(&pipefds[1].to_le_bytes());

    SecurityValidator::copy_to_user(pipefd_ptr, &buf)?;
    Ok(0)
}

/// Change program break (heap management)
fn sys_brk(addr: u64) -> SyscallResult {
    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    // Validate current process exists
    if current_pid == 0 {
        return Err(SyscallError::InvalidSyscall);
    }

    // Read-only snapshot for the heap-bounds math; the heap_size update is applied
    // to the live process via `with_process_mut`.
    let process = match process_manager.get_process(current_pid) {
        Some(p) => p,
        None => return Err(SyscallError::InvalidSyscall),
    };

    let current_heap_end = process.memory.heap_start + process.memory.heap_size;

    // If addr is 0, return current break
    if addr == 0 {
        return Ok(current_heap_end);
    }

    // Validate new break address
    if addr < process.memory.heap_start {
        return Err(SyscallError::InvalidArgument);
    }

    // Check if we're expanding or shrinking the heap
    if addr > current_heap_end {
        // Expand heap
        let expansion_size = addr - current_heap_end;

        // Limit heap expansion to prevent abuse (max 1GB heap)
        if process.memory.heap_size + expansion_size > 1024 * 1024 * 1024 {
            return Err(SyscallError::OutOfMemory);
        }

        // Use memory manager to allocate additional heap space
        match expand_process_heap(current_pid, expansion_size) {
            Ok(()) => {
                // Persist the new heap size on the REAL process.
                process_manager.with_process_mut(current_pid, |p| {
                    p.memory.heap_size += expansion_size;
                });
                Ok(addr)
            }
            Err(_) => Err(SyscallError::OutOfMemory),
        }
    } else if addr < current_heap_end {
        // Shrink heap
        let shrink_size = current_heap_end - addr;

        // Use memory manager to deallocate heap space
        match shrink_process_heap(current_pid, shrink_size) {
            Ok(()) => {
                // Persist the new heap size on the REAL process.
                process_manager.with_process_mut(current_pid, |p| {
                    p.memory.heap_size -= shrink_size;
                });
                Ok(addr)
            }
            Err(_) => Err(SyscallError::InvalidArgument),
        }
    } else {
        // No change
        Ok(addr)
    }
}

/// Expand process heap by the specified size
fn expand_process_heap(pid: Pid, size: u64) -> Result<(), &'static str> {
    use crate::memory::{allocate_memory, MemoryProtection, MemoryRegionType};

    let process_manager = crate::process::get_process_manager();
    let _process = process_manager
        .get_process(pid)
        .ok_or("Process not found")?;

    // Allocate new heap memory
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

    match allocate_memory(size as usize, MemoryRegionType::UserHeap, protection) {
        Ok(_virt_addr) => Ok(()),
        Err(_) => Err("Failed to allocate heap memory"),
    }
}

/// Shrink process heap by the specified size
fn shrink_process_heap(pid: Pid, size: u64) -> Result<(), &'static str> {
    use crate::memory::deallocate_memory;

    let process_manager = crate::process::get_process_manager();
    let process = process_manager
        .get_process(pid)
        .ok_or("Process not found")?;

    // Calculate the address range to deallocate
    let heap_end = process.memory.heap_start + process.memory.heap_size;
    let dealloc_start = heap_end - size;

    // Deallocate heap pages
    match deallocate_memory(x86_64::VirtAddr::new(dealloc_start)) {
        Ok(()) => Ok(()),
        Err(_) => Err("Failed to deallocate heap memory"),
    }
}

/// Memory map
fn sys_mmap(_addr: u64, length: u64, prot: i32, flags: i32, fd: i32, offset: u64) -> SyscallResult {
    // Security validation
    if length == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    // Limit mapping size to prevent abuse
    if length > 1024 * 1024 * 1024 {
        // 1GB max
        return Err(SyscallError::InvalidArgument);
    }

    // Convert protection flags
    let readable = (prot & 0x1) != 0;
    let writable = (prot & 0x2) != 0;
    let executable = (prot & 0x4) != 0;

    let protection = crate::memory::MemoryProtection {
        readable,
        writable,
        executable,
        user_accessible: true,
        cache_disabled: false,
        write_through: false,
        copy_on_write: false,
        guard_page: false,
    };

    // Check for anonymous mapping (MAP_ANONYMOUS)
    let is_anonymous = (flags & 0x20) != 0;

    if !is_anonymous && fd >= 0 {
        // File-backed mapping.
        //
        // We allocate an anonymous region (temporarily writable so we can fill
        // it), copy the file contents into it in page-sized chunks through the
        // process-local FileDescriptor, zero-fill any remainder when the file is
        // shorter than the requested length, and finally re-protect the region.

        let process_manager = crate::process::get_process_manager();
        let current_pid = process_manager.current_process();
        if current_pid == 0 {
            return Err(SyscallError::InvalidSyscall);
        }

        let fill_protection = crate::memory::MemoryProtection {
            readable: true,
            writable: true,
            executable: false,
            user_accessible: true,
            cache_disabled: false,
            write_through: false,
            copy_on_write: false,
            guard_page: false,
        };

        let virt_addr = match crate::memory::allocate_memory(
            length as usize,
            crate::memory::MemoryRegionType::UserHeap,
            fill_protection,
        ) {
            Ok(virt_addr) => virt_addr,
            Err(memory_error) => {
                let syscall_error = match memory_error {
                    crate::memory::MemoryError::OutOfMemory => SyscallError::OutOfMemory,
                    crate::memory::MemoryError::NoVirtualSpace => SyscallError::OutOfMemory,
                    _ => SyscallError::InvalidArgument,
                };
                return Err(syscall_error);
            }
        };

        let free_and_fail = |err: SyscallError| -> SyscallResult {
            let _ = crate::memory::deallocate_memory(virt_addr);
            Err(err)
        };

        const PAGE_SIZE: usize = 4096;
        let base_ptr = virt_addr.as_u64() as *mut u8;

        // Read through the live FileDescriptor so VfsHandle and VfsFile are
        // both handled correctly. Preserve the descriptor's offset.
        let copied = match process_manager
            .with_process_mut(current_pid, |p| {
                let fd_entry = p.file_descriptors.get_mut(&(fd as u32))?;
                let file_size = fd_entry.size().ok()? as usize;
                let available = if (offset as usize) >= file_size {
                    0
                } else {
                    file_size - offset as usize
                };
                let to_read = core::cmp::min(available, length as usize);

                let original_offset = fd_entry.offset();
                fd_entry.set_offset(offset);

                let mut copied = 0usize;
                while copied < to_read {
                    let chunk_len = core::cmp::min(PAGE_SIZE, to_read - copied);
                    let dst =
                        unsafe { core::slice::from_raw_parts_mut(base_ptr.add(copied), chunk_len) };
                    match fd_entry.read(dst) {
                        Ok(0) => break,
                        Ok(n) => copied += n,
                        Err(_) => return None,
                    }
                }

                fd_entry.set_offset(original_offset);
                Some(copied)
            })
            .flatten()
        {
            Some(c) => c,
            None => return free_and_fail(SyscallError::InvalidArgument),
        };

        if copied < length as usize {
            unsafe {
                core::ptr::write_bytes(base_ptr.add(copied), 0u8, length as usize - copied);
            }
        }

        if let Err(_) = crate::memory::protect_memory(virt_addr, length as usize, protection) {
            return free_and_fail(SyscallError::OutOfMemory);
        }

        return Ok(virt_addr.as_u64());
    }

    // For anonymous mappings
    if is_anonymous {
        match crate::memory::allocate_memory(
            length as usize,
            crate::memory::MemoryRegionType::UserHeap,
            protection,
        ) {
            Ok(virt_addr) => Ok(virt_addr.as_u64()),
            Err(memory_error) => {
                let syscall_error = match memory_error {
                    crate::memory::MemoryError::OutOfMemory => SyscallError::OutOfMemory,
                    crate::memory::MemoryError::NoVirtualSpace => SyscallError::OutOfMemory,
                    _ => SyscallError::InvalidArgument,
                };
                Err(syscall_error)
            }
        }
    } else {
        // Non-anonymous mapping without a valid fd
        Err(SyscallError::InvalidArgument)
    }
}

/// Sleep for specified timespec (nanosleep ABI: pointer to {sec, nsec})
fn sys_nanosleep(req_ptr: u64) -> SyscallResult {
    if req_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    let req = SecurityValidator::copy_from_user(req_ptr, 16)?;
    let sec = u64::from_le_bytes([
        req[0], req[1], req[2], req[3], req[4], req[5], req[6], req[7],
    ]);
    let nsec = u64::from_le_bytes([
        req[8], req[9], req[10], req[11], req[12], req[13], req[14], req[15],
    ]);
    let total_us = sec.saturating_mul(1_000_000).saturating_add(nsec / 1_000);
    if total_us > 0 {
        crate::time::sleep_us(total_us);
    }
    Ok(0)
}

/// wait4(pid, status*, options, rusage*) — minimal waitpid wrapper
fn sys_wait4(pid: i32, status_ptr: u64) -> SyscallResult {
    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    let target = if pid == -1 {
        None
    } else if pid <= 0 {
        return Err(SyscallError::InvalidArgument);
    } else {
        Some(pid as Pid)
    };

    let children: alloc::vec::Vec<Pid> = process_manager
        .list_processes()
        .into_iter()
        .filter(|(child_pid, _, state, _)| {
            if *state != crate::process::ProcessState::Zombie {
                return false;
            }
            let parent = process_manager
                .get_process(*child_pid)
                .and_then(|p| p.parent_pid);
            parent == Some(current_pid) && target.map_or(true, |t| t == *child_pid)
        })
        .map(|(child_pid, _, _, _)| child_pid)
        .collect();

    let child_pid = children.first().copied().ok_or(SyscallError::NoChild)?;

    let exit_status = process_manager
        .get_process(child_pid)
        .and_then(|p| p.exit_code)
        .unwrap_or(0);

    if status_ptr != 0 {
        let status = (exit_status & 0xFF) as u32;
        SecurityValidator::copy_to_user(status_ptr, &status.to_le_bytes())?;
    }

    Ok(child_pid as u64)
}

/// lseek(fd, offset, whence)
///
/// Supports SEEK_SET (0), SEEK_CUR (1), and SEEK_END (2). The file size is
/// obtained from the `FileDescriptor` so it works for both VFS inodes and VFS
/// handles.
fn sys_lseek(fd: i32, offset: i64, whence: i32) -> SyscallResult {
    SecurityValidator::validate_fd(fd)?;

    if whence != 0 && whence != 1 && whence != 2 {
        return Err(SyscallError::InvalidArgument);
    }

    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    let result = process_manager.with_process_mut(current_pid, |p| {
        let fd_entry = p.file_descriptors.get_mut(&(fd as u32))?;
        let current = fd_entry.offset() as i64;
        let file_size = match whence {
            2 => fd_entry.size().ok()? as i64,
            _ => 0,
        };
        let new_offset = match whence {
            0 => offset,
            1 => current.checked_add(offset)?,
            2 => file_size.checked_add(offset)?,
            _ => return None,
        };
        if new_offset < 0 {
            return None;
        }
        fd_entry.set_offset(new_offset as u64);
        Some(new_offset as u64)
    });

    let new_offset = result.flatten().ok_or(SyscallError::InvalidArgument)?;

    process_manager.with_process_mut(current_pid, |p| {
        p.file_offsets.insert(fd as u32, new_offset as usize);
    });

    Ok(new_offset)
}

fn sys_mprotect(addr: u64, length: u64, prot: i32) -> SyscallResult {
    if length == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    let readable = (prot & 0x1) != 0;
    let writable = (prot & 0x2) != 0;
    let executable = (prot & 0x4) != 0;

    let protection = crate::memory::MemoryProtection {
        readable,
        writable,
        executable,
        user_accessible: true,
        cache_disabled: false,
        write_through: false,
        copy_on_write: false,
        guard_page: false,
    };

    match crate::memory::protect_memory(x86_64::VirtAddr::new(addr), length as usize, protection) {
        Ok(()) => Ok(0),
        Err(_) => Err(SyscallError::InvalidArgument),
    }
}

/// getcwd(buf, size)
fn sys_getcwd(buf: u64, size: u64) -> SyscallResult {
    if buf == 0 || size == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();
    let cwd = process_manager
        .get_process(current_pid)
        .map(|p| p.cwd)
        .unwrap_or_else(|| String::from("/"));

    let bytes = cwd.as_bytes();
    if (bytes.len() + 1) as u64 > size {
        return Err(SyscallError::InvalidArgument);
    }

    let mut out = vec![0u8; bytes.len() + 1];
    out[..bytes.len()].copy_from_slice(bytes);
    SecurityValidator::copy_to_user(buf, &out)?;
    Ok(buf)
}

/// chdir(path)
fn sys_chdir(path_ptr: u64) -> SyscallResult {
    let path = SecurityValidator::copy_string_from_user(path_ptr, 4096)?;

    if crate::fs::vfs().stat(&path).is_err() {
        return Err(SyscallError::NotFound);
    }

    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();
    process_manager.with_process_mut(current_pid, |p| {
        p.cwd = path;
    });

    Ok(0)
}

/// Sleep for specified microseconds (legacy helper)
fn sys_sleep(microseconds: u64) -> SyscallResult {
    let milliseconds = microseconds / 1000;
    if milliseconds > 0 {
        crate::time::sleep_ms(milliseconds);
    }
    Ok(0)
}

/// gettimeofday(tv_ptr, tz_ptr)
///
/// Writes the current wall-clock time as a Linux `timeval` (seconds +
/// microseconds) to the user-supplied buffer. The timezone pointer is ignored
/// per modern Linux behavior.
fn sys_gettimeofday(tv_ptr: u64) -> SyscallResult {
    if tv_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    let now_us = crate::time::get_system_time_ms() * 1000;
    let sec = now_us / 1_000_000;
    let usec = now_us % 1_000_000;

    let mut buf = [0u8; 16];
    buf[0..8].copy_from_slice(&sec.to_le_bytes());
    buf[8..16].copy_from_slice(&usec.to_le_bytes());

    SecurityValidator::copy_to_user(tv_ptr, &buf)?;
    Ok(0)
}

/// clock_gettime(clockid, tp_ptr)
///
/// Writes the current wall-clock time as a Linux `timespec` (seconds +
/// nanoseconds) to the user-supplied buffer.
fn sys_clock_gettime(_clockid: u64, tp_ptr: u64) -> SyscallResult {
    if tp_ptr == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    let now_ms = crate::time::get_system_time_ms();
    let sec = now_ms / 1000;
    let nsec = (now_ms % 1000) * 1_000_000;

    let mut buf = [0u8; 16];
    buf[0..8].copy_from_slice(&sec.to_le_bytes());
    buf[8..16].copy_from_slice(&nsec.to_le_bytes());

    SecurityValidator::copy_to_user(tp_ptr, &buf)?;
    Ok(0)
}

/// Set process priority with privilege validation
fn sys_setpriority(priority: i32) -> SyscallResult {
    let new_priority = match priority {
        0 => crate::scheduler::Priority::RealTime,
        1 => crate::scheduler::Priority::High,
        2 => crate::scheduler::Priority::Normal,
        3 => crate::scheduler::Priority::Low,
        4 => crate::scheduler::Priority::Idle,
        _ => return Err(SyscallError::InvalidArgument),
    };

    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    // Check privilege requirements for different priority levels
    match new_priority {
        crate::scheduler::Priority::RealTime => {
            // Real-time priority requires system admin capability
            if !crate::security::check_permission(current_pid, "sys_admin") {
                return Err(SyscallError::PermissionDenied);
            }
        }
        crate::scheduler::Priority::High => {
            // High priority requires elevated privileges
            if let Some(ctx) = crate::security::get_context(current_pid) {
                if ctx.level == crate::security::SecurityLevel::User && !ctx.is_root() {
                    return Err(SyscallError::PermissionDenied);
                }
            }
        }
        _ => {
            // Normal, Low, and Idle priorities are available to all processes
        }
    }

    // Validate current privilege level
    if let Some(ctx) = crate::security::get_context(current_pid) {
        // Ensure privilege level is appropriate for the requested priority
        match (ctx.level, new_priority) {
            (crate::security::SecurityLevel::User, crate::scheduler::Priority::RealTime) => {
                return Err(SyscallError::PermissionDenied);
            }
            _ => {}
        }
    }

    // Convert scheduler::Priority to process::Priority
    let process_priority = match new_priority {
        crate::scheduler::Priority::RealTime => crate::process::Priority::RealTime,
        crate::scheduler::Priority::High => crate::process::Priority::High,
        crate::scheduler::Priority::Normal => crate::process::Priority::Normal,
        crate::scheduler::Priority::Low => crate::process::Priority::Low,
        crate::scheduler::Priority::Idle => crate::process::Priority::Idle,
    };

    // Update priority on the REAL process control block (a clone would be
    // discarded, so the priority change would never take effect).
    match process_manager.with_process_mut(current_pid, |p| {
        p.priority = process_priority;
    }) {
        Some(()) => {
            // Notify scheduler of priority change
            crate::scheduler::update_process_priority(current_pid, new_priority);

            Ok(0)
        }
        None => Err(SyscallError::InvalidSyscall),
    }
}

/// Get process priority
fn sys_getpriority() -> SyscallResult {
    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    // Validate that we have a valid process ID
    if current_pid == 0 {
        return Err(SyscallError::InvalidSyscall);
    }

    // Get priority from process control block
    match process_manager.get_process(current_pid) {
        Some(process) => {
            let priority_value = match process.priority {
                crate::process::Priority::RealTime => 0,
                crate::process::Priority::High => 1,
                crate::process::Priority::Normal => 2,
                crate::process::Priority::Low => 3,
                crate::process::Priority::Idle => 4,
            };
            Ok(priority_value)
        }
        None => Err(SyscallError::InvalidSyscall),
    }
}

/// Memory unmap with privilege validation
fn sys_munmap(addr: u64, length: u64) -> SyscallResult {
    // Security validation
    if length == 0 {
        return Err(SyscallError::InvalidArgument);
    }

    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    // Validate user space memory access
    SecurityValidator::validate_user_ptr(addr, length, true)?;

    // Check if process has permission to unmap memory
    if let Some(_ctx) = crate::security::get_context(current_pid) {
        // Validate process isolation - can only unmap own memory
        if let Err(_) =
            crate::security::validate_process_isolation(current_pid, current_pid, "memory_access")
        {
            return Err(SyscallError::PermissionDenied);
        }
    }

    // Page-align the address and length
    let page_size = 4096u64;
    let aligned_addr = addr & !(page_size - 1);

    // Additional security check: ensure address is in user space
    const USER_SPACE_START: u64 = 0x0000_1000_0000;
    const USER_SPACE_END: u64 = 0x0000_8000_0000;

    if aligned_addr < USER_SPACE_START || aligned_addr >= USER_SPACE_END {
        return Err(SyscallError::InvalidAddress);
    }

    // Deallocate memory
    match crate::memory::deallocate_memory(x86_64::VirtAddr::new(aligned_addr)) {
        Ok(()) => Ok(0),
        Err(memory_error) => {
            let syscall_error = match memory_error {
                crate::memory::MemoryError::RegionNotFound => SyscallError::InvalidArgument,
                crate::memory::MemoryError::PermissionDenied => SyscallError::PermissionDenied,
                _ => SyscallError::InvalidArgument,
            };
            Err(syscall_error)
        }
    }
}

/// Get system information
fn sys_uname(buf: u64) -> SyscallResult {
    use core::mem::size_of;

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

    // Security validation
    SecurityValidator::validate_user_ptr(buf, UTSNAME_SIZE as u64, true)?;

    // Create and populate utsname structure
    let mut utsname = UtsName {
        sysname: [0; 65],
        nodename: [0; 65],
        release: [0; 65],
        version: [0; 65],
        machine: [0; 65],
    };

    // Fill in system information
    copy_str_to_array(&mut utsname.sysname, "RustOS");
    copy_str_to_array(&mut utsname.nodename, "rustos-node");
    copy_str_to_array(&mut utsname.release, env!("CARGO_PKG_VERSION"));
    copy_str_to_array(&mut utsname.version, "RustOS Production Kernel");
    copy_str_to_array(&mut utsname.machine, "x86_64");

    // Copy to user space
    let utsname_bytes =
        unsafe { core::slice::from_raw_parts(&utsname as *const _ as *const u8, UTSNAME_SIZE) };

    SecurityValidator::copy_to_user(buf, utsname_bytes)?;
    Ok(0)
}

/// Helper function to copy string to fixed-size array
fn copy_str_to_array(dest: &mut [u8], src: &str) {
    let bytes = src.as_bytes();
    let copy_len = core::cmp::min(bytes.len(), dest.len() - 1);
    dest[..copy_len].copy_from_slice(&bytes[..copy_len]);
    dest[copy_len] = 0; // Null terminator
}

/// Get current process ID (production)
fn get_current_pid() -> Pid {
    // Get current PID from process manager
    let process_manager = crate::process::get_process_manager();
    let current_pid = process_manager.current_process();

    // If no current process, return kernel PID (0)
    if current_pid == 0 {
        // This should only happen during early boot or kernel threads
        0
    } else {
        current_pid
    }
}

/// Get current working directory for a process
fn get_process_cwd(pid: Pid) -> Option<String> {
    let process_manager = crate::process::get_process_manager();

    match process_manager.get_process(pid) {
        Some(process) => Some(process.cwd.clone()),
        None => None,
    }
}

/// Get system call statistics
pub fn get_syscall_stats() -> SyscallStats {
    unsafe { core::ptr::addr_of!(SYSCALL_STATS).read() }
}

/// User-space system call wrapper macro
#[macro_export]
macro_rules! syscall {
    ($num:expr) => {
        syscall!($num, 0, 0, 0, 0, 0, 0)
    };
    ($num:expr, $arg1:expr) => {
        syscall!($num, $arg1, 0, 0, 0, 0, 0)
    };
    ($num:expr, $arg1:expr, $arg2:expr) => {
        syscall!($num, $arg1, $arg2, 0, 0, 0, 0)
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr) => {
        syscall!($num, $arg1, $arg2, $arg3, 0, 0, 0)
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr) => {
        syscall!($num, $arg1, $arg2, $arg3, $arg4, 0, 0)
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr) => {
        syscall!($num, $arg1, $arg2, $arg3, $arg4, $arg5, 0)
    };
    ($num:expr, $arg1:expr, $arg2:expr, $arg3:expr, $arg4:expr, $arg5:expr, $arg6:expr) => {{
        let result: u64;
        unsafe {
            core::arch::asm!(
                "mov rax, {num:r}",
                "mov rdi, {arg1:r}",
                "mov rsi, {arg2:r}",
                "mov rdx, {arg3:r}",
                "mov r10, {arg4:r}",
                "mov r8, {arg5:r}",
                "mov r9, {arg6:r}",
                "int 0x80",
                num = in(reg) $num,
                arg1 = in(reg) $arg1,
                arg2 = in(reg) $arg2,
                arg3 = in(reg) $arg3,
                arg4 = in(reg) $arg4,
                arg5 = in(reg) $arg5,
                arg6 = in(reg) $arg6,
                lateout("rax") result,
                options(preserves_flags)
            );
        }
        result
    }};
}

/// User-space system call functions
pub mod userspace {
    use super::*;

    /// Exit the current process
    pub fn exit(exit_code: i32) -> ! {
        syscall!(SyscallNumber::Exit as u64, exit_code as u64);
        loop {} // Should never reach here
    }

    /// Get current process ID
    pub fn getpid() -> Pid {
        syscall!(SyscallNumber::GetPid as u64) as Pid
    }

    /// Write to file descriptor
    pub fn write(fd: i32, buf: *const u8, count: usize) -> isize {
        let result = syscall!(
            SyscallNumber::Write as u64,
            fd as u64,
            buf as u64,
            count as u64
        );
        result as isize
    }

    /// Read from file descriptor
    pub fn read(fd: i32, buf: *mut u8, count: usize) -> isize {
        let result = syscall!(
            SyscallNumber::Read as u64,
            fd as u64,
            buf as u64,
            count as u64
        );
        result as isize
    }

    /// Sleep for specified microseconds (via nanosleep)
    pub fn sleep(microseconds: u64) {
        syscall!(SyscallNumber::Nanosleep as u64, microseconds);
    }

    /// Yield CPU to other processes
    pub fn yield_cpu() {
        syscall!(SyscallNumber::SchedYield as u64);
    }
}
