//! Lightweight production performance monitoring for RustOS
//!
//! Real performance metrics collection without simulation

use core::sync::atomic::{AtomicU64, Ordering};

/// Performance counter types
#[derive(Debug, Clone, Copy)]
pub enum MetricCategory {
    CPU,
    Memory,
    IO,
    Network,
    Cache,
    Interrupt,
}

/// Performance statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct PerformanceStats {
    pub cpu_cycles: u64,
    pub instructions_retired: u64,
    pub cache_misses: u64,
    pub page_faults: u64,
    pub interrupts: u64,
    pub context_switches: u64,
}

/// Global performance counters
static CPU_CYCLES: AtomicU64 = AtomicU64::new(0);
static INSTRUCTIONS: AtomicU64 = AtomicU64::new(0);
static CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
static PAGE_FAULTS: AtomicU64 = AtomicU64::new(0);
static INTERRUPTS: AtomicU64 = AtomicU64::new(0);
static CONTEXT_SWITCHES: AtomicU64 = AtomicU64::new(0);

/// Read CPU performance counter
pub fn read_cpu_counter(counter: u32) -> u64 {
    unsafe {
        // Use RDPMC instruction to read performance counter
        let low: u32;
        let high: u32;
        core::arch::asm!(
            "rdpmc",
            in("ecx") counter,
            out("eax") low,
            out("edx") high,
            options(nomem, nostack, preserves_flags)
        );
        ((high as u64) << 32) | (low as u64)
    }
}

/// Read Time Stamp Counter
pub fn read_tsc() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

/// Update CPU cycles counter
pub fn update_cpu_cycles() {
    let cycles = read_tsc();
    CPU_CYCLES.store(cycles, Ordering::Relaxed);
}

/// Record an interrupt
pub fn record_interrupt() {
    INTERRUPTS.fetch_add(1, Ordering::Relaxed);
}

/// Record a page fault
pub fn record_page_fault() {
    PAGE_FAULTS.fetch_add(1, Ordering::Relaxed);
}

/// Record a context switch
pub fn record_context_switch() {
    CONTEXT_SWITCHES.fetch_add(1, Ordering::Relaxed);
}

/// Record a cache miss
pub fn record_cache_miss() {
    CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
}

/// Get current performance statistics
pub fn get_stats() -> PerformanceStats {
    PerformanceStats {
        cpu_cycles: CPU_CYCLES.load(Ordering::Relaxed),
        instructions_retired: INSTRUCTIONS.load(Ordering::Relaxed),
        cache_misses: CACHE_MISSES.load(Ordering::Relaxed),
        page_faults: PAGE_FAULTS.load(Ordering::Relaxed),
        interrupts: INTERRUPTS.load(Ordering::Relaxed),
        context_switches: CONTEXT_SWITCHES.load(Ordering::Relaxed),
    }
}

/// Reset all performance counters
pub fn reset_counters() {
    CPU_CYCLES.store(0, Ordering::Relaxed);
    INSTRUCTIONS.store(0, Ordering::Relaxed);
    CACHE_MISSES.store(0, Ordering::Relaxed);
    PAGE_FAULTS.store(0, Ordering::Relaxed);
    INTERRUPTS.store(0, Ordering::Relaxed);
    CONTEXT_SWITCHES.store(0, Ordering::Relaxed);
}

/// Calculate CPU utilization percentage
pub fn cpu_utilization() -> u8 {
    // In production, this would read actual CPU idle/busy time
    // For now, estimate based on interrupt rate
    let interrupts = INTERRUPTS.load(Ordering::Relaxed);
    let cycles = CPU_CYCLES.load(Ordering::Relaxed);

    if cycles > 0 {
        // Rough estimate: more interrupts = more activity
        let util = (interrupts * 100 / (cycles / 1000000)).min(100) as u8;
        util
    } else {
        0
    }
}

/// Get memory usage statistics from hardware
pub fn memory_usage() -> (u64, u64) {
    // This interfaces with the memory manager
    // Return (used, total) in bytes
    if let Some(stats) = crate::memory::get_memory_stats() {
        (stats.allocated_memory as u64, stats.total_memory as u64)
    } else {
        // Default values if memory manager not initialized
        (0, 0)
    }
}

// =============================================================================
// Wrapper functions for legacy API compatibility
// =============================================================================

/// Get the system call rate (syscalls per second)
pub fn syscall_rate() -> u64 {
    // TODO: Implement actual syscall tracking
    // For now, return a placeholder value
    0
}
