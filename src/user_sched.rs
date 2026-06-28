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

    if run_user_process(pid).is_err() {
        PENDING_USER_PID.store(pid, Ordering::Release);
    }
}

/// True while a bootstrap user task is running.
pub fn user_bootstrap_active() -> bool {
    USER_BOOTSTRAP_ACTIVE.load(Ordering::Acquire)
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

/// Take the `iretq` resume target after bootstrap `exit`, if any.
pub fn take_user_resume() -> Option<(u64, u64)> {
    if !USER_BOOTSTRAP_ACTIVE.load(Ordering::Acquire) {
        return None;
    }
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

extern "C" fn user_bootstrap_after_exit() {
    end_user_bootstrap();

    if let Some(hook) = AFTER_USER_HOOK.lock().take() {
        hook();
    }

    let resume = USER_KERNEL_RESUME.load(Ordering::Acquire);
    if resume != RESUME_NONE {
        unsafe {
            core::arch::asm!("jmp {}", in(reg) resume, options(noreturn));
        }
    }
    loop {
        x86_64::instructions::hlt();
    }
}

extern "C" fn user_bootstrap_kernel_entry() {}

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

fn push_return_address(sp: u64, return_addr: u64) -> u64 {
    let sp = sp.wrapping_sub(8);
    unsafe {
        (sp as *mut u64).write(return_addr);
    }
    sp
}

fn run_user_process(child_pid: u32) -> Result<(), ()> {
    if user_bootstrap_active() {
        return Err(());
    }

    let kernel_pm = process::get_process_manager();
    let pcb = kernel_pm.get_process(child_pid).ok_or(())?;

    if !is_user_process(&pcb) {
        return Err(());
    }

    let entry = pcb.entry_point;
    let rsp = pcb.context.rsp;
    let parent_pid = process::current_pid();
    let resume_sp = push_return_address(
        bootstrap_stack_top(),
        user_bootstrap_after_exit as *const () as u64,
    );
    begin_user_bootstrap(
        parent_pid,
        user_bootstrap_kernel_entry as *const () as u64,
        resume_sp,
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

    unsafe {
        crate::usermode::switch_to_user_mode(entry, rsp);
    }
}
