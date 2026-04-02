//! Linux Syscall Interrupt Handler
//!
//! This module provides the INT 0x80 syscall handler that bridges
//! user-space Linux syscalls to kernel implementations.

use x86_64::structures::idt::InterruptStackFrame;

/// Linux syscall numbers (x86_64 calling convention)
#[repr(u64)]
#[derive(Debug, Clone, Copy)]
pub enum LinuxSyscall {
    Read = 0,
    Write = 1,
    Open = 2,
    Close = 3,
    Stat = 4,
    Fstat = 5,
    Lstat = 6,
    Poll = 7,
    Lseek = 8,
    Mmap = 9,
    Mprotect = 10,
    Munmap = 11,
    Brk = 12,
    // ... more syscalls
    Fork = 57,
    Execve = 59,
    Exit = 60,
    Wait4 = 61,
    // IPC
    Msgget = 68,
    Msgsnd = 69,
    Msgrcv = 70,
    Semget = 64,
    Semop = 65,
    Shmget = 29,
    Shmat = 30,
    Shmdt = 67,
}

/// Syscall dispatcher - routes syscalls to appropriate handlers
///
/// Syscall arguments are passed in registers (System V AMD64 ABI):
/// - rax: syscall number
/// - rdi: arg1
/// - rsi: arg2
/// - rdx: arg3
/// - r10: arg4
/// - r8: arg5
/// - r9: arg6
///
/// Return value in rax
pub fn dispatch_syscall(
    syscall_num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
) -> i64 {
    match syscall_num {
        // File operations
        0 => syscall_read(arg1 as i32, arg2 as *mut u8, arg3 as usize),
        1 => syscall_write(arg1 as i32, arg2 as *const u8, arg3 as usize),
        2 => syscall_open(arg1 as *const u8, arg2 as i32, arg3 as u32),
        3 => syscall_close(arg1 as i32),
        4 => syscall_stat(arg1 as *const u8, arg2 as *mut u8),
        5 => syscall_fstat(arg1 as i32, arg2 as *mut u8),
        6 => syscall_lstat(arg1 as *const u8, arg2 as *mut u8),
        8 => syscall_lseek(arg1 as i32, arg2 as i64, arg3 as i32),

        // Memory operations
        9 => syscall_mmap(arg1 as *mut u8, arg2 as usize, arg3 as i32, arg4 as i32, arg5 as i32, arg6 as i64),
        10 => syscall_mprotect(arg1 as *mut u8, arg2 as usize, arg3 as i32),
        11 => syscall_munmap(arg1 as *mut u8, arg2 as usize),
        12 => syscall_brk(arg1 as *mut u8),

        // Process operations
        57 => syscall_fork(),
        59 => syscall_execve(arg1 as *const u8, arg2 as *const *const u8, arg3 as *const *const u8),
        60 => syscall_exit(arg1 as i32),
        61 => syscall_wait4(arg1 as i32, arg2 as *mut i32, arg3 as i32, arg4 as *mut u8),

        // IPC operations
        29 => syscall_shmget(arg1 as i32, arg2 as usize, arg3 as i32),
        30 => syscall_shmat(arg1 as i32, arg2 as *const u8, arg3 as i32),
        64 => syscall_semget(arg1 as i32, arg2 as i32, arg3 as i32),
        65 => syscall_semop(arg1 as i32, arg2 as *mut u8, arg3 as usize),
        67 => syscall_shmdt(arg1 as *const u8),
        68 => syscall_msgget(arg1 as i32, arg2 as i32),
        69 => syscall_msgsnd(arg1 as i32, arg2 as *const u8, arg3 as usize, arg4 as i32),
        70 => syscall_msgrcv(arg1 as i32, arg2 as *mut u8, arg3 as usize, arg4 as i64, arg5 as i32),

        _ => {
            // Unknown syscall - return ENOSYS (-38)
            -38
        }
    }
}

// Syscall implementations - these call into linux_compat

fn syscall_read(fd: i32, buf: *mut u8, count: usize) -> i64 {
    match crate::linux_compat::file_ops::read(fd, buf, count) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_write(fd: i32, buf: *const u8, count: usize) -> i64 {
    match crate::linux_compat::file_ops::write(fd, buf, count) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_open(pathname: *const u8, flags: i32, mode: u32) -> i64 {
    match crate::linux_compat::file_ops::open(pathname, flags, mode) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_close(fd: i32) -> i64 {
    match crate::linux_compat::file_ops::close(fd) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_stat(pathname: *const u8, statbuf: *mut u8) -> i64 {
    match crate::linux_compat::file_ops::stat(pathname, statbuf as *mut crate::linux_compat::file_ops::Stat) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_fstat(fd: i32, statbuf: *mut u8) -> i64 {
    match crate::linux_compat::file_ops::fstat(fd, statbuf as *mut crate::linux_compat::file_ops::Stat) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_lstat(pathname: *const u8, statbuf: *mut u8) -> i64 {
    match crate::linux_compat::file_ops::lstat(pathname, statbuf as *mut crate::linux_compat::file_ops::Stat) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_lseek(fd: i32, offset: i64, whence: i32) -> i64 {
    match crate::linux_compat::file_ops::lseek(fd, offset, whence) {
        Ok(pos) => pos,
        Err(e) => -(e as i64),
    }
}

fn syscall_mmap(addr: *mut u8, length: usize, prot: i32, flags: i32, fd: i32, offset: i64) -> i64 {
    match crate::linux_compat::memory_ops::mmap(addr, length, prot, flags, fd, offset) {
        Ok(ptr) => ptr as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_mprotect(addr: *mut u8, len: usize, prot: i32) -> i64 {
    match crate::linux_compat::memory_ops::mprotect(addr, len, prot) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_munmap(addr: *mut u8, length: usize) -> i64 {
    match crate::linux_compat::memory_ops::munmap(addr, length) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_brk(addr: *mut u8) -> i64 {
    match crate::linux_compat::memory_ops::brk(addr) {
        Ok(new_brk) => new_brk as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_fork() -> i64 {
    match crate::linux_compat::process_ops::fork() {
        Ok(pid) => pid as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_execve(filename: *const u8, argv: *const *const u8, envp: *const *const u8) -> i64 {
    match crate::linux_compat::process_ops::execve(filename, argv, envp) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_exit(status: i32) -> i64 {
    crate::linux_compat::process_ops::exit(status);
    0 // Never returns
}

fn syscall_wait4(pid: i32, wstatus: *mut i32, options: i32, rusage: *mut u8) -> i64 {
    match crate::linux_compat::process_ops::wait4(pid, wstatus, options, rusage as *mut crate::linux_compat::process_ops::Rusage) {
        Ok(pid) => pid as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_msgget(key: i32, msgflg: i32) -> i64 {
    match crate::linux_compat::ipc_ops::msgget(key, msgflg) {
        Ok(id) => id as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_msgsnd(msqid: i32, msgp: *const u8, msgsz: usize, msgflg: i32) -> i64 {
    match crate::linux_compat::ipc_ops::msgsnd(msqid, msgp, msgsz, msgflg) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_msgrcv(msqid: i32, msgp: *mut u8, msgsz: usize, msgtyp: i64, msgflg: i32) -> i64 {
    match crate::linux_compat::ipc_ops::msgrcv(msqid, msgp, msgsz, msgtyp, msgflg) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_semget(key: i32, nsems: i32, semflg: i32) -> i64 {
    match crate::linux_compat::ipc_ops::semget(key, nsems, semflg) {
        Ok(id) => id as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_semop(semid: i32, sops: *mut u8, nsops: usize) -> i64 {
    match crate::linux_compat::ipc_ops::semop(semid, sops, nsops) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_shmget(key: i32, size: usize, shmflg: i32) -> i64 {
    match crate::linux_compat::ipc_ops::shmget(key, size, shmflg) {
        Ok(id) => id as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_shmat(shmid: i32, shmaddr: *const u8, shmflg: i32) -> i64 {
    match crate::linux_compat::ipc_ops::shmat(shmid, shmaddr, shmflg) {
        Ok(addr) => addr as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_shmdt(shmaddr: *const u8) -> i64 {
    match crate::linux_compat::ipc_ops::shmdt(shmaddr) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

/// INT 0x80 handler entry point
///
/// This handler extracts syscall arguments from registers following
/// the Linux x86_64 syscall convention and dispatches to the appropriate handler.
///
/// Register convention (System V AMD64 ABI):
/// - RAX: syscall number
/// - RDI: arg1
/// - RSI: arg2
/// - RDX: arg3
/// - R10: arg4 (note: not RCX, which is clobbered by syscall instruction)
/// - R8:  arg5
/// - R9:  arg6
///
/// Return value goes in RAX
pub extern "x86-interrupt" fn syscall_0x80_handler(_stack_frame: InterruptStackFrame) {
    // Extract syscall arguments from registers using inline assembly
    let syscall_num: u64;
    let arg1: u64;
    let arg2: u64;
    let arg3: u64;
    let arg4: u64;
    let arg5: u64;
    let arg6: u64;

    unsafe {
        core::arch::asm!(
            // Save all registers we need to read
            "mov {syscall_num}, rax",
            "mov {arg1}, rdi",
            "mov {arg2}, rsi",
            "mov {arg3}, rdx",
            "mov {arg4}, r10",
            "mov {arg5}, r8",
            "mov {arg6}, r9",
            syscall_num = out(reg) syscall_num,
            arg1 = out(reg) arg1,
            arg2 = out(reg) arg2,
            arg3 = out(reg) arg3,
            arg4 = out(reg) arg4,
            arg5 = out(reg) arg5,
            arg6 = out(reg) arg6,
            options(nostack, preserves_flags)
        );
    }

    // Validate we're coming from user mode
    let from_user_mode = crate::usermode::in_user_mode();

    if from_user_mode {
        // Ensure we're now in kernel mode after the interrupt
        // The interrupt handler has already switched to kernel segments
        crate::serial_println!("Syscall {} from user mode", syscall_num);
    }

    // Dispatch the syscall
    let result = dispatch_syscall(syscall_num, arg1, arg2, arg3, arg4, arg5, arg6);

    // Write result back to RAX for return to caller
    unsafe {
        core::arch::asm!(
            "mov rax, {result}",
            result = in(reg) result,
            options(nostack, preserves_flags)
        );
    }

    // Note: We don't send EOI for software interrupts (INT 0x80)
    // The interrupt return (iretq) will handle the return to user mode automatically
}
