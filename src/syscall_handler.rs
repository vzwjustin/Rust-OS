#![allow(unused)]
//! Linux Syscall Interrupt Handler
//!
//! This module provides the INT 0x80 syscall handler that bridges
//! user-space Linux syscalls to kernel implementations.

use crate::syscall::SyscallNumber;
use x86_64::structures::idt::InterruptStackFrame;

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
    crate::performance_monitor::record_syscall();

    let dispatch_addr = crate::syscall_handler::dispatch_syscall as *const () as u64;
    crate::kprobes::run_probes_at(dispatch_addr, syscall_num);

    // Seccomp check — filter syscalls before dispatching
    let args = [arg1, arg2, arg3, arg4, arg5, arg6];
    crate::audit::audit_syscall_entry(syscall_num as i32, &args);
    if let Err(errno) = crate::seccomp::check_syscall(syscall_num as i32, &args) {
        if crate::audit::is_enabled() {
            crate::audit::audit_log_syscall(syscall_num as i32, &args, errno as i64);
        }
        return errno as i64;
    }

    if crate::trace::tracing_on() {
        crate::trace::tracepoint_emit("syscalls:sys_enter", syscall_num);
    }
    if crate::trace::function_tracer_enabled() {
        crate::trace::record_function_trace("dispatch_syscall", dispatch_addr, syscall_num);
    }

    let pid = crate::process::current_pid();
    if crate::ptrace::is_traced(pid) {
        crate::ptrace::syscall_event(pid, true);
    }

    let result = match crate::linux_integration::route_syscall(syscall_num, &args) {
        Ok(v) => v as i64,
        Err(e) => -(e as i64),
    };

    if crate::audit::is_enabled() {
        crate::audit::audit_log_syscall(syscall_num as i32, &args, result);
    }
    if crate::trace::tracing_on() {
        crate::trace::tracepoint_emit("syscalls:sys_exit", result as u64);
    }

    crate::debug::trace_syscall(syscall_num, arg1, arg2, arg3, arg4, arg5, arg6, result);

    // Ptrace syscall exit notification
    if crate::ptrace::is_traced(pid) {
        crate::ptrace::syscall_event(pid, false);
    }

    result
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
    match crate::linux_compat::file_ops::stat(
        pathname,
        statbuf as *mut crate::linux_compat::file_ops::Stat,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_fstat(fd: i32, statbuf: *mut u8) -> i64 {
    match crate::linux_compat::file_ops::fstat(
        fd,
        statbuf as *mut crate::linux_compat::file_ops::Stat,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_lstat(pathname: *const u8, statbuf: *mut u8) -> i64 {
    match crate::linux_compat::file_ops::lstat(
        pathname,
        statbuf as *mut crate::linux_compat::file_ops::Stat,
    ) {
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
    if crate::user_sched::user_bootstrap_active() {
        crate::user_sched::complete_user_exit(status);
        return 0;
    }
    crate::linux_compat::process_ops::exit(status);
}

fn syscall_wait4(pid: i32, wstatus: *mut i32, options: i32, rusage: *mut u8) -> i64 {
    match crate::linux_compat::process_ops::wait4(
        pid,
        wstatus,
        options,
        rusage as *mut crate::linux_compat::process_ops::Rusage,
    ) {
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

fn syscall_getpid() -> i64 {
    crate::linux_compat::process_ops::getpid() as i64
}

fn syscall_getppid() -> i64 {
    crate::linux_compat::process_ops::getppid() as i64
}

fn syscall_gettid() -> i64 {
    crate::linux_compat::thread_ops::gettid() as i64
}

fn syscall_uname(buf: *mut u8) -> i64 {
    match crate::linux_compat::sysinfo_ops::uname(
        buf as *mut crate::linux_compat::sysinfo_ops::UtsName,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_clock_gettime(clockid: i32, tp: *mut u8) -> i64 {
    match crate::linux_compat::time_ops::clock_gettime(
        clockid,
        tp as *mut crate::linux_compat::TimeSpec,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_gettimeofday(tv: *mut u8, tz: *mut u8) -> i64 {
    match crate::linux_compat::time_ops::gettimeofday(tv as *mut crate::linux_compat::TimeVal, tz) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_nanosleep(req: *const u8, rem: *mut u8) -> i64 {
    match crate::linux_compat::time_ops::nanosleep(
        req as *const crate::linux_compat::TimeSpec,
        rem as *mut crate::linux_compat::TimeSpec,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_rt_sigaction(signum: i32, act: *const u8, oldact: *mut u8, sigsetsize: usize) -> i64 {
    match crate::linux_compat::signal_ops::rt_sigaction(
        signum,
        act as *const crate::linux_compat::SigAction,
        oldact as *mut crate::linux_compat::SigAction,
        sigsetsize,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_rt_sigprocmask(how: i32, set: *const u8, oldset: *mut u8, sigsetsize: usize) -> i64 {
    match crate::linux_compat::signal_ops::rt_sigprocmask(
        how,
        set as *const crate::linux_compat::SigSet,
        oldset as *mut crate::linux_compat::SigSet,
        sigsetsize,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_arch_prctl(code: i32, addr: u64) -> i64 {
    match crate::linux_compat::thread_ops::arch_prctl(code, addr) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_set_tid_address(tidptr: *mut i32) -> i64 {
    crate::linux_compat::thread_ops::set_tid_address(tidptr) as i64
}

fn syscall_futex(
    uaddr: *mut i32,
    futex_op: i32,
    val: i32,
    timeout: *const u8,
    uaddr2: *mut i32,
    val3: i32,
) -> i64 {
    // Use the dedicated futex module for full operation support
    let to = if timeout.is_null() {
        None
    } else {
        let ts = unsafe { &*(timeout as *const crate::linux_compat::TimeSpec) };
        Some(crate::futex::FutexTimeout::from_timespec(ts, false))
    };
    crate::futex::do_futex(uaddr, futex_op, val, to.as_ref(), uaddr2, val as i32, val3) as i64
}

fn syscall_clone(
    flags: u64,
    stack: *mut u8,
    parent_tid: *mut i32,
    child_tid: *mut i32,
    tls: u64,
) -> i64 {
    match crate::linux_compat::thread_ops::clone(flags, stack, parent_tid, child_tid, tls) {
        Ok(pid) => pid as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_openat(dirfd: i32, pathname: *const u8, flags: i32, mode: u32) -> i64 {
    match crate::linux_compat::file_ops::openat(dirfd, pathname, flags, mode) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_newfstatat(dirfd: i32, pathname: *const u8, statbuf: *mut u8, flags: i32) -> i64 {
    match crate::linux_compat::file_ops::newfstatat(
        dirfd,
        pathname,
        statbuf as *mut crate::linux_compat::types::Stat,
        flags,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_access(path: *const u8, mode: i32) -> i64 {
    match crate::linux_compat::file_ops::access(path, mode) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_readlink(path: *const u8, buf: *mut u8, bufsiz: usize) -> i64 {
    match crate::linux_compat::file_ops::readlink(path, buf, bufsiz) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_getcwd(buf: *mut u8, size: usize) -> i64 {
    match crate::linux_compat::file_ops::getcwd(buf, size) {
        Ok(ptr) => ptr as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_chdir(path: *const u8) -> i64 {
    match crate::linux_compat::file_ops::chdir(path) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_fcntl(fd: i32, cmd: i32, arg: u64) -> i64 {
    match crate::linux_compat::ioctl_ops::fcntl(fd, cmd, arg) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_ioctl(fd: i32, request: u64, argp: u64) -> i64 {
    match crate::linux_compat::ioctl_ops::ioctl(fd, request, argp) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_pipe(pipefd: *mut i32) -> i64 {
    match crate::linux_compat::ipc_ops::pipe(pipefd as *mut [i32; 2]) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_dup(oldfd: i32) -> i64 {
    match crate::linux_compat::file_ops::dup(oldfd) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_dup2(oldfd: i32, newfd: i32) -> i64 {
    match crate::linux_compat::file_ops::dup2(oldfd, newfd) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}

fn syscall_mkdir(path: *const u8, mode: u32) -> i64 {
    match crate::linux_compat::file_ops::mkdir(path, mode) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_unlink(path: *const u8) -> i64 {
    match crate::linux_compat::file_ops::unlink(path) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_getdents64(fd: i32, dirp: *mut u8, count: u32) -> i64 {
    match crate::linux_compat::advanced_io::getdents64(fd, dirp, count) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}

// ── Credential syscalls ──

fn syscall_getuid() -> i64 {
    crate::linux_compat::process_ops::getuid() as i64
}
fn syscall_geteuid() -> i64 {
    crate::linux_compat::process_ops::geteuid() as i64
}
fn syscall_getgid() -> i64 {
    crate::linux_compat::process_ops::getgid() as i64
}
fn syscall_getegid() -> i64 {
    crate::linux_compat::process_ops::getegid() as i64
}
fn syscall_setuid(uid: u32) -> i64 {
    match crate::linux_compat::process_ops::setuid(uid) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_setgid(gid: u32) -> i64 {
    match crate::linux_compat::process_ops::setgid(gid) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_setreuid(_ruid: u32, euid: u32) -> i64 {
    match crate::linux_compat::process_ops::seteuid(euid) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_setregid(_rgid: u32, egid: u32) -> i64 {
    match crate::linux_compat::process_ops::setegid(egid) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_setresuid(_ruid: u32, euid: u32, _suid: u32) -> i64 {
    match crate::linux_compat::process_ops::seteuid(euid) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_getresuid(ruid: *mut u32, euid: *mut u32, suid: *mut u32) -> i64 {
    unsafe {
        if !ruid.is_null() {
            *ruid = crate::linux_compat::process_ops::getuid();
        }
        if !euid.is_null() {
            *euid = crate::linux_compat::process_ops::geteuid();
        }
        if !suid.is_null() {
            *suid = crate::linux_compat::process_ops::geteuid();
        }
    }
    0
}
fn syscall_setresgid(_rgid: u32, egid: u32, _sgid: u32) -> i64 {
    match crate::linux_compat::process_ops::setegid(egid) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_getresgid(rgid: *mut u32, egid: *mut u32, sgid: *mut u32) -> i64 {
    unsafe {
        if !rgid.is_null() {
            *rgid = crate::linux_compat::process_ops::getgid();
        }
        if !egid.is_null() {
            *egid = crate::linux_compat::process_ops::getegid();
        }
        if !sgid.is_null() {
            *sgid = crate::linux_compat::process_ops::getegid();
        }
    }
    0
}
fn syscall_getgroups(size: i32, list: *mut u32) -> i64 {
    if size == 0 {
        return 1;
    }
    if size < 1 {
        return -(crate::linux_compat::LinuxError::EINVAL as i64);
    }
    unsafe {
        if !list.is_null() {
            *list = 0;
        }
    }
    1
}
fn syscall_setgroups(size: i32, _list: *const u32) -> i64 {
    if size < 0 {
        return -(crate::linux_compat::LinuxError::EINVAL as i64);
    }
    0
}
fn syscall_setpgid(pid: i32, pgid: i32) -> i64 {
    match crate::linux_compat::process_ops::setpgid(pid, pgid) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_getpgid(pid: i32) -> i64 {
    match crate::linux_compat::process_ops::getpgid(pid) {
        Ok(p) => p as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_getpgrp() -> i64 {
    crate::linux_compat::process_ops::getpgrp() as i64
}
fn syscall_setsid() -> i64 {
    match crate::linux_compat::process_ops::setsid() {
        Ok(p) => p as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_getsid(pid: i32) -> i64 {
    match crate::linux_compat::process_ops::getsid(pid) {
        Ok(p) => p as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_umask(mask: u32) -> i64 {
    match crate::linux_compat::process_ops::umask(mask) {
        Ok(old) => old as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_chroot(path: *const u8) -> i64 {
    match crate::linux_compat::process_ops::chroot(path) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

// ── Resource limit syscalls ──

fn syscall_getrlimit(resource: i32, rlim: *mut u8) -> i64 {
    match crate::linux_compat::resource_ops::getrlimit(
        resource,
        rlim as *mut crate::linux_compat::resource_ops::RLimit,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_setrlimit(resource: i32, rlim: *const u8) -> i64 {
    match crate::linux_compat::resource_ops::setrlimit(
        resource,
        rlim as *const crate::linux_compat::resource_ops::RLimit,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_prlimit64(pid: i32, resource: i32, new_limit: *const u8, old_limit: *mut u8) -> i64 {
    match crate::linux_compat::resource_ops::prlimit(
        pid,
        resource,
        new_limit as *const crate::linux_compat::resource_ops::RLimit,
        old_limit as *mut crate::linux_compat::resource_ops::RLimit,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_getrusage(who: i32, usage: *mut u8) -> i64 {
    match crate::linux_compat::process_ops::getrusage(
        who,
        usage as *mut crate::linux_compat::process_ops::Rusage,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_times(buf: *mut u8) -> i64 {
    match crate::linux_compat::process_ops::times(buf) {
        Ok(v) => v,
        Err(e) => -(e as i64),
    }
}
fn syscall_sysinfo(info: *mut u8) -> i64 {
    match crate::linux_compat::sysinfo_ops::sysinfo(
        info as *mut crate::linux_compat::sysinfo_ops::SysInfo,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_prctl(option: i32, arg2: u64, arg3: u64, arg4: u64, arg5: u64) -> i64 {
    match crate::linux_compat::process_ops::prctl(option, arg2, arg3, arg4, arg5) {
        Ok(v) => v as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_capget(hdrp: *mut u8, datap: *mut u8) -> i64 {
    match crate::linux_compat::process_ops::capget(hdrp, datap) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_capset(hdrp: *const u8, datap: *const u8) -> i64 {
    match crate::linux_compat::process_ops::capset(hdrp, datap) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

// ── Scheduling syscalls ──

fn syscall_sched_yield() -> i64 {
    match crate::linux_compat::process_ops::sched_yield() {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_sched_getaffinity(pid: i32, cpusetsize: usize, mask: *mut u8) -> i64 {
    match crate::linux_compat::process_ops::sched_getaffinity(pid, cpusetsize, mask) {
        Ok(v) => v as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_sched_setaffinity(pid: i32, cpusetsize: usize, mask: *const u8) -> i64 {
    match crate::linux_compat::process_ops::sched_setaffinity(pid, cpusetsize, mask) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_sched_setscheduler(pid: i32, policy: i32, param: *const u8) -> i64 {
    match crate::linux_compat::resource_ops::sched_setscheduler(
        pid,
        policy,
        param as *const crate::linux_compat::resource_ops::SchedParam,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_sched_getscheduler(pid: i32) -> i64 {
    match crate::linux_compat::resource_ops::sched_getscheduler(pid) {
        Ok(v) => v as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_sched_setparam(pid: i32, param: *const u8) -> i64 {
    match crate::linux_compat::resource_ops::sched_setparam(
        pid,
        param as *const crate::linux_compat::resource_ops::SchedParam,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_sched_getparam(pid: i32, param: *mut u8) -> i64 {
    match crate::linux_compat::resource_ops::sched_getparam(
        pid,
        param as *mut crate::linux_compat::resource_ops::SchedParam,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_sched_get_priority_max(policy: i32) -> i64 {
    match crate::linux_compat::resource_ops::sched_get_priority_max(policy) {
        Ok(v) => v as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_sched_get_priority_min(policy: i32) -> i64 {
    match crate::linux_compat::resource_ops::sched_get_priority_min(policy) {
        Ok(v) => v as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_sched_rr_get_interval(pid: i32, tp: *mut u8) -> i64 {
    match crate::linux_compat::resource_ops::sched_rr_get_interval(
        pid,
        tp as *mut crate::linux_compat::TimeSpec,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

// ── Extended file syscalls ──

fn syscall_pread64(fd: i32, buf: *mut u8, count: usize, offset: i64) -> i64 {
    match crate::linux_compat::advanced_io::pread(fd, buf, count, offset) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_pwrite64(fd: i32, buf: *const u8, count: usize, offset: i64) -> i64 {
    match crate::linux_compat::advanced_io::pwrite(fd, buf, count, offset) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_readv(fd: i32, iov: *const u8, iovcnt: usize) -> i64 {
    match crate::linux_compat::advanced_io::readv(
        fd,
        iov as *const crate::linux_compat::advanced_io::IoVec,
        iovcnt as i32,
    ) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_writev(fd: i32, iov: *const u8, iovcnt: usize) -> i64 {
    match crate::linux_compat::advanced_io::writev(
        fd,
        iov as *const crate::linux_compat::advanced_io::IoVec,
        iovcnt as i32,
    ) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_preadv(fd: i32, iov: *const u8, iovcnt: usize, offset: i64) -> i64 {
    match crate::linux_compat::advanced_io::preadv(
        fd,
        iov as *const crate::linux_compat::advanced_io::IoVec,
        iovcnt as i32,
        offset,
    ) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_pwritev(fd: i32, iov: *const u8, iovcnt: usize, offset: i64) -> i64 {
    match crate::linux_compat::advanced_io::pwritev(
        fd,
        iov as *const crate::linux_compat::advanced_io::IoVec,
        iovcnt as i32,
        offset,
    ) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_dup3(oldfd: i32, newfd: i32, flags: i32) -> i64 {
    match crate::linux_compat::file_ops::dup3(oldfd, newfd, flags) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_pipe2(pipefd: *mut i32, flags: i32) -> i64 {
    match crate::linux_compat::special_fd::pipe2(pipefd as *mut [i32; 2], flags) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_faccessat(dirfd: i32, path: *const u8, mode: i32) -> i64 {
    match crate::linux_compat::file_ops::faccessat(dirfd, path, mode, 0) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_faccessat2(dirfd: i32, path: *const u8, mode: i32, flags: i32) -> i64 {
    match crate::linux_compat::file_ops::faccessat(dirfd, path, mode, flags) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_readlinkat(dirfd: i32, path: *const u8, buf: *mut u8, bufsiz: usize) -> i64 {
    match crate::linux_compat::file_ops::readlinkat(dirfd, path, buf, bufsiz) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_fchmodat(dirfd: i32, path: *const u8, mode: u32) -> i64 {
    match crate::linux_compat::file_ops::fchmodat(dirfd, path, mode, 0) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_mkdirat(dirfd: i32, path: *const u8, mode: u32) -> i64 {
    match crate::linux_compat::file_ops::mkdirat(dirfd, path, mode) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_unlinkat(dirfd: i32, path: *const u8, flags: i32) -> i64 {
    match crate::linux_compat::file_ops::unlinkat(dirfd, path, flags) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_renameat(olddirfd: i32, oldpath: *const u8, newdirfd: i32, newpath: *const u8) -> i64 {
    match crate::linux_compat::file_ops::renameat(olddirfd, oldpath, newdirfd, newpath) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_renameat2(
    olddirfd: i32,
    oldpath: *const u8,
    newdirfd: i32,
    newpath: *const u8,
    flags: i32,
) -> i64 {
    match crate::linux_compat::file_ops::renameat2(
        olddirfd,
        oldpath,
        newdirfd,
        newpath,
        flags as u32,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_linkat(
    olddirfd: i32,
    oldpath: *const u8,
    newdirfd: i32,
    newpath: *const u8,
    flags: i32,
) -> i64 {
    match crate::linux_compat::file_ops::linkat(olddirfd, oldpath, newdirfd, newpath, flags) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_symlinkat(target: *const u8, newdirfd: i32, linkpath: *const u8) -> i64 {
    match crate::linux_compat::file_ops::symlinkat(target, newdirfd, linkpath) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_fchownat(dirfd: i32, path: *const u8, uid: u32, gid: u32, flags: i32) -> i64 {
    match crate::linux_compat::file_ops::fchownat(dirfd, path, uid, gid, flags) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_utimensat(dirfd: i32, path: *const u8, times: *const u8, flags: i32) -> i64 {
    match crate::linux_compat::file_ops::utimensat(
        dirfd,
        path,
        times as *const [crate::linux_compat::TimeSpec; 2],
        flags,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_rename(oldpath: *const u8, newpath: *const u8) -> i64 {
    match crate::linux_compat::file_ops::rename(oldpath, newpath) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_rmdir(path: *const u8) -> i64 {
    match crate::linux_compat::file_ops::rmdir(path) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_symlink(target: *const u8, linkpath: *const u8) -> i64 {
    match crate::linux_compat::file_ops::symlink(target, linkpath) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_link(oldpath: *const u8, newpath: *const u8) -> i64 {
    match crate::linux_compat::file_ops::link(oldpath, newpath) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_chmod(path: *const u8, mode: u32) -> i64 {
    match crate::linux_compat::file_ops::chmod(path, mode) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_fchmod(fd: i32, mode: u32) -> i64 {
    match crate::linux_compat::file_ops::fchmod(fd, mode) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_chown(path: *const u8, uid: u32, gid: u32) -> i64 {
    match crate::linux_compat::file_ops::chown(path, uid, gid) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_fchown(fd: i32, uid: u32, gid: u32) -> i64 {
    match crate::linux_compat::file_ops::fchown(fd, uid, gid) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_lchown(path: *const u8, uid: u32, gid: u32) -> i64 {
    match crate::linux_compat::file_ops::lchown(path, uid, gid) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_truncate(path: *const u8, length: i64) -> i64 {
    match crate::linux_compat::file_ops::truncate(path, length) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_ftruncate(fd: i32, length: i64) -> i64 {
    match crate::linux_compat::file_ops::ftruncate(fd, length) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_fsync(fd: i32) -> i64 {
    match crate::linux_compat::file_ops::fsync(fd) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_fdatasync(fd: i32) -> i64 {
    match crate::linux_compat::file_ops::fdatasync(fd) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_mount(
    source: *const u8,
    target: *const u8,
    filesystemtype: *const u8,
    mountflags: u64,
    data: *const u8,
) -> i64 {
    match crate::linux_compat::fs_ops::mount(source, target, filesystemtype, mountflags, data) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

fn syscall_pkg(syscall_number: usize, arg1: usize, arg2: usize, arg3: usize, arg4: usize) -> i64 {
    match crate::package::handle_package_syscall(syscall_number, arg1, arg2, arg3, arg4) {
        Ok(val) => val as i64,
        Err(_) => -(crate::linux_compat::LinuxError::EPERM as i64),
    }
}

fn syscall_statfs(path: *const u8, buf: *mut u8) -> i64 {
    match crate::linux_compat::fs_ops::statfs(path, buf as *mut crate::linux_compat::fs_ops::StatFs)
    {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_fstatfs(fd: i32, buf: *mut u8) -> i64 {
    match crate::linux_compat::fs_ops::fstatfs(fd, buf as *mut crate::linux_compat::fs_ops::StatFs)
    {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_sync() -> i64 {
    crate::linux_compat::fs_ops::sync();
    0
}
fn syscall_syncfs(fd: i32) -> i64 {
    match crate::linux_compat::fs_ops::syncfs(fd) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_statx(dirfd: i32, path: *const u8, flags: i32, mask: u32, statx_buf: *mut u8) -> i64 {
    match crate::linux_compat::file_ops::statx(
        dirfd,
        path,
        flags,
        mask,
        statx_buf as *mut crate::linux_compat::types::Statx,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_openat2(dirfd: i32, path: *const u8, how: *const u8, size: usize) -> i64 {
    match crate::linux_compat::file_ops::openat2(
        dirfd,
        path,
        how as *const crate::linux_compat::types::OpenHow,
        size,
    ) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}

// ── Memory extension syscalls ──

fn syscall_mremap(
    addr: *mut u8,
    old_size: usize,
    new_size: usize,
    flags: i32,
    new_addr: *mut u8,
) -> i64 {
    match crate::linux_compat::memory_ops::mremap(addr, old_size, new_size, flags, new_addr) {
        Ok(ptr) => ptr as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_madvise(addr: *mut u8, length: usize, advice: i32) -> i64 {
    match crate::linux_compat::memory_ops::madvise(addr, length, advice) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_mincore(addr: *const u8, length: usize, vec: *mut u8) -> i64 {
    match crate::linux_compat::memory_ops::mincore(addr as *mut u8, length, vec) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_mlock(addr: *const u8, len: usize) -> i64 {
    match crate::linux_compat::memory_ops::mlock(addr, len) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_munlock(addr: *const u8, len: usize) -> i64 {
    match crate::linux_compat::memory_ops::munlock(addr, len) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_mlockall(flags: i32) -> i64 {
    match crate::linux_compat::memory_ops::mlockall(flags) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_munlockall() -> i64 {
    match crate::linux_compat::memory_ops::munlockall() {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_msync(addr: *mut u8, len: usize, flags: i32) -> i64 {
    match crate::linux_compat::memory_ops::msync(addr, len, flags) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_memfd_create(name: *const u8, flags: u32) -> i64 {
    match crate::linux_compat::ipc_ops::memfd_create(name, flags) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}

// ── Socket syscalls ──

fn syscall_socket(domain: i32, sock_type: i32, protocol: i32) -> i64 {
    match crate::linux_compat::socket_ops::socket(domain, sock_type, protocol) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_bind(sockfd: i32, addr: *const u8, addrlen: u32) -> i64 {
    match crate::linux_compat::socket_ops::bind(
        sockfd,
        addr as *const crate::linux_compat::SockAddr,
        addrlen,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_connect(sockfd: i32, addr: *const u8, addrlen: u32) -> i64 {
    match crate::linux_compat::socket_ops::connect(
        sockfd,
        addr as *const crate::linux_compat::SockAddr,
        addrlen,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_listen(sockfd: i32, backlog: i32) -> i64 {
    match crate::linux_compat::socket_ops::listen(sockfd, backlog) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_accept(sockfd: i32, addr: *mut u8, addrlen: *mut u32) -> i64 {
    match crate::linux_compat::socket_ops::accept(
        sockfd,
        addr as *mut crate::linux_compat::SockAddr,
        addrlen,
    ) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_accept4(sockfd: i32, addr: *mut u8, addrlen: *mut u32, flags: i32) -> i64 {
    match crate::linux_compat::socket_ops::accept4(
        sockfd,
        addr as *mut crate::linux_compat::SockAddr,
        addrlen,
        flags,
    ) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_socketpair(domain: i32, sock_type: i32, protocol: i32, sv: *mut i32) -> i64 {
    match crate::linux_compat::socket_ops::socketpair(domain, sock_type, protocol, sv) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_sendto(
    sockfd: i32,
    buf: *const u8,
    len: usize,
    flags: i32,
    dest_addr: *const u8,
    addrlen: u32,
) -> i64 {
    match crate::linux_compat::socket_ops::sendto(
        sockfd,
        buf,
        len,
        flags,
        dest_addr as *const crate::linux_compat::SockAddr,
        addrlen,
    ) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_recvfrom(
    sockfd: i32,
    buf: *mut u8,
    len: usize,
    flags: i32,
    src_addr: *mut u8,
    addrlen: *mut u32,
) -> i64 {
    match crate::linux_compat::socket_ops::recvfrom(
        sockfd,
        buf,
        len,
        flags,
        src_addr as *mut crate::linux_compat::SockAddr,
        addrlen,
    ) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_sendmsg(sockfd: i32, msg: *const u8, flags: i32) -> i64 {
    match crate::linux_compat::socket_ops::sendmsg(sockfd, msg, flags) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_recvmsg(sockfd: i32, msg: *mut u8, flags: i32) -> i64 {
    match crate::linux_compat::socket_ops::recvmsg(sockfd, msg, flags) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_sendmmsg(sockfd: i32, msgvec: *mut u8, vlen: u32, flags: i32) -> i64 {
    match crate::linux_compat::socket_ops::sendmmsg(sockfd, msgvec, vlen, flags) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_recvmmsg(
    sockfd: i32,
    msgvec: *mut u8,
    vlen: u32,
    flags: i32,
    timeout: *const u8,
) -> i64 {
    match crate::linux_compat::socket_ops::recvmmsg(sockfd, msgvec, vlen, flags, timeout) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_setsockopt(
    sockfd: i32,
    level: i32,
    optname: i32,
    optval: *const u8,
    optlen: u32,
) -> i64 {
    match crate::linux_compat::socket_ops::setsockopt(sockfd, level, optname, optval, optlen) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_getsockopt(
    sockfd: i32,
    level: i32,
    optname: i32,
    optval: *mut u8,
    optlen: *mut u32,
) -> i64 {
    match crate::linux_compat::socket_ops::getsockopt(sockfd, level, optname, optval, optlen) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_getsockname(sockfd: i32, addr: *mut u8, addrlen: *mut u32) -> i64 {
    match crate::linux_compat::socket_ops::getsockname(
        sockfd,
        addr as *mut crate::linux_compat::SockAddr,
        addrlen,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_getpeername(sockfd: i32, addr: *mut u8, addrlen: *mut u32) -> i64 {
    match crate::linux_compat::socket_ops::getpeername(
        sockfd,
        addr as *mut crate::linux_compat::SockAddr,
        addrlen,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_shutdown(sockfd: i32, how: i32) -> i64 {
    match crate::linux_compat::socket_ops::shutdown(sockfd, how) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

// ── Event loop syscalls ──

fn syscall_poll(fds: *mut u8, nfds: u64, timeout: i32) -> i64 {
    match crate::linux_compat::special_fd::poll(
        fds as *mut crate::linux_compat::PollFd,
        nfds,
        timeout,
    ) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_ppoll(fds: *mut u8, nfds: u64, ts: *const u8, sigmask: *const u8) -> i64 {
    let timeout_ms = if ts.is_null() {
        -1
    } else {
        unsafe {
            (*(ts as *const crate::linux_compat::TimeSpec)).tv_sec as i32 * 1000
                + (*(ts as *const crate::linux_compat::TimeSpec)).tv_nsec as i32 / 1_000_000
        }
    };
    let _ = sigmask;
    match crate::linux_compat::special_fd::poll(
        fds as *mut crate::linux_compat::PollFd,
        nfds,
        timeout_ms,
    ) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_select(
    nfds: i32,
    readfds: *mut u64,
    writefds: *mut u64,
    exceptfds: *mut u64,
    timeout: *const u8,
) -> i64 {
    match crate::linux_compat::socket_ops::select(
        nfds,
        readfds,
        writefds,
        exceptfds,
        timeout as *mut crate::linux_compat::TimeVal,
    ) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_pselect6(
    nfds: i32,
    readfds: *mut u64,
    writefds: *mut u64,
    exceptfds: *mut u64,
    timeout: *const u8,
    sigmask: *const u8,
) -> i64 {
    match crate::linux_compat::socket_ops::pselect(
        nfds,
        readfds,
        writefds,
        exceptfds,
        timeout as *const crate::linux_compat::TimeSpec,
        sigmask as *const crate::linux_compat::SigSet,
    ) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_epoll_create1(flags: i32) -> i64 {
    match crate::linux_compat::special_fd::epoll_create1(flags) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_epoll_ctl(epfd: i32, op: i32, fd: i32, event: *mut u8) -> i64 {
    match crate::linux_compat::special_fd::epoll_ctl(epfd, op, fd, event) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_epoll_wait(epfd: i32, events: *mut u8, maxevents: i32, timeout: i32) -> i64 {
    match crate::linux_compat::special_fd::epoll_wait(epfd, events, maxevents, timeout) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_epoll_pwait(
    epfd: i32,
    events: *mut u8,
    maxevents: i32,
    timeout: i32,
    sigmask: *const u8,
) -> i64 {
    let _ = sigmask;
    match crate::linux_compat::special_fd::epoll_wait(epfd, events, maxevents, timeout) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_epoll_pwait2(
    epfd: i32,
    events: *mut u8,
    maxevents: i32,
    timeout: *const u8,
    sigmask: *const u8,
) -> i64 {
    let timeout_ms = if timeout.is_null() {
        -1
    } else {
        unsafe {
            (*(timeout as *const crate::linux_compat::TimeSpec)).tv_sec as i32 * 1000
                + (*(timeout as *const crate::linux_compat::TimeSpec)).tv_nsec as i32 / 1_000_000
        }
    };
    let _ = sigmask;
    match crate::linux_compat::special_fd::epoll_wait(epfd, events, maxevents, timeout_ms) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}

// ── eventfd / timerfd / signalfd ──

fn syscall_eventfd(initval: u32) -> i64 {
    match crate::linux_compat::special_fd::eventfd2(initval, 0) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_eventfd2(initval: u32, flags: i32) -> i64 {
    match crate::linux_compat::special_fd::eventfd2(initval, flags) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_signalfd(fd: i32, mask: *const u8, sizemask: u32) -> i64 {
    match read_signalfd_mask(mask, sizemask) {
        Ok(m) => match crate::linux_compat::special_fd::signalfd(fd, m, 0) {
            Ok(fd) => fd as i64,
            Err(e) => -(e as i64),
        },
        Err(e) => -(e as i64),
    }
}
fn syscall_signalfd4(fd: i32, mask: *const u8, sizemask: u32, flags: i32) -> i64 {
    match read_signalfd_mask(mask, sizemask) {
        Ok(m) => match crate::linux_compat::special_fd::signalfd(fd, m, flags) {
            Ok(fd) => fd as i64,
            Err(e) => -(e as i64),
        },
        Err(e) => -(e as i64),
    }
}

/// Read a sigset_t from userspace and return it as a u64.
fn read_signalfd_mask(mask: *const u8, sizemask: u32) -> crate::linux_compat::LinuxResult<u64> {
    use crate::linux_compat::LinuxError;
    if mask.is_null() {
        return Err(crate::linux_compat::LinuxError::EFAULT);
    }
    if sizemask < 8 {
        return Err(crate::linux_compat::LinuxError::EINVAL);
    }
    let mut bytes = [0u8; 8];
    // SAFETY: caller guarantees mask points to at least sizemask bytes.
    read_user_bytes(mask, &mut bytes)?;
    Ok(u64::from_ne_bytes(bytes))
}

fn read_user_bytes(ptr: *const u8, dst: &mut [u8]) -> crate::linux_compat::LinuxResult<()> {
    if ptr.is_null() {
        return Err(crate::linux_compat::LinuxError::EFAULT);
    }
    let valid = crate::memory::check_memory_access(ptr as usize, dst.len(), false, 3)
        .map_err(|_| crate::linux_compat::LinuxError::EFAULT)?;
    if !valid {
        return Err(crate::linux_compat::LinuxError::EFAULT);
    }

    unsafe {
        core::ptr::copy_nonoverlapping(ptr, dst.as_mut_ptr(), dst.len());
    }
    Ok(())
}

fn write_user_bytes(ptr: *mut u8, src: &[u8]) -> crate::linux_compat::LinuxResult<()> {
    if ptr.is_null() {
        return Err(crate::linux_compat::LinuxError::EFAULT);
    }
    let valid = crate::memory::check_memory_access(ptr as usize, src.len(), true, 3)
        .map_err(|_| crate::linux_compat::LinuxError::EFAULT)?;
    if !valid {
        return Err(crate::linux_compat::LinuxError::EFAULT);
    }

    unsafe {
        core::ptr::copy_nonoverlapping(src.as_ptr(), ptr, src.len());
    }
    Ok(())
}
fn syscall_timerfd_create(clockid: i32, flags: i32) -> i64 {
    match crate::linux_compat::special_fd::timerfd_create(clockid, flags) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_timerfd_settime(fd: i32, flags: i32, new_value: *const u8, old_value: *mut u8) -> i64 {
    match crate::linux_compat::special_fd::timerfd_settime(fd, flags, new_value, old_value) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_timerfd_gettime(fd: i32, curr_value: *mut u8) -> i64 {
    match crate::linux_compat::special_fd::timerfd_gettime(fd, curr_value) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

// ── inotify ──

fn syscall_inotify_init1(flags: i32) -> i64 {
    match crate::linux_compat::fs_ops::inotify_init1(flags) {
        Ok(fd) => fd as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_inotify_add_watch(fd: i32, pathname: *const u8, mask: u32) -> i64 {
    match crate::linux_compat::fs_ops::inotify_add_watch(fd, pathname, mask) {
        Ok(wd) => wd as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_inotify_rm_watch(fd: i32, wd: i32) -> i64 {
    match crate::linux_compat::fs_ops::inotify_rm_watch(fd, wd) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

// ── Thread / process extension syscalls ──

fn syscall_vfork() -> i64 {
    match crate::linux_compat::process_ops::vfork() {
        Ok(pid) => pid as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_waitid(idtype: i32, id: i32, infop: *mut u8, options: i32, _rusage: *mut u8) -> i64 {
    // Map waitid to wait4 for now
    if idtype == 0 {
        // P_ALL
        return syscall_wait4(-1, infop as *mut i32, options, core::ptr::null_mut());
    }
    if idtype == 1 && id > 0 {
        // P_PID
        return syscall_wait4(id, infop as *mut i32, options, core::ptr::null_mut());
    }
    if idtype == 2 {
        // P_PGID: wait for any child in process group `id`.
        // wait4 interprets pid < -1 as "wait for pgid == -pid".
        if id > 0 {
            return syscall_wait4(-id, infop as *mut i32, options, core::ptr::null_mut());
        }
        // id == 0 means wait for any child in the caller's process group
        return syscall_wait4(0, infop as *mut i32, options, core::ptr::null_mut());
    }
    -38
}
fn syscall_clone3(args: *const u8, size: usize) -> i64 {
    match crate::linux_compat::thread_ops::clone3(
        args as *const crate::linux_compat::types::CloneArgs,
        size,
    ) {
        Ok(pid) => pid as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_execveat(
    dirfd: i32,
    pathname: *const u8,
    argv: *const *const u8,
    envp: *const *const u8,
    flags: i32,
) -> i64 {
    match crate::linux_compat::process_ops::execveat(dirfd, pathname, argv, envp, flags) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_set_robust_list(head: *mut u8, len: usize) -> i64 {
    match crate::linux_compat::thread_ops::set_robust_list(
        head as *mut crate::linux_compat::thread_ops::RobustListHead,
        len,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_get_robust_list(pid: i32, head: *mut *mut u8, len: *mut usize) -> i64 {
    match crate::linux_compat::thread_ops::get_robust_list(
        pid,
        head as *mut *mut crate::linux_compat::thread_ops::RobustListHead,
        len,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_tkill(tid: i32, sig: i32) -> i64 {
    match crate::linux_compat::thread_ops::tkill(tid, sig) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_tgkill(tgid: i32, tid: i32, sig: i32) -> i64 {
    match crate::linux_compat::thread_ops::tgkill(tgid, tid, sig) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_membarrier(cmd: i32, flags: i32) -> i64 {
    match crate::linux_compat::thread_ops::membarrier(cmd, flags) {
        Ok(v) => v as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_rt_sigpending(set: *mut u8, sigsetsize: usize) -> i64 {
    match crate::linux_compat::signal_ops::rt_sigpending(
        set as *mut crate::linux_compat::types::SigSet,
        sigsetsize,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_rt_sigtimedwait(
    set: *const u8,
    info: *mut u8,
    timeout: *const u8,
    sigsetsize: usize,
) -> i64 {
    use crate::linux_compat::LinuxError;

    if set.is_null() || sigsetsize < 8 {
        return -(crate::linux_compat::LinuxError::EINVAL as i64);
    }

    // Read the signal set
    let mut set_bytes = [0u8; 8];
    if let Err(e) = read_user_bytes(set, &mut set_bytes) {
        return -(e as i64);
    }
    let set_val = u64::from_ne_bytes(set_bytes);

    // Read the timeout if provided (struct timespec: tv_sec: i64, tv_nsec: i64)
    let timeout_ns = if timeout.is_null() {
        None
    } else {
        let mut ts = [0u8; 16];
        if let Err(e) = read_user_bytes(timeout, &mut ts) {
            return -(e as i64);
        }
        let secs = i64::from_ne_bytes([ts[0], ts[1], ts[2], ts[3], ts[4], ts[5], ts[6], ts[7]]);
        let nsecs =
            i64::from_ne_bytes([ts[8], ts[9], ts[10], ts[11], ts[12], ts[13], ts[14], ts[15]]);
        if secs < 0 || nsecs < 0 {
            return -(crate::linux_compat::LinuxError::EINVAL as i64);
        }
        Some(secs as u64 * 1_000_000_000 + nsecs as u64)
    };

    match crate::linux_compat::signal_ops::rt_sigtimedwait(set_val, timeout_ns) {
        Ok(sig) => {
            // Write signinfo if requested
            if !info.is_null() {
                let mut siginfo = [0u8; 128];
                siginfo[..4].copy_from_slice(&(sig as u32).to_ne_bytes());
                if let Err(e) = write_user_bytes(info, &siginfo) {
                    return -(e as i64);
                }
            }
            sig as i64
        }
        Err(e) => -(e as i64),
    }
}
fn syscall_rt_sigsuspend(mask: *const u8, sigsetsize: usize) -> i64 {
    match crate::linux_compat::signal_ops::rt_sigsuspend(
        mask as *const crate::linux_compat::types::SigSet,
        sigsetsize,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_sigaltstack(ss: *const u8, old_ss: *mut u8) -> i64 {
    match crate::linux_compat::signal_ops::sigaltstack(ss, old_ss) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

// ── Time extension syscalls ──

fn syscall_clock_getres(clockid: i32, res: *mut u8) -> i64 {
    match crate::linux_compat::time_ops::clock_getres(
        clockid,
        res as *mut crate::linux_compat::TimeSpec,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_clock_nanosleep(clockid: i32, flags: i32, req: *const u8, rem: *mut u8) -> i64 {
    match crate::linux_compat::time_ops::clock_nanosleep(
        clockid,
        flags,
        req as *const crate::linux_compat::TimeSpec,
        rem as *mut crate::linux_compat::TimeSpec,
    ) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_timer_create(clockid: i32, sevp: *const u8, timerid: *mut i32) -> i64 {
    match crate::linux_compat::time_ops::timer_create(clockid, sevp, timerid) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_timer_settime(
    timerid: i32,
    flags: i32,
    new_value: *const u8,
    old_value: *mut u8,
) -> i64 {
    match crate::linux_compat::time_ops::timer_settime(timerid, flags, new_value, old_value) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_timer_gettime(timerid: i32, curr_value: *mut u8) -> i64 {
    match crate::linux_compat::time_ops::timer_gettime(timerid, curr_value) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_timer_delete(timerid: i32) -> i64 {
    match crate::linux_compat::time_ops::timer_delete(timerid) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_timer_getoverrun(timerid: i32) -> i64 {
    match crate::linux_compat::time_ops::timer_getoverrun(timerid) {
        Ok(v) => v as i64,
        Err(e) => -(e as i64),
    }
}

// ── Sysinfo extension syscalls ──

fn syscall_getrandom(buf: *mut u8, buflen: usize, flags: u32) -> i64 {
    match crate::linux_compat::sysinfo_ops::getrandom(buf, buflen, flags) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_sethostname(name: *const u8, len: usize) -> i64 {
    match crate::linux_compat::sysinfo_ops::sethostname(name, len) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_setdomainname(name: *const u8, len: usize) -> i64 {
    match crate::linux_compat::sysinfo_ops::setdomainname(name, len) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_syslog(log_type: i32, bufp: *mut u8, len: i32) -> i64 {
    match crate::linux_compat::sysinfo_ops::syslog(log_type, bufp, len) {
        Ok(v) => v as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_reboot(magic1: i32, magic2: i32, cmd: u32, arg: *mut u8) -> i64 {
    match crate::linux_compat::sysinfo_ops::reboot(magic1, magic2, cmd, arg) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

// ── Kill ──
fn syscall_kill(pid: i32, sig: i32) -> i64 {
    match crate::linux_compat::signal_ops::kill(pid, sig) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}

// ── sendfile / fallocate ──
fn syscall_sendfile(out_fd: i32, in_fd: i32, offset: *mut i64, count: usize) -> i64 {
    match crate::linux_compat::advanced_io::sendfile(out_fd, in_fd, offset, count) {
        Ok(n) => n as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_fallocate(fd: i32, mode: i32, offset: i64, len: i64) -> i64 {
    match crate::linux_compat::file_ops::fallocate(fd, mode, offset, len) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_flock(fd: i32, operation: i32) -> i64 {
    match crate::linux_compat::file_ops::flock(fd, operation) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_pause() -> i64 {
    match crate::linux_compat::signal_ops::pause() {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_alarm(seconds: u32) -> i64 {
    match crate::linux_compat::process_ops::alarm(seconds) {
        Ok(prev) => prev as i64,
        Err(e) => -(e as i64),
    }
}
fn syscall_getitimer(which: i32, curr_value: *mut u8) -> i64 {
    match crate::linux_compat::process_ops::getitimer(which, curr_value) {
        Ok(_) => 0,
        Err(e) => -(e as i64),
    }
}
fn syscall_setitimer(which: i32, new_value: *const u8, old_value: *mut u8) -> i64 {
    match crate::linux_compat::process_ops::setitimer(which, new_value, old_value) {
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
/// Saved user register frame built by `syscall_0x80_handler` before dispatch.
///
/// Field order matches the on-stack layout (lowest address first), i.e. the
/// reverse of the push order in the handler.
#[repr(C)]
struct Int80Frame {
    r15: u64,
    r14: u64,
    r13: u64,
    r12: u64,
    r11: u64,
    r10: u64,
    r9: u64,
    r8: u64,
    rbp: u64,
    rdi: u64,
    rsi: u64,
    rdx: u64,
    rcx: u64,
    rbx: u64,
    rax: u64,
}

/// C-ABI dispatch target for the naked INT 0x80 entry.
///
/// Reads the syscall number and arguments from the saved register frame
/// (Linux x86_64 convention: num=rax, arg1=rdi, arg2=rsi, arg3=rdx, arg4=r10,
/// arg5=r8, arg6=r9) and returns the i64 result, which the asm trampoline writes
/// back into the saved RAX slot.
extern "C" fn syscall_0x80_dispatch(frame: *const Int80Frame) -> i64 {
    let f = unsafe { &*frame };

    if crate::usermode::in_user_mode() {
        crate::serial_println!("Syscall {} from user mode", f.rax);
    }

    let result = dispatch_syscall(f.rax, f.rdi, f.rsi, f.rdx, f.r10, f.r8, f.r9);

    // Successful execve must not return to the old user RIP; redirect the iretq frame.
    if let Some((entry, stack)) = crate::usermode::take_pending_user_entry() {
        unsafe {
            crate::usermode::patch_syscall_return_to_user(frame as *mut u8, entry, stack);
        }
    } else if let Some((rip, rsp)) = crate::user_sched::take_user_resume() {
        unsafe {
            crate::usermode::patch_syscall_return_to_kernel(frame as *mut u8, rip, rsp);
        }
    }

    result
}

/// INT 0x80 entry point (naked).
///
/// ponytail: this is a naked function so the user GP registers are captured
/// *before* any compiler-emitted prologue can clobber them. The previous
/// `extern "x86-interrupt"` body read rax/rdi/... via `asm!` only after the
/// prologue had already reused those registers (and wrote the result into RAX
/// only for the epilogue to overwrite it before `iretq`), so every syscall saw
/// garbage args and returned the wrong value.
///
/// The signature stays `extern "x86-interrupt" fn(InterruptStackFrame)` so the
/// IDT registration in `interrupts.rs` (`idt[0x80].set_handler_fn(...)`) keeps
/// type-checking; the `InterruptStackFrame` parameter is unused — we read the
/// CPU-pushed frame directly and return via `iretq`.
///
/// Entry state (ring3 -> ring0 via interrupt gate, no error code): the CPU has
/// pushed SS, RSP, RFLAGS, CS, RIP; the user GP registers are still live with
/// num=rax, arg1=rdi, arg2=rsi, arg3=rdx, arg4=r10, arg5=r8, arg6=r9.
#[unsafe(naked)]
pub extern "x86-interrupt" fn syscall_0x80_handler(_stack_frame: InterruptStackFrame) {
    use core::arch::naked_asm;

    naked_asm!(
        // Save the full user GP register set as an `Int80Frame` (push high->low;
        // read from the frame pointer the layout is reversed: r15 at offset 0,
        // rax at offset 14*8 = 112).
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rsi",
        "push rdi",
        "push rbp",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        // Pass &frame (rsp) as the single SysV argument.
        "mov rdi, rsp",

        // Align stack to 16 bytes for the call, preserving the frame base in rbp
        // (callee-saved, so it survives the call).
        "mov rbp, rsp",
        "and rsp, -16",

        "call {dispatch}",

        // Restore exact stack, then overwrite the saved RAX slot with the result.
        "mov rsp, rbp",
        "mov [rsp + 112], rax",

        // Restore all user GP registers (reverse push order); RAX now holds the
        // syscall return value.
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rbp",
        "pop rdi",
        "pop rsi",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",

        // Return to user mode. No EOI for software interrupts.
        "iretq",

        dispatch = sym syscall_0x80_dispatch
    );
}
