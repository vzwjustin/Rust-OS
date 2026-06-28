//! Production time and timer subsystem for RustOS
//!
//! Provides real timer functionality using x86_64 hardware timers including
//! HPET, APIC timer, and PIT with proper hardware abstraction.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::instructions::port::Port;
use x86_64::VirtAddr;

/// PIT (Programmable Interval Timer) frequency
const PIT_FREQUENCY: u32 = 1193182;
/// Target timer frequency in Hz
const TIMER_FREQUENCY: u32 = 1000; // 1kHz for better precision
/// PIT divisor for desired frequency
const PIT_DIVISOR: u16 = (PIT_FREQUENCY / TIMER_FREQUENCY) as u16;

/// HPET register offsets
#[repr(u64)]
#[derive(Debug, Clone, Copy)]
pub enum HpetRegister {
    GeneralCapabilities = 0x00,
    GeneralConfiguration = 0x10,
    GeneralInterruptStatus = 0x20,
    MainCounterValue = 0xF0,
    Timer0Configuration = 0x100,
    Timer0Comparator = 0x108,
    Timer1Configuration = 0x120,
    Timer1Comparator = 0x128,
    Timer2Configuration = 0x140,
    Timer2Comparator = 0x148,
}

/// Timer types supported by the system
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TimerType {
    Pit,
    ApicTimer,
    Hpet,
}

/// Hardware timer abstraction trait
pub trait HardwareTimer {
    /// Initialize the timer
    fn init(&mut self) -> Result<(), &'static str>;

    /// Set timer frequency in Hz
    fn set_frequency(&mut self, frequency: u32) -> Result<(), &'static str>;

    /// Get timer frequency in Hz
    fn get_frequency(&self) -> u32;

    /// Enable the timer
    fn enable(&mut self);

    /// Disable the timer
    fn disable(&mut self);

    /// Get timer type
    fn timer_type(&self) -> TimerType;

    /// Read current counter value
    fn read_counter(&self) -> u64;
}

/// Global tick counter
static TICKS: AtomicU64 = AtomicU64::new(0);
/// TSC frequency in Hz (calibrated at boot)
static TSC_FREQUENCY: AtomicU64 = AtomicU64::new(0);
/// Boot TSC value
static BOOT_TSC: AtomicU64 = AtomicU64::new(0);
/// System initialization timestamp
static BOOT_TIME: AtomicU64 = AtomicU64::new(0);
/// Timer system initialized flag
static TIMER_INITIALIZED: AtomicBool = AtomicBool::new(false);

/// PIT (Programmable Interval Timer) implementation
pub struct PitTimer {
    frequency: u32,
    enabled: bool,
}

impl PitTimer {
    pub fn new() -> Self {
        Self {
            frequency: TIMER_FREQUENCY,
            enabled: false,
        }
    }
}

impl HardwareTimer for PitTimer {
    fn init(&mut self) -> Result<(), &'static str> {
        // Configure PIT channel 0
        unsafe {
            let mut cmd = Port::<u8>::new(0x43);
            let mut data = Port::<u8>::new(0x40);

            // Channel 0, lobyte/hibyte, rate generator
            cmd.write(0x36);

            // Write frequency divisor
            let divisor = (PIT_FREQUENCY / self.frequency) as u16;
            data.write((divisor & 0xFF) as u8);
            data.write((divisor >> 8) as u8);
        }

        self.enabled = true;
        Ok(())
    }

    fn set_frequency(&mut self, frequency: u32) -> Result<(), &'static str> {
        if frequency == 0 || frequency > PIT_FREQUENCY {
            return Err("Invalid PIT frequency");
        }

        self.frequency = frequency;

        if self.enabled {
            self.init()?;
        }

        Ok(())
    }

    fn get_frequency(&self) -> u32 {
        self.frequency
    }

    fn enable(&mut self) {
        self.enabled = true;
    }

    fn disable(&mut self) {
        self.enabled = false;
        // Disable PIT by setting maximum divisor
        unsafe {
            let mut cmd = Port::<u8>::new(0x43);
            let mut data = Port::<u8>::new(0x40);

            cmd.write(0x36);
            data.write(0xFF);
            data.write(0xFF);
        }
    }

    fn timer_type(&self) -> TimerType {
        TimerType::Pit
    }

    fn read_counter(&self) -> u64 {
        unsafe {
            let mut cmd = Port::<u8>::new(0x43);
            let mut data = Port::<u8>::new(0x40);

            // Latch counter value
            cmd.write(0x00);

            // Read counter (low byte first, then high byte)
            let low = data.read() as u16;
            let high = data.read() as u16;

            ((high << 8) | low) as u64
        }
    }
}

/// APIC Timer implementation
pub struct ApicTimer {
    frequency: u32,
    enabled: bool,
    base_address: Option<VirtAddr>,
}

impl ApicTimer {
    pub fn new() -> Self {
        Self {
            frequency: TIMER_FREQUENCY,
            enabled: false,
            base_address: None,
        }
    }

    pub fn set_base_address(&mut self, base: VirtAddr) {
        self.base_address = Some(base);
    }

    /// Calibrate APIC timer count for desired frequency using PIT as reference
    fn calibrate_timer_count(&self) -> Result<u32, &'static str> {
        if self.base_address.is_none() {
            return Err("APIC base address not set");
        }

        // Use PIT as reference timer for calibration
        let calibration_ms = 10; // 10ms calibration period

        // Set up PIT for calibration (channel 2, one-shot mode)
        unsafe {
            let mut cmd = Port::<u8>::new(0x43);
            let mut data = Port::<u8>::new(0x42);

            // Channel 2, lobyte/hibyte, one-shot
            cmd.write(0xB0);

            // Set PIT for 10ms (approximately 11932 ticks at 1.193182 MHz)
            let pit_ticks = (PIT_FREQUENCY / 100) as u16; // 10ms
            data.write((pit_ticks & 0xFF) as u8);
            data.write((pit_ticks >> 8) as u8);
        }

        // Set APIC timer to maximum count for measurement
        self.write_register(0x380, 0xFFFFFFFF);

        // Start PIT channel 2
        unsafe {
            let mut port61 = Port::<u8>::new(0x61);
            let val = port61.read();
            port61.write(val | 0x01); // Enable PIT channel 2
        }

        // Wait for PIT to complete (busy wait)
        let start_apic_count = self.read_register(0x390);

        // Wait for calibration period using TSC if available
        if let Some(tsc_freq) = get_tsc_frequency() {
            let start_tsc = read_tsc();
            let target_tsc = start_tsc + (tsc_freq * calibration_ms as u64) / 1000;
            while read_tsc() < target_tsc {
                core::hint::spin_loop();
            }
        } else {
            // Fallback: busy wait with PIT status check
            unsafe {
                let mut port61 = Port::<u8>::new(0x61);
                while (port61.read() & 0x20) == 0 {
                    core::hint::spin_loop();
                }
            }
        }

        let end_apic_count = self.read_register(0x390);

        // Calculate APIC timer frequency
        let apic_ticks = start_apic_count.saturating_sub(end_apic_count) as u64;
        let apic_freq = (apic_ticks * 1000) / calibration_ms as u64;

        // Calculate initial count for desired frequency
        if apic_freq > 0 {
            let initial_count = (apic_freq / self.frequency as u64) as u32;
            if initial_count > 0 {
                Ok(initial_count)
            } else {
                Ok(0x1000) // Minimum reasonable value
            }
        } else {
            Err("APIC timer calibration failed")
        }
    }

    fn write_register(&self, offset: u32, value: u32) {
        if let Some(base) = self.base_address {
            unsafe {
                let addr = base.as_u64() + offset as u64;
                core::ptr::write_volatile(addr as *mut u32, value);
            }
        }
    }

    fn read_register(&self, offset: u32) -> u32 {
        if let Some(base) = self.base_address {
            unsafe {
                let addr = base.as_u64() + offset as u64;
                core::ptr::read_volatile(addr as *const u32)
            }
        } else {
            0
        }
    }
}

impl HardwareTimer for ApicTimer {
    fn init(&mut self) -> Result<(), &'static str> {
        if self.base_address.is_none() {
            return Err("APIC base address not set");
        }

        // Configure APIC timer
        // Set divide configuration register (divide by 16)
        self.write_register(0x3E0, 0x03);

        // Calibrate timer for desired frequency
        let initial_count = self.calibrate_timer_count()?;
        self.write_register(0x380, initial_count);

        // Configure timer LVT (Local Vector Table)
        // Vector 32 (timer interrupt), periodic mode
        self.write_register(0x320, 32 | (1 << 17));

        self.enabled = true;
        Ok(())
    }

    fn set_frequency(&mut self, frequency: u32) -> Result<(), &'static str> {
        self.frequency = frequency;

        if self.enabled && self.base_address.is_some() {
            // Recalibrate timer with new frequency
            let initial_count = self.calibrate_timer_count()?;
            self.write_register(0x380, initial_count);
        }

        Ok(())
    }

    fn get_frequency(&self) -> u32 {
        self.frequency
    }

    fn enable(&mut self) {
        self.enabled = true;
        if self.base_address.is_some() {
            // Enable timer by setting calibrated initial count
            if let Ok(initial_count) = self.calibrate_timer_count() {
                self.write_register(0x380, initial_count);
            } else {
                // Fallback to reasonable default if calibration fails
                self.write_register(0x380, 0x10000);
            }
        }
    }

    fn disable(&mut self) {
        self.enabled = false;
        if self.base_address.is_some() {
            // Disable timer by setting initial count to 0
            self.write_register(0x380, 0);
        }
    }

    fn timer_type(&self) -> TimerType {
        TimerType::ApicTimer
    }

    fn read_counter(&self) -> u64 {
        self.read_register(0x390) as u64
    }
}

/// HPET (High Precision Event Timer) implementation
pub struct HpetTimer {
    frequency: u32,
    enabled: bool,
    base_address: Option<VirtAddr>,
    pub(crate) period_fs: u64, // Period in femtoseconds
}

impl HpetTimer {
    pub fn new() -> Self {
        Self {
            frequency: TIMER_FREQUENCY,
            enabled: false,
            base_address: None,
            period_fs: 0,
        }
    }

    pub fn set_base_address(&mut self, base: VirtAddr) {
        self.base_address = Some(base);
    }

    fn write_register(&self, offset: HpetRegister, value: u64) {
        if let Some(base) = self.base_address {
            unsafe {
                let addr = base.as_u64() + offset as u64;
                core::ptr::write_volatile(addr as *mut u64, value);
            }
        }
    }

    fn read_register(&self, offset: HpetRegister) -> u64 {
        if let Some(base) = self.base_address {
            unsafe {
                let addr = base.as_u64() + offset as u64;
                core::ptr::read_volatile(addr as *const u64)
            }
        } else {
            0
        }
    }
}

impl HardwareTimer for HpetTimer {
    fn init(&mut self) -> Result<(), &'static str> {
        if self.base_address.is_none() {
            return Err("HPET base address not set");
        }

        // Read capabilities to get period and validate HPET
        let capabilities = self.read_register(HpetRegister::GeneralCapabilities);
        self.period_fs = capabilities >> 32;

        if self.period_fs == 0 || self.period_fs > 100_000_000 {
            return Err("Invalid HPET period - hardware may not be functional");
        }

        // Check if HPET supports the required features
        let num_timers = ((capabilities >> 8) & 0x1F) + 1;
        if num_timers == 0 {
            return Err("HPET has no timers available");
        }

        // Disable HPET before configuration
        let config = self.read_register(HpetRegister::GeneralConfiguration);
        self.write_register(HpetRegister::GeneralConfiguration, config & !1);

        // Reset main counter
        self.write_register(HpetRegister::MainCounterValue, 0);

        // Configure timer 0 for periodic interrupts
        let timer_config = (1 << 2) | (1 << 3) | (1 << 6); // Periodic, interrupt enable, 64-bit
        self.write_register(HpetRegister::Timer0Configuration, timer_config);

        // Calculate and set comparator value for desired frequency
        let period_ns = self.period_fs / 1_000_000; // Convert fs to ns
        if period_ns == 0 {
            return Err("HPET period too small for reliable operation");
        }

        let comparator_value = (1_000_000_000 / self.frequency as u64) / period_ns;
        if comparator_value == 0 {
            return Err("HPET frequency too high for hardware capabilities");
        }

        self.write_register(HpetRegister::Timer0Comparator, comparator_value);

        // Enable HPET
        self.write_register(HpetRegister::GeneralConfiguration, config | 1);

        self.enabled = true;
        Ok(())
    }

    fn set_frequency(&mut self, frequency: u32) -> Result<(), &'static str> {
        self.frequency = frequency;

        if self.enabled && self.base_address.is_some() && self.period_fs > 0 {
            let period_ns = self.period_fs / 1_000_000;
            let comparator_value = (1_000_000_000 / frequency as u64) / period_ns;
            self.write_register(HpetRegister::Timer0Comparator, comparator_value);
        }

        Ok(())
    }

    fn get_frequency(&self) -> u32 {
        self.frequency
    }

    fn enable(&mut self) {
        self.enabled = true;
        if self.base_address.is_some() {
            let config = self.read_register(HpetRegister::GeneralConfiguration);
            self.write_register(HpetRegister::GeneralConfiguration, config | 1);
        }
    }

    fn disable(&mut self) {
        self.enabled = false;
        if self.base_address.is_some() {
            let config = self.read_register(HpetRegister::GeneralConfiguration);
            self.write_register(HpetRegister::GeneralConfiguration, config & !1);
        }
    }

    fn timer_type(&self) -> TimerType {
        TimerType::Hpet
    }

    fn read_counter(&self) -> u64 {
        self.read_register(HpetRegister::MainCounterValue)
    }
}

lazy_static! {
    static ref TIMER_MANAGER: Mutex<TimerManager> = Mutex::new(TimerManager::new());
}

/// Timer management system
pub struct TimerManager {
    active_timer: Option<TimerType>,
    pit_timer: PitTimer,
    apic_timer: ApicTimer,
    hpet_timer: HpetTimer,
}

impl TimerManager {
    pub fn new() -> Self {
        Self {
            active_timer: None,
            pit_timer: PitTimer::new(),
            apic_timer: ApicTimer::new(),
            hpet_timer: HpetTimer::new(),
        }
    }

    pub fn init(&mut self) -> Result<(), &'static str> {
        // Check if ACPI is available for advanced timer detection
        let acpi_available = crate::acpi::is_initialized();
        if !acpi_available {
            crate::serial_println!("ACPI not available, falling back to PIT timer");
        } else {
            // Try to parse ACPI tables if not already done
            if let Err(_) = crate::acpi::init_acpi_tables() {
                // Continue without ACPI - we can still use PIT
            }
        }

        // Try to initialize timers in order of preference: HPET > APIC > PIT
        let mut initialization_errors = Vec::new();

        // Try HPET first (highest precision)
        match self.detect_hpet() {
            Ok(hpet_base) => {
                self.hpet_timer.set_base_address(hpet_base);
                match self.hpet_timer.init() {
                    Ok(()) => {
                        self.active_timer = Some(TimerType::Hpet);
                        return Ok(());
                    }
                    Err(e) => initialization_errors.push(("HPET", e)),
                }
            }
            Err(e) => initialization_errors.push(("HPET detection", e)),
        }

        // Try APIC timer (good precision, widely available)
        match self.detect_apic() {
            Ok(apic_base) => {
                self.apic_timer.set_base_address(apic_base);
                match self.apic_timer.init() {
                    Ok(()) => {
                        self.active_timer = Some(TimerType::ApicTimer);
                        return Ok(());
                    }
                    Err(e) => initialization_errors.push(("APIC Timer", e)),
                }
            }
            Err(e) => initialization_errors.push(("APIC detection", e)),
        }

        // Fall back to PIT (always available on x86)
        match self.pit_timer.init() {
            Ok(()) => {
                self.active_timer = Some(TimerType::Pit);

                // Log initialization warnings if higher precision timers failed
                for (timer_name, error) in initialization_errors {
                    crate::serial_println!(
                        "Warning: {} initialization failed: {}",
                        timer_name,
                        error
                    );
                }

                Ok(())
            }
            Err(_e) => {
                // If even PIT fails, we have a serious problem
                Err("All timer initialization failed - system cannot continue")
            }
        }
    }

    fn detect_hpet(&self) -> Result<VirtAddr, &'static str> {
        // Try to get cached HPET address from ACPI first
        if let Some(acpi_info) = crate::acpi::acpi_info() {
            if let Some(hpet_info) = acpi_info.hpet {
                let physical_offset = acpi_info
                    .physical_memory_offset
                    .ok_or("Physical memory offset not available")?;

                // Validate HPET base address
                if hpet_info.base_address == 0 {
                    return Err("Invalid HPET base address");
                }

                let virt = physical_offset + hpet_info.base_address;
                // HPET MMIO isn't mapped by the bootloader; map it before init
                // writes the config register (else a page fault halts boot).
                crate::memory::map_mmio_region(virt as usize, 0x1000)
                    .map_err(|_| "Failed to map HPET MMIO")?;
                return Ok(VirtAddr::new(virt));
            }
        }

        // Try to parse HPET from ACPI if not cached
        match crate::acpi::parse_hpet() {
            Ok(hpet_info) => {
                if let Some(acpi_info) = crate::acpi::acpi_info() {
                    let physical_offset = acpi_info
                        .physical_memory_offset
                        .ok_or("Physical memory offset not available")?;

                    if hpet_info.base_address == 0 {
                        return Err("Invalid HPET base address from ACPI");
                    }

                    let virt = physical_offset + hpet_info.base_address;
                    crate::memory::map_mmio_region(virt as usize, 0x1000)
                        .map_err(|_| "Failed to map HPET MMIO")?;
                    return Ok(VirtAddr::new(virt));
                }
                Err("ACPI info not available")
            }
            Err(e) => Err(e),
        }
    }

    fn detect_apic(&self) -> Result<VirtAddr, &'static str> {
        // Get APIC base address from ACPI MADT
        if let Some(madt) = crate::acpi::madt() {
            if let Some(acpi_info) = crate::acpi::acpi_info() {
                let physical_offset = acpi_info
                    .physical_memory_offset
                    .ok_or("Physical memory offset not available")?;
                return Ok(VirtAddr::new(
                    physical_offset + madt.local_apic_address as u64,
                ));
            }
        }
        Err("APIC not found")
    }

    pub fn get_active_timer(&mut self) -> Option<&mut dyn HardwareTimer> {
        match self.active_timer? {
            TimerType::Pit => Some(&mut self.pit_timer),
            TimerType::ApicTimer => Some(&mut self.apic_timer),
            TimerType::Hpet => Some(&mut self.hpet_timer),
        }
    }

    pub fn get_active_timer_type(&self) -> Option<TimerType> {
        self.active_timer
    }
}

/// Initialize the timer subsystem
pub fn init() -> Result<(), &'static str> {
    // Initialize timer hardware
    let mut timer_manager = TIMER_MANAGER.lock();
    timer_manager.init()?;

    // Record boot TSC before calibration
    BOOT_TSC.store(read_tsc(), Ordering::Relaxed);

    // Calibrate TSC using the active timer as reference
    calibrate_tsc();

    // Verify TSC calibration was successful
    let tsc_freq = TSC_FREQUENCY.load(Ordering::Relaxed);
    if tsc_freq == 0 {
        crate::serial_println!("Warning: TSC calibration failed - timing may be less accurate");
    }

    // Initialize system time (will be set by RTC or network time later)
    BOOT_TIME.store(0, Ordering::Relaxed);

    // Synchronize timer system with hardware
    synchronize_timer_system(&mut timer_manager)?;

    TIMER_INITIALIZED.store(true, Ordering::Relaxed);

    Ok(())
}

/// Check if the timer system has been initialized
/// This is safe to call from interrupt context
pub fn is_timer_initialized() -> bool {
    TIMER_INITIALIZED.load(Ordering::Relaxed)
}

/// Synchronize timer system with hardware timers
fn synchronize_timer_system(timer_manager: &mut TimerManager) -> Result<(), &'static str> {
    if let Some(active_timer) = timer_manager.get_active_timer() {
        // Set the timer frequency to our desired rate
        active_timer.set_frequency(TIMER_FREQUENCY)?;

        // Ensure timer is enabled
        active_timer.enable();

        // Verify timer is working by checking counter changes
        let initial_counter = active_timer.read_counter();

        // Wait a brief moment
        for _ in 0..1000 {
            core::hint::spin_loop();
        }

        let final_counter = active_timer.read_counter();

        // For PIT and APIC, counter decreases; for HPET, it increases
        match active_timer.timer_type() {
            TimerType::Pit | TimerType::ApicTimer => {
                if initial_counter <= final_counter {
                    return Err("Timer hardware not functioning - counter not decreasing");
                }
            }
            TimerType::Hpet => {
                if final_counter <= initial_counter {
                    return Err("HPET hardware not functioning - counter not increasing");
                }
            }
        }

        Ok(())
    } else {
        Err("No active timer available for synchronization")
    }
}

/// Timer interrupt handler - called by interrupt system
pub fn timer_tick() {
    // Increment tick counter - always do this for basic uptime tracking
    TICKS.fetch_add(1, Ordering::Relaxed);

    // Only do advanced processing if timer system is initialized
    // This prevents crashes during early boot when other subsystems aren't ready
    if !TIMER_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    // Update timer statistics and handle timer-specific operations
    update_timer_statistics();

    // Call process timer integration if available
    if crate::process::integration::timer_interrupt_handler().is_some() {
        crate::user_sched::note_schedule_tick();
    }

    // Fire any expired ITIMER_REAL alarms (delivers SIGALRM)
    crate::linux_compat::process_ops::fire_expired_alarms();

    // Process any scheduled software timers
    process_scheduled_timers();
}

/// Update timer statistics and handle timer-specific operations
fn update_timer_statistics() {
    let timer_manager = TIMER_MANAGER.lock();

    if let Some(timer_type) = timer_manager.get_active_timer_type() {
        match timer_type {
            TimerType::Hpet => {
                // HPET provides high-resolution timing - no additional processing needed
            }
            TimerType::ApicTimer => {
                // APIC timer may need periodic recalibration for accuracy
                static APIC_RECALIBRATION_COUNTER: AtomicU64 = AtomicU64::new(0);
                let counter = APIC_RECALIBRATION_COUNTER.fetch_add(1, Ordering::Relaxed);

                // Recalibrate every 60 seconds (60000 ticks at 1kHz)
                if counter % 60000 == 0 {
                    // Schedule recalibration (can't do it in interrupt context)
                    // This would be handled by a kernel thread in a full implementation
                }
            }
            TimerType::Pit => {
                // PIT is stable but lower resolution - no special handling needed
            }
        }
    }
}

/// Get system uptime in milliseconds
pub fn uptime_ms() -> u64 {
    if !TIMER_INITIALIZED.load(Ordering::Relaxed) {
        return 0;
    }

    let ticks = TICKS.load(Ordering::Relaxed);
    let timer_manager = TIMER_MANAGER.lock();

    if let Some(timer) = timer_manager.get_active_timer_type() {
        match timer {
            TimerType::Pit => (ticks * 1000) / TIMER_FREQUENCY as u64,
            TimerType::ApicTimer | TimerType::Hpet => {
                // Use high-precision timing for APIC and HPET
                if let Some(tsc_freq) = get_tsc_frequency() {
                    let current_tsc = read_tsc();
                    let boot_tsc = BOOT_TSC.load(Ordering::Relaxed);
                    if current_tsc > boot_tsc {
                        // u128 scaling avoids u64 overflow (delta * 1000 wraps within seconds).
                        // tsc_freq is guaranteed > 0 here (get_tsc_frequency returns None for 0).
                        (((current_tsc - boot_tsc) as u128 * 1000) / tsc_freq as u128) as u64
                    } else {
                        (ticks * 1000) / TIMER_FREQUENCY as u64
                    }
                } else {
                    (ticks * 1000) / TIMER_FREQUENCY as u64
                }
            }
        }
    } else {
        (ticks * 1000) / TIMER_FREQUENCY as u64
    }
}

/// Get system uptime in microseconds
pub fn uptime_us() -> u64 {
    if !TIMER_INITIALIZED.load(Ordering::Relaxed) {
        return 0;
    }

    if let Some(tsc_freq) = get_tsc_frequency() {
        // Use high-precision TSC if calibrated
        let current_tsc = read_tsc();
        let boot_tsc = BOOT_TSC.load(Ordering::Relaxed);

        if current_tsc > boot_tsc {
            // u128 scaling avoids u64 overflow; tsc_freq is guaranteed > 0 here.
            (((current_tsc - boot_tsc) as u128 * 1_000_000) / tsc_freq as u128) as u64
        } else {
            // Fallback to tick-based timing
            uptime_ms() * 1000
        }
    } else {
        uptime_ms() * 1000
    }
}

/// Get system uptime in nanoseconds (highest precision)
pub fn uptime_ns() -> u64 {
    if !TIMER_INITIALIZED.load(Ordering::Relaxed) {
        return 0;
    }

    if let Some(tsc_freq) = get_tsc_frequency() {
        let current_tsc = read_tsc();
        let boot_tsc = BOOT_TSC.load(Ordering::Relaxed);

        if current_tsc > boot_tsc {
            // u128 scaling avoids u64 overflow; tsc_freq is guaranteed > 0 here.
            (((current_tsc - boot_tsc) as u128 * 1_000_000_000) / tsc_freq as u128) as u64
        } else {
            uptime_us() * 1000
        }
    } else {
        uptime_us() * 1000
    }
}

/// Get TSC frequency if available
pub fn get_tsc_frequency() -> Option<u64> {
    let freq = TSC_FREQUENCY.load(Ordering::Relaxed);
    if freq > 0 {
        Some(freq)
    } else {
        None
    }
}

/// Read the Time Stamp Counter
pub fn read_tsc() -> u64 {
    unsafe { core::arch::x86_64::_rdtsc() }
}

/// Calibrate TSC frequency using hardware timers
fn calibrate_tsc() {
    // Use PIT as the most reliable reference for TSC calibration
    calibrate_tsc_with_pit();

    // If PIT calibration failed, try HPET if available
    if TSC_FREQUENCY.load(Ordering::Relaxed) == 0 {
        if let Ok(_) = calibrate_tsc_with_hpet() {
            return;
        }
    }

    // Final fallback: use APIC timer if available
    if TSC_FREQUENCY.load(Ordering::Relaxed) == 0 {
        calibrate_tsc_with_apic();
    }
}

/// Calibrate TSC using PIT as reference (most reliable method)
fn calibrate_tsc_with_pit() {
    let calibration_ms = 50; // 50ms calibration period for good accuracy

    // Configure PIT channel 2 for one-shot mode
    unsafe {
        let mut cmd = Port::<u8>::new(0x43);
        let mut data = Port::<u8>::new(0x42);

        // Channel 2, lobyte/hibyte, one-shot mode
        cmd.write(0xB0);

        // Calculate PIT ticks for calibration period
        let pit_ticks = ((PIT_FREQUENCY as u64 * calibration_ms as u64) / 1000) as u16;
        data.write((pit_ticks & 0xFF) as u8);
        data.write((pit_ticks >> 8) as u8);

        // Enable PIT channel 2
        let mut port61 = Port::<u8>::new(0x61);
        let val = port61.read();
        port61.write((val & 0xFD) | 0x01);
    }

    // Measure TSC during PIT countdown
    let start_tsc = read_tsc();

    // Wait for PIT to complete
    unsafe {
        let mut port61 = Port::<u8>::new(0x61);
        while (port61.read() & 0x20) == 0 {
            core::hint::spin_loop();
        }
    }

    let end_tsc = read_tsc();

    // Calculate TSC frequency
    let tsc_delta = end_tsc.saturating_sub(start_tsc);
    if tsc_delta > 0 {
        let freq = (tsc_delta * 1000) / calibration_ms as u64;
        TSC_FREQUENCY.store(freq, Ordering::Relaxed);
    }
}

/// Calibrate TSC using HPET as reference
fn calibrate_tsc_with_hpet() -> Result<(), &'static str> {
    let timer_manager = TIMER_MANAGER.lock();
    if let Some(timer) = timer_manager.get_active_timer_type() {
        if timer == TimerType::Hpet {
            // Use HPET main counter for calibration
            let calibration_ms = 50;

            // Read HPET period from the timer itself
            if timer_manager.hpet_timer.period_fs > 0 {
                let start_tsc = read_tsc();
                let start_hpet = timer_manager.hpet_timer.read_counter();

                // Wait for calibration period
                let period_fs = timer_manager.hpet_timer.period_fs;
                let target_hpet_ticks = (calibration_ms as u64 * 1_000_000_000_000) / period_fs;

                while timer_manager
                    .hpet_timer
                    .read_counter()
                    .saturating_sub(start_hpet)
                    < target_hpet_ticks
                {
                    core::hint::spin_loop();
                }

                let end_tsc = read_tsc();
                let tsc_delta = end_tsc.saturating_sub(start_tsc);

                if tsc_delta > 0 {
                    let freq = (tsc_delta * 1000) / calibration_ms as u64;
                    TSC_FREQUENCY.store(freq, Ordering::Relaxed);
                    return Ok(());
                }
            }
        }
    }
    Err("HPET not available for TSC calibration")
}

/// Calibrate TSC using APIC timer as reference (least reliable)
fn calibrate_tsc_with_apic() {
    // This is a fallback method - less accurate than PIT or HPET
    let calibration_ms = 100; // Longer period for better accuracy

    let start_tsc = read_tsc();
    let start_time = uptime_ms();

    // Wait for calibration period
    let target_time = start_time + calibration_ms;
    while uptime_ms() < target_time {
        core::hint::spin_loop();
    }

    let end_tsc = read_tsc();
    let end_time = uptime_ms();

    let tsc_delta = end_tsc.saturating_sub(start_tsc);
    let time_delta_ms = end_time.saturating_sub(start_time);

    if time_delta_ms > 0 && tsc_delta > 0 {
        let freq = (tsc_delta * 1000) / time_delta_ms;
        TSC_FREQUENCY.store(freq, Ordering::Relaxed);
    }
}

/// Recalibrate TSC frequency (can be called periodically for accuracy)
pub fn recalibrate_tsc() -> Result<u64, &'static str> {
    if !TIMER_INITIALIZED.load(Ordering::Relaxed) {
        return Err("Timer system not initialized");
    }

    calibrate_tsc();
    let freq = TSC_FREQUENCY.load(Ordering::Relaxed);

    if freq > 0 {
        Ok(freq)
    } else {
        Err("TSC calibration failed")
    }
}

/// Sleep for specified milliseconds (busy wait)
pub fn sleep_ms(ms: u64) {
    if ms == 0 {
        return;
    }

    let start = uptime_ms();
    let target = start + ms;

    while uptime_ms() < target {
        // Use pause instruction to be more CPU-friendly
        core::hint::spin_loop();
    }
}

/// Sleep for specified microseconds (busy wait)
pub fn sleep_us(us: u64) {
    if us == 0 {
        return;
    }

    let start = uptime_us();
    let target = start + us;

    while uptime_us() < target {
        core::hint::spin_loop();
    }
}

/// Get current system time in Unix timestamp format (seconds since epoch)
pub fn system_time() -> u64 {
    let boot_time = BOOT_TIME.load(Ordering::Relaxed);
    let uptime_sec = uptime_ms() / 1000;
    boot_time + uptime_sec
}

/// Get current system time in milliseconds since epoch
pub fn get_system_time_ms() -> u64 {
    let boot_time = BOOT_TIME.load(Ordering::Relaxed);
    let uptime_ms = uptime_ms();
    (boot_time * 1000) + uptime_ms
}

/// Set system time (Unix timestamp in seconds)
pub fn set_system_time(timestamp: u64) {
    let uptime_sec = uptime_ms() / 1000;
    let boot_time = timestamp.saturating_sub(uptime_sec);
    BOOT_TIME.store(boot_time, Ordering::Relaxed);
}

/// Initialize system time from RTC (Real-Time Clock)
pub fn init_system_time_from_rtc() -> Result<(), &'static str> {
    // Read time from CMOS RTC
    let rtc_time = read_rtc_time()?;
    set_system_time(rtc_time);
    Ok(())
}

/// Read time from CMOS RTC
fn read_rtc_time() -> Result<u64, &'static str> {
    use x86_64::instructions::port::Port;

    unsafe {
        let mut cmos_address = Port::<u8>::new(0x70);
        let mut cmos_data = Port::<u8>::new(0x71);

        // Wait for RTC update to complete
        loop {
            cmos_address.write(0x0A);
            if (cmos_data.read() & 0x80) == 0 {
                break;
            }
        }

        // Read RTC registers
        cmos_address.write(0x00);
        let seconds = bcd_to_binary(cmos_data.read());

        cmos_address.write(0x02);
        let minutes = bcd_to_binary(cmos_data.read());

        cmos_address.write(0x04);
        let hours = bcd_to_binary(cmos_data.read());

        cmos_address.write(0x07);
        let day = bcd_to_binary(cmos_data.read());

        cmos_address.write(0x08);
        let month = bcd_to_binary(cmos_data.read());

        cmos_address.write(0x09);
        let year_raw = bcd_to_binary(cmos_data.read()) as u32;

        // Read the century register (0x32) if available. Not all
        // motherboards implement it, so default to 20 (21st century).
        cmos_address.write(0x32);
        let century_raw = cmos_data.read();
        let century = if century_raw != 0 {
            bcd_to_binary(century_raw) as u32 * 100
        } else {
            2000
        };

        let year = century + year_raw;

        // Convert to Unix timestamp
        let days_since_epoch = days_since_unix_epoch(year, month, day)?;
        let timestamp = days_since_epoch * 86400
            + (hours as u64 * 3600)
            + (minutes as u64 * 60)
            + seconds as u64;

        Ok(timestamp)
    }
}

/// Convert BCD to binary
fn bcd_to_binary(bcd: u8) -> u8 {
    (bcd & 0x0F) + ((bcd >> 4) * 10)
}

/// Calculate days since Unix epoch (January 1, 1970)
fn days_since_unix_epoch(year: u32, month: u8, day: u8) -> Result<u64, &'static str> {
    if year < 1970 || month == 0 || month > 12 || day == 0 || day > 31 {
        return Err("Invalid date");
    }

    let mut days = 0u64;

    // Add days for complete years
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    // Add days for complete months in current year
    let days_in_month = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    for m in 1..month {
        days += days_in_month[(m - 1) as usize] as u64;
        if m == 2 && is_leap_year(year) {
            days += 1; // February has 29 days in leap years
        }
    }

    // Add days in current month
    days += (day - 1) as u64;

    Ok(days)
}

/// Check if a year is a leap year
fn is_leap_year(year: u32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Get timer statistics
pub fn get_timer_stats() -> TimerStats {
    let timer_manager = TIMER_MANAGER.lock();
    let active_timer = timer_manager.get_active_timer_type();
    let ticks = TICKS.load(Ordering::Relaxed);
    let tsc_frequency = TSC_FREQUENCY.load(Ordering::Relaxed);
    let initialized = TIMER_INITIALIZED.load(Ordering::Relaxed);

    // Calculate uptime inline to avoid deadlock (uptime_ms() also acquires this lock)
    let uptime = if initialized {
        if let Some(timer) = active_timer {
            match timer {
                TimerType::Pit => (ticks * 1000) / TIMER_FREQUENCY as u64,
                TimerType::ApicTimer | TimerType::Hpet => {
                    if tsc_frequency > 0 {
                        let current_tsc = read_tsc();
                        let boot_tsc = BOOT_TSC.load(Ordering::Relaxed);
                        if current_tsc > boot_tsc {
                            ((current_tsc - boot_tsc) * 1000) / tsc_frequency
                        } else {
                            (ticks * 1000) / TIMER_FREQUENCY as u64
                        }
                    } else {
                        (ticks * 1000) / TIMER_FREQUENCY as u64
                    }
                }
            }
        } else {
            (ticks * 1000) / TIMER_FREQUENCY as u64
        }
    } else {
        0
    };

    // Drop lock before calling system_time()
    drop(timer_manager);

    TimerStats {
        active_timer,
        ticks,
        tsc_frequency,
        uptime_ms: uptime,
        system_time: system_time(),
        initialized,
    }
}

/// Timer system statistics
#[derive(Debug, Clone)]
pub struct TimerStats {
    pub active_timer: Option<TimerType>,
    pub ticks: u64,
    pub tsc_frequency: u64,
    pub uptime_ms: u64,
    pub system_time: u64,
    pub initialized: bool,
}

/// High-resolution timer for performance measurement
pub struct Timer {
    start_tsc: u64,
    start_time_us: u64,
}

impl Timer {
    /// Create a new timer starting now
    pub fn new() -> Self {
        Self {
            start_tsc: read_tsc(),
            start_time_us: uptime_us(),
        }
    }

    /// Get elapsed time in nanoseconds (highest precision)
    pub fn elapsed_ns(&self) -> u64 {
        if let Some(freq) = get_tsc_frequency() {
            let delta = read_tsc().saturating_sub(self.start_tsc);
            // u128 scaling avoids u64 overflow; freq is guaranteed > 0 here.
            ((delta as u128 * 1_000_000_000) / freq as u128) as u64
        } else {
            (uptime_us().saturating_sub(self.start_time_us)) * 1000
        }
    }

    /// Get elapsed time in microseconds
    pub fn elapsed_us(&self) -> u64 {
        if let Some(freq) = get_tsc_frequency() {
            let delta = read_tsc().saturating_sub(self.start_tsc);
            // u128 scaling avoids u64 overflow; freq is guaranteed > 0 here.
            ((delta as u128 * 1_000_000) / freq as u128) as u64
        } else {
            uptime_us().saturating_sub(self.start_time_us)
        }
    }

    /// Get elapsed time in milliseconds
    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_us() / 1000
    }

    /// Reset the timer to current time
    pub fn reset(&mut self) {
        self.start_tsc = read_tsc();
        self.start_time_us = uptime_us();
    }
}

/// Timer callback function type
pub type TimerCallback = fn();

/// Timer ID for managing scheduled timers
pub type TimerId = u64;

/// Scheduled timer entry
#[derive(Debug, Clone)]
struct ScheduledTimer {
    id: TimerId,
    target_time_us: u64,
    callback: TimerCallback,
    periodic: bool,
    interval_us: u64,
}

lazy_static! {
    static ref SCHEDULED_TIMERS: Mutex<Vec<ScheduledTimer>> = Mutex::new(Vec::new());
    static ref NEXT_TIMER_ID: AtomicU64 = AtomicU64::new(1);
}

/// Schedule a one-shot timer
pub fn schedule_timer(delay_us: u64, callback: TimerCallback) -> TimerId {
    let id = NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed);
    let target_time = uptime_us() + delay_us;

    let timer = ScheduledTimer {
        id,
        target_time_us: target_time,
        callback,
        periodic: false,
        interval_us: 0,
    };

    SCHEDULED_TIMERS.lock().push(timer);
    id
}

/// Schedule a periodic timer
pub fn schedule_periodic_timer(interval_us: u64, callback: TimerCallback) -> TimerId {
    let id = NEXT_TIMER_ID.fetch_add(1, Ordering::Relaxed);
    let target_time = uptime_us() + interval_us;

    let timer = ScheduledTimer {
        id,
        target_time_us: target_time,
        callback,
        periodic: true,
        interval_us,
    };

    SCHEDULED_TIMERS.lock().push(timer);
    id
}

/// Cancel a scheduled timer
pub fn cancel_timer(timer_id: TimerId) -> bool {
    let mut timers = SCHEDULED_TIMERS.lock();
    if let Some(pos) = timers.iter().position(|t| t.id == timer_id) {
        timers.remove(pos);
        true
    } else {
        false
    }
}

/// Process scheduled timers (called from timer interrupt)
pub fn process_scheduled_timers() {
    // Don't process timers if system isn't fully initialized
    if !TIMER_INITIALIZED.load(Ordering::Relaxed) {
        return;
    }

    let current_time = uptime_us();
    let mut timers = SCHEDULED_TIMERS.lock();
    let mut expired_timers = Vec::new();

    // Find expired timers
    let mut i = 0;
    while i < timers.len() {
        if timers[i].target_time_us <= current_time {
            let timer = timers.remove(i);
            expired_timers.push(timer);
        } else {
            i += 1;
        }
    }

    // Release the lock before calling callbacks to avoid deadlocks
    drop(timers);

    // Process expired timers
    for mut timer in expired_timers {
        // Call the callback (be careful about panic handling in interrupt context)
        (timer.callback)();

        // Reschedule if periodic
        if timer.periodic {
            timer.target_time_us = current_time + timer.interval_us;
            SCHEDULED_TIMERS.lock().push(timer);
        }
    }
}

/// Get active timer type
pub fn get_active_timer_type() -> Option<TimerType> {
    TIMER_MANAGER.lock().get_active_timer_type()
}

/// Check if timer system is initialized
pub fn is_initialized() -> bool {
    TIMER_INITIALIZED.load(Ordering::Relaxed)
}

/// Display time system information for debugging
pub fn display_time_info() {
    if !is_initialized() {
        crate::println!("Timer system not initialized");
        return;
    }

    let stats = get_timer_stats();
    let uptime_sec = uptime_ms() / 1000;
    let hours = uptime_sec / 3600;
    let minutes = (uptime_sec % 3600) / 60;
    let seconds = uptime_sec % 60;

    crate::println!("=== Timer System Status ===");
    crate::println!("Active Timer: {:?}", stats.active_timer);
    crate::println!("Uptime: {:02}:{:02}:{:02}", hours, minutes, seconds);
    crate::println!("Total Ticks: {}", stats.ticks);

    if stats.tsc_frequency > 0 {
        crate::println!(
            "TSC Frequency: {:.2} GHz",
            stats.tsc_frequency as f64 / 1_000_000_000.0
        );
    } else {
        crate::println!("TSC Frequency: Not calibrated");
    }

    if stats.system_time > 0 {
        crate::println!("System Time: {} (Unix timestamp)", stats.system_time);
    } else {
        crate::println!("System Time: Not set");
    }

    crate::println!("Initialized: {}", stats.initialized);
    crate::println!("===========================");
}

/// Test timer accuracy and functionality
pub fn test_timer_accuracy() -> Result<(), &'static str> {
    if !is_initialized() {
        return Err("Timer system not initialized");
    }

    crate::println!("Testing timer accuracy...");

    // Test 1: Basic timing test
    let start_ms = uptime_ms();
    let start_us = uptime_us();
    let start_tsc = read_tsc();

    // Wait approximately 100ms
    sleep_ms(100);

    let end_ms = uptime_ms();
    let end_us = uptime_us();
    let end_tsc = read_tsc();

    let elapsed_ms = end_ms - start_ms;
    let elapsed_us = end_us - start_us;
    let elapsed_tsc = end_tsc - start_tsc;

    crate::println!("Sleep test (100ms target):");
    crate::println!("  Elapsed (ms): {}", elapsed_ms);
    crate::println!("  Elapsed (us): {}", elapsed_us);
    crate::println!("  TSC cycles: {}", elapsed_tsc);

    // Check if timing is reasonably accurate (within 10% tolerance)
    if elapsed_ms < 90 || elapsed_ms > 110 {
        crate::println!("Warning: Timer accuracy may be poor");
    } else {
        crate::println!("Timer accuracy test passed");
    }

    // Test 2: TSC frequency validation
    if let Some(tsc_freq) = get_tsc_frequency() {
        let calculated_ms = (elapsed_tsc * 1000) / tsc_freq;
        crate::println!("TSC-calculated elapsed: {} ms", calculated_ms);

        if calculated_ms > 0 && (calculated_ms as i64 - elapsed_ms as i64).abs() < 20 {
            crate::println!("TSC calibration appears accurate");
        } else {
            crate::println!("Warning: TSC calibration may be inaccurate");
        }
    }

    Ok(())
}

/// Check if High Precision Event Timer is available.
///
/// This function checks ACPI tables to determine if HPET hardware is present
/// and properly configured. Returns true if HPET is available and can be used
/// for high-precision timing.
///
/// # Returns
/// - `true` if HPET is available and has a valid base address
/// - `false` if HPET is not present or ACPI tables are not available
pub fn hpet_available() -> bool {
    // First check if ACPI is initialized
    if !crate::acpi::is_initialized() {
        return false;
    }

    // Check for cached HPET information in ACPI info
    if let Some(acpi_info) = crate::acpi::acpi_info() {
        if let Some(hpet_info) = acpi_info.hpet {
            // Verify the HPET has a valid base address
            if hpet_info.base_address != 0 {
                return true;
            }
        }
    }

    // Try to parse HPET from ACPI tables directly
    match crate::acpi::parse_hpet() {
        Ok(hpet_info) => hpet_info.base_address != 0,
        Err(_) => false,
    }
}
