//! Linux-style syscall tracing for compatibility bring-up.
//!
//! Logs syscall name, decoded arguments where practical, return values, and
//! errno on failure. Enable at boot or via [`set_enabled`].

extern crate alloc;

use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};

use crate::process;
use crate::syscall::SyscallNumber;

static ENABLED: AtomicBool = AtomicBool::new(true);

/// Whether syscall tracing is enabled.
pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Enable or disable syscall tracing at runtime.
pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
}

/// Trace one syscall invocation (no-op when disabled).
pub fn trace_syscall(
    num: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64,
    result: i64,
) {
    if !is_enabled() {
        return;
    }

    let pid = process::current_pid();
    let tid = pid;
    let name = syscall_name(SyscallNumber::from_u64(num));
    let args = decode_args(SyscallNumber::from_u64(num), arg1, arg2, arg3, arg4, arg5, arg6);
    let ret = format_result(SyscallNumber::from_u64(num), result);

    crate::serial_println!(
        "[pid={} tid={} exe=kernel]\nsyscall={}({})\n-> {}",
        pid,
        tid,
        name,
        args,
        ret
    );

    if result < 0 && should_dump_fds(SyscallNumber::from_u64(num)) {
        dump_open_fds();
    }
}

fn should_dump_fds(syscall: SyscallNumber) -> bool {
    matches!(
        syscall,
        SyscallNumber::Read
            | SyscallNumber::Write
            | SyscallNumber::Open
            | SyscallNumber::Openat
            | SyscallNumber::Close
            | SyscallNumber::Fstat
            | SyscallNumber::Poll
            | SyscallNumber::EpollWait
            | SyscallNumber::EpollPwait
    )
}

fn dump_open_fds() {
  let table = crate::vfs::get_vfs().open_fd_snapshot();
  if table.is_empty() {
      crate::serial_println!("  fd-table: (no tracked fds >= 3)");
      return;
  }
  for (fd, kind) in table {
      crate::serial_println!("  fd-table: fd={} kind={:?}", fd, kind);
  }
}

fn format_result(syscall: SyscallNumber, result: i64) -> String {
    if result >= 0 {
        match syscall {
            SyscallNumber::Open | SyscallNumber::Openat | SyscallNumber::Socket
            | SyscallNumber::Pipe | SyscallNumber::Pipe2 | SyscallNumber::EpollCreate1
            | SyscallNumber::Accept | SyscallNumber::Accept4 => {
                return format!("fd={result}");
            }
            SyscallNumber::Mmap => return format!("addr=0x{result:x}"),
            SyscallNumber::Clone | SyscallNumber::Fork => return format!("pid={result}"),
            _ => {}
        }
        return format!("{result}");
    }

    let errno = (-result) as i32;
    format!("-1 errno={errno} ({})", errno_name(errno))
}

fn errno_name(errno: i32) -> &'static str {
    match errno {
        1 => "EPERM",
        2 => "ENOENT",
        3 => "ESRCH",
        4 => "EINTR",
        5 => "EIO",
        6 => "ENXIO",
        9 => "EBADF",
        11 => "EAGAIN",
        12 => "ENOMEM",
        13 => "EACCES",
        14 => "EFAULT",
        17 => "EEXIST",
        19 => "ENODEV",
        20 => "ENOTDIR",
        21 => "EISDIR",
        22 => "EINVAL",
        28 => "ENOSPC",
        38 => "ENOSYS",
        95 => "EOPNOTSUPP",
        _ => "ERR",
    }
}

fn decode_args(
    syscall: SyscallNumber,
    a1: u64,
    a2: u64,
    a3: u64,
    a4: u64,
    a5: u64,
    a6: u64,
) -> String {
    match syscall {
        SyscallNumber::Read => format!("fd={}, buf=0x{:x}, count={}", a1 as i32, a2, a3),
        SyscallNumber::Write => format!("fd={}, buf=0x{:x}, count={}", a1 as i32, a2, a3),
        SyscallNumber::Open => format!(
            "path={}, flags=0x{:x}, mode=0o{:o}",
            cstr_preview(a1 as *const u8),
            a2,
            a3
        ),
        SyscallNumber::Openat => format!(
            "dirfd={}, path={}, flags=0x{:x}, mode=0o{:o}",
            a1 as i32,
            cstr_preview(a2 as *const u8),
            a3,
            a4
        ),
        SyscallNumber::Close => format!("fd={}", a1 as i32),
        SyscallNumber::Mmap => format!(
            "addr=0x{:x}, len={}, prot=0x{:x}, flags=0x{:x}, fd={}, off={}",
            a1, a2, a3, a4, a5 as i32, a6
        ),
        SyscallNumber::Brk => format!("addr=0x{:x}", a1),
        SyscallNumber::Futex => format!(
            "uaddr=0x{:x}, op={}, val={}, timeout=0x{:x}",
            a1, a2, a3, a4
        ),
        SyscallNumber::EpollWait | SyscallNumber::EpollPwait => format!(
            "epfd={}, maxevents={}, timeout={}",
            a1 as i32, a3, a4 as i32
        ),
        SyscallNumber::Poll | SyscallNumber::Ppoll => {
            format!("fds=0x{:x}, nfds={}, timeout=0x{:x}", a1, a2, a3)
        }
        SyscallNumber::Clone => format!("flags=0x{:x}, stack=0x{:x}", a1, a2),
        SyscallNumber::Execve => format!(
            "path={}, argv=0x{:x}, envp=0x{:x}",
            cstr_preview(a1 as *const u8),
            a2,
            a3
        ),
        SyscallNumber::Exit => format!("status={}", a1 as i32),
        SyscallNumber::Wait4 => format!(
            "pid={}, status=0x{:x}, options=0x{:x}",
            a1 as i32,
            a2,
            a3
        ),
        SyscallNumber::Socket => format!("domain={}, type={}, proto={}", a1, a2, a3),
        SyscallNumber::Connect | SyscallNumber::Bind => {
            format!("fd={}, addr=0x{:x}, addrlen={}", a1 as i32, a2, a3)
        }
        SyscallNumber::ClockGettime => format!("clockid={}, tp=0x{:x}", a1 as i32, a2),
        SyscallNumber::Nanosleep => format!("req=0x{:x}, rem=0x{:x}", a1, a2),
        SyscallNumber::Getrandom => format!("buf=0x{:x}, buflen={}, flags=0x{:x}", a1, a2, a3),
        SyscallNumber::RtSigaction => format!("signum={}, act=0x{:x}", a1 as i32, a2),
        SyscallNumber::Ioctl => format!("fd={}, request=0x{:x}, arg=0x{:x}", a1 as i32, a2, a3),
        SyscallNumber::Lseek => format!("fd={}, offset={}, whence={}", a1 as i32, a2 as i64, a3),
        _ => format!(
            "arg1=0x{:x}, arg2=0x{:x}, arg3=0x{:x}, arg4=0x{:x}, arg5=0x{:x}, arg6=0x{:x}",
            a1, a2, a3, a4, a5, a6
        ),
    }
}

fn cstr_preview(ptr: *const u8) -> String {
    if ptr.is_null() {
        return String::from("NULL");
    }
    let mut out = String::from("\"");
    for i in 0..96 {
        let byte = unsafe { *ptr.add(i) };
        if byte == 0 {
            break;
        }
        if byte.is_ascii_graphic() || byte == b' ' {
            out.push(byte as char);
        } else {
            out.push_str(&format!("\\x{byte:02x}"));
        }
    }
    out.push('"');
    out
}

fn syscall_name(num: SyscallNumber) -> &'static str {
    match num {
        SyscallNumber::Read => "read",
        SyscallNumber::Write => "write",
        SyscallNumber::Open => "open",
        SyscallNumber::Close => "close",
        SyscallNumber::Stat => "stat",
        SyscallNumber::Fstat => "fstat",
        SyscallNumber::Lstat => "lstat",
        SyscallNumber::Poll => "poll",
        SyscallNumber::Lseek => "lseek",
        SyscallNumber::Mmap => "mmap",
        SyscallNumber::Mprotect => "mprotect",
        SyscallNumber::Munmap => "munmap",
        SyscallNumber::Brk => "brk",
        SyscallNumber::RtSigaction => "rt_sigaction",
        SyscallNumber::RtSigprocmask => "rt_sigprocmask",
        SyscallNumber::RtSigreturn => "rt_sigreturn",
        SyscallNumber::Ioctl => "ioctl",
        SyscallNumber::Pipe => "pipe",
        SyscallNumber::Dup => "dup",
        SyscallNumber::Dup2 => "dup2",
        SyscallNumber::Nanosleep => "nanosleep",
        SyscallNumber::GetPid => "getpid",
        SyscallNumber::Socket => "socket",
        SyscallNumber::Connect => "connect",
        SyscallNumber::Accept => "accept",
        SyscallNumber::Accept4 => "accept4",
        SyscallNumber::Bind => "bind",
        SyscallNumber::Listen => "listen",
        SyscallNumber::Clone => "clone",
        SyscallNumber::Fork => "fork",
        SyscallNumber::Execve => "execve",
        SyscallNumber::Exit => "exit",
        SyscallNumber::Wait4 => "wait4",
        SyscallNumber::Kill => "kill",
        SyscallNumber::Uname => "uname",
        SyscallNumber::Fcntl => "fcntl",
        SyscallNumber::Futex => "futex",
        SyscallNumber::Getdents64 => "getdents64",
        SyscallNumber::Getcwd => "getcwd",
        SyscallNumber::Chdir => "chdir",
        SyscallNumber::Openat => "openat",
        SyscallNumber::Newfstatat => "newfstatat",
        SyscallNumber::ClockGettime => "clock_gettime",
        SyscallNumber::EpollCreate1 => "epoll_create1",
        SyscallNumber::EpollCtl => "epoll_ctl",
        SyscallNumber::EpollWait => "epoll_wait",
        SyscallNumber::EpollPwait => "epoll_pwait",
        SyscallNumber::Pipe2 => "pipe2",
        SyscallNumber::Getrandom => "getrandom",
        SyscallNumber::Prlimit64 => "prlimit64",
        SyscallNumber::SetRobustList => "set_robust_list",
        SyscallNumber::GetRobustList => "get_robust_list",
        SyscallNumber::SetTidAddress => "set_tid_address",
        SyscallNumber::ArchPrctl => "arch_prctl",
        SyscallNumber::Readlink => "readlink",
        SyscallNumber::Readlinkat => "readlinkat",
        SyscallNumber::Mremap => "mremap",
        SyscallNumber::SchedYield => "sched_yield",
        SyscallNumber::ExitGroup => "exit_group",
        SyscallNumber::Invalid => "invalid",
        other => {
            let _ = other;
            "syscall"
        }
    }
}
