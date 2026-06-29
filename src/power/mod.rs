//! Power management framework — suspend/hibernate state machine, notifier
//! chains, ACPI hooks, and `/sys/power/state` integration.

use alloc::string::String;
use core::sync::atomic::{AtomicU8, Ordering};
use spin::Mutex;

use crate::linux_compat::LinuxError;
use crate::notifier::{NotifierBlock, NotifierChain, NotifierFn};

/// Notifier event codes for suspend and hibernate chains.
pub const PM_SUSPEND_PREP: u32 = 0;
pub const PM_POST_SUSPEND: u32 = 1;
pub const PM_HIBERNATION_PREP: u32 = 2;
pub const PM_POST_HIBERNATION: u32 = 3;
pub const PM_RESTORE_PREP: u32 = 4;
pub const PM_POST_RESTORE: u32 = 5;

/// Suspend target written to `/sys/power/state`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuspendState {
    Freeze,
    Mem,
    Disk,
    Off,
}

impl SuspendState {
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim() {
            "freeze" => Some(Self::Freeze),
            "mem" | "standby" | "suspend" => Some(Self::Mem),
            "disk" | "hibernate" => Some(Self::Disk),
            "off" | "poweroff" => Some(Self::Off),
            _ => None,
        }
    }
}

/// Internal PM state machine phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PmPhase {
    Running,
    SuspendPrepare,
    Suspending,
    Suspended,
    Resuming,
    Hibernating,
    ShuttingDown,
}

static PM_PHASE: AtomicU8 = AtomicU8::new(PmPhase::Running as u8);
static LAST_ERROR: Mutex<Option<i32>> = Mutex::new(None);

lazy_static::lazy_static! {
    /// Notifier chain run before suspend (freeze/mem/disk).
    pub static ref PM_SUSPEND_CHAIN: NotifierChain = NotifierChain::new();
    /// Notifier chain run before hibernate-specific work.
    pub static ref PM_HIBERNATE_CHAIN: NotifierChain = NotifierChain::new();
}

fn set_phase(phase: PmPhase) {
    PM_PHASE.store(phase as u8, Ordering::Release);
}

fn current_phase() -> PmPhase {
    match PM_PHASE.load(Ordering::Acquire) {
        1 => PmPhase::SuspendPrepare,
        2 => PmPhase::Suspending,
        3 => PmPhase::Suspended,
        4 => PmPhase::Resuming,
        5 => PmPhase::Hibernating,
        6 => PmPhase::ShuttingDown,
        _ => PmPhase::Running,
    }
}

fn record_error(code: i32) {
    *LAST_ERROR.lock() = Some(code);
}

/// Register a callback on the suspend notifier chain.
pub fn register_suspend_notifier(call: NotifierFn, priority: i32) {
    PM_SUSPEND_CHAIN.register(NotifierBlock::new(call, priority));
}

/// Register a callback on the hibernate notifier chain.
pub fn register_hibernate_notifier(call: NotifierFn, priority: i32) {
    PM_HIBERNATE_CHAIN.register(NotifierBlock::new(call, priority));
}

/// Text for `/sys/power/state` read (space-separated states, newline terminated).
pub fn available_states_text() -> String {
    let mut out = String::from("freeze");
    if crate::acpi::power_management_available() {
        out.push_str(" mem");
    }
    out.push_str(" disk off\n");
    out
}

fn notify_suspend(event: u32) -> i32 {
    PM_SUSPEND_CHAIN.notify(event, core::ptr::null_mut())
}

fn notify_hibernate(event: u32) -> i32 {
    PM_HIBERNATE_CHAIN.notify(event, core::ptr::null_mut())
}

fn cpu_freeze_idle() {
    crate::serial_println!("[power] freeze: entering idle (HLT)");
    x86_64::instructions::interrupts::disable();
    loop {
        unsafe {
            core::arch::asm!("hlt", options(nomem, nostack, preserves_flags));
        }
    }
}

/// Suspend-to-idle (`freeze`): runs notifier chain then idles the boot CPU.
pub fn pm_suspend_freeze() -> Result<(), i32> {
    set_phase(PmPhase::SuspendPrepare);
    let _ = notify_suspend(PM_SUSPEND_PREP);
    set_phase(PmPhase::Suspending);

    cpu_freeze_idle();

    // Unreachable unless an NMI/device wakes the CPU.
    set_phase(PmPhase::Suspended);
    let _ = notify_suspend(PM_POST_SUSPEND);
    set_phase(PmPhase::Running);
    Ok(())
}

/// Suspend-to-RAM (`mem`): notifier chain + ACPI S3 attempt.
pub fn pm_suspend_mem() -> Result<(), i32> {
    set_phase(PmPhase::SuspendPrepare);
    let _ = notify_suspend(PM_SUSPEND_PREP);
    set_phase(PmPhase::Suspending);

    match crate::acpi::enter_sleep_state(crate::acpi::AcpiSleepState::SuspendToRam) {
        Ok(()) => {
            set_phase(PmPhase::Suspended);
            let _ = notify_suspend(PM_POST_SUSPEND);
            set_phase(PmPhase::Running);
            Ok(())
        }
        Err(code) => {
            record_error(code);
            set_phase(PmPhase::Running);
            Err(code)
        }
    }
}

/// Suspend-to-disk (`disk` / hibernate): separate notifier chain + ACPI S4 attempt.
pub fn pm_hibernate() -> Result<(), i32> {
    set_phase(PmPhase::Hibernating);
    let _ = notify_hibernate(PM_HIBERNATION_PREP);
    let _ = notify_suspend(PM_SUSPEND_PREP);

    match crate::acpi::enter_sleep_state(crate::acpi::AcpiSleepState::SuspendToDisk) {
        Ok(()) => {
            let _ = notify_hibernate(PM_POST_HIBERNATION);
            let _ = notify_suspend(PM_POST_SUSPEND);
            set_phase(PmPhase::Running);
            Ok(())
        }
        Err(code) => {
            record_error(code);
            set_phase(PmPhase::Running);
            Err(code)
        }
    }
}

/// Unified entry used by `/sys/power/state` writes.
pub fn request_state(raw: &str) -> Result<(), i32> {
    let state = SuspendState::parse(raw).ok_or(LinuxError::EINVAL as i32)?;
    match state {
        SuspendState::Freeze => pm_suspend_freeze(),
        SuspendState::Mem => pm_suspend_mem(),
        SuspendState::Disk => pm_hibernate(),
        SuspendState::Off => {
            set_phase(PmPhase::ShuttingDown);
            let _ = notify_suspend(PM_SUSPEND_PREP);
            let _ = crate::kernel::shutdown();
            Ok(())
        }
    }
}

/// Current PM phase (for debug / proc export).
pub fn phase() -> PmPhase {
    current_phase()
}

/// Last sleep error code, if any.
pub fn last_error() -> Option<i32> {
    *LAST_ERROR.lock()
}

/// Initialize PM framework and refresh sysfs power state file.
pub fn init() {
    lazy_static::initialize(&PM_SUSPEND_CHAIN);
    lazy_static::initialize(&PM_HIBERNATE_CHAIN);
    set_phase(PmPhase::Running);
    *LAST_ERROR.lock() = None;

    let text = available_states_text();
    let _ = crate::fs::update_sysfs_file("power/state", text.as_bytes());
}
