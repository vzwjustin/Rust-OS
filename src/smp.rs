//! Production SMP (Symmetric MultiProcessing) support for RustOS
//!
//! Real multiprocessor support using APIC and x86_64 features

use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use spin::Mutex;
use x86_64::VirtAddr;

/// Maximum number of CPUs supported
pub const MAX_CPUS: usize = 256;

/// Per-CPU data structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CpuData {
    pub cpu_id: u32,
    pub apic_id: u32,
    pub online: bool,
    pub idle_time: u64,
    pub kernel_stack: VirtAddr,
    pub tss_selector: u16,
}

impl CpuData {
    const fn new() -> Self {
        Self {
            cpu_id: 0,
            apic_id: 0,
            online: false,
            idle_time: 0,
            kernel_stack: VirtAddr::zero(),
            tss_selector: 0,
        }
    }
}

/// Global CPU data array
static CPU_DATA: Mutex<[CpuData; MAX_CPUS]> = Mutex::new([CpuData::new(); MAX_CPUS]);
/// Number of detected CPUs
static CPU_COUNT: AtomicU32 = AtomicU32::new(0);
/// Number of online CPUs
static ONLINE_CPUS: AtomicU32 = AtomicU32::new(1); // BSP is always online
/// SMP initialized flag
static INITIALIZED: AtomicBool = AtomicBool::new(false);

/// Local APIC base address
static LOCAL_APIC_BASE: AtomicU64 = AtomicU64::new(0);

use core::sync::atomic::AtomicU64;

/// APIC register offsets
mod apic_regs {
    pub const APIC_ID: u32 = 0x20;
    pub const APIC_VERSION: u32 = 0x30;
    pub const APIC_TPR: u32 = 0x80;
    pub const APIC_EOI: u32 = 0xB0;
    pub const APIC_SPURIOUS: u32 = 0xF0;
    pub const APIC_ICR_LOW: u32 = 0x300;
    pub const APIC_ICR_HIGH: u32 = 0x310;
    pub const APIC_TIMER_LVT: u32 = 0x320;
    pub const APIC_TIMER_INITIAL: u32 = 0x380;
    pub const APIC_TIMER_CURRENT: u32 = 0x390;
    pub const APIC_TIMER_DIVIDE: u32 = 0x3E0;
}

/// Initialize SMP subsystem
pub fn init() -> Result<(), &'static str> {
    // Check if already initialized
    if INITIALIZED.load(Ordering::Acquire) {
        return Ok(());
    }

    // Get Local APIC base from MSR
    let apic_base = unsafe { read_msr(0x1B) };
    if apic_base & (1 << 11) == 0 {
        return Err("APIC not enabled");
    }

    let apic_phys = apic_base & 0xFFFF_F000;
    LOCAL_APIC_BASE.store(apic_phys, Ordering::Release);

    // Initialize BSP (Bootstrap Processor) data
    let mut cpu_data = CPU_DATA.lock();
    cpu_data[0] = CpuData {
        cpu_id: 0,
        apic_id: get_apic_id(),
        online: true,
        idle_time: 0,
        kernel_stack: VirtAddr::zero(), // Will be set by memory manager
        tss_selector: 0,                // Will be set by GDT
    };

    CPU_COUNT.store(1, Ordering::Release);
    ONLINE_CPUS.store(1, Ordering::Release);
    INITIALIZED.store(true, Ordering::Release);

    Ok(())
}

/// Get current CPU's APIC ID
pub fn get_apic_id() -> u32 {
    if let Some(base) = get_apic_base() {
        unsafe { read_apic(base, apic_regs::APIC_ID) >> 24 }
    } else {
        // Fallback to CPUID
        unsafe {
            let result = core::arch::x86_64::__cpuid(1);
            (result.ebx >> 24) as u32
        }
    }
}

/// Get current CPU ID
pub fn current_cpu() -> u32 {
    let apic_id = get_apic_id();
    let cpu_data = CPU_DATA.lock();

    for i in 0..CPU_COUNT.load(Ordering::Acquire) as usize {
        if cpu_data[i].apic_id == apic_id {
            return cpu_data[i].cpu_id;
        }
    }

    // Default to 0 if not found (shouldn't happen)
    0
}

/// Get number of CPUs
pub fn cpu_count() -> u32 {
    CPU_COUNT.load(Ordering::Acquire)
}

/// Get number of online CPUs
pub fn online_cpus() -> u32 {
    ONLINE_CPUS.load(Ordering::Acquire)
}

/// Check if SMP is initialized
pub fn is_initialized() -> bool {
    INITIALIZED.load(Ordering::Acquire)
}

/// Send Inter-Processor Interrupt
pub fn send_ipi(target_cpu: u32, vector: u8) -> Result<(), &'static str> {
    let cpu_data = CPU_DATA.lock();

    if target_cpu >= CPU_COUNT.load(Ordering::Acquire) {
        return Err("Invalid CPU ID");
    }

    if !cpu_data[target_cpu as usize].online {
        return Err("Target CPU not online");
    }

    let apic_id = cpu_data[target_cpu as usize].apic_id;
    drop(cpu_data);

    if let Some(base) = get_apic_base() {
        unsafe {
            // Set target APIC ID in ICR high
            write_apic(base, apic_regs::APIC_ICR_HIGH, apic_id << 24);
            // Send IPI with vector in ICR low
            write_apic(base, apic_regs::APIC_ICR_LOW, vector as u32);
        }
        Ok(())
    } else {
        Err("APIC not mapped")
    }
}

/// Broadcast IPI to all CPUs except self
pub fn broadcast_ipi(vector: u8) -> Result<(), &'static str> {
    if let Some(base) = get_apic_base() {
        unsafe {
            // Set broadcast mode in ICR high
            write_apic(base, apic_regs::APIC_ICR_HIGH, 0);
            // Send IPI with broadcast flag (bit 19) and all except self (bit 18)
            write_apic(base, apic_regs::APIC_ICR_LOW, (vector as u32) | (3 << 18));
        }
        Ok(())
    } else {
        Err("APIC not mapped")
    }
}

/// Signal End Of Interrupt
pub fn eoi() {
    if let Some(base) = get_apic_base() {
        unsafe {
            write_apic(base, apic_regs::APIC_EOI, 0);
        }
    }
}

/// Get Local APIC base address
fn get_apic_base() -> Option<VirtAddr> {
    let phys = LOCAL_APIC_BASE.load(Ordering::Acquire);
    if phys != 0 {
        // In production, this should be properly mapped by memory manager
        // For now, return identity-mapped address
        Some(VirtAddr::new(phys))
    } else {
        None
    }
}

/// Read APIC register
unsafe fn read_apic(base: VirtAddr, offset: u32) -> u32 {
    let addr = (base.as_u64() + offset as u64) as *const u32;
    addr.read_volatile()
}

/// Write APIC register
unsafe fn write_apic(base: VirtAddr, offset: u32, value: u32) {
    let addr = (base.as_u64() + offset as u64) as *mut u32;
    addr.write_volatile(value);
}

/// Read Model-Specific Register
unsafe fn read_msr(msr: u32) -> u64 {
    let (high, low): (u32, u32);
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        out("eax") low,
        out("edx") high,
        options(nomem, nostack, preserves_flags)
    );
    ((high as u64) << 32) | (low as u64)
}

/// Write Model-Specific Register
unsafe fn write_msr(msr: u32, value: u64) {
    let low = value as u32;
    let high = (value >> 32) as u32;
    core::arch::asm!(
        "wrmsr",
        in("ecx") msr,
        in("eax") low,
        in("edx") high,
        options(nomem, nostack, preserves_flags)
    );
}

// =============================================================================
// SMP Query and Management Functions
// =============================================================================

/// Check if Symmetric Multiprocessing is available and initialized.
///
/// Returns `true` if:
/// - The SMP subsystem has been initialized
/// - More than one CPU has been detected via ACPI/APIC enumeration
///
/// This function is safe to call at any time and will return `false` if
/// SMP initialization has not completed.
pub fn smp_available() -> bool {
    is_initialized() && cpu_count() > 1
}

/// Detect and register additional CPUs from ACPI MADT data.
///
/// This function parses the MADT (Multiple APIC Description Table) to discover
/// all available processors and registers them in the CPU data array.
///
/// # Returns
/// - `Ok(u32)` - The number of CPUs detected (including BSP)
/// - `Err(&'static str)` - Error message if detection fails
pub fn detect_cpus_from_acpi() -> Result<u32, &'static str> {
    // Ensure SMP is initialized first
    if !INITIALIZED.load(Ordering::Acquire) {
        init()?;
    }

    // Try to get MADT information from ACPI
    let madt = crate::acpi::madt().ok_or("MADT not available - cannot detect CPUs")?;

    let mut cpu_data = CPU_DATA.lock();
    let mut detected_count = 0u32;

    for processor in &madt.processors {
        // Check if processor is enabled (bit 0) or can be enabled (bit 1)
        let is_enabled = processor.flags & 0x01 != 0;
        let can_be_enabled = processor.flags & 0x02 != 0;

        if !is_enabled && !can_be_enabled {
            continue; // Skip disabled processors that cannot be enabled
        }

        // Check if we already have this processor registered
        let apic_id = processor.apic_id as u32;
        let mut already_registered = false;

        for i in 0..detected_count as usize {
            if cpu_data[i].apic_id == apic_id {
                already_registered = true;
                break;
            }
        }

        if !already_registered && (detected_count as usize) < MAX_CPUS {
            cpu_data[detected_count as usize] = CpuData {
                cpu_id: detected_count,
                apic_id,
                online: detected_count == 0, // Only BSP is initially online
                idle_time: 0,
                kernel_stack: VirtAddr::zero(),
                tss_selector: 0,
            };
            detected_count += 1;
        }
    }

    // Ensure we have at least the BSP registered
    if detected_count == 0 {
        detected_count = 1;
        cpu_data[0].online = true;
    }

    CPU_COUNT.store(detected_count, Ordering::Release);

    Ok(detected_count)
}

/// Start an Application Processor (AP) by sending INIT-SIPI-SIPI sequence.
///
/// This function brings an AP online using the standard x86 startup sequence:
/// 1. Send INIT IPI to reset the AP
/// 2. Wait 10ms for AP to initialize
/// 3. Send SIPI (Startup IPI) with startup code address
/// 4. Wait 200us then send second SIPI if needed
///
/// # Arguments
/// - `cpu_id` - The CPU ID of the processor to start
/// - `startup_addr` - Physical address of the AP startup code (must be 4K aligned, < 1MB)
///
/// # Returns
/// - `Ok(())` - AP startup sequence initiated successfully
/// - `Err(&'static str)` - Error message if startup fails
pub fn start_ap(cpu_id: u32, startup_addr: u64) -> Result<(), &'static str> {
    if !is_initialized() {
        return Err("SMP not initialized");
    }

    if cpu_id == 0 {
        return Err("Cannot start BSP - it is already running");
    }

    let cpu_count_val = CPU_COUNT.load(Ordering::Acquire);
    if cpu_id >= cpu_count_val {
        return Err("Invalid CPU ID");
    }

    // Validate startup address (must be 4K aligned and in first 1MB)
    if startup_addr & 0xFFF != 0 {
        return Err("Startup address must be 4K aligned");
    }
    if startup_addr >= 0x100000 {
        return Err("Startup address must be below 1MB");
    }

    let cpu_data = CPU_DATA.lock();
    if cpu_data[cpu_id as usize].online {
        return Ok(()); // Already online
    }
    let apic_id = cpu_data[cpu_id as usize].apic_id;
    drop(cpu_data);

    let base = get_apic_base().ok_or("APIC not mapped")?;

    // Calculate startup vector (address / 4096)
    let startup_vector = (startup_addr >> 12) as u32;

    unsafe {
        // Send INIT IPI
        write_apic(base, apic_regs::APIC_ICR_HIGH, apic_id << 24);
        write_apic(base, apic_regs::APIC_ICR_LOW, 0x00004500); // INIT, level, assert

        // Wait for delivery (poll ICR delivery status)
        wait_for_ipi_delivery(base)?;

        // Deassert INIT
        write_apic(base, apic_regs::APIC_ICR_HIGH, apic_id << 24);
        write_apic(base, apic_regs::APIC_ICR_LOW, 0x00008500); // INIT, level, deassert

        wait_for_ipi_delivery(base)?;

        // Wait 10ms (in a real kernel, use a proper timer)
        delay_microseconds(10000);

        // Send first SIPI
        write_apic(base, apic_regs::APIC_ICR_HIGH, apic_id << 24);
        write_apic(base, apic_regs::APIC_ICR_LOW, 0x00004600 | startup_vector); // SIPI

        wait_for_ipi_delivery(base)?;

        // Wait 200us
        delay_microseconds(200);

        // Send second SIPI (some processors need this)
        write_apic(base, apic_regs::APIC_ICR_HIGH, apic_id << 24);
        write_apic(base, apic_regs::APIC_ICR_LOW, 0x00004600 | startup_vector); // SIPI

        wait_for_ipi_delivery(base)?;
    }

    Ok(())
}

/// Mark an AP as online after it has completed initialization.
///
/// This should be called by the AP itself once it has completed its
/// initialization sequence and is ready to receive work.
///
/// # Arguments
/// - `cpu_id` - The CPU ID of the processor that is now online
pub fn mark_cpu_online(cpu_id: u32) {
    let cpu_count_val = CPU_COUNT.load(Ordering::Acquire);
    if cpu_id < cpu_count_val {
        let mut cpu_data = CPU_DATA.lock();
        if !cpu_data[cpu_id as usize].online {
            cpu_data[cpu_id as usize].online = true;
            ONLINE_CPUS.fetch_add(1, Ordering::AcqRel);
        }
    }
}

/// Mark a CPU as offline.
///
/// # Arguments
/// - `cpu_id` - The CPU ID to mark as offline
pub fn mark_cpu_offline(cpu_id: u32) {
    if cpu_id == 0 {
        return; // Cannot take BSP offline
    }

    let cpu_count_val = CPU_COUNT.load(Ordering::Acquire);
    if cpu_id < cpu_count_val {
        let mut cpu_data = CPU_DATA.lock();
        if cpu_data[cpu_id as usize].online {
            cpu_data[cpu_id as usize].online = false;
            ONLINE_CPUS.fetch_sub(1, Ordering::AcqRel);
        }
    }
}

/// Get CPU data for a specific processor.
///
/// # Arguments
/// - `cpu_id` - The CPU ID to query
///
/// # Returns
/// - `Some(CpuData)` - CPU data if the ID is valid
/// - `None` - If the CPU ID is invalid
pub fn get_cpu_data(cpu_id: u32) -> Option<CpuData> {
    let cpu_count_val = CPU_COUNT.load(Ordering::Acquire);
    if cpu_id < cpu_count_val {
        let cpu_data = CPU_DATA.lock();
        Some(cpu_data[cpu_id as usize])
    } else {
        None
    }
}

/// Set the kernel stack for a specific CPU.
///
/// # Arguments
/// - `cpu_id` - The CPU ID to configure
/// - `stack_addr` - Virtual address of the kernel stack top
pub fn set_cpu_kernel_stack(cpu_id: u32, stack_addr: VirtAddr) {
    let cpu_count_val = CPU_COUNT.load(Ordering::Acquire);
    if cpu_id < cpu_count_val {
        let mut cpu_data = CPU_DATA.lock();
        cpu_data[cpu_id as usize].kernel_stack = stack_addr;
    }
}

/// Set the TSS selector for a specific CPU.
///
/// # Arguments
/// - `cpu_id` - The CPU ID to configure
/// - `selector` - The GDT selector for this CPU's TSS
pub fn set_cpu_tss_selector(cpu_id: u32, selector: u16) {
    let cpu_count_val = CPU_COUNT.load(Ordering::Acquire);
    if cpu_id < cpu_count_val {
        let mut cpu_data = CPU_DATA.lock();
        cpu_data[cpu_id as usize].tss_selector = selector;
    }
}

/// Check if a specific CPU is online.
///
/// # Arguments
/// - `cpu_id` - The CPU ID to check
///
/// # Returns
/// `true` if the CPU is online, `false` otherwise
pub fn is_cpu_online(cpu_id: u32) -> bool {
    let cpu_count_val = CPU_COUNT.load(Ordering::Acquire);
    if cpu_id < cpu_count_val {
        let cpu_data = CPU_DATA.lock();
        cpu_data[cpu_id as usize].online
    } else {
        false
    }
}

/// Wait for IPI delivery to complete by polling the ICR delivery status bit.
unsafe fn wait_for_ipi_delivery(base: VirtAddr) -> Result<(), &'static str> {
    // Poll the delivery status bit (bit 12) - 0 means delivered
    for _ in 0..1000 {
        let icr_low = read_apic(base, apic_regs::APIC_ICR_LOW);
        if icr_low & (1 << 12) == 0 {
            return Ok(());
        }
        // Small delay between polls
        for _ in 0..1000 {
            core::hint::spin_loop();
        }
    }
    Err("IPI delivery timeout")
}

/// Simple microsecond delay using busy-waiting.
/// In production, this should use a calibrated timer (PIT, HPET, or APIC timer).
fn delay_microseconds(us: u64) {
    // Approximate delay using spin loops
    // This is very imprecise but works for startup sequences
    // A proper implementation would use RDTSC or a calibrated timer
    for _ in 0..(us * 100) {
        core::hint::spin_loop();
    }
}
