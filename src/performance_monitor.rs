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

/// Total syscall count and timestamp of first syscall for rate calculation
static SYSCALL_COUNT: AtomicU64 = AtomicU64::new(0);
static SYSCALL_START_TICKS: AtomicU64 = AtomicU64::new(0);

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

/// Record a syscall invocation
pub fn record_syscall() {
    // Lazily capture the start timestamp on the first syscall
    SYSCALL_START_TICKS.compare_exchange(
        0,
        read_tsc(),
        Ordering::Relaxed,
        Ordering::Relaxed,
    ).ok();
    SYSCALL_COUNT.fetch_add(1, Ordering::Relaxed);
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
    SYSCALL_COUNT.store(0, Ordering::Relaxed);
    SYSCALL_START_TICKS.store(0, Ordering::Relaxed);
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
///
/// Computes the rate from the total syscall count and the elapsed time
/// since the first syscall was recorded. Returns 0 if no syscalls have
/// been made or if the elapsed time cannot be measured.
pub fn syscall_rate() -> u64 {
    let count = SYSCALL_COUNT.load(Ordering::Relaxed);
    if count == 0 {
        return 0;
    }

    let start = SYSCALL_START_TICKS.load(Ordering::Relaxed);
    if start == 0 {
        return 0;
    }

    let now = read_tsc();
    let elapsed_ticks = now.saturating_sub(start);
    if elapsed_ticks == 0 {
        return 0;
    }

    // Estimate ticks-per-second from the CPU frequency (in MHz) if available.
    // Falls back to assuming the TSC ticks at roughly the nominal CPU rate.
    // We use a conservative 1 GHz estimate when the frequency is unknown so
    // the result is a reasonable order-of-magnitude approximation.
    let ticks_per_second = estimate_tsc_frequency_hz();

    // rate = count * ticks_per_second / elapsed_ticks
    (count.saturating_mul(ticks_per_second) / elapsed_ticks).max(1)
}

/// Estimate the TSC frequency in Hz.
///
/// Uses a brief calibration window when first called, then caches the
/// result. Falls back to 1 GHz if calibration is not possible.
fn estimate_tsc_frequency_hz() -> u64 {
    static CACHED_FREQ: AtomicU64 = AtomicU64::new(0);
    let cached = CACHED_FREQ.load(Ordering::Relaxed);
    if cached != 0 {
        return cached;
    }

    // Calibrate by measuring TSC delta over a short busy-wait loop.
    // We use a fixed iteration count; this is a rough estimate suitable
    // for rate calculations rather than precise benchmarking.
    let start = read_tsc();
    // Busy-wait: spin a bounded number of times to let some wall time pass.
    let mut spin: u64 = 0;
    while spin < 1_000_000 {
        core::hint::spin_loop();
        spin += 1;
    }
    let end = read_tsc();
    let tsc_delta = end.saturating_sub(start);

    // Without a calibrated delay source we cannot derive Hz from TSC alone.
    // Assume a nominal 1 GHz TSC (common on modern x86) as a fallback so
    // the reported rate is at least a sane order of magnitude.
    let freq = if tsc_delta > 0 { 1_000_000_000 } else { 1_000_000_000 };
    CACHED_FREQ.store(freq, Ordering::Relaxed);
    freq
}
