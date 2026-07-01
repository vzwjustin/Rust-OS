//! Production kernel core module for RustOS
//!
//! Coordinates initialization and management of all kernel subsystems

use alloc::string::String;
use core::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use spin::Mutex;

/// Kernel system state — mirrors Linux's `system_state` enum in
/// include/linux/kernel.h.  Tracks the high-level boot phase so
/// subsystems can adjust behavior (e.g. accept certain syscalls only
/// when SYSTEM_RUNNING).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SystemState {
    /// Initial state during early boot (start_kernel phase).
    Booting = 0,
    /// Scheduler is running, SMP bringup in progress.
    Scheduling = 1,
    /// Freeing __init memory (between do_basic_setup and userspace).
    FreeingInitmem = 2,
    /// Fully operational — userspace init has been launched.
    Running = 3,
    /// Shutdown in progress.
    Halting = 4,
    /// Power off in progress.
    PowerOff = 5,
    /// Restart in progress.
    Restart = 6,
    /// Suspending to RAM.
    Suspending = 7,
}

impl SystemState {
    pub fn as_str(self) -> &'static str {
        match self {
            SystemState::Booting => "BOOTING",
            SystemState::Scheduling => "SCHEDULING",
            SystemState::FreeingInitmem => "FREEING_INITMEM",
            SystemState::Running => "RUNNING",
            SystemState::Halting => "HALTING",
            SystemState::PowerOff => "POWER_OFF",
            SystemState::Restart => "RESTART",
            SystemState::Suspending => "SUSPENDING",
        }
    }
}

/// Global system state (mirrors Linux's `system_state`).
static SYSTEM_STATE: AtomicU8 = AtomicU8::new(SystemState::Booting as u8);

/// Get the current system state.
pub fn system_state() -> SystemState {
    // SAFETY: SystemState is repr(u8) and all valid values are defined.
    match SYSTEM_STATE.load(Ordering::Acquire) {
        0 => SystemState::Booting,
        1 => SystemState::Scheduling,
        2 => SystemState::FreeingInitmem,
        3 => SystemState::Running,
        4 => SystemState::Halting,
        5 => SystemState::PowerOff,
        6 => SystemState::Restart,
        7 => SystemState::Suspending,
        _ => SystemState::Booting,
    }
}

/// Set the system state.  Called at key boot milestones to mirror
/// Linux's `system_state = SYSTEM_*` assignments.
pub fn set_system_state(state: SystemState) {
    SYSTEM_STATE.store(state as u8, Ordering::Release);
    crate::serial_println!("[kernel] system_state = {}", state.as_str());
}

/// Kernel subsystem state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubsystemState {
    Uninitialized,
    Initializing,
    Ready,
    Failed,
    Shutdown,
}

/// Kernel subsystem information
#[derive(Debug, Clone)]
pub struct Subsystem {
    pub name: &'static str,
    pub state: SubsystemState,
    pub init_order: u32,
    pub dependencies: &'static [&'static str],
}

/// Kernel panic information
#[derive(Debug)]
pub struct PanicInfo {
    pub message: String,
    pub file: &'static str,
    pub line: u32,
    pub column: u32,
}

/// Global kernel state
static KERNEL_INITIALIZED: AtomicBool = AtomicBool::new(false);
static KERNEL_READY: AtomicBool = AtomicBool::new(false);

/// Subsystem registry
static SUBSYSTEMS: Mutex<alloc::vec::Vec<Subsystem>> = Mutex::new(alloc::vec::Vec::new());

/// Initialize kernel core
pub fn init() -> Result<(), &'static str> {
    if KERNEL_INITIALIZED.load(Ordering::Acquire) {
        return Ok(());
    }

    // Register core subsystems
    register_subsystem("memory", 1, &[]);
    register_subsystem("gdt", 2, &["memory"]);
    register_subsystem("interrupts", 3, &["gdt"]);
    register_subsystem("time", 4, &["interrupts"]);
    register_subsystem("notifier", 5, &[]);
    register_subsystem("arch", 6, &[]);
    register_subsystem("acpi", 62, &["arch"]);
    register_subsystem("apic", 63, &["interrupts", "acpi"]);
    register_subsystem("smp", 7, &["arch", "interrupts"]);
    register_subsystem("scheduler", 8, &["smp", "time"]);
    register_subsystem("security", 9, &[]);
    register_subsystem("crypto", 9, &["memory"]);
    register_subsystem("process", 10, &["scheduler", "security", "memory"]);
    register_subsystem("drivers", 11, &["interrupts", "memory"]);
    register_subsystem("filesystem", 12, &["drivers"]);
    register_subsystem("network", 13, &["drivers"]);
    register_subsystem("linux_compat", 14, &["filesystem", "network", "process"]);
    register_subsystem(
        "linux_integration",
        15,
        &["linux_compat", "filesystem", "network", "process"],
    );
    register_subsystem("syscall", 16, &["gdt", "interrupts"]);
    register_subsystem("softirq", 16, &["interrupts"]);
    register_subsystem("futex", 17, &["process"]);
    register_subsystem("epoll", 18, &["linux_compat"]);
    register_subsystem("oom", 19, &["memory", "process"]);
    register_subsystem("swap", 20, &["memory", "block_io"]);
    register_subsystem("block_io", 21, &["drivers"]);
    register_subsystem("cgroup", 22, &["process", "memory"]);
    register_subsystem("seccomp", 23, &["process"]);
    register_subsystem("namespace", 24, &["process"]);
    register_subsystem("ptrace", 25, &["process"]);
    register_subsystem("inotify", 26, &["filesystem"]);
    register_subsystem("pidfd", 27, &["process", "filesystem"]);
    register_subsystem("io_uring", 28, &["filesystem", "block_io"]);
    register_subsystem("fanotify", 29, &["filesystem", "vfs"]);
    register_subsystem("mount_api", 30, &["vfs", "filesystem"]);
    register_subsystem("landlock", 31, &["process", "vfs"]);
    register_subsystem("bpf", 32, &["vfs", "net"]);
    register_subsystem("perf_event", 33, &["time", "process"]);
    register_subsystem("keyring", 34, &["process"]);
    register_subsystem("sysv_ipc", 35, &["process", "memory"]);
    register_subsystem("aio", 36, &["vfs", "block_io"]);
    register_subsystem("module_loader", 37, &[]);
    register_subsystem("kexec", 38, &[]);
    register_subsystem("userfaultfd", 39, &["linux_compat", "memory", "vfs"]);
    register_subsystem("memfd_secret", 40, &["linux_compat", "memory", "vfs"]);
    register_subsystem("file_handle", 41, &["linux_compat", "vfs"]);
    register_subsystem("privileged_syscalls", 42, &["linux_compat", "process"]);
    register_subsystem("thp", 43, &["memory", "hugetlb"]);
    register_subsystem("memory_hotplug", 44, &["memory"]);
    register_subsystem("efi", 45, &[]);
    register_subsystem("of", 46, &[]);
    register_subsystem("kasan", 47, &["memory"]);
    register_subsystem("kcsan", 48, &[]);
    register_subsystem("hugetlb", 49, &["memory"]);
    register_subsystem("power", 50, &["notifier"]);
    register_subsystem("numa", 51, &["memory", "smp"]);
    register_subsystem("rcu", 52, &["softirq", "smp"]);
    register_subsystem("cpufreq", 53, &["smp", "scheduler"]);
    register_subsystem("cpuidle", 54, &["smp"]);
    register_subsystem("audit", 55, &[]);
    register_subsystem("livepatch", 56, &["module_loader"]);
    register_subsystem("edac", 57, &["numa"]);
    register_subsystem("mfd", 58, &["drivers"]);
    register_subsystem("nvdimm", 59, &["acpi", "numa"]);
    register_subsystem("trace", 60, &["time", "smp"]);
    register_subsystem("kprobes", 61, &["trace", "ptrace"]);

    // Subsystems initialized in the manual Linux-compat boot path but not
    // previously tracked in the registry.
    register_subsystem("graphics", 64, &["drivers"]);
    register_subsystem("sound", 65, &["drivers"]);
    register_subsystem("gpu", 66, &["drivers"]);
    register_subsystem("process_manager", 67, &["process"]);
    register_subsystem("vfs", 68, &["filesystem"]);
    register_subsystem("initramfs", 69, &["vfs"]);
    register_subsystem("dbus", 70, &["linux_integration"]);
    register_subsystem("wayland", 71, &["dbus", "graphics"]);
    register_subsystem("desktop", 72, &["wayland"]);
    register_subsystem("workqueue", 73, &["softirq"]);

    crate::notifier::init();
    update_subsystem_state("notifier", SubsystemState::Ready)?;

    KERNEL_INITIALIZED.store(true, Ordering::Release);
    Ok(())
}

/// Mark a subsystem as Ready.  Silently ignores unknown subsystem names
/// so callers don't need to check whether a name is registered.
pub fn mark_subsystem_ready(name: &'static str) {
    let _ = update_subsystem_state(name, SubsystemState::Ready);
}

/// Mark a subsystem as Failed.  Silently ignores unknown subsystem names.
pub fn mark_subsystem_failed(name: &'static str) {
    let _ = update_subsystem_state(name, SubsystemState::Failed);
}

/// Mark the manually initialized boot path as ready for normal operation.
/// Only subsystems still in `Uninitialized` or `Initializing` state are
/// promoted to `Ready`; subsystems explicitly marked `Failed` retain
/// their failure state so `is_subsystem_ready()` returns false for them.
pub fn mark_boot_ready() {
    let mut systems = SUBSYSTEMS.lock();
    for system in systems.iter_mut() {
        if matches!(system.state, SubsystemState::Initializing) {
            system.state = SubsystemState::Ready;
        }
    }
    KERNEL_READY.store(true, Ordering::Release);
}

/// Register a kernel subsystem
pub fn register_subsystem(name: &'static str, order: u32, deps: &'static [&'static str]) {
    let mut systems = SUBSYSTEMS.lock();
    systems.push(Subsystem {
        name,
        state: SubsystemState::Uninitialized,
        init_order: order,
        dependencies: deps,
    });
}

/// Update subsystem state
pub fn update_subsystem_state(
    name: &'static str,
    state: SubsystemState,
) -> Result<(), &'static str> {
    let mut systems = SUBSYSTEMS.lock();

    for system in systems.iter_mut() {
        if system.name == name {
            system.state = state;
            return Ok(());
        }
    }

    Err("Subsystem not found")
}

/// Check if a subsystem is ready
pub fn is_subsystem_ready(name: &'static str) -> bool {
    let systems = SUBSYSTEMS.lock();

    for system in systems.iter() {
        if system.name == name {
            return system.state == SubsystemState::Ready;
        }
    }

    false
}

/// Check if all dependencies are met for a subsystem
pub fn check_dependencies(name: &'static str) -> bool {
    let systems = SUBSYSTEMS.lock();

    if let Some(system) = systems.iter().find(|s| s.name == name) {
        for dep in system.dependencies {
            let dep_ready = systems
                .iter()
                .find(|s| s.name == *dep)
                .map(|s| s.state == SubsystemState::Ready)
                .unwrap_or(false);

            if !dep_ready {
                return false;
            }
        }
        true
    } else {
        false
    }
}

/// Check if kernel is fully initialized
pub fn is_initialized() -> bool {
    KERNEL_INITIALIZED.load(Ordering::Acquire)
}

/// Check if kernel is ready for normal operation
pub fn is_ready() -> bool {
    KERNEL_READY.load(Ordering::Acquire)
}

/// Get list of all subsystems and their states
pub fn get_subsystem_status() -> alloc::vec::Vec<(String, SubsystemState)> {
    use alloc::string::ToString;
    let systems = SUBSYSTEMS.lock();
    let mut result = alloc::vec::Vec::new();
    for system in systems.iter() {
        result.push((system.name.to_string(), system.state));
    }
    result
}

/// Kernel panic handler
pub fn panic(info: PanicInfo) -> ! {
    // Disable interrupts
    x86_64::instructions::interrupts::disable();

    let info_ptr = &info as *const PanicInfo as *mut core::ffi::c_void;
    let _ = crate::notifier::PANIC_CHAIN.notify(1, info_ptr);

    // Try to print panic info if possible
    crate::println!("KERNEL PANIC!");
    crate::println!("{}", info.message);
    crate::println!("Location: {}:{}:{}", info.file, info.line, info.column);

    // Halt all CPUs
    if crate::smp::is_initialized() {
        let _ = crate::smp::broadcast_ipi(0xFF); // Send halt IPI
    }

    // Infinite loop
    loop {
        x86_64::instructions::hlt();
    }
}

/// Shutdown kernel
pub fn shutdown() -> Result<(), &'static str> {
    if !KERNEL_READY.load(Ordering::Acquire) {
        return Err("Kernel not ready");
    }

    let _ = crate::notifier::REBOOT_CHAIN.notify(1, core::ptr::null_mut());

    // Shutdown subsystems in reverse order
    let systems = {
        let systems_lock = SUBSYSTEMS.lock();
        let mut sys_vec = (*systems_lock).clone();
        sys_vec.sort_by_key(|s| core::cmp::Reverse(s.init_order));
        sys_vec
    };

    for system in systems {
        update_subsystem_state(system.name, SubsystemState::Shutdown)?;
    }

    KERNEL_READY.store(false, Ordering::Release);
    Ok(())
}
