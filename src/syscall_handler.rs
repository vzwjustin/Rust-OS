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
    let result = match SyscallNumber::from_u64(syscall_num) {
        // File operations
        SyscallNumber::Read => syscall_read(arg1 as i32, arg2 as *mut u8, arg3 as usize),
        SyscallNumber::Write => syscall_write(arg1 as i32, arg2 as *const u8, arg3 as usize),
        SyscallNumber::Open => syscall_open(arg1 as *const u8, arg2 as i32, arg3 as u32),
        SyscallNumber::Close => syscall_close(arg1 as i32),
        SyscallNumber::Stat => syscall_stat(arg1 as *const u8, arg2 as *mut u8),
        SyscallNumber::Fstat => syscall_fstat(arg1 as i32, arg2 as *mut u8),
        SyscallNumber::Lstat => syscall_lstat(arg1 as *const u8, arg2 as *mut u8),
        SyscallNumber::Lseek => syscall_lseek(arg1 as i32, arg2 as i64, arg3 as i32),

        // Memory operations
        SyscallNumber::Mmap => syscall_mmap(
            arg1 as *mut u8,
            arg2 as usize,
            arg3 as i32,
            arg4 as i32,
            arg5 as i32,
            arg6 as i64,
        ),
        SyscallNumber::Mprotect => syscall_mprotect(arg1 as *mut u8, arg2 as usize, arg3 as i32),
        SyscallNumber::Munmap => syscall_munmap(arg1 as *mut u8, arg2 as usize),
        SyscallNumber::Brk => syscall_brk(arg1 as *mut u8),

        // Process operations
        SyscallNumber::Fork => syscall_fork(),
        SyscallNumber::Execve => syscall_execve(
            arg1 as *const u8,
            arg2 as *const *const u8,
            arg3 as *const *const u8,
        ),
        SyscallNumber::Exit | SyscallNumber::ExitGroup => syscall_exit(arg1 as i32),
        SyscallNumber::Wait4 => {
            syscall_wait4(arg1 as i32, arg2 as *mut i32, arg3 as i32, arg4 as *mut u8)
        }

        // IPC operations
        SyscallNumber::Shmget => syscall_shmget(arg1 as i32, arg2 as usize, arg3 as i32),
        SyscallNumber::Shmat => syscall_shmat(arg1 as i32, arg2 as *const u8, arg3 as i32),
        SyscallNumber::Semget => syscall_semget(arg1 as i32, arg2 as i32, arg3 as i32),
        SyscallNumber::Semop => syscall_semop(arg1 as i32, arg2 as *mut u8, arg3 as usize),
        SyscallNumber::Shmdt => syscall_shmdt(arg1 as *const u8),
        SyscallNumber::Msgget => syscall_msgget(arg1 as i32, arg2 as i32),
        SyscallNumber::Msgsnd => {
            syscall_msgsnd(arg1 as i32, arg2 as *const u8, arg3 as usize, arg4 as i32)
        }
        SyscallNumber::Msgrcv => syscall_msgrcv(
            arg1 as i32,
            arg2 as *mut u8,
            arg3 as usize,
            arg4 as i64,
            arg5 as i32,
        ),

        // Process / thread identity
        SyscallNumber::GetPid => syscall_getpid(),
        SyscallNumber::GetPpid => syscall_getppid(),
        SyscallNumber::Gettid => syscall_gettid(),

        // System information
        SyscallNumber::Uname => syscall_uname(arg1 as *mut u8),

        // Time operations
        SyscallNumber::ClockGettime => syscall_clock_gettime(arg1 as i32, arg2 as *mut u8),
        SyscallNumber::Gettimeofday => syscall_gettimeofday(arg1 as *mut u8, arg2 as *mut u8),
        SyscallNumber::Nanosleep => syscall_nanosleep(arg1 as *const u8, arg2 as *mut u8),

        // Signal operations
        SyscallNumber::RtSigaction => syscall_rt_sigaction(
            arg1 as i32,
            arg2 as *const u8,
            arg3 as *mut u8,
            arg4 as usize,
        ),
        SyscallNumber::RtSigprocmask => syscall_rt_sigprocmask(
            arg1 as i32,
            arg2 as *const u8,
            arg3 as *mut u8,
            arg4 as usize,
        ),

        // Thread operations
        SyscallNumber::ArchPrctl => syscall_arch_prctl(arg1 as i32, arg2),
        SyscallNumber::SetTidAddress => syscall_set_tid_address(arg1 as *mut i32),
        SyscallNumber::Futex => syscall_futex(
            arg1 as *mut i32,
            arg2 as i32,
            arg3 as i32,
            arg4 as *const u8,
            arg5 as *mut i32,
            arg6 as i32,
        ),
        SyscallNumber::Clone => syscall_clone(
            arg1,
            arg2 as *mut u8,
            arg3 as *mut i32,
            arg4 as *mut i32,
            arg5,
        ),

        // Extended file operations
        SyscallNumber::Openat => {
            syscall_openat(arg1 as i32, arg2 as *const u8, arg3 as i32, arg4 as u32)
        }
        SyscallNumber::Newfstatat => {
            syscall_newfstatat(arg1 as i32, arg2 as *const u8, arg3 as *mut u8, arg4 as i32)
        }
        SyscallNumber::Access => syscall_access(arg1 as *const u8, arg2 as i32),
        SyscallNumber::Readlink => {
            syscall_readlink(arg1 as *const u8, arg2 as *mut u8, arg3 as usize)
        }
        SyscallNumber::Getcwd => syscall_getcwd(arg1 as *mut u8, arg2 as usize),
        SyscallNumber::Chdir => syscall_chdir(arg1 as *const u8),
        SyscallNumber::Fcntl => syscall_fcntl(arg1 as i32, arg2 as i32, arg3),
        SyscallNumber::Ioctl => syscall_ioctl(arg1 as i32, arg2, arg3),
        SyscallNumber::Pipe => syscall_pipe(arg1 as *mut i32),
        SyscallNumber::Dup => syscall_dup(arg1 as i32),
        SyscallNumber::Dup2 => syscall_dup2(arg1 as i32, arg2 as i32),
        SyscallNumber::Mkdir => syscall_mkdir(arg1 as *const u8, arg2 as u32),
        SyscallNumber::Unlink => syscall_unlink(arg1 as *const u8),
        SyscallNumber::Getdents64 => syscall_getdents64(arg1 as i32, arg2 as *mut u8, arg3 as u32),

        // ── Credentials / identity (category 15) ──
        SyscallNumber::Getuid => syscall_getuid(),
        SyscallNumber::Geteuid => syscall_geteuid(),
        SyscallNumber::Getgid => syscall_getgid(),
        SyscallNumber::Getegid => syscall_getegid(),
        SyscallNumber::Setuid => syscall_setuid(arg1 as u32),
        SyscallNumber::Setgid => syscall_setgid(arg1 as u32),
        SyscallNumber::Setreuid => syscall_setreuid(arg1 as u32, arg2 as u32),
        SyscallNumber::Setregid => syscall_setregid(arg1 as u32, arg2 as u32),
        SyscallNumber::Setresuid => syscall_setresuid(arg1 as u32, arg2 as u32, arg3 as u32),
        SyscallNumber::Getresuid => {
            syscall_getresuid(arg1 as *mut u32, arg2 as *mut u32, arg3 as *mut u32)
        }
        SyscallNumber::Setresgid => syscall_setresgid(arg1 as u32, arg2 as u32, arg3 as u32),
        SyscallNumber::Getresgid => {
            syscall_getresgid(arg1 as *mut u32, arg2 as *mut u32, arg3 as *mut u32)
        }
        SyscallNumber::Getgroups => syscall_getgroups(arg1 as i32, arg2 as *mut u32),
        SyscallNumber::Setgroups => syscall_setgroups(arg1 as i32, arg2 as *const u32),
        SyscallNumber::Setpgid => syscall_setpgid(arg1 as i32, arg2 as i32),
        SyscallNumber::Getpgid => syscall_getpgid(arg1 as i32),
        SyscallNumber::Getpgrp => syscall_getpgrp(),
        SyscallNumber::Setsid => syscall_setsid(),
        SyscallNumber::Getsid => syscall_getsid(arg1 as i32),
        SyscallNumber::Umask => syscall_umask(arg1 as u32),
        SyscallNumber::Chroot => syscall_chroot(arg1 as *const u8),

        // ── Resource limits / usage (category 16) ──
        SyscallNumber::Getrlimit => syscall_getrlimit(arg1 as i32, arg2 as *mut u8),
        SyscallNumber::Setrlimit => syscall_setrlimit(arg1 as i32, arg2 as *const u8),
        SyscallNumber::Prlimit64 => {
            syscall_prlimit64(arg1 as i32, arg2 as i32, arg3 as *const u8, arg4 as *mut u8)
        }
        SyscallNumber::Getrusage => syscall_getrusage(arg1 as i32, arg2 as *mut u8),
        SyscallNumber::Times => syscall_times(arg1 as *mut u8),
        SyscallNumber::Sysinfo => syscall_sysinfo(arg1 as *mut u8),
        SyscallNumber::Prctl => syscall_prctl(arg1 as i32, arg2, arg3, arg4, arg5),
        SyscallNumber::Capget => syscall_capget(arg1 as *mut u8, arg2 as *mut u8),
        SyscallNumber::Capset => syscall_capset(arg1 as *const u8, arg2 as *const u8),

        // ── Scheduling (category 5) ──
        SyscallNumber::SchedYield => syscall_sched_yield(),
        SyscallNumber::SchedGetaffinity => {
            syscall_sched_getaffinity(arg1 as i32, arg2 as usize, arg3 as *mut u8)
        }
        SyscallNumber::SchedSetaffinity => {
            syscall_sched_setaffinity(arg1 as i32, arg2 as usize, arg3 as *const u8)
        }
        SyscallNumber::SchedSetscheduler => {
            syscall_sched_setscheduler(arg1 as i32, arg2 as i32, arg3 as *const u8)
        }
        SyscallNumber::SchedGetscheduler => syscall_sched_getscheduler(arg1 as i32),
        SyscallNumber::SchedSetparam => syscall_sched_setparam(arg1 as i32, arg2 as *const u8),
        SyscallNumber::SchedGetparam => syscall_sched_getparam(arg1 as i32, arg2 as *mut u8),
        SyscallNumber::SchedGetPriorityMax => syscall_sched_get_priority_max(arg1 as i32),
        SyscallNumber::SchedGetPriorityMin => syscall_sched_get_priority_min(arg1 as i32),
        SyscallNumber::SchedRrGetInterval => {
            syscall_sched_rr_get_interval(arg1 as i32, arg2 as *mut u8)
        }

        // ── Extended file ops (category 3, 4) ──
        SyscallNumber::Pread64 => {
            syscall_pread64(arg1 as i32, arg2 as *mut u8, arg3 as usize, arg4 as i64)
        }
        SyscallNumber::Pwrite64 => {
            syscall_pwrite64(arg1 as i32, arg2 as *const u8, arg3 as usize, arg4 as i64)
        }
        SyscallNumber::Readv => syscall_readv(arg1 as i32, arg2 as *const u8, arg3 as usize),
        SyscallNumber::Writev => syscall_writev(arg1 as i32, arg2 as *const u8, arg3 as usize),
        SyscallNumber::Preadv => {
            syscall_preadv(arg1 as i32, arg2 as *const u8, arg3 as usize, arg4 as i64)
        }
        SyscallNumber::Pwritev => {
            syscall_pwritev(arg1 as i32, arg2 as *const u8, arg3 as usize, arg4 as i64)
        }
        SyscallNumber::Dup3 => syscall_dup3(arg1 as i32, arg2 as i32, arg3 as i32),
        SyscallNumber::Pipe2 => syscall_pipe2(arg1 as *mut i32, arg2 as i32),
        SyscallNumber::Faccessat => syscall_faccessat(arg1 as i32, arg2 as *const u8, arg3 as i32),
        SyscallNumber::Faccessat2 => {
            syscall_faccessat2(arg1 as i32, arg2 as *const u8, arg3 as i32, arg4 as i32)
        }
        SyscallNumber::Readlinkat => syscall_readlinkat(
            arg1 as i32,
            arg2 as *const u8,
            arg3 as *mut u8,
            arg4 as usize,
        ),
        SyscallNumber::Fchmodat => syscall_fchmodat(arg1 as i32, arg2 as *const u8, arg3 as u32),
        SyscallNumber::Mkdirat => syscall_mkdirat(arg1 as i32, arg2 as *const u8, arg3 as u32),
        SyscallNumber::Unlinkat => syscall_unlinkat(arg1 as i32, arg2 as *const u8, arg3 as i32),
        SyscallNumber::Renameat => syscall_renameat(
            arg1 as i32,
            arg2 as *const u8,
            arg3 as i32,
            arg4 as *const u8,
        ),
        SyscallNumber::Renameat2 => syscall_renameat2(
            arg1 as i32,
            arg2 as *const u8,
            arg3 as i32,
            arg4 as *const u8,
            arg5 as i32,
        ),
        SyscallNumber::Linkat => syscall_linkat(
            arg1 as i32,
            arg2 as *const u8,
            arg3 as i32,
            arg4 as *const u8,
            arg5 as i32,
        ),
        SyscallNumber::Symlinkat => {
            syscall_symlinkat(arg1 as *const u8, arg2 as i32, arg3 as *const u8)
        }
        SyscallNumber::Fchownat => syscall_fchownat(
            arg1 as i32,
            arg2 as *const u8,
            arg3 as u32,
            arg4 as u32,
            arg5 as i32,
        ),
        SyscallNumber::Utimensat => syscall_utimensat(
            arg1 as i32,
            arg2 as *const u8,
            arg3 as *const u8,
            arg4 as i32,
        ),
        SyscallNumber::Rename => syscall_rename(arg1 as *const u8, arg2 as *const u8),
        SyscallNumber::Rmdir => syscall_rmdir(arg1 as *const u8),
        SyscallNumber::Symlink => syscall_symlink(arg1 as *const u8, arg2 as *const u8),
        SyscallNumber::Link => syscall_link(arg1 as *const u8, arg2 as *const u8),
        SyscallNumber::Chmod => syscall_chmod(arg1 as *const u8, arg2 as u32),
        SyscallNumber::Fchmod => syscall_fchmod(arg1 as i32, arg2 as u32),
        SyscallNumber::Chown => syscall_chown(arg1 as *const u8, arg2 as u32, arg3 as u32),
        SyscallNumber::Fchown => syscall_fchown(arg1 as i32, arg2 as u32, arg3 as u32),
        SyscallNumber::Lchown => syscall_lchown(arg1 as *const u8, arg2 as u32, arg3 as u32),
        SyscallNumber::Truncate => syscall_truncate(arg1 as *const u8, arg2 as i64),
        SyscallNumber::Ftruncate => syscall_ftruncate(arg1 as i32, arg2 as i64),
        SyscallNumber::Fsync => syscall_fsync(arg1 as i32),
        SyscallNumber::Fdatasync => syscall_fdatasync(arg1 as i32),
        SyscallNumber::Statfs => syscall_statfs(arg1 as *const u8, arg2 as *mut u8),
        SyscallNumber::Fstatfs => syscall_fstatfs(arg1 as i32, arg2 as *mut u8),
        SyscallNumber::Sync => syscall_sync(),
        SyscallNumber::Syncfs => syscall_syncfs(arg1 as i32),
        SyscallNumber::Statx => syscall_statx(
            arg1 as i32,
            arg2 as *const u8,
            arg3 as i32,
            arg4 as u32,
            arg5 as *mut u8,
        ),
        SyscallNumber::Openat2 => syscall_openat2(
            arg1 as i32,
            arg2 as *const u8,
            arg3 as *const u8,
            arg4 as usize,
        ),

        // ── Memory extensions (category 2) ──
        SyscallNumber::Mremap => syscall_mremap(
            arg1 as *mut u8,
            arg2 as usize,
            arg3 as usize,
            arg4 as i32,
            arg5 as *mut u8,
        ),
        SyscallNumber::Madvise => syscall_madvise(arg1 as *mut u8, arg2 as usize, arg3 as i32),
        SyscallNumber::Mincore => {
            syscall_mincore(arg1 as *const u8, arg2 as usize, arg3 as *mut u8)
        }
        SyscallNumber::Mlock => syscall_mlock(arg1 as *const u8, arg2 as usize),
        SyscallNumber::Munlock => syscall_munlock(arg1 as *const u8, arg2 as usize),
        SyscallNumber::Mlockall => syscall_mlockall(arg1 as i32),
        SyscallNumber::Munlockall => syscall_munlockall(),
        SyscallNumber::Msync => syscall_msync(arg1 as *mut u8, arg2 as usize, arg3 as i32),
        SyscallNumber::MemfdCreate => syscall_memfd_create(arg1 as *const u8, arg2 as u32),

        // ── Sockets (category 7, 8) ──
        SyscallNumber::Socket => syscall_socket(arg1 as i32, arg2 as i32, arg3 as i32),
        SyscallNumber::Bind => syscall_bind(arg1 as i32, arg2 as *const u8, arg3 as u32),
        SyscallNumber::Connect => syscall_connect(arg1 as i32, arg2 as *const u8, arg3 as u32),
        SyscallNumber::Listen => syscall_listen(arg1 as i32, arg2 as i32),
        SyscallNumber::Accept => syscall_accept(arg1 as i32, arg2 as *mut u8, arg3 as *mut u32),
        SyscallNumber::Accept4 => {
            syscall_accept4(arg1 as i32, arg2 as *mut u8, arg3 as *mut u32, arg4 as i32)
        }
        SyscallNumber::Socketpair => {
            syscall_socketpair(arg1 as i32, arg2 as i32, arg3 as i32, arg4 as *mut i32)
        }
        SyscallNumber::Sendto => syscall_sendto(
            arg1 as i32,
            arg2 as *const u8,
            arg3 as usize,
            arg4 as i32,
            arg5 as *const u8,
            arg6 as u32,
        ),
        SyscallNumber::Recvfrom => syscall_recvfrom(
            arg1 as i32,
            arg2 as *mut u8,
            arg3 as usize,
            arg4 as i32,
            arg5 as *mut u8,
            arg6 as *mut u32,
        ),
        SyscallNumber::Sendmsg => syscall_sendmsg(arg1 as i32, arg2 as *const u8, arg3 as i32),
        SyscallNumber::Recvmsg => syscall_recvmsg(arg1 as i32, arg2 as *mut u8, arg3 as i32),
        SyscallNumber::Sendmmsg => {
            syscall_sendmmsg(arg1 as i32, arg2 as *mut u8, arg3 as u32, arg4 as i32)
        }
        SyscallNumber::Recvmmsg => syscall_recvmmsg(
            arg1 as i32,
            arg2 as *mut u8,
            arg3 as u32,
            arg4 as i32,
            arg5 as *const u8,
        ),
        SyscallNumber::SetSockopt => syscall_setsockopt(
            arg1 as i32,
            arg2 as i32,
            arg3 as i32,
            arg4 as *const u8,
            arg5 as u32,
        ),
        SyscallNumber::GetSockopt => syscall_getsockopt(
            arg1 as i32,
            arg2 as i32,
            arg3 as i32,
            arg4 as *mut u8,
            arg5 as *mut u32,
        ),
        SyscallNumber::Getsockname => {
            syscall_getsockname(arg1 as i32, arg2 as *mut u8, arg3 as *mut u32)
        }
        SyscallNumber::Getpeername => {
            syscall_getpeername(arg1 as i32, arg2 as *mut u8, arg3 as *mut u32)
        }
        SyscallNumber::Shutdown => syscall_shutdown(arg1 as i32, arg2 as i32),

        // ── Event loop / poll (category 3, 6) ──
        SyscallNumber::Poll => syscall_poll(arg1 as *mut u8, arg2 as u64, arg3 as i32),
        SyscallNumber::Ppoll => syscall_ppoll(
            arg1 as *mut u8,
            arg2 as u64,
            arg3 as *const u8,
            arg4 as *const u8,
        ),
        SyscallNumber::Select => syscall_select(
            arg1 as i32,
            arg2 as *mut u64,
            arg3 as *mut u64,
            arg4 as *mut u64,
            arg5 as *const u8,
        ),
        SyscallNumber::Pselect6 => syscall_pselect6(
            arg1 as i32,
            arg2 as *mut u64,
            arg3 as *mut u64,
            arg4 as *mut u64,
            arg5 as *const u8,
            arg6 as *const u8,
        ),
        SyscallNumber::EpollCreate1 => syscall_epoll_create1(arg1 as i32),
        SyscallNumber::EpollCtl => {
            syscall_epoll_ctl(arg1 as i32, arg2 as i32, arg3 as i32, arg4 as *mut u8)
        }
        SyscallNumber::EpollWait => {
            syscall_epoll_wait(arg1 as i32, arg2 as *mut u8, arg3 as i32, arg4 as i32)
        }
        SyscallNumber::EpollPwait => syscall_epoll_pwait(
            arg1 as i32,
            arg2 as *mut u8,
            arg3 as i32,
            arg4 as i32,
            arg5 as *const u8,
        ),
        SyscallNumber::EpollPwait2 => syscall_epoll_pwait2(
            arg1 as i32,
            arg2 as *mut u8,
            arg3 as i32,
            arg4 as *const u8,
            arg5 as *const u8,
        ),

        // ── eventfd / timerfd / signalfd (category 6) ──
        SyscallNumber::Eventfd => syscall_eventfd(arg1 as u32),
        SyscallNumber::Eventfd2 => syscall_eventfd2(arg1 as u32, arg2 as i32),
        SyscallNumber::Signalfd => syscall_signalfd(arg1 as i32, arg2 as *const u8, arg3 as u32),
        SyscallNumber::Signalfd4 => {
            syscall_signalfd4(arg1 as i32, arg2 as *const u8, arg3 as u32, arg4 as i32)
        }
        SyscallNumber::TimerfdCreate => syscall_timerfd_create(arg1 as i32, arg2 as i32),
        SyscallNumber::TimerfdSettime => {
            syscall_timerfd_settime(arg1 as i32, arg2 as i32, arg3 as *const u8, arg4 as *mut u8)
        }
        SyscallNumber::TimerfdGettime => syscall_timerfd_gettime(arg1 as i32, arg2 as *mut u8),

        // ── inotify (category 6) ──
        SyscallNumber::InotifyInit1 => syscall_inotify_init1(arg1 as i32),
        SyscallNumber::InotifyAddWatch => {
            syscall_inotify_add_watch(arg1 as i32, arg2 as *const u8, arg3 as u32)
        }
        SyscallNumber::InotifyRmWatch => syscall_inotify_rm_watch(arg1 as i32, arg2 as i32),

        // ── Thread ops (category 1, 5) ──
        SyscallNumber::Vfork => syscall_vfork(),
        SyscallNumber::Waitid => syscall_waitid(
            arg1 as i32,
            arg2 as i32,
            arg3 as *mut u8,
            arg4 as i32,
            arg5 as *mut u8,
        ),
        SyscallNumber::Clone3 => syscall_clone3(arg1 as *const u8, arg2 as usize),
        SyscallNumber::Execveat => syscall_execveat(
            arg1 as i32,
            arg2 as *const u8,
            arg3 as *const *const u8,
            arg4 as *const *const u8,
            arg5 as i32,
        ),
        SyscallNumber::SetRobustList => syscall_set_robust_list(arg1 as *mut u8, arg2 as usize),
        SyscallNumber::GetRobustList => {
            syscall_get_robust_list(arg1 as i32, arg2 as *mut *mut u8, arg3 as *mut usize)
        }
        SyscallNumber::Tkill => syscall_tkill(arg1 as i32, arg2 as i32),
        SyscallNumber::Tgkill => syscall_tgkill(arg1 as i32, arg2 as i32, arg3 as i32),
        SyscallNumber::Membarrier => syscall_membarrier(arg1 as i32, arg2 as i32),
        SyscallNumber::RtSigreturn => 0,
        SyscallNumber::RtSigpending => syscall_rt_sigpending(arg1 as *mut u8, arg2 as usize),
        SyscallNumber::RtSigtimedwait => syscall_rt_sigtimedwait(
            arg1 as *const u8,
            arg2 as *mut u8,
            arg3 as *const u8,
            arg4 as usize,
        ),
        SyscallNumber::RtSigsuspend => syscall_rt_sigsuspend(arg1 as *const u8, arg2 as usize),
        SyscallNumber::Sigaltstack => syscall_sigaltstack(arg1 as *const u8, arg2 as *mut u8),

        // ── Time extensions (category 6) ──
        SyscallNumber::ClockGetres => syscall_clock_getres(arg1 as i32, arg2 as *mut u8),
        SyscallNumber::ClockNanosleep => {
            syscall_clock_nanosleep(arg1 as i32, arg2 as i32, arg3 as *const u8, arg4 as *mut u8)
        }
        SyscallNumber::TimerCreate => {
            syscall_timer_create(arg1 as i32, arg2 as *const u8, arg3 as *mut i32)
        }
        SyscallNumber::TimerSettime => {
            syscall_timer_settime(arg1 as i32, arg2 as i32, arg3 as *const u8, arg4 as *mut u8)
        }
        SyscallNumber::TimerGettime => syscall_timer_gettime(arg1 as i32, arg2 as *mut u8),
        SyscallNumber::TimerDelete => syscall_timer_delete(arg1 as i32),
        SyscallNumber::TimerGetoverrun => syscall_timer_getoverrun(arg1 as i32),

        // ── Sysinfo extensions (category 16) ──
        SyscallNumber::Getrandom => syscall_getrandom(arg1 as *mut u8, arg2 as usize, arg3 as u32),
        SyscallNumber::Sethostname => syscall_sethostname(arg1 as *const u8, arg2 as usize),
        SyscallNumber::Setdomainname => syscall_setdomainname(arg1 as *const u8, arg2 as usize),
        SyscallNumber::Syslog => syscall_syslog(arg1 as i32, arg2 as *mut u8, arg3 as i32),
        SyscallNumber::Reboot => {
            syscall_reboot(arg1 as i32, arg2 as i32, arg3 as u32, arg4 as *mut u8)
        }

        // ── Kill / signals ──
        SyscallNumber::Kill => syscall_kill(arg1 as i32, arg2 as i32),

        // ── Fsync/fdatasync already above, add sendfile ──
        SyscallNumber::Sendfile => {
            syscall_sendfile(arg1 as i32, arg2 as i32, arg3 as *mut i64, arg4 as usize)
        }
        SyscallNumber::Fadvise64 => 0, // advisory, always succeed
        SyscallNumber::Fallocate => {
            syscall_fallocate(arg1 as i32, arg2 as i32, arg3 as i64, arg4 as i64)
        }
        SyscallNumber::Flock => syscall_flock(arg1 as i32, arg2 as i32),
        SyscallNumber::Pause => syscall_pause(),
        SyscallNumber::Alarm => syscall_alarm(arg1 as u32),
        SyscallNumber::Getitimer => syscall_getitimer(arg1 as i32, arg2 as *mut u8),
        SyscallNumber::Setitimer => {
            syscall_setitimer(arg1 as i32, arg2 as *const u8, arg3 as *mut u8)
        }

        _ => -38, // ENOSYS
    };
    crate::debug::trace_syscall(syscall_num, arg1, arg2, arg3, arg4, arg5, arg6, result);
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
    match crate::linux_compat::thread_ops::futex(
        uaddr,
        futex_op,
        val,
        timeout as *const crate::linux_compat::TimeSpec,
        uaddr2,
        val3,
    ) {
        Ok(v) => v as i64,
        Err(e) => -(e as i64),
    }
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
    if dirfd == crate::linux_compat::file_ops::AT_FDCWD && flags == 0 {
        return syscall_stat(pathname, statbuf);
    }
    if dirfd == crate::linux_compat::file_ops::AT_FDCWD && (flags & 0x100) != 0 {
        // AT_SYMLINK_NOFOLLOW
        return syscall_lstat(pathname, statbuf);
    }
    -(crate::linux_compat::LinuxError::ENOSYS as i64)
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
fn syscall_signalfd(_fd: i32, _mask: *const u8, _sizemask: u32) -> i64 {
    -38
}
fn syscall_signalfd4(_fd: i32, _mask: *const u8, _sizemask: u32, _flags: i32) -> i64 {
    -38
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
fn syscall_rt_sigpending(_set: *mut u8, _sigsetsize: usize) -> i64 {
    0
}
fn syscall_rt_sigtimedwait(
    _set: *const u8,
    _info: *mut u8,
    _timeout: *const u8,
    _sigsetsize: usize,
) -> i64 {
    -38
}
fn syscall_rt_sigsuspend(_mask: *const u8, _sigsetsize: usize) -> i64 {
    -38
}
fn syscall_sigaltstack(_ss: *const u8, _old_ss: *mut u8) -> i64 {
    0
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
    // pause() blocks until a signal is delivered. Without a wait queue we
    // return EINTR (4) to indicate the call was interrupted, matching the
    // behavior when no signal handler is installed.
    -4 // EINTR
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
