//! Run Ring-3 processes from kernel context (spawn bootstrap + scheduler service).
//!
//! Uses `iretq` return patching on INT 0x80 `exit` so kernel code can resume after
//! a user task finishes without blocking forever in `switch_to_user_mode`.

use crate::process::{self, ProcessControlBlock, ProcessState};
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

const RESUME_NONE: u64 = u64::MAX;
const BOOTSTRAP_STACK_SIZE: usize = 8192;

static USER_BOOTSTRAP_ACTIVE: AtomicBool = AtomicBool::new(false);
static USER_HOST_PARENT: AtomicU32 = AtomicU32::new(0);
static USER_CHILD_EXIT: AtomicI32 = AtomicI32::new(0);
static USER_IRETQ_RIP: AtomicU64 = AtomicU64::new(RESUME_NONE);
static USER_IRETQ_RSP: AtomicU64 = AtomicU64::new(RESUME_NONE);
static USER_KERNEL_RESUME: AtomicU64 = AtomicU64::new(RESUME_NONE);
static PENDING_USER_PID: AtomicU32 = AtomicU32::new(0);
static SCHED_TICK_PENDING: AtomicBool = AtomicBool::new(false);
/// Budget for one-shot Ring-3 bring-up diagnostics (avoids flooding serial when
/// service_pending re-queues a failing pid every idle-loop iteration).
static DIAG_BUDGET: AtomicU32 = AtomicU32::new(12);

/// Returns true at most `DIAG_BUDGET` times, then suppresses further logs.
fn diag_allowed() -> bool {
    DIAG_BUDGET
        .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| n.checked_sub(1))
        .is_ok()
}
static BOOTSTRAP_STACK_TOP: AtomicU64 = AtomicU64::new(0);

static AFTER_USER_HOOK: Mutex<Option<fn()>> = Mutex::new(None);

/// Returns true when `cs` selects a Ring-3 code segment.
pub fn is_user_code_segment(cs: u16) -> bool {
    cs & 0x3 == 0x3
}

/// Returns true when the process has a user-mode entry context.
pub fn is_user_process(pcb: &ProcessControlBlock) -> bool {
    pcb.entry_point != 0
        && is_user_code_segment(pcb.context.cs)
        && crate::usermode::is_valid_user_address(pcb.entry_point, 1)
        && crate::usermode::is_valid_user_address(pcb.context.rsp, 16)
}

/// Queue a user process to run on the next `service_pending` call.
pub fn queue_user_pid(pid: u32) {
    if pid != 0 {
        PENDING_USER_PID.store(pid, Ordering::Release);
    }
}

/// Mark that the timer tick requested a scheduling pass (serviced outside IRQ context).
pub fn note_schedule_tick() {
    SCHED_TICK_PENDING.store(true, Ordering::Release);
}

/// Register a one-shot hook invoked after a bootstrap user task exits (before kernel resume).
pub fn set_after_user_hook(hook: Option<fn()>) {
    *AFTER_USER_HOOK.lock() = hook;
}

/// Record the kernel `rip` to jump to after bootstrap user exit completes.
pub fn set_kernel_resume(rip: u64) {
    USER_KERNEL_RESUME.store(rip, Ordering::Release);
}

/// Service queued user processes. `resume_rip` is where the kernel continues after user exit.
pub fn service_pending(resume_rip: u64) {
    if user_bootstrap_active() {
        return;
    }

    set_kernel_resume(resume_rip);

    if SCHED_TICK_PENDING.swap(false, Ordering::AcqRel) {
        if let Ok(Some(next_pid)) = process::get_process_manager().schedule() {
            process::get_process_manager().set_current_process(next_pid);
            if let Some(pcb) = process::get_process_manager().get_process(next_pid) {
                if is_user_process(&pcb)
                    && matches!(pcb.state, ProcessState::Ready | ProcessState::Running)
                {
                    queue_user_pid(next_pid);
                }
            }
        }
    }

    let pid = PENDING_USER_PID.swap(0, Ordering::AcqRel);
    if pid == 0 {
        return;
    }

    if diag_allowed() {
        crate::serial_println!("user_sched: service_pending picked pid={}", pid);
    }
    if run_user_process(pid).is_err() {
        if diag_allowed() {
            crate::serial_println!("user_sched: run_user_process({}) failed; re-queueing", pid);
        }
        PENDING_USER_PID.store(pid, Ordering::Release);
    }
}

/// True while a bootstrap user task is running.
pub fn user_bootstrap_active() -> bool {
    USER_BOOTSTRAP_ACTIVE.load(Ordering::Acquire)
}

fn user_resume_pending() -> bool {
    let rip = USER_IRETQ_RIP.load(Ordering::Acquire);
    let rsp = USER_IRETQ_RSP.load(Ordering::Acquire);
    rip != RESUME_NONE && rsp != RESUME_NONE
}

/// Handle `exit` from a bootstrap user task (INT 0x80 path).
pub fn complete_user_exit(status: i32) {
    let child_pid = process::current_pid();
    let parent_pid = USER_HOST_PARENT.load(Ordering::Acquire);

    USER_CHILD_EXIT.store(status, Ordering::Release);

    let pm = crate::process_manager::get_process_manager();
    let _ = pm.exit(child_pid, status);
    process::get_process_manager().set_current_process(parent_pid);
    pm.set_current_pid(parent_pid);
}

/// Complete a bootstrap user exit when a kernel resume target is still armed.
pub fn complete_user_exit_if_pending(status: i32) -> bool {
    if !user_resume_pending() {
        if USER_KERNEL_RESUME.load(Ordering::Acquire) == RESUME_NONE {
            return false;
        }

        USER_IRETQ_RIP.store(
            user_bootstrap_kernel_entry as *const () as u64,
            Ordering::Release,
        );
        USER_IRETQ_RSP.store(bootstrap_stack_top(), Ordering::Release);
    }

    complete_user_exit(status);
    true
}

/// Take the `iretq` resume target after bootstrap `exit`, if any.
pub fn take_user_resume() -> Option<(u64, u64)> {
    let rip = USER_IRETQ_RIP.load(Ordering::Acquire);
    let rsp = USER_IRETQ_RSP.load(Ordering::Acquire);
    if rip == RESUME_NONE || rsp == RESUME_NONE {
        return None;
    }
    Some((rip, rsp))
}

pub fn last_child_exit_status() -> i32 {
    USER_CHILD_EXIT.load(Ordering::Acquire)
}

fn end_user_bootstrap() {
    USER_BOOTSTRAP_ACTIVE.store(false, Ordering::Release);
    USER_IRETQ_RIP.store(RESUME_NONE, Ordering::Release);
    USER_IRETQ_RSP.store(RESUME_NONE, Ordering::Release);
}

extern "C" fn user_bootstrap_kernel_entry() {
    // First kernel code executed after a bootstrap Ring-3 task exits.
    // The iretq return frame was patched by the syscall handler to land
    // here with the bootstrap stack. We finish the bootstrap, run any
    // registered post-exit hook, and jump to the kernel resume address
    // registered by `service_pending` before the user task started.
    end_user_bootstrap();

    if let Some(hook) = AFTER_USER_HOOK.lock().take() {
        hook();
    }

    let resume = USER_KERNEL_RESUME.load(Ordering::Acquire);
    if resume != RESUME_NONE {
        // SAFETY: `sti` followed by `jmp` atomically enables interrupts and
        // jumps to the resume address. The `sti` instruction executes before
        // any interrupt can be delivered (single-instruction window), and the
        // `jmp` is `noreturn`. `resume` is a validated user-space entry point
        // loaded from `USER_KERNEL_RESUME`.
        unsafe {
            core::arch::asm!("sti", "jmp {}", in(reg) resume, options(noreturn));
        }
    }

    loop {
        x86_64::instructions::interrupts::enable();
        x86_64::instructions::hlt();
    }
}

fn begin_user_bootstrap(parent_pid: u32, entry_rip: u64, entry_rsp: u64) {
    USER_HOST_PARENT.store(parent_pid, Ordering::Release);
    USER_IRETQ_RIP.store(entry_rip, Ordering::Release);
    USER_IRETQ_RSP.store(entry_rsp, Ordering::Release);
    USER_BOOTSTRAP_ACTIVE.store(true, Ordering::Release);
}

fn bootstrap_stack_top() -> u64 {
    let existing = BOOTSTRAP_STACK_TOP.load(Ordering::Acquire);
    if existing != 0 {
        return existing;
    }

    let mut stack = Vec::new();
    stack.resize(BOOTSTRAP_STACK_SIZE, 0);
    let stack = Box::leak(stack.into_boxed_slice());
    let base = stack.as_mut_ptr() as u64;
    let top = (base + stack.len() as u64) & !0xF;

    match BOOTSTRAP_STACK_TOP.compare_exchange(0, top, Ordering::AcqRel, Ordering::Acquire) {
        Ok(_) => top,
        Err(existing) => existing,
    }
}

fn run_user_process(child_pid: u32) -> Result<(), ()> {
    if user_bootstrap_active() {
        return Err(());
    }

    let kernel_pm = process::get_process_manager();
    let pcb = match kernel_pm.get_process(child_pid) {
        Some(p) => p,
        None => {
            if diag_allowed() {
                crate::serial_println!("user_sched: pid {} not found", child_pid);
            }
            return Err(());
        }
    };

    if !is_user_process(&pcb) {
        if diag_allowed() {
            crate::serial_println!(
                "user_sched: pid {} not user-ready entry={:#x} cs={:#x} rsp={:#x}",
                child_pid,
                pcb.entry_point,
                pcb.context.cs,
                pcb.context.rsp
            );
        }
        return Err(());
    }

    let entry = pcb.entry_point;
    let rsp = pcb.context.rsp;
    if diag_allowed() {
        crate::serial_println!(
            "user_sched: ENTER Ring-3 pid={} entry={:#x} rsp={:#x}",
            child_pid,
            entry,
            rsp
        );
    }
    let parent_pid = process::current_pid();
    begin_user_bootstrap(
        parent_pid,
        user_bootstrap_kernel_entry as *const () as u64,
        bootstrap_stack_top(),
    );

    let pm = crate::process_manager::get_process_manager();
    pm.set_current_pid(child_pid);
    kernel_pm.set_current_process(child_pid);
    let _ = pm.set_process_state(
        child_pid,
        crate::process_manager::pcb::ProcessState::Running,
    );
    let _ = kernel_pm.with_process_mut(child_pid, |pcb| {
        pcb.set_state(ProcessState::Running);
    });

    // SAFETY: sti instruction enables interrupts; called from ring 0 after scheduling setup.
    unsafe {
        crate::usermode::switch_to_user_mode_without_interrupts(entry, rsp);
    }
}
