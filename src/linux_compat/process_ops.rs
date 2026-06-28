//! Linux process/thread operation APIs
//!
//! This module implements Linux-compatible process and thread operations
//! including user/group IDs, process groups, sessions, and resource usage.
//!
//! Integrated with RustOS process manager, scheduler, and ELF loader.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};

use super::types::*;
use super::{LinuxError, LinuxResult};

// Re-export types for external access
pub use super::types::{Rusage, TimeVal};

// Import process management infrastructure
use crate::process::Pid as KernelPid;
use crate::process::{self};
/// Operation counter for statistics
static PROCESS_OPS_COUNT: AtomicU64 = AtomicU64::new(0);

/// Initialize process operations subsystem
pub fn init_process_operations() {
    PROCESS_OPS_COUNT.store(0, Ordering::Relaxed);
}

/// Get number of process operations performed
pub fn get_operation_count() -> u64 {
    PROCESS_OPS_COUNT.load(Ordering::Relaxed)
}

/// Increment operation counter
fn inc_ops() {
    PROCESS_OPS_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Get current process PCB or return error
fn current_pcb() -> LinuxResult<process::ProcessControlBlock> {
    let pid = process::current_pid();
    let process_manager = process::get_process_manager();
    process_manager.get_process(pid).ok_or(LinuxError::ESRCH)
}

/// Get any process PCB by PID
fn get_pcb(pid: KernelPid) -> LinuxResult<process::ProcessControlBlock> {
    let process_manager = process::get_process_manager();
    process_manager.get_process(pid).ok_or(LinuxError::ESRCH)
}

/// Linux USER_HZ (clock ticks per second).
const USER_HZ: u64 = 100;

/// Linux capability header version 1.
const CAP_VERSION_1: u32 = 0x19980330;

/// Capability bit: CAP_SETPCAP.
const CAP_SETPCAP: u64 = 1 << 8;

/// Linux `struct __user_cap_header_struct`.
#[repr(C)]
struct CapUserHeader {
    version: u32,
    pid: i32,
}

/// Linux `struct __user_cap_data_struct`.
#[repr(C)]
struct CapUserData {
    effective: u32,
    permitted: u32,
    inheritable: u32,
}

fn pcb_user_ticks(pcb: &process::ProcessControlBlock) -> u64 {
    if pcb.user_time_ticks > 0 {
        pcb.user_time_ticks
    } else {
        pcb.cpu_time / 10
    }
}

fn ticks_to_timeval(ticks: u64) -> TimeVal {
    TimeVal {
        tv_sec: (ticks / USER_HZ) as i64,
        tv_usec: (((ticks % USER_HZ) * 1_000_000) / USER_HZ) as i64,
    }
}

fn pcb_to_rusage(pcb: &process::ProcessControlBlock) -> Rusage {
    Rusage {
        ru_utime: ticks_to_timeval(pcb_user_ticks(pcb)),
        ru_stime: ticks_to_timeval(pcb.system_time_ticks),
        ru_maxrss: ((pcb.memory.heap_size
            + pcb.memory.stack_size
            + pcb.memory.code_size
            + pcb.memory.data_size)
            / 1024) as i64,
        ru_ixrss: 0,
        ru_idrss: 0,
        ru_isrss: 0,
        ru_minflt: pcb.minor_faults as i64,
        ru_majflt: pcb.major_faults as i64,
        ru_nswap: 0,
        ru_inblock: 0,
        ru_oublock: 0,
        ru_msgsnd: 0,
        ru_msgrcv: 0,
        ru_nsignals: 0,
        ru_nvcsw: pcb.sched_info.schedule_count as i64,
        ru_nivcsw: 0,
    }
}

fn pcb_children_rusage(pcb: &process::ProcessControlBlock) -> Rusage {
    Rusage {
        ru_utime: ticks_to_timeval(pcb.child_user_time),
        ru_stime: ticks_to_timeval(pcb.child_system_time),
        ru_maxrss: 0,
        ru_ixrss: 0,
        ru_idrss: 0,
        ru_isrss: 0,
        ru_minflt: 0,
        ru_majflt: 0,
        ru_nswap: 0,
        ru_inblock: 0,
        ru_oublock: 0,
        ru_msgsnd: 0,
        ru_msgrcv: 0,
        ru_nsignals: 0,
        ru_nvcsw: 0,
        ru_nivcsw: 0,
    }
}

fn wait_child_matches(pid: Pid, caller_pgid: u32) -> impl Fn(&process::ProcessControlBlock) -> bool {
    move |pcb: &process::ProcessControlBlock| -> bool {
        if pid == -1 {
            return true;
        }
        if pid == 0 {
            return pcb.pgid == caller_pgid;
        }
        if pid < -1 {
            let target_pgid = pid.unsigned_abs();
            return pcb.pgid == target_pgid;
        }
        pcb.pid == pid as u32
    }
}

//
// Process Lifecycle Operations
//

/// fork - create child process
pub fn fork() -> LinuxResult<Pid> {
    inc_ops();

    let parent_pid = process::current_pid();
    crate::process::integration::get_integration_manager()
        .fork_process(parent_pid)
        .map(|child_pid| child_pid as i32)
        .map_err(|_| LinuxError::EAGAIN)
}

/// exec - execute new program in current process
pub fn exec(program: &[u8], args: &[&str]) -> LinuxResult<i32> {
    inc_ops();

    let pid = process::current_pid();
    process::exec::exec_elf_binary(pid, program, args, &[])
        .map_err(|_| LinuxError::ENOEXEC)?;

    Ok(0)
}

/// wait - wait for any child process to exit
pub fn wait(status: *mut i32) -> LinuxResult<Pid> {
    inc_ops();
    waitpid(-1, status, 0)
}

/// waitpid - wait for specific child process
pub fn waitpid(pid: Pid, status: *mut i32, _options: i32) -> LinuxResult<Pid> {
    inc_ops();

    let parent_pid = process::current_pid();
    let process_mgr = process::get_process_manager();

    let caller_pgid = process_mgr
        .get_process(parent_pid)
        .map(|pcb| pcb.pgid)
        .ok_or(LinuxError::ESRCH)?;

    let matches = |pcb: &process::ProcessControlBlock| -> bool {
        if pid == -1 {
            return true;
        }
        if pid == 0 {
            return pcb.pgid == caller_pgid;
        }
        if pid < -1 {
            let target_pgid = (-pid) as u32;
            return pcb.pgid == target_pgid;
        }
        pcb.pid == pid as u32
    };

    match process_mgr.reap_zombie_child(parent_pid, matches) {
        Ok((child_pid, exit_status)) => {
            if !status.is_null() {
                unsafe {
                    *status = exit_status;
                }
            }
            Ok(child_pid as i32)
        }
        Err("No child processes") => Err(LinuxError::ECHILD),
        Err("Would block waiting for child") => Err(LinuxError::EAGAIN),
        Err(_) => Err(LinuxError::EINVAL),
    }
}

/// Linux auxiliary vector types (uapi/linux/auxvec.h)
mod auxv {
    pub const AT_NULL: u64 = 0;
    pub const AT_PHDR: u64 = 3;
    pub const AT_PHENT: u64 = 4;
    pub const AT_PHNUM: u64 = 5;
    pub const AT_PAGESZ: u64 = 6;
    pub const AT_BASE: u64 = 7;
    pub const AT_ENTRY: u64 = 9;
    pub const AT_EXECFN: u64 = 31;
    pub const AT_RANDOM: u64 = 25;
}

const MAX_USER_STRING: usize = 4096;
const MAX_PTR_ARRAY: usize = 256;
const MAX_EXEC_SIZE: usize = 16 * 1024 * 1024;

/// Convert null-terminated C string from user space to Rust `String`.
///
/// Same validation pattern as `file_ops::c_str_to_string`.
unsafe fn c_str_to_string(ptr: *const u8) -> Result<String, LinuxError> {
    if ptr.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let mut len = 0;
    while len < MAX_USER_STRING && *ptr.add(len) != 0 {
        len += 1;
    }

    if len >= MAX_USER_STRING {
        return Err(LinuxError::ENAMETOOLONG);
    }

    let slice = core::slice::from_raw_parts(ptr, len);
    String::from_utf8(slice.to_vec()).map_err(|_| LinuxError::EINVAL)
}

/// Read a NULL-terminated array of C string pointers from user space.
unsafe fn read_string_array(arr_ptr: *const *const u8) -> Result<Vec<String>, LinuxError> {
    if arr_ptr.is_null() {
        return Ok(Vec::new());
    }

    let mut strings = Vec::new();
    for i in 0..MAX_PTR_ARRAY {
        let str_ptr = *arr_ptr.add(i);
        if str_ptr.is_null() {
            break;
        }
        strings.push(c_str_to_string(str_ptr)?);
    }
    Ok(strings)
}

fn elf_error_to_linux(err: crate::process::elf_loader::ElfLoaderError) -> LinuxError {
    use crate::process::elf_loader::ElfLoaderError;
    match err {
        ElfLoaderError::MemoryAllocationFailed | ElfLoaderError::MappingFailed => {
            LinuxError::ENOMEM
        }
        _ => LinuxError::ENOEXEC,
    }
}

fn fs_error_to_linux(err: crate::fs::FsError) -> LinuxError {
    use crate::fs::FsError;
    match err {
        FsError::NotFound => LinuxError::ENOENT,
        FsError::PermissionDenied => LinuxError::EACCES,
        FsError::NoSpaceLeft => LinuxError::ENOMEM,
        _ => LinuxError::EIO,
    }
}

/// Write a u64 onto the descending stack and return the new stack pointer.
unsafe fn push_u64(sp: &mut u64, value: u64) {
    *sp = sp.wrapping_sub(8);
    (*sp as *mut u64).write(value);
}

/// Write a null-terminated string onto the descending stack; return its address.
unsafe fn push_cstring(sp: &mut u64, s: &str) -> u64 {
    let bytes = s.as_bytes();
    *sp = sp.wrapping_sub((bytes.len() + 1) as u64);
    let addr = *sp;
    core::ptr::copy_nonoverlapping(bytes.as_ptr(), addr as *mut u8, bytes.len());
    (addr as *mut u8).add(bytes.len()).write(0);
    addr
}

/// Fill 16 random bytes for AT_RANDOM (security RNG with TSC fallback).
fn fill_random_16(buf: &mut [u8; 16]) {
    if crate::security::get_random_bytes(buf).is_err() {
        let tsc = unsafe { core::arch::x86_64::_rdtsc() };
        buf[..8].copy_from_slice(&tsc.to_le_bytes());
        let tsc2 = unsafe { core::arch::x86_64::_rdtsc() };
        buf[8..].copy_from_slice(&tsc2.to_le_bytes());
    }
}

/// Compute virtual address of the program header table for auxv AT_PHDR.
fn compute_phdr_addr(
    loaded: &crate::process::elf_loader::LoadedBinary,
    header: &crate::process::elf_loader::Elf64Header,
) -> u64 {
    use crate::process::elf_loader::elf_constants;
    if let Some(ph) = loaded
        .program_headers
        .iter()
        .find(|p| p.p_type == elf_constants::PT_PHDR)
    {
        loaded.base_address.as_u64() + ph.p_vaddr
    } else {
        loaded.base_address.as_u64() + header.e_phoff
    }
}

/// Build the Linux x86_64 initial stack (argc/argv/envp/auxv) on `stack_top`.
fn build_linux_initial_stack(
    stack_top: u64,
    argv: &[String],
    envp: &[String],
    loaded: &crate::process::elf_loader::LoadedBinary,
    header: &crate::process::elf_loader::Elf64Header,
    exec_path: &str,
) -> Result<u64, LinuxError> {
    let mut sp = stack_top & !0xF;

    let mut arg_addrs = Vec::with_capacity(argv.len());
    for arg in argv.iter().rev() {
        arg_addrs.push(unsafe { push_cstring(&mut sp, arg) });
    }
    arg_addrs.reverse();

    let mut env_addrs = Vec::with_capacity(envp.len());
    for env in envp.iter().rev() {
        env_addrs.push(unsafe { push_cstring(&mut sp, env) });
    }
    env_addrs.reverse();

    sp &= !0xF;

    let mut random = [0u8; 16];
    fill_random_16(&mut random);
    sp = sp.wrapping_sub(16);
    let random_addr = sp;
    unsafe {
        core::ptr::copy_nonoverlapping(random.as_ptr(), sp as *mut u8, 16);
    }

    let execfn_addr = unsafe { push_cstring(&mut sp, exec_path) };

    let phdr_addr = compute_phdr_addr(loaded, header);
    let auxv_entries: [(u64, u64); 9] = [
        (auxv::AT_PAGESZ, 4096),
        (auxv::AT_PHDR, phdr_addr),
        (
            auxv::AT_PHENT,
            core::mem::size_of::<crate::process::elf_loader::Elf64ProgramHeader>() as u64,
        ),
        (auxv::AT_PHNUM, header.e_phnum as u64),
        (auxv::AT_ENTRY, loaded.entry_point.as_u64()),
        (auxv::AT_BASE, loaded.base_address.as_u64()),
        (auxv::AT_EXECFN, execfn_addr),
        (auxv::AT_RANDOM, random_addr),
        (auxv::AT_NULL, 0),
    ];

    for &(tag, val) in auxv_entries.iter().rev() {
        unsafe {
            push_u64(&mut sp, val);
            push_u64(&mut sp, tag);
        }
    }

    unsafe {
        push_u64(&mut sp, 0);
        for &addr in env_addrs.iter().rev() {
            push_u64(&mut sp, addr);
        }

        push_u64(&mut sp, 0);
        for &addr in arg_addrs.iter().rev() {
            push_u64(&mut sp, addr);
        }

        push_u64(&mut sp, argv.len() as u64);
    }

    if sp % 16 != 0 {
        return Err(LinuxError::EINVAL);
    }

    Ok(sp)
}

/// Load an executable from the VFS and parse it with the ELF loader.
fn load_executable_from_vfs(
    path: &str,
    pid: KernelPid,
) -> Result<(Vec<u8>, crate::process::elf_loader::LoadedBinary), LinuxError> {
    use crate::fs::OpenFlags;
    use crate::process::elf_loader::ElfLoader;

    let vfs = crate::fs::vfs();
    let fd = vfs
        .open(
            path,
            OpenFlags {
                read: true,
                write: false,
                create: false,
                append: false,
                truncate: false,
                exclusive: false,
            },
        )
        .map_err(fs_error_to_linux)?;

    let file_size = vfs.stat(path).map_err(fs_error_to_linux)?.size as usize;

    if file_size == 0 || file_size > MAX_EXEC_SIZE {
        let _ = vfs.close(fd);
        return Err(LinuxError::ENOEXEC);
    }

    let mut binary_data = Vec::with_capacity(file_size);
    binary_data.resize(file_size, 0);

    match vfs.read(fd, &mut binary_data) {
        Ok(n) if n == file_size => {}
        _ => {
            let _ = vfs.close(fd);
            return Err(LinuxError::EIO);
        }
    }
    let _ = vfs.close(fd);

    let elf_loader = ElfLoader::new(true, true);
    let loaded = elf_loader
        .load_elf_binary(&binary_data, pid)
        .map_err(elf_error_to_linux)?;

    Ok((binary_data, loaded))
}

/// Apply a loaded ELF image and initial stack to the process PCB.
fn apply_loaded_binary(
    pid: KernelPid,
    loaded: &crate::process::elf_loader::LoadedBinary,
    rsp: u64,
    program_name: &str,
) -> Result<(), LinuxError> {
    use crate::process::ProcessState;

    let process_manager = process::get_process_manager();
    process_manager
        .with_process_mut(pid, |pcb| {
            let user_code = crate::gdt::get_user_code_selector().0 | 3;
            let user_data = crate::gdt::get_user_data_selector().0 | 3;

            pcb.memory.code_start = loaded.base_address.as_u64();
            pcb.memory.code_size = loaded.code_regions.iter().map(|r| r.size as u64).sum();
            pcb.memory.data_start = loaded
                .data_regions
                .first()
                .map(|r| r.start.as_u64())
                .unwrap_or(0);
            pcb.memory.data_size = loaded.data_regions.iter().map(|r| r.size as u64).sum();
            pcb.memory.heap_start = loaded.heap_start.as_u64();
            pcb.memory.heap_size = 8 * 1024;
            pcb.memory.stack_start = loaded.stack_top.as_u64().saturating_sub(8 * 1024 * 1024);
            pcb.memory.stack_size = 8 * 1024 * 1024;

            pcb.entry_point = loaded.entry_point.as_u64();
            pcb.context.rip = loaded.entry_point.as_u64();
            pcb.context.rsp = rsp;
            pcb.context.rax = 0;
            pcb.context.rbx = 0;
            pcb.context.rcx = 0;
            pcb.context.rdx = 0;
            pcb.context.rsi = 0;
            pcb.context.rdi = 0;
            pcb.context.rbp = rsp;
            pcb.context.rflags = 0x202;
            pcb.context.cs = user_code;
            pcb.context.ss = user_data;
            pcb.context.ds = user_data;
            pcb.context.es = user_data;
            pcb.context.fs = user_data;
            pcb.context.gs = user_data;

            pcb.state = ProcessState::Ready;
            pcb.cpu_time = 0;

            pcb.file_descriptors.retain(|&fd, _| fd <= 2);
            pcb.file_offsets.retain(|&fd, _| fd <= 2);
            pcb.signal_handlers.clear();

            let name_bytes = program_name.as_bytes();
            let copy_len = core::cmp::min(name_bytes.len(), pcb.name.len().saturating_sub(1));
            pcb.name = [0u8; 32];
            pcb.name[..copy_len].copy_from_slice(&name_bytes[..copy_len]);
        })
        .ok_or(LinuxError::ESRCH)
}

/// execve - execute program (Linux-compatible syscall interface)
pub fn execve(
    filename: *const u8,
    argv: *const *const u8,
    envp: *const *const u8,
) -> LinuxResult<i32> {
    inc_ops();

    use crate::process::elf_loader::Elf64Header;

    let path = unsafe { c_str_to_string(filename)? };
    if path.is_empty() {
        return Err(LinuxError::EINVAL);
    }

    let argv_strings = unsafe { read_string_array(argv)? };
    let envp_strings = unsafe { read_string_array(envp)? };

    let pid = process::current_pid();

    let (binary_data, loaded) = load_executable_from_vfs(&path, pid)?;

    if binary_data.len() < core::mem::size_of::<Elf64Header>() {
        return Err(LinuxError::ENOEXEC);
    }

    let header = unsafe { core::ptr::read(binary_data.as_ptr() as *const Elf64Header) };

    let rsp = build_linux_initial_stack(
        loaded.stack_top.as_u64(),
        &argv_strings,
        &envp_strings,
        &loaded,
        &header,
        &path,
    )?;

    let prog_name = argv_strings
        .first()
        .map(|s| s.as_str())
        .unwrap_or(path.as_str());

    let entry_point = loaded.entry_point.as_u64();
    apply_loaded_binary(pid, &loaded, rsp, prog_name)?;

    if crate::usermode::in_user_mode() {
        crate::usermode::schedule_user_entry(entry_point, rsp);
    }

    Ok(0)
}

/// Load and jump to a program from kernel mode (boot / init path).
///
/// # Safety
/// Never returns on success; caller must have initialized syscalls, GDT, and paging.
pub unsafe fn execve_and_enter_user_mode(
    filename: *const u8,
    argv: *const *const u8,
    envp: *const *const u8,
) -> Result<(), LinuxError> {
    use crate::process::elf_loader::Elf64Header;

    let path = c_str_to_string(filename)?;
    if path.is_empty() {
        return Err(LinuxError::EINVAL);
    }

    let argv_strings = read_string_array(argv)?;
    let envp_strings = read_string_array(envp)?;
    let pid = process::current_pid();

    let (binary_data, loaded) = load_executable_from_vfs(&path, pid)?;
    if binary_data.len() < core::mem::size_of::<Elf64Header>() {
        return Err(LinuxError::ENOEXEC);
    }

    let header = core::ptr::read(binary_data.as_ptr() as *const Elf64Header);
    let rsp = build_linux_initial_stack(
        loaded.stack_top.as_u64(),
        &argv_strings,
        &envp_strings,
        &loaded,
        &header,
        &path,
    )?;

    let prog_name = argv_strings
        .first()
        .map(|s| s.as_str())
        .unwrap_or(path.as_str());

    apply_loaded_binary(pid, &loaded, rsp, prog_name)?;
    crate::usermode::switch_to_user_mode(loaded.entry_point.as_u64(), rsp);
}

/// wait4 - wait for process to change state (Linux-compatible syscall interface)
pub fn wait4(pid: Pid, wstatus: *mut i32, options: i32, rusage: *mut Rusage) -> LinuxResult<Pid> {
    inc_ops();

    if !rusage.is_null() {
        let parent_pid = process::current_pid();
        let process_mgr = process::get_process_manager();
        let caller_pgid = process_mgr
            .get_process(parent_pid)
            .map(|pcb| pcb.pgid)
            .ok_or(LinuxError::ESRCH)?;
        let matches = wait_child_matches(pid, caller_pgid);
        if let Some(child) = process_mgr.find_zombie_child(parent_pid, &matches) {
            unsafe {
                *rusage = pcb_to_rusage(&child);
            }
        } else {
            unsafe {
                core::ptr::write_bytes(rusage, 0, 1);
            }
        }
    }

    waitpid(pid, wstatus, options)
}

/// exit - terminate current process
pub fn exit(status: i32) -> ! {
    inc_ops();

    let pid = process::current_pid();
    let _ = process::get_process_manager().terminate_process(pid, status);

    loop {
        x86_64::instructions::hlt();
    }
}

//
// Process Identity Operations
//

/// getpid - get process ID
pub fn getpid() -> Pid {
    inc_ops();
    process::current_pid() as Pid
}

/// getppid - get parent process ID
pub fn getppid() -> Pid {
    inc_ops();

    match current_pcb() {
        Ok(pcb) => pcb.parent_pid.unwrap_or(0) as Pid,
        Err(_) => 0, // Return 0 if cannot get PCB
    }
}

//
// User/Group ID Operations
//

/// getuid - get real user ID
pub fn getuid() -> Uid {
    inc_ops();
    current_pcb().map(|pcb| pcb.uid).unwrap_or(0)
}

/// geteuid - get effective user ID
pub fn geteuid() -> Uid {
    inc_ops();
    current_pcb().map(|pcb| pcb.euid).unwrap_or(0)
}

/// getgid - get real group ID
pub fn getgid() -> Gid {
    inc_ops();
    current_pcb().map(|pcb| pcb.gid).unwrap_or(0)
}

/// getegid - get effective group ID
pub fn getegid() -> Gid {
    inc_ops();
    current_pcb().map(|pcb| pcb.egid).unwrap_or(0)
}

/// setuid - set user ID
pub fn setuid(uid: Uid) -> LinuxResult<i32> {
    inc_ops();

    let pid = process::current_pid();
    let process_mgr = process::get_process_manager();
    let pcb = current_pcb()?;

    if pcb.euid != 0 && uid != pcb.uid {
        return Err(LinuxError::EPERM);
    }

    process_mgr
        .with_process_mut(pid, |pcb| {
            if uid == pcb.uid {
                pcb.euid = uid;
            } else if pcb.euid == 0 {
                pcb.uid = uid;
                pcb.euid = uid;
            }
        })
        .ok_or(LinuxError::ESRCH)?;

    Ok(0)
}

/// seteuid - set effective user ID
pub fn seteuid(uid: Uid) -> LinuxResult<i32> {
    inc_ops();

    let pid = process::current_pid();
    let process_mgr = process::get_process_manager();
    let pcb = current_pcb()?;

    if pcb.euid != 0 && uid != pcb.uid && uid != pcb.euid {
        return Err(LinuxError::EPERM);
    }

    process_mgr
        .with_process_mut(pid, |pcb| {
            pcb.euid = uid;
        })
        .ok_or(LinuxError::ESRCH)?;

    Ok(0)
}

/// setgid - set group ID
pub fn setgid(gid: Gid) -> LinuxResult<i32> {
    inc_ops();

    let pid = process::current_pid();
    let process_mgr = process::get_process_manager();
    let pcb = current_pcb()?;

    if pcb.euid != 0 && gid != pcb.gid {
        return Err(LinuxError::EPERM);
    }

    process_mgr
        .with_process_mut(pid, |pcb| {
            if gid == pcb.gid {
                pcb.egid = gid;
            } else if pcb.euid == 0 {
                pcb.gid = gid;
                pcb.egid = gid;
            }
        })
        .ok_or(LinuxError::ESRCH)?;

    Ok(0)
}

/// setegid - set effective group ID
pub fn setegid(gid: Gid) -> LinuxResult<i32> {
    inc_ops();

    let pid = process::current_pid();
    let process_mgr = process::get_process_manager();
    let pcb = current_pcb()?;

    if pcb.euid != 0 && gid != pcb.gid && gid != pcb.egid {
        return Err(LinuxError::EPERM);
    }

    process_mgr
        .with_process_mut(pid, |pcb| {
            pcb.egid = gid;
        })
        .ok_or(LinuxError::ESRCH)?;

    Ok(0)
}

//
// Process Group and Session Operations
//

/// getpgid - get process group ID
pub fn getpgid(pid: Pid) -> LinuxResult<Pid> {
    inc_ops();

    let target_pid = if pid == 0 {
        process::current_pid()
    } else {
        pid as u32
    };

    get_pcb(target_pid).map(|pcb| pcb.pgid as i32)
}

/// setpgid - set process group ID
pub fn setpgid(pid: Pid, pgid: Pid) -> LinuxResult<i32> {
    inc_ops();

    if pid < 0 || pgid < 0 {
        return Err(LinuxError::EINVAL);
    }

    let target_pid = if pid == 0 {
        process::current_pid()
    } else {
        pid as u32
    };

    let new_pgid = pgid as u32;
    let process_mgr = process::get_process_manager();

    // Verify target process exists
    let _ = get_pcb(target_pid)?;

    process_mgr
        .with_process_mut(target_pid, |pcb| {
            pcb.pgid = new_pgid;
        })
        .ok_or(LinuxError::ESRCH)?;

    Ok(0)
}

/// getsid - get session ID
pub fn getsid(pid: Pid) -> LinuxResult<Pid> {
    inc_ops();

    let target_pid = if pid == 0 {
        process::current_pid()
    } else {
        pid as u32
    };

    get_pcb(target_pid).map(|pcb| pcb.sid as i32)
}

/// setsid - create new session
pub fn setsid() -> LinuxResult<Pid> {
    inc_ops();

    let pid = process::current_pid();
    let process_mgr = process::get_process_manager();
    let pcb = current_pcb()?;

    if pcb.sid == pid {
        return Err(LinuxError::EPERM);
    }

    process_mgr
        .with_process_mut(pid, |pcb| {
            pcb.sid = pid;
            pcb.pgid = pid;
        })
        .ok_or(LinuxError::ESRCH)?;

    Ok(pid as i32)
}

/// getpgrp - get process group
pub fn getpgrp() -> Pid {
    inc_ops();
    current_pcb().map(|pcb| pcb.pgid as i32).unwrap_or(0)
}

//
// Scheduling and Priority Operations
//

/// sched_yield - yield the processor
pub fn sched_yield() -> LinuxResult<i32> {
    inc_ops();

    // Use scheduler's yield function
    process::scheduler::yield_cpu();
    Ok(0)
}

/// getpriority - get scheduling priority
pub fn getpriority(which: i32, who: i32) -> LinuxResult<i32> {
    inc_ops();

    const PRIO_PROCESS: i32 = 0;
    const PRIO_PGRP: i32 = 1;
    const PRIO_USER: i32 = 2;

    let process_mgr = process::get_process_manager();

    match which {
        PRIO_PROCESS => {
            let target_pid = if who == 0 {
                process::current_pid()
            } else {
                who as u32
            };

            if let Some(priority) = process::scheduler::get_process_priority(target_pid) {
                Ok(process::priority_to_nice(priority))
            } else {
                Err(LinuxError::ESRCH)
            }
        }
        PRIO_PGRP => {
            let target_pgid = if who == 0 {
                current_pcb()?.pgid
            } else {
                who as u32
            };

            process_mgr
                .max_nice_among(|pcb| pcb.pgid == target_pgid)
                .ok_or(LinuxError::ESRCH)
        }
        PRIO_USER => {
            let target_uid = if who == 0 {
                getuid()
            } else {
                who as u32
            };

            process_mgr
                .max_nice_among(|pcb| pcb.uid == target_uid)
                .ok_or(LinuxError::ESRCH)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// setpriority - set scheduling priority
pub fn setpriority(which: i32, who: i32, prio: i32) -> LinuxResult<i32> {
    inc_ops();

    const PRIO_PROCESS: i32 = 0;
    const PRIO_PGRP: i32 = 1;
    const PRIO_USER: i32 = 2;

    if prio < -20 || prio > 19 {
        return Err(LinuxError::EINVAL);
    }

    let priority = process::nice_to_priority(prio);
    let process_mgr = process::get_process_manager();

    match which {
        PRIO_PROCESS => {
            let target_pid = if who == 0 {
                process::current_pid()
            } else {
                who as u32
            };

            let current_uid = getuid();
            if current_uid != 0 && prio < 0 {
                return Err(LinuxError::EACCES);
            }

            process::scheduler::set_process_priority(target_pid, priority)
                .map_err(|_| LinuxError::ESRCH)?;

            process_mgr
                .with_process_mut(target_pid, |pcb| {
                    pcb.priority = priority;
                })
                .ok_or(LinuxError::ESRCH)?;

            Ok(0)
        }
        PRIO_PGRP => {
            let current_uid = getuid();
            if current_uid != 0 && prio < 0 {
                return Err(LinuxError::EACCES);
            }

            let target_pgid = if who == 0 {
                current_pcb()?.pgid
            } else {
                who as u32
            };

            process_mgr
                .set_priority_among(|pcb| pcb.pgid == target_pgid, priority)
                .map_err(|_| LinuxError::ESRCH)?;
            Ok(0)
        }
        PRIO_USER => {
            let current_uid = getuid();
            if current_uid != 0 && prio < 0 {
                return Err(LinuxError::EACCES);
            }

            let target_uid = if who == 0 {
                getuid()
            } else {
                who as u32
            };

            process_mgr
                .set_priority_among(|pcb| pcb.uid == target_uid, priority)
                .map_err(|_| LinuxError::ESRCH)?;
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

/// nice - change process priority
pub fn nice(inc: i32) -> LinuxResult<i32> {
    inc_ops();

    let pid = process::current_pid();

    // Get current priority
    let current_nice = getpriority(0, 0)?;
    let new_nice = (current_nice + inc).clamp(-20, 19);

    // Set new priority
    setpriority(0, pid as i32, new_nice)?;

    Ok(new_nice)
}

//
// CPU Affinity Operations
//

/// sched_setaffinity - set CPU affinity
pub fn sched_setaffinity(pid: Pid, cpusetsize: usize, mask: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if mask.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if cpusetsize == 0 || cpusetsize > 128 {
        return Err(LinuxError::EINVAL);
    }

    let target_pid = if pid == 0 {
        process::current_pid()
    } else {
        pid as u32
    };

    // Verify process exists
    let _ = get_pcb(target_pid)?;

    // Read CPU mask from user space
    let mut cpu_mask: u64 = 0;
    unsafe {
        let bytes = core::slice::from_raw_parts(mask, core::cmp::min(cpusetsize, 8));
        for (i, &byte) in bytes.iter().enumerate() {
            cpu_mask |= (byte as u64) << (i * 8);
        }
    }

    // Validate mask has at least one CPU
    if cpu_mask == 0 {
        return Err(LinuxError::EINVAL);
    }

    let process_mgr = process::get_process_manager();
    process_mgr
        .with_process_mut(target_pid, |pcb| {
            pcb.sched_info.cpu_affinity = cpu_mask;
        })
        .ok_or(LinuxError::ESRCH)?;

    Ok(0)
}

/// sched_getaffinity - get CPU affinity
pub fn sched_getaffinity(pid: Pid, cpusetsize: usize, mask: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    if mask.is_null() {
        return Err(LinuxError::EFAULT);
    }

    if cpusetsize == 0 {
        return Err(LinuxError::EINVAL);
    }

    let target_pid = if pid == 0 {
        process::current_pid()
    } else {
        pid as u32
    };

    // Get CPU affinity from PCB
    let pcb = get_pcb(target_pid)?;
    let cpu_affinity = pcb.sched_info.cpu_affinity;

    // Write affinity mask to user space
    unsafe {
        let bytes = core::slice::from_raw_parts_mut(mask, cpusetsize);
        for (i, byte) in bytes.iter_mut().enumerate() {
            if i < 8 {
                *byte = ((cpu_affinity >> (i * 8)) & 0xFF) as u8;
            } else {
                *byte = 0;
            }
        }
    }

    Ok(core::cmp::min(cpusetsize, 8) as i32)
}

//
// Resource Usage Operations
//

/// getrusage - get resource usage
pub fn getrusage(who: i32, usage: *mut Rusage) -> LinuxResult<i32> {
    inc_ops();

    if usage.is_null() {
        return Err(LinuxError::EFAULT);
    }

    // WHO constants
    const RUSAGE_SELF: i32 = 0;
    const RUSAGE_CHILDREN: i32 = -1;
    const RUSAGE_THREAD: i32 = 1;

    match who {
        RUSAGE_SELF => {
            let pcb = current_pcb()?;
            unsafe {
                *usage = pcb_to_rusage(&pcb);
            }
            Ok(0)
        }
        RUSAGE_CHILDREN => {
            let pcb = current_pcb()?;
            unsafe {
                *usage = pcb_children_rusage(&pcb);
            }
            Ok(0)
        }
        RUSAGE_THREAD => {
            let pcb = current_pcb()?;
            unsafe {
                *usage = pcb_to_rusage(&pcb);
            }
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

//
// Process Control Operations
//

/// prctl - process control operations
pub fn prctl(option: i32, arg2: u64, _arg3: u64, _arg4: u64, _arg5: u64) -> LinuxResult<i32> {
    inc_ops();

    // Common prctl options
    const PR_SET_NAME: i32 = 15;
    const PR_GET_NAME: i32 = 16;
    const PR_SET_DUMPABLE: i32 = 4;
    const PR_GET_DUMPABLE: i32 = 3;
    const PR_SET_PDEATHSIG: i32 = 1;
    const PR_GET_PDEATHSIG: i32 = 2;

    match option {
        PR_SET_NAME => {
            let name_ptr = arg2 as *const u8;
            if name_ptr.is_null() {
                return Err(LinuxError::EFAULT);
            }

            let mut name_buf = [0u8; 16];
            unsafe {
                for (i, slot) in name_buf.iter_mut().enumerate() {
                    let byte = *name_ptr.add(i);
                    *slot = byte;
                    if byte == 0 {
                        break;
                    }
                }
            }

            let pid = process::current_pid();
            process::get_process_manager()
                .with_process_mut(pid, |pcb| {
                    pcb.name = [0u8; 32];
                    let copy_len = core::cmp::min(16, pcb.name.len());
                    pcb.name[..copy_len].copy_from_slice(&name_buf[..copy_len]);
                })
                .ok_or(LinuxError::ESRCH)?;

            Ok(0)
        }
        PR_GET_NAME => {
            let name_ptr = arg2 as *mut u8;
            if name_ptr.is_null() {
                return Err(LinuxError::EFAULT);
            }

            let pcb = current_pcb()?;
            unsafe {
                let dest = core::slice::from_raw_parts_mut(name_ptr, 16);
                dest.fill(0);
                let copy_len = core::cmp::min(dest.len(), pcb.name.len());
                dest[..copy_len].copy_from_slice(&pcb.name[..copy_len]);
            }
            Ok(0)
        }
        PR_SET_DUMPABLE => {
            let dumpable = arg2 != 0;
            let pid = process::current_pid();
            process::get_process_manager()
                .with_process_mut(pid, |pcb| {
                    pcb.dumpable = dumpable;
                })
                .ok_or(LinuxError::ESRCH)?;
            Ok(0)
        }
        PR_GET_DUMPABLE => {
            let pcb = current_pcb()?;
            Ok(if pcb.dumpable { 1 } else { 0 })
        }
        PR_SET_PDEATHSIG => {
            let sig = arg2 as u32;
            let pid = process::current_pid();
            process::get_process_manager()
                .with_process_mut(pid, |pcb| {
                    pcb.parent_death_signal = sig;
                })
                .ok_or(LinuxError::ESRCH)?;
            Ok(0)
        }
        PR_GET_PDEATHSIG => {
            let sig_ptr = arg2 as *mut i32;
            if sig_ptr.is_null() {
                return Err(LinuxError::EFAULT);
            }

            let pcb = current_pcb()?;
            unsafe {
                *sig_ptr = pcb.parent_death_signal as i32;
            }
            Ok(0)
        }
        _ => Err(LinuxError::EINVAL),
    }
}

//
// Capability Operations
//

fn caps_for_pcb(pcb: &process::ProcessControlBlock) -> (u32, u32, u32) {
    (
        pcb.cap_effective as u32,
        pcb.cap_permitted as u32,
        pcb.cap_inheritable as u32,
    )
}

fn read_cap_header(hdrp: *mut u8) -> Result<(u32, i32), LinuxError> {
    unsafe {
        let header = core::ptr::read(hdrp as *const CapUserHeader);
        if header.version != CAP_VERSION_1 {
            return Err(LinuxError::EINVAL);
        }
        Ok((header.version, header.pid))
    }
}

fn read_cap_data(datap: *const u8) -> Result<(u32, u32, u32), LinuxError> {
    unsafe {
        let data = core::ptr::read(datap as *const CapUserData);
        Ok((data.effective, data.permitted, data.inheritable))
    }
}

fn write_cap_data(datap: *mut u8, effective: u32, permitted: u32, inheritable: u32) -> Result<(), LinuxError> {
    if datap.is_null() {
        return Err(LinuxError::EFAULT);
    }
    unsafe {
        core::ptr::write(
            datap as *mut CapUserData,
            CapUserData {
                effective,
                permitted,
                inheritable,
            },
        );
    }
    Ok(())
}

/// capget - get process capabilities
pub fn capget(hdrp: *mut u8, datap: *mut u8) -> LinuxResult<i32> {
    inc_ops();

    if hdrp.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let (_version, target_pid) = read_cap_header(hdrp)?;
    let pid = if target_pid == 0 {
        process::current_pid()
    } else if target_pid < 0 {
        return Err(LinuxError::EINVAL);
    } else {
        target_pid as u32
    };

    let pcb = get_pcb(pid)?;
    let (effective, permitted, inheritable) = caps_for_pcb(&pcb);
    write_cap_data(datap, effective, permitted, inheritable)?;
    Ok(0)
}

/// capset - set process capabilities
pub fn capset(hdrp: *const u8, datap: *const u8) -> LinuxResult<i32> {
    inc_ops();

    if hdrp.is_null() || datap.is_null() {
        return Err(LinuxError::EFAULT);
    }

    let header = unsafe { core::ptr::read(hdrp as *const CapUserHeader) };
    if header.version != CAP_VERSION_1 {
        return Err(LinuxError::EINVAL);
    }

    let pid = if header.pid == 0 {
        process::current_pid()
    } else if header.pid < 0 {
        return Err(LinuxError::EINVAL);
    } else {
        header.pid as u32
    };

    let caller = current_pcb()?;
    if caller.euid != 0 && (caller.cap_effective & CAP_SETPCAP) == 0 {
        return Err(LinuxError::EPERM);
    }

    let (effective, permitted, inheritable) = read_cap_data(datap)?;
    if (effective & !permitted) != 0 {
        return Err(LinuxError::EPERM);
    }

    process::get_process_manager()
        .with_process_mut(pid, |pcb| {
            pcb.cap_effective = effective as u64;
            pcb.cap_permitted = permitted as u64;
            pcb.cap_inheritable = inheritable as u64;
        })
        .ok_or(LinuxError::ESRCH)?;

    Ok(0)
}

//
// Process Times Operations
//

/// times - get process times
pub fn times(buf: *mut u8) -> LinuxResult<i64> {
    inc_ops();

    if !buf.is_null() {
        let pcb = current_pcb()?;

        unsafe {
            let tms = buf as *mut i64;
            *tms.offset(0) = pcb_user_ticks(&pcb) as i64;
            *tms.offset(1) = pcb.system_time_ticks as i64;
            *tms.offset(2) = pcb.child_user_time as i64;
            *tms.offset(3) = pcb.child_system_time as i64;
        }
    }

    // Return clock ticks since boot
    let uptime_ms = process::get_system_time();
    let clock_ticks = uptime_ms / 10; // Assume 100Hz clock
    Ok(clock_ticks as i64)
}

/// execveat - execute program relative to a directory fd
pub fn execveat(
    dirfd: Fd,
    pathname: *const u8,
    argv: *const *const u8,
    envp: *const *const u8,
    _flags: i32,
) -> LinuxResult<i32> {
    inc_ops();

    let path_str = unsafe { c_str_to_string(pathname)? };
    if path_str.is_empty() {
        return Err(LinuxError::EINVAL);
    }

    // If pathname is absolute or dirfd is AT_FDCWD, delegate to execve
    if path_str.starts_with('/') || dirfd == -100 {
        // AT_FDCWD is -100
        return execve(pathname, argv, envp);
    }

    // Otherwise, verify dirfd is a directory
    match crate::vfs::vfs_fstat(dirfd) {
        Ok(stat) => {
            if stat.inode_type != crate::vfs::InodeType::Directory {
                return Err(LinuxError::ENOTDIR);
            }
        }
        Err(_) => return Err(LinuxError::EBADF),
    }

    // Construct full path relative to CWD (as done in openat)
    let pid = process::current_pid();
    let cwd = process::get_process_manager()
        .get_process(pid)
        .map(|pcb| pcb.cwd.clone())
        .ok_or(LinuxError::ESRCH)?;

    let full_path = if cwd.ends_with('/') {
        alloc::format!("{}{}", cwd, path_str)
    } else {
        alloc::format!("{}/{}", cwd, path_str)
    };

    // Allocate c-string for full_path
    let full_path_raw = alloc::format!("{}\0", full_path);
    execve(full_path_raw.as_ptr(), argv, envp)
}

/// vfork - create child process and block parent
pub fn vfork() -> LinuxResult<Pid> {
    inc_ops();
    fork()
}

#[cfg(any())]
mod tests {
    use super::*;

    #[test_case]
    fn test_getpid() {
        let pid = getpid();
        assert!(pid >= 0);
    }

    #[test_case]
    fn test_uid_gid_operations() {
        let uid = getuid();
        let gid = getgid();
        assert!(uid == 0); // Root for now
        assert!(gid == 0); // Root group

        let euid = geteuid();
        let egid = getegid();
        assert_eq!(uid, euid);
        assert_eq!(gid, egid);
    }

    #[test_case]
    fn test_process_group_operations() {
        let pid = getpid();
        let pgid = getpgid(0).unwrap();
        assert!(pgid > 0);

        let pgrp = getpgrp();
        assert!(pgrp > 0);
    }

    #[test_case]
    fn test_priority_operations() {
        assert!(sched_yield().is_ok());

        let prio = getpriority(0, 0);
        assert!(prio.is_ok());
    }

    #[test_case]
    fn test_session_operations() {
        let sid = getsid(0);
        assert!(sid.is_ok());
    }
}
