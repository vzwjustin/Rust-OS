//! Production kernel core module for RustOS
//!
//! Coordinates initialization and management of all kernel subsystems

use alloc::string::String;
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;

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
static INIT_STAGE: AtomicU32 = AtomicU32::new(0);

/// Subsystem registry
static SUBSYSTEMS: Mutex<alloc::vec::Vec<Subsystem>> = Mutex::new(alloc::vec::Vec::new());

/// Initialize kernel core
pub fn init() -> Result<(), &'static str> {
    if KERNEL_INITIALIZED.load(Ordering::Acquire) {
        return Ok(());
    }

    INIT_STAGE.store(1, Ordering::Release);

    // Register core subsystems
    register_subsystem("memory", 1, &[]);
    register_subsystem("gdt", 2, &["memory"]);
    register_subsystem("interrupts", 3, &["gdt"]);
    register_subsystem("time", 4, &["interrupts"]);
    register_subsystem("notifier", 5, &[]);
    register_subsystem("arch", 6, &[]);
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

    KERNEL_INITIALIZED.store(true, Ordering::Release);
    Ok(())
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

/// Initialize all kernel subsystems in order
pub fn init_all_subsystems() -> Result<(), &'static str> {
    // Get sorted list of subsystems by init_order
    let mut systems = {
        let systems_lock = SUBSYSTEMS.lock();
        let mut sys_vec = (*systems_lock).clone();
        sys_vec.sort_by_key(|s| s.init_order);
        sys_vec
    };

    // Initialize each subsystem
    for system in &mut systems {
        // Check dependencies
        if !check_dependencies(system.name) {
            return Err("Dependency check failed");
        }

        // Update state
        update_subsystem_state(system.name, SubsystemState::Initializing)?;

        // Call subsystem-specific init
        let result = match system.name {
            "memory" => Ok(()), // Already initialized by bootloader
            "gdt" => {
                crate::gdt::init();
                Ok(())
            }
            "interrupts" => Ok(()), // IDT initialization handled in main.rs
            "time" => crate::time::init(),
            "notifier" => {
                crate::notifier::init();
                Ok(())
            }
            "arch" => crate::arch::init(),
            "smp" => crate::smp::init(),
            "scheduler" => {
                let _ = crate::scheduler::init();
                Ok(())
            }
            "security" => crate::security::init(),
            "crypto" => {
                crate::crypto::init();
                Ok(())
            }
            "process" => crate::process::init(),
            "drivers" => crate::drivers::init_drivers(),
            "filesystem" => crate::fs::init().map_err(|_| "Filesystem init failed"),
            "network" => Ok(()), // Network init handled by drivers
            "softirq" => {
                crate::softirq::init();
                Ok(())
            }
            "futex" => {
                crate::futex::init();
                Ok(())
            }
            "epoll" => {
                crate::epoll::init();
                Ok(())
            }
            "oom" => {
                crate::oom::init();
                Ok(())
            }
            "swap" => {
                crate::swap::init();
                Ok(())
            }
            "block_io" => {
                crate::block_io::init();
                Ok(())
            }
            "cgroup" => {
                crate::cgroup::init();
                Ok(())
            }
            "seccomp" => {
                crate::seccomp::init();
                Ok(())
            }
            "namespace" => {
                crate::namespace::init();
                Ok(())
            }
            "ptrace" => {
                crate::ptrace::init();
                Ok(())
            }
            "inotify" => {
                crate::inotify::init();
                Ok(())
            }
            "pidfd" => {
                crate::pidfd::init();
                Ok(())
            }
            "io_uring" => {
                crate::io_uring::init();
                Ok(())
            }
            "fanotify" => {
                crate::fanotify::init();
                Ok(())
            }
            "mount_api" => {
                crate::mount_api::init();
                Ok(())
            }
            "landlock" => {
                crate::landlock::init();
                Ok(())
            }
            "bpf" => {
                crate::bpf::init();
                Ok(())
            }
            "perf_event" => {
                crate::perf_event::init();
                Ok(())
            }
            "keyring" => {
                crate::keyring::init();
                Ok(())
            }
            "sysv_ipc" => {
                crate::sysv_ipc::init();
                Ok(())
            }
            "aio" => {
                crate::aio::init();
                Ok(())
            }
            "module_loader" => {
                crate::module_loader::init();
                Ok(())
            }
            "kexec" => {
                crate::kexec::init();
                Ok(())
            }
            "thp" => {
                crate::thp::init();
                Ok(())
            }
            "memory_hotplug" => {
                crate::memory_hotplug::init();
                Ok(())
            }
            "efi" => {
                crate::efi::init();
                Ok(())
            }
            "of" => {
                crate::of::init();
                Ok(())
            }
            "kasan" => {
                crate::kasan::init();
                Ok(())
            }
            "kcsan" => {
                crate::kcsan::init();
                Ok(())
            }
            "hugetlb" => {
                crate::hugetlb::init();
                Ok(())
            }
            "power" => {
                crate::power::init();
                Ok(())
            }
            "audit" => {
                crate::audit::init();
                Ok(())
            }
            "numa" => {
                crate::numa::init();
                Ok(())
            }
            "rcu" => {
                crate::rcu::init();
                Ok(())
            }
            "cpufreq" => {
                crate::cpufreq::init();
                Ok(())
            }
            "cpuidle" => {
                crate::cpuidle::init();
                Ok(())
            }
            "livepatch" => {
                crate::livepatch::init();
                Ok(())
            }
            "edac" => {
                crate::edac::init();
                Ok(())
            }
            "mfd" => {
                crate::mfd::init();
                Ok(())
            }
            "nvdimm" => {
                crate::nvdimm::init();
                Ok(())
            }
            "trace" => {
                crate::trace::init();
                Ok(())
            }
            "kprobes" => {
                crate::kprobes::init();
                Ok(())
            }
            _ => Ok(()),
        };

        match result {
            Ok(()) => {
                update_subsystem_state(system.name, SubsystemState::Ready)?;
                INIT_STAGE.fetch_add(1, Ordering::Relaxed);
            }
            Err(e) => {
                update_subsystem_state(system.name, SubsystemState::Failed)?;
                return Err(e);
            }
        }
    }

    KERNEL_READY.store(true, Ordering::Release);
    Ok(())
}

/// Get current kernel initialization stage
pub fn init_stage() -> u32 {
    INIT_STAGE.load(Ordering::Acquire)
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
