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
    register_subsystem("arch", 5, &[]);
    register_subsystem("smp", 6, &["arch", "interrupts"]);
    register_subsystem("scheduler", 7, &["smp", "time"]);
    register_subsystem("security", 8, &[]);
    register_subsystem("process", 9, &["scheduler", "security", "memory"]);
    register_subsystem("drivers", 10, &["interrupts", "memory"]);
    register_subsystem("filesystem", 11, &["drivers"]);
    register_subsystem("network", 12, &["drivers"]);
    register_subsystem("linux_compat", 13, &["filesystem", "network", "process"]);
    register_subsystem(
        "linux_integration",
        14,
        &["linux_compat", "filesystem", "network", "process"],
    );

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
            "arch" => crate::arch::init(),
            "smp" => crate::smp::init(),
            "scheduler" => {
                crate::scheduler::init();
                Ok(())
            }
            "security" => crate::security::init(),
            "process" => crate::process::init(),
            "drivers" => crate::drivers::init_drivers(),
            "filesystem" => crate::fs::init().map_err(|_| "Filesystem init failed"),
            "network" => Ok(()), // Network init handled by drivers
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
