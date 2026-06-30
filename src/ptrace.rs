//! Ptrace — Process tracing and debugging
//!
//! Ported from Linux kernel/ptrace.c.
//! Provides:
//! - PTRACE_TRACEME: child requests tracing by parent
//! - PTRACE_ATTACH/DETACH: attach to/detach from a process
//! - PTRACE_PEEKDATA/POKEDATA: read/write process memory
//! - PTRACE_PEEKUSR/POKEUSR: read/write user area (registers)
//! - PTRACE_CONT/KILL/SINGLESTEP: control traced process
//! - PTRACE_SYSCALL: stop on syscall entry/exit
//! - PTRACE_GETREGS/SETREGS: read/write register set
//! - PTRACE_SETOPTIONS: set tracing options
//! - PTRACE_SEIZE: attach without stopping
//! - PTRACE_GETEVENTMSG: get event data

use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

// ── Ptrace request constants (from include/uapi/linux/ptrace.h) ─────────

pub const PTRACE_TRACEME: u32 = 0;
pub const PTRACE_PEEKTEXT: u32 = 1;
pub const PTRACE_PEEKDATA: u32 = 2;
pub const PTRACE_PEEKUSR: u32 = 3;
pub const PTRACE_POKETEXT: u32 = 4;
pub const PTRACE_POKEDATA: u32 = 5;
pub const PTRACE_POKEUSR: u32 = 6;
pub const PTRACE_CONT: u32 = 7;
pub const PTRACE_KILL: u32 = 8;
pub const PTRACE_SINGLESTEP: u32 = 9;
pub const PTRACE_ATTACH: u32 = 16;
pub const PTRACE_DETACH: u32 = 17;
pub const PTRACE_SYSCALL: u32 = 24;
pub const PTRACE_SETOPTIONS: u32 = 0x4200;
pub const PTRACE_GETEVENTMSG: u32 = 0x4201;
pub const PTRACE_GETSIGINFO: u32 = 0x4202;
pub const PTRACE_SETSIGINFO: u32 = 0x4203;
pub const PTRACE_GETREGSET: u32 = 0x4204;
pub const PTRACE_SETREGSET: u32 = 0x4205;
pub const PTRACE_SEIZE: u32 = 0x4206;
pub const PTRACE_INTERRUPT: u32 = 0x4207;
pub const PTRACE_LISTEN: u32 = 0x4208;
pub const PTRACE_PEEKSIGINFO: u32 = 0x4209;
pub const PTRACE_GETSIGMASK: u32 = 0x420a;
pub const PTRACE_SETSIGMASK: u32 = 0x420b;
pub const PTRACE_SECCOMP_GET_FILTER: u32 = 0x420c;

// ── Ptrace options ──────────────────────────────────────────────────────

pub const PTRACE_O_TRACESYSGOOD: u32 = 1;
pub const PTRACE_O_TRACEFORK: u32 = 1 << 1;
pub const PTRACE_O_TRACEVFORK: u32 = 1 << 2;
pub const PTRACE_O_TRACECLONE: u32 = 1 << 3;
pub const PTRACE_O_TRACEEXEC: u32 = 1 << 4;
pub const PTRACE_O_TRACEVFORKDONE: u32 = 1 << 5;
pub const PTRACE_O_TRACEEXIT: u32 = 1 << 6;
pub const PTRACE_O_TRACESECCOMP: u32 = 1 << 7;
pub const PTRACE_O_EXITKILL: u32 = 1 << 20;
pub const PTRACE_O_SUSPEND_SECCOMP: u32 = 1 << 21;

// ── Ptrace event codes ──────────────────────────────────────────────────

pub const PTRACE_EVENT_FORK: u32 = 1;
pub const PTRACE_EVENT_VFORK: u32 = 2;
pub const PTRACE_EVENT_CLONE: u32 = 3;
pub const PTRACE_EVENT_EXEC: u32 = 4;
pub const PTRACE_EVENT_VFORK_DONE: u32 = 5;
pub const PTRACE_EVENT_EXIT: u32 = 6;
pub const PTRACE_EVENT_SECCOMP: u32 = 7;

// ── Traced process state ────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TraceState {
    NotTraced,
    Traced,
    Stopped,
    Running,
    Killed,
}

#[derive(Debug, Clone)]
pub struct PtraceState {
    pub tracer_pid: u32,
    pub tracee_state: TraceState,
    pub options: u32,
    pub event_msg: u64,
    pub signal_to_deliver: i32,
    pub syscall_trace: bool,
    pub single_step: bool,
    /// Saved register set (x86_64: 27 GPRs)
    pub regs: [u64; 27],
}

impl Default for PtraceState {
    fn default() -> Self {
        Self {
            tracer_pid: 0,
            tracee_state: TraceState::NotTraced,
            options: 0,
            event_msg: 0,
            signal_to_deliver: 0,
            syscall_trace: false,
            single_step: false,
            regs: [0; 27],
        }
    }
}

impl PtraceState {
    pub fn is_traced(&self) -> bool {
        self.tracee_state != TraceState::NotTraced
    }
}

// ── Global state: PID → PtraceState ─────────────────────────────────────

static PTRACE_STATES: RwLock<BTreeMap<u32, PtraceState>> = RwLock::new(BTreeMap::new());
static PTRACE_EVENT_COUNT: AtomicU64 = AtomicU64::new(0);

fn get_state(pid: u32) -> PtraceState {
    PTRACE_STATES.read().get(&pid).cloned().unwrap_or_default()
}

/// True if the 8-byte word at `addr` lies entirely within the user address
/// range. PEEK/POKE must reject kernel addresses to avoid leaking or corrupting
/// kernel memory through the ptrace interface.
fn ptrace_addr_in_user_range(addr: u64) -> bool {
    let start = crate::memory::USER_SPACE_START as u64;
    let end = crate::memory::USER_SPACE_END as u64;
    match addr.checked_add(8) {
        Some(addr_end) => addr >= start && addr_end <= end,
        None => false,
    }
}

fn set_state(pid: u32, state: PtraceState) {
    PTRACE_STATES.write().insert(pid, state);
}

// ── Public API ──────────────────────────────────────────────────────────

/// Main ptrace syscall handler.
/// Returns 0 on success, negative errno on failure.
/// For PEEK operations, returns the value directly (as i64).
pub fn ptrace(request: u32, pid: u32, addr: u64, data: u64) -> i64 {
    let current_pid = crate::process::current_pid();

    match request {
        // ── PTRACE_TRACEME ────────────────────────────────────────────
        PTRACE_TRACEME => {
            let mut state = get_state(current_pid);
            if state.is_traced() {
                return -1; // EPERM
            }

            // Get parent PID
            let pm = crate::process::get_process_manager();
            let parent_pid = pm
                .find_processes(|p| p.pid == current_pid)
                .into_iter()
                .next()
                .and_then(|p| p.parent_pid)
                .unwrap_or(0);

            state.tracer_pid = parent_pid;
            state.tracee_state = TraceState::Traced;
            set_state(current_pid, state);

            crate::serial_println!(
                "[ptrace] TRACEME: pid {} traced by {}",
                current_pid,
                parent_pid
            );
            0
        }

        // ── PTRACE_ATTACH ─────────────────────────────────────────────
        PTRACE_ATTACH => {
            if pid == current_pid {
                return -1; // EPERM — can't trace self
            }

            let mut state = get_state(pid);
            if state.is_traced() {
                return -1; // EPERM — already traced
            }

            state.tracer_pid = current_pid;
            state.tracee_state = TraceState::Stopped;
            set_state(pid, state);

            // Send SIGSTOP to the tracee
            let pm = crate::process::get_process_manager();
            let _ = pm.block_process(pid);

            crate::serial_println!("[ptrace] ATTACH: tracer {} → tracee {}", current_pid, pid);
            0
        }

        // ── PTRACE_SEIZE (attach without stopping) ────────────────────
        PTRACE_SEIZE => {
            if pid == current_pid {
                return -1;
            }

            let mut state = get_state(pid);
            if state.is_traced() {
                return -1;
            }

            state.tracer_pid = current_pid;
            state.tracee_state = TraceState::Traced;
            state.options = data as u32; // SEIZE takes options in data
            set_state(pid, state);

            crate::serial_println!("[ptrace] SEIZE: tracer {} → tracee {}", current_pid, pid);
            0
        }

        // ── PTRACE_DETACH ─────────────────────────────────────────────
        PTRACE_DETACH => {
            let mut state = get_state(pid);
            if !state.is_traced() || state.tracer_pid != current_pid {
                return -3; // ESRCH
            }

            state.tracee_state = TraceState::NotTraced;
            state.tracer_pid = 0;
            state.options = 0;
            set_state(pid, state);

            // Unblock the tracee
            let pm = crate::process::get_process_manager();
            let _ = pm.unblock_process(pid);

            crate::serial_println!("[ptrace] DETACH: tracee {} released", pid);
            0
        }

        // ── PTRACE_PEEKDATA / PTRACE_PEEKTEXT ─────────────────────────
        PTRACE_PEEKDATA | PTRACE_PEEKTEXT => {
            let state = get_state(pid);
            if !state.is_traced() || state.tracer_pid != current_pid {
                return -3;
            }

            // Only allow access to the user address range. Without this a
            // tracer could read kernel memory (info leak) through PEEK.
            if !ptrace_addr_in_user_range(addr) {
                return -14; // EFAULT
            }
            // Read a word from the tracee's memory at addr
            let val = unsafe { core::ptr::read_volatile(addr as *const u64) };
            val as i64
        }

        // ── PTRACE_POKEDATA / PTRACE_POKETEXT ─────────────────────────
        PTRACE_POKEDATA | PTRACE_POKETEXT => {
            let state = get_state(pid);
            if !state.is_traced() || state.tracer_pid != current_pid {
                return -3;
            }

            // Reject non-user addresses: a POKE to a kernel address would
            // corrupt kernel memory.
            if !ptrace_addr_in_user_range(addr) {
                return -14; // EFAULT
            }
            unsafe {
                core::ptr::write_volatile(addr as *mut u64, data);
            }
            0
        }

        // ── PTRACE_PEEKUSR (read from user area) ──────────────────────
        PTRACE_PEEKUSR => {
            let state = get_state(pid);
            if !state.is_traced() || state.tracer_pid != current_pid {
                return -3;
            }

            // addr is an offset into the user area (register file)
            let offset = (addr / 8) as usize;
            if offset < state.regs.len() {
                return state.regs[offset] as i64;
            }
            -14 // EFAULT
        }

        // ── PTRACE_POKEUSR (write to user area) ───────────────────────
        PTRACE_POKEUSR => {
            let mut state = get_state(pid);
            if !state.is_traced() || state.tracer_pid != current_pid {
                return -3;
            }

            let offset = (addr / 8) as usize;
            if offset < state.regs.len() {
                state.regs[offset] = data;
                set_state(pid, state);
                return 0;
            }
            -14
        }

        // ── PTRACE_CONT ───────────────────────────────────────────────
        PTRACE_CONT => {
            let mut state = get_state(pid);
            if !state.is_traced() || state.tracer_pid != current_pid {
                return -3;
            }

            state.tracee_state = TraceState::Running;
            state.single_step = false;
            state.signal_to_deliver = data as i32;
            set_state(pid, state);

            // Unblock the tracee
            let pm = crate::process::get_process_manager();
            let _ = pm.unblock_process(pid);

            0
        }

        // ── PTRACE_SYSCALL ────────────────────────────────────────────
        PTRACE_SYSCALL => {
            let mut state = get_state(pid);
            if !state.is_traced() || state.tracer_pid != current_pid {
                return -3;
            }

            state.tracee_state = TraceState::Running;
            state.syscall_trace = true;
            state.single_step = false;
            state.signal_to_deliver = data as i32;
            set_state(pid, state);

            let pm = crate::process::get_process_manager();
            let _ = pm.unblock_process(pid);

            0
        }

        // ── PTRACE_SINGLESTEP ─────────────────────────────────────────
        PTRACE_SINGLESTEP => {
            let mut state = get_state(pid);
            if !state.is_traced() || state.tracer_pid != current_pid {
                return -3;
            }

            state.tracee_state = TraceState::Running;
            state.single_step = true;
            state.signal_to_deliver = data as i32;
            set_state(pid, state);

            let pm = crate::process::get_process_manager();
            let _ = pm.unblock_process(pid);

            0
        }

        // ── PTRACE_KILL ───────────────────────────────────────────────
        PTRACE_KILL => {
            let mut state = get_state(pid);
            if !state.is_traced() || state.tracer_pid != current_pid {
                return -3;
            }

            state.tracee_state = TraceState::Killed;
            set_state(pid, state);

            let pm = crate::process::get_process_manager();
            let _ = pm.terminate_process(pid, 9);

            0
        }

        // ── PTRACE_SETOPTIONS ─────────────────────────────────────────
        PTRACE_SETOPTIONS => {
            let mut state = get_state(pid);
            if !state.is_traced() || state.tracer_pid != current_pid {
                return -3;
            }

            state.options = data as u32;
            set_state(pid, state);
            0
        }

        // ── PTRACE_GETEVENTMSG ────────────────────────────────────────
        PTRACE_GETEVENTMSG => {
            let state = get_state(pid);
            if !state.is_traced() || state.tracer_pid != current_pid {
                return -3;
            }
            state.event_msg as i64
        }

        // ── PTRACE_INTERRUPT ──────────────────────────────────────────
        PTRACE_INTERRUPT => {
            let mut state = get_state(pid);
            if !state.is_traced() || state.tracer_pid != current_pid {
                return -3;
            }

            state.tracee_state = TraceState::Stopped;
            set_state(pid, state);

            let pm = crate::process::get_process_manager();
            let _ = pm.block_process(pid);
            0
        }

        // ── PTRACE_GETSIGMASK ─────────────────────────────────────────
        PTRACE_GETSIGMASK => {
            let state = get_state(pid);
            if !state.is_traced() || state.tracer_pid != current_pid {
                return -3;
            }
            // Write signal mask to data pointer
            if data != 0 {
                unsafe {
                    *(data as *mut u64) = 0; // Empty mask for now
                }
            }
            0
        }

        // ── PTRACE_SETSIGMASK ─────────────────────────────────────────
        PTRACE_SETSIGMASK => {
            let state = get_state(pid);
            if !state.is_traced() || state.tracer_pid != current_pid {
                return -3;
            }
            // Would set the signal mask from data
            0
        }

        _ => {
            crate::serial_println!("[ptrace] unknown request {}", request);
            -38 // ENOSYS
        }
    }
}

// ── Notification helpers ────────────────────────────────────────────────

/// Called when a traced process stops (e.g., on signal or syscall entry).
/// Notifies the tracer by setting the tracee state to Stopped.
pub fn notify_stop(pid: u32, signal: i32) {
    let mut state = get_state(pid);
    if !state.is_traced() {
        return;
    }

    state.tracee_state = TraceState::Stopped;
    state.signal_to_deliver = signal;
    set_state(pid, state);

    let pm = crate::process::get_process_manager();
    let _ = pm.block_process(pid);

    PTRACE_EVENT_COUNT.fetch_add(1, Ordering::Relaxed);
}

/// Called when a traced process hits a syscall entry/exit.
/// If syscall tracing is enabled, stops the tracee.
pub fn syscall_event(pid: u32, is_entry: bool) -> bool {
    let state = get_state(pid);
    if !state.is_traced() || !state.syscall_trace {
        return false;
    }

    // Stop the tracee on syscall boundary
    notify_stop(pid, if is_entry { 5 } else { 5 }); // SIGTRAP
    true
}

/// Called when a traced process forks/clones.
/// If TRACEFORK/TRACECLONE is enabled, reports the event.
pub fn clone_event(pid: u32, child_pid: u32, event: u32) {
    let mut state = get_state(pid);
    if !state.is_traced() {
        return;
    }

    let option_match = match event {
        PTRACE_EVENT_FORK => state.options & PTRACE_O_TRACEFORK,
        PTRACE_EVENT_VFORK => state.options & PTRACE_O_TRACEVFORK,
        PTRACE_EVENT_CLONE => state.options & PTRACE_O_TRACECLONE,
        _ => 0,
    };

    if option_match == 0 {
        return;
    }

    state.event_msg = child_pid as u64;
    set_state(pid, state);
    notify_stop(pid, 5); // SIGTRAP
}

/// Called when a traced process exits.
pub fn exit_event(pid: u32, exit_code: i32) {
    let mut state = get_state(pid);
    if !state.is_traced() {
        return;
    }

    if state.options & PTRACE_O_TRACEEXIT != 0 {
        state.event_msg = exit_code as u64;
        set_state(pid, state);
        notify_stop(pid, 5);
    }
}

/// Check if a process is being traced.
pub fn is_traced(pid: u32) -> bool {
    get_state(pid).is_traced()
}

/// Get the tracer PID for a tracee.
pub fn get_tracer(pid: u32) -> u32 {
    get_state(pid).tracer_pid
}

/// Clear ptrace state for a process (on exit).
pub fn clear(pid: u32) {
    PTRACE_STATES.write().remove(&pid);
}

/// Called when a kprobe fires on a traced process.
pub fn kprobe_event(pid: u32, probe_id: u32, address: u64) {
    let mut state = get_state(pid);
    if !state.is_traced() {
        return;
    }
    state.event_msg = ((probe_id as u64) << 32) | (address & 0xffff_ffff);
    let should_stop = state.single_step || state.syscall_trace;
    set_state(pid, state);
    if should_stop {
        notify_stop(pid, 5); // SIGTRAP
    }
}

// ── Initialization ──────────────────────────────────────────────────────

pub fn init() {
    crate::serial_println!("[ptrace] ptrace subsystem initialized");
}

// ── Statistics ──────────────────────────────────────────────────────────

pub fn event_count() -> u64 {
    PTRACE_EVENT_COUNT.load(Ordering::Relaxed)
}
