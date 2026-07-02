//! RustOS native PID 1 — Stage-1 init / service manager.
//!
//! This is the canonical init binary (see `userspace/rust-init/` for the
//! now-unused earlier duplicate, kept only as a historical stub — do not add
//! services there, it is not embedded into the kernel image).
//!
//! It is a static, freestanding ELF executable — our own Rust code, no libc,
//! no dynamic linker — running as a real Ring-3 process on the kernel's Linux
//! syscall ABI (`INT 0x80`, dispatched by `src/syscall_handler.rs` /
//! `src/syscall_fast.rs` into `src/linux_integration.rs::route_syscall`).
//! `src/initramfs.rs::install_native_init()` writes the embedded
//! `userspace/init.elf` (built from this crate) to `/init` and `/sbin/init` and
//! execs it as PID 1.
//!
//! Stage-1 responsibilities (this file):
//!   - Launch a fixed, ordered list of services (`SERVICES`) via `fork()` +
//!     `execve()` — no dependency graph, just sequential launch order.
//!   - Reap children forever via `wait4(-1, ...)` (PID 1 must never exit).
//!   - Apply a per-service restart policy: `Once` services are launched a
//!     single time and never restarted; `Always` services are re-forked/exec'd
//!     immediately on exit, up to `max_restarts` times, after which they are
//!     logged as dead and left unsupervised.
//!
//! No heap allocator is linked into this binary, so everything here uses
//! fixed-capacity, `#![no_std]`-friendly arrays sized from `SERVICES.len()`.

#![no_std]
#![no_main]

use core::arch::asm;
use core::panic::PanicInfo;

// Linux x86_64 syscall numbers (see src/syscall/linux.rs::SyscallNumber for
// the kernel-side source of truth; these are standard Linux ABI numbers, not
// RustOS-specific).
const SYS_OPEN: u64 = 2;
const SYS_CLOSE: u64 = 3;
const SYS_WRITE: u64 = 1;
const SYS_FORK: u64 = 57;
const SYS_EXECVE: u64 = 59;
const SYS_EXIT: u64 = 60;
const SYS_WAIT4: u64 = 61;
const SYS_SCHED_YIELD: u64 = 24;

// `open()` flags, verified against `src/linux_compat/types.rs` /
// `src/linux_compat/file_ops.rs::linux_flags_to_vfs` — standard Linux x86_64
// octal layout (O_WRONLY=0o1, O_CREAT=0o100), not RustOS-specific.
const O_WRONLY: u64 = 0o1;
const O_CREAT: u64 = 0o100;

/// Linux/RustOS `INT 0x80` syscall, 0 args.
#[inline(always)]
unsafe fn syscall0(n: u64) -> i64 {
    let ret: i64;
    asm!(
        "int 0x80",
        inlateout("rax") n => ret,
        options(nostack, preserves_flags),
    );
    ret
}

/// Linux/RustOS `INT 0x80` syscall, 3 args.
#[inline(always)]
unsafe fn syscall3(n: u64, a1: u64, a2: u64, a3: u64) -> i64 {
    let ret: i64;
    asm!(
        "int 0x80",
        inlateout("rax") n => ret,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
        options(nostack, preserves_flags),
    );
    ret
}

/// Linux/RustOS `INT 0x80` syscall, 4 args (arg4 goes in r10, matching the
/// Linux x86_64 syscall convention used by both `SYSCALL` and this kernel's
/// `INT 0x80` handler — see `src/syscall_handler.rs::syscall_0x80_dispatch`).
#[inline(always)]
unsafe fn syscall4(n: u64, a1: u64, a2: u64, a3: u64, a4: u64) -> i64 {
    let ret: i64;
    asm!(
        "int 0x80",
        inlateout("rax") n => ret,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
        in("r10") a4,
        options(nostack, preserves_flags),
    );
    ret
}

#[inline(always)]
unsafe fn write(fd: u64, buf: &[u8]) -> i64 {
    syscall3(SYS_WRITE, fd, buf.as_ptr() as u64, buf.len() as u64)
}

/// `open(pathname, flags, mode)`.
#[inline(always)]
unsafe fn open(path: *const u8, flags: u64, mode: u64) -> i64 {
    syscall3(SYS_OPEN, path as u64, flags, mode)
}

/// `close(fd)`.
#[inline(always)]
unsafe fn close(fd: u64) -> i64 {
    syscall3(SYS_CLOSE, fd, 0, 0)
}

#[inline(always)]
unsafe fn fork() -> i64 {
    syscall0(SYS_FORK)
}

#[inline(always)]
unsafe fn execve(path: *const u8, argv: *const *const u8, envp: *const *const u8) -> i64 {
    syscall3(SYS_EXECVE, path as u64, argv as u64, envp as u64)
}

/// `wait4(pid, wstatus, options, rusage=NULL)`.
#[inline(always)]
unsafe fn wait4(pid: i64, wstatus: *mut i32, options: i64) -> i64 {
    syscall4(SYS_WAIT4, pid as u64, wstatus as u64, options as u64, 0)
}

#[inline(always)]
unsafe fn sched_yield() {
    let _ = syscall0(SYS_SCHED_YIELD);
}

#[inline(always)]
unsafe fn exit(code: u64) -> ! {
    loop {
        let _ = syscall3(SYS_EXIT, code, 0, 0);
        sched_yield();
        asm!("pause", options(nomem, nostack, preserves_flags));
    }
}

/// Restart policy applied when a service's process exits.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RestartPolicy {
    /// Launch once; never restart regardless of exit status.
    Once,
    /// Restart immediately on exit, up to `max_restarts` times.
    Always,
}

/// A single supervised service. `path`/`argv`/`envp` entries are NUL-terminated
/// byte strings so they can be handed to `execve()` without any string
/// formatting or heap allocation.
struct Service {
    name: &'static str,
    path: &'static [u8],
    argv: &'static [&'static [u8]],
    envp: &'static [&'static [u8]],
    restart: RestartPolicy,
    max_restarts: u32,
}

/// Shared environment for launched services (matches the PATH used elsewhere
/// in the kernel's Linux-compat session bootstrap, see
/// `src/linux_compat/process_ops.rs::default_session_envp`).
const DEFAULT_ENVP: &[&[u8]] =
    &[b"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin\0"];

/// Fixed, ordered service list — no dependency graph, just launch order.
///
/// These are real binaries present in `userspace/rootfs/{sbin,bin}` (Alpine
/// busybox multi-call symlinks) at the time this was written:
///   - `/bin/hostname` (`-> /bin/busybox`): one-shot setup, proves fork/exec
///     works for a simple exiting child.
///   - `/sbin/syslogd` (`-> /bin/busybox`): long-running daemon; `-n` keeps it
///     in the foreground so init can actually supervise/reap it (without `-n`
///     busybox syslogd daemonizes and the foreground copy exits immediately).
///   - `/sbin/getty` (`-> /bin/busybox`): long-running daemon; kept `Always`
///     so a crash-loop (e.g. if `/dev/tty1` isn't wired up yet) demonstrates
///     the restart-policy/`max_restarts` path rather than silently vanishing.
const SERVICES: &[Service] = &[
    Service {
        name: "hostname-init",
        path: b"/bin/hostname\0",
        argv: &[b"hostname\0", b"rustos\0"],
        envp: DEFAULT_ENVP,
        restart: RestartPolicy::Once,
        max_restarts: 0,
    },
    Service {
        name: "syslogd",
        path: b"/sbin/syslogd\0",
        argv: &[b"syslogd\0", b"-n\0"],
        envp: DEFAULT_ENVP,
        restart: RestartPolicy::Always,
        max_restarts: 5,
    },
    Service {
        name: "getty",
        path: b"/sbin/getty\0",
        argv: &[b"getty\0", b"115200\0", b"tty1\0"],
        envp: DEFAULT_ENVP,
        restart: RestartPolicy::Always,
        max_restarts: 5,
    },
];

/// Max argv/envp entries per service supported by the fixed-size pointer
/// arrays built before `execve()`. `SERVICES` above never gets close to this;
/// bump it if a future service needs more.
const MAX_PTRS: usize = 8;

/// Build a NUL-terminated pointer array (in the caller's stack storage `out`)
/// from a list of NUL-terminated byte-string entries, for handing to
/// `execve()`. Returns the number of entries written (excluding the trailing
/// NULL sentinel).
fn build_ptr_array(entries: &[&[u8]], out: &mut [*const u8; MAX_PTRS + 1]) -> usize {
    let n = core::cmp::min(entries.len(), MAX_PTRS);
    for i in 0..n {
        out[i] = entries[i].as_ptr();
    }
    out[n] = core::ptr::null();
    n
}

/// Format an unsigned integer into `buf`, returning the written slice.
/// (No heap, no `core::fmt` writer plumbed in — this is the minimal decimal
/// formatter needed for restart-count logging.)
fn u32_to_str(mut value: u32, buf: &mut [u8; 10]) -> &[u8] {
    if value == 0 {
        buf[0] = b'0';
        return &buf[..1];
    }
    let mut tmp = [0u8; 10];
    let mut i = 0;
    while value > 0 {
        tmp[i] = b'0' + (value % 10) as u8;
        value /= 10;
        i += 1;
    }
    for j in 0..i {
        buf[j] = tmp[i - 1 - j];
    }
    &buf[..i]
}

fn log(msg: &[u8]) {
    unsafe {
        let _ = write(1, b"[init] ");
        let _ = write(1, msg);
        let _ = write(1, b"\n");
    }
}

fn log2(msg1: &[u8], msg2: &str) {
    unsafe {
        let _ = write(1, b"[init] ");
        let _ = write(1, msg1);
        let _ = write(1, msg2.as_bytes());
        let _ = write(1, b"\n");
    }
}

/// Child-side body after `fork()` returns 0: exec the service's program and,
/// if that fails, log why and exit with a distinct error code (never returns).
fn exec_service_child(svc: &Service) -> ! {
    let mut argv_storage = [core::ptr::null::<u8>(); MAX_PTRS + 1];
    let mut envp_storage = [core::ptr::null::<u8>(); MAX_PTRS + 1];
    build_ptr_array(svc.argv, &mut argv_storage);
    build_ptr_array(svc.envp, &mut envp_storage);

    let rc = unsafe { execve(svc.path.as_ptr(), argv_storage.as_ptr(), envp_storage.as_ptr()) };

    // execve() only returns on failure.
    log2(b"execve failed for service: ", svc.name);
    unsafe { exit(127 + (-rc) as u64 % 100) }
}

/// Fork + exec one service, returning the child pid (parent side) or exiting
/// the process (child side, via `exec_service_child`, never returns there).
fn launch_service(svc: &Service) -> i64 {
    let pid = unsafe { fork() };
    if pid == 0 {
        exec_service_child(svc);
    }
    if pid < 0 {
        log2(b"fork failed for service: ", svc.name);
    } else {
        log2(b"launched service: ", svc.name);
    }
    pid
}

/// Signal readiness to the kernel: create/write `/run/init.ready` once all
/// services have been launched (fork/exec'd) at least one time. The kernel
/// polls for this file's existence as the explicit trigger to enter the
/// compositor-shell session loop, replacing the previous coincidental-timing
/// dependency (see `src/main.rs` around `spawn_userspace_init()`).
fn signal_ready() {
    const PATH: &[u8] = b"/run/init.ready\0";
    const MARKER: &[u8] = b"ready\n";
    unsafe {
        let fd = open(PATH.as_ptr(), O_WRONLY | O_CREAT, 0o644);
        if fd < 0 {
            log(b"failed to create /run/init.ready");
            return;
        }
        let _ = write(fd as u64, MARKER);
        let _ = close(fd as u64);
    }
    log(b"signaled readiness: /run/init.ready");
}

const NUM_SERVICES: usize = SERVICES.len();

#[no_mangle]
#[link_section = ".text._start"]
pub extern "C" fn _start() -> ! {
    log(b"Stage-1 service manager starting");

    // Parallel fixed-size arrays indexed by SERVICES position — this is the
    // no_std/no-alloc substitute for a HashMap<pid, service index>.
    let mut pids: [i64; NUM_SERVICES] = [-1; NUM_SERVICES];
    let mut restarts: [u32; NUM_SERVICES] = [0; NUM_SERVICES];
    let mut dead: [bool; NUM_SERVICES] = [false; NUM_SERVICES];

    for (i, svc) in SERVICES.iter().enumerate() {
        pids[i] = launch_service(svc);
    }

    // All services have been launched (forked/exec'd) once — signal
    // readiness once, here, before entering the reap loop below (never
    // repeated on restarts).
    signal_ready();

    // Reap loop: PID 1 must never exit. wait4() on this kernel does not block
    // internally — it returns -EAGAIN when no zombie is ready yet — so retry
    // with a sched_yield() between polls instead of trusting a single call to
    // block until a child exits.
    loop {
        let mut status: i32 = 0;
        let reaped = unsafe { wait4(-1, &mut status as *mut i32, 0) };

        if reaped < 0 {
            unsafe { sched_yield() };
            continue;
        }

        match pids.iter().position(|&p| p == reaped) {
            Some(idx) => {
                let svc = &SERVICES[idx];
                match svc.restart {
                    RestartPolicy::Once => {
                        pids[idx] = -1;
                        log2(b"service exited (Once, not restarting): ", svc.name);
                    }
                    RestartPolicy::Always => {
                        if dead[idx] {
                            // Already gave up on this one; ignore further exits.
                        } else if restarts[idx] >= svc.max_restarts {
                            dead[idx] = true;
                            pids[idx] = -1;
                            log2(b"service exceeded max_restarts, giving up: ", svc.name);
                        } else {
                            restarts[idx] += 1;
                            let mut count_buf = [0u8; 10];
                            let count_str = u32_to_str(restarts[idx], &mut count_buf);
                            unsafe {
                                let _ = write(1, b"[init] restarting service: ");
                                let _ = write(1, svc.name.as_bytes());
                                let _ = write(1, b" (attempt ");
                                let _ = write(1, count_str);
                                let _ = write(1, b")\n");
                            }
                            pids[idx] = launch_service(svc);
                        }
                    }
                }
            }
            None => {
                // Reaped pid isn't one of our directly-launched services —
                // e.g. a grandchild reparented to init on its parent's exit.
                // Nothing to restart; just keep reaping.
            }
        }
    }
}

#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    log(b"panic");
    unsafe { exit(101) }
}
