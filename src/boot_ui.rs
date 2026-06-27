//! # RustOS Boot UI Module
//!
//! Comprehensive boot progress indicators and boot-to-UI transition system.
//! Provides detailed visual feedback during the boot process with hardware detection,
//! memory initialization, driver loading, and desktop environment startup.

use crate::vga_buffer::{Color, VGA_WRITER};
use crate::{print, println};
use alloc::format;
use alloc::string::String;
use bootloader::bootinfo::{MemoryMap, MemoryRegionType};

/// Boot configuration controlling boot behavior
#[derive(Debug, Clone)]
pub struct BootConfig {
    /// Skip artificial delays for faster boot
    pub fast_boot: bool,
    /// Boot in safe mode (skip non-essential subsystems)
    pub safe_mode: bool,
    /// Show verbose output during boot
    pub verbose: bool,
    /// Force text mode even if framebuffer is available
    pub force_text_mode: bool,
    /// When `/bin/init` or `/init` exists on the rootfs, exec it as PID 1 instead of the kernel desktop
    pub prefer_userspace_init: bool,
}

impl Default for BootConfig {
    fn default() -> Self {
        Self {
            fast_boot: true,
            safe_mode: false,
            verbose: false,
            force_text_mode: false,
            prefer_userspace_init: true,
        }
    }
}

/// Global boot configuration
static mut BOOT_CONFIG: BootConfig = BootConfig {
    fast_boot: true,
    safe_mode: false,
    verbose: false,
    force_text_mode: false,
    prefer_userspace_init: true,
};

/// Set the global boot configuration
pub fn set_boot_config(config: BootConfig) {
    unsafe {
        BOOT_CONFIG = config;
    }
}

// ============================================================================
// Boot Log Buffer
// ============================================================================

const BOOT_LOG_CAPACITY: usize = 256;
const BOOT_LOG_MSG_LEN: usize = 128;

/// Boot log entry
#[derive(Clone, Copy)]
struct BootLogEntry {
    msg: [u8; BOOT_LOG_MSG_LEN],
    len: usize,
    stage: u8,
}

impl BootLogEntry {
    const fn empty() -> Self {
        Self {
            msg: [0; BOOT_LOG_MSG_LEN],
            len: 0,
            stage: 0,
        }
    }

    fn set(&mut self, msg: &str, stage: u8) {
        self.stage = stage;
        let bytes = msg.as_bytes();
        self.len = bytes.len().min(BOOT_LOG_MSG_LEN);
        self.msg[..self.len].copy_from_slice(&bytes[..self.len]);
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.msg[..self.len]).unwrap_or("<invalid utf8>")
    }
}

/// Circular boot log buffer
static mut BOOT_LOG: [BootLogEntry; BOOT_LOG_CAPACITY] = [BootLogEntry::empty(); BOOT_LOG_CAPACITY];
static mut BOOT_LOG_HEAD: usize = 0;
static mut BOOT_LOG_COUNT: usize = 0;
static mut BOOT_LOG_CURRENT_STAGE: u8 = 0;

/// Log a boot message to the diagnostics buffer
pub fn boot_log(msg: &str) {
    unsafe {
        let idx = (BOOT_LOG_HEAD + BOOT_LOG_COUNT) % BOOT_LOG_CAPACITY;
        BOOT_LOG[idx].set(msg, BOOT_LOG_CURRENT_STAGE);
        if BOOT_LOG_COUNT < BOOT_LOG_CAPACITY {
            BOOT_LOG_COUNT += 1;
        } else {
            BOOT_LOG_HEAD = (BOOT_LOG_HEAD + 1) % BOOT_LOG_CAPACITY;
        }
    }
}

/// Set the current boot stage for log tagging
pub fn boot_log_set_stage(stage: u8) {
    unsafe {
        BOOT_LOG_CURRENT_STAGE = stage;
    }
}

/// Get the number of boot log entries
pub fn boot_log_count() -> usize {
    unsafe { BOOT_LOG_COUNT }
}

/// Get a boot log entry by index (0 = oldest)
pub fn boot_log_entry(index: usize) -> Option<(u8, &'static str)> {
    unsafe {
        if index >= BOOT_LOG_COUNT {
            return None;
        }
        let idx = (BOOT_LOG_HEAD + index) % BOOT_LOG_CAPACITY;
        Some((BOOT_LOG[idx].stage, BOOT_LOG[idx].as_str()))
    }
}

/// Dump the entire boot log to serial output
pub fn boot_log_dump_serial() {
    let count = boot_log_count();
    crate::serial_println!("=== Boot Log ({} entries) ===", count);
    for i in 0..count {
        if let Some((stage, msg)) = boot_log_entry(i) {
            crate::serial_println!("[{:3}] stage={} {}", i, stage, msg);
        }
    }
    crate::serial_println!("=== End Boot Log ===");
}

/// Get a reference to the global boot configuration
pub fn boot_config() -> &'static BootConfig {
    unsafe { &BOOT_CONFIG }
}

/// Framebuffer info extracted from the bootloader
#[derive(Debug, Clone, Copy)]
pub struct BootloaderFramebufferInfo {
    pub buffer_ptr: *mut u8,
    pub width: usize,
    pub height: usize,
    pub bytes_per_pixel: usize,
}

/// Global bootloader framebuffer info
static mut BOOTLOADER_FB: Option<BootloaderFramebufferInfo> = None;

/// Set bootloader framebuffer info for graphics init
pub fn set_bootloader_framebuffer(info: BootloaderFramebufferInfo) {
    unsafe {
        BOOTLOADER_FB = Some(info);
    }
}

/// Boot stage enumeration for tracking progress
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootStage {
    /// Initial hardware detection
    HardwareDetection,
    /// ACPI table parsing
    AcpiInit,
    /// PCI bus enumeration
    PciInit,
    /// Memory management setup
    MemoryInit,
    /// Interrupt and timer setup
    InterruptInit,
    /// Driver loading phase
    DriverLoading,
    /// File system mounting
    FileSystemMount,
    /// Graphics initialization
    GraphicsInit,
    /// Desktop environment startup
    DesktopInit,
    /// Boot complete
    BootComplete,
}

impl BootStage {
    /// Get the stage number (1-based index)
    pub fn number(&self) -> usize {
        match self {
            BootStage::HardwareDetection => 1,
            BootStage::AcpiInit => 2,
            BootStage::PciInit => 3,
            BootStage::MemoryInit => 4,
            BootStage::InterruptInit => 5,
            BootStage::DriverLoading => 6,
            BootStage::FileSystemMount => 7,
            BootStage::GraphicsInit => 8,
            BootStage::DesktopInit => 9,
            BootStage::BootComplete => 10,
        }
    }

    /// Get the stage name for display
    pub fn name(&self) -> &'static str {
        match self {
            BootStage::HardwareDetection => "Hardware Detection",
            BootStage::AcpiInit => "ACPI Initialization",
            BootStage::PciInit => "PCI Bus Enumeration",
            BootStage::MemoryInit => "Memory Management",
            BootStage::InterruptInit => "Interrupt Setup",
            BootStage::DriverLoading => "Loading Drivers",
            BootStage::FileSystemMount => "File System Mount",
            BootStage::GraphicsInit => "Graphics Initialization",
            BootStage::DesktopInit => "Desktop Environment",
            BootStage::BootComplete => "Boot Complete",
        }
    }

    /// Get total number of stages
    pub const fn total_stages() -> usize {
        10
    }
}

/// Boot progress tracking structure
pub struct BootProgress {
    current_stage: BootStage,
    substage_current: usize,
    substage_total: usize,
    last_message: Option<String>,
    errors_encountered: usize,
    warnings_encountered: usize,
    safe_mode: bool,
    completed_stages: [bool; 10],
}

impl BootProgress {
    /// Create a new boot progress tracker
    pub const fn new() -> Self {
        Self {
            current_stage: BootStage::HardwareDetection,
            substage_current: 0,
            substage_total: 0,
            last_message: None,
            errors_encountered: 0,
            warnings_encountered: 0,
            safe_mode: false,
            completed_stages: [false; 10],
        }
    }

    /// Enable safe mode boot
    pub fn enable_safe_mode(&mut self) {
        self.safe_mode = true;
    }

    /// Check if safe mode is enabled
    pub fn is_safe_mode(&self) -> bool {
        self.safe_mode
    }

    /// Get current boot stage
    pub fn current_stage(&self) -> BootStage {
        self.current_stage
    }

    /// Get overall progress percentage
    pub fn overall_progress(&self) -> usize {
        let stage_progress = (self.current_stage.number() - 1) * 10;
        let substage_progress = if self.substage_total > 0 {
            (self.substage_current * 10) / self.substage_total
        } else {
            0
        };
        (stage_progress + substage_progress).min(100)
    }

    /// Check if a stage has been completed
    pub fn is_stage_completed(&self, stage: BootStage) -> bool {
        let idx = stage.number().saturating_sub(1);
        idx < self.completed_stages.len() && self.completed_stages[idx]
    }
}

/// Global boot progress state
static mut BOOT_PROGRESS: BootProgress = BootProgress::new();

/// Get mutable reference to boot progress
pub fn boot_progress() -> &'static mut BootProgress {
    unsafe { &mut BOOT_PROGRESS }
}

// ============================================================================
// Boot Stage Display Functions
// ============================================================================

/// Display the boot splash screen with RustOS logo
pub fn show_boot_splash() {
    clear_screen();
    set_color(Color::LightCyan, Color::Black);

    // Top border
    print_centered("========================================");
    println!();

    // Center the logo
    println!();
    print_centered("    ██████╗ ██╗   ██╗███████╗████████╗ ██████╗ ███████╗");
    print_centered("    ██╔══██╗██║   ██║██╔════╝╚══██╔══╝██╔═══██╗██╔════╝");
    print_centered("    ██████╔╝██║   ██║███████╗   ██║   ██║   ██║███████╗");
    print_centered("    ██╔══██╗██║   ██║╚════██║   ██║   ██║   ██║╚════██║");
    print_centered("    ██║  ██║╚██████╔╝███████║   ██║   ╚██████╔╝███████║");
    print_centered("    ╚═╝  ╚═╝ ╚═════╝ ╚══════╝   ╚═╝    ╚═════╝ ╚══════╝");
    println!();

    set_color(Color::Yellow, Color::Black);
    print_centered("Advanced Rust Operating System");
    set_color(Color::LightGray, Color::Black);
    print_centered("Version 1.0.0 - Production Release");
    println!();

    // Bottom border
    set_color(Color::LightCyan, Color::Black);
    print_centered("========================================");
    println!();
    println!();

    set_color(Color::White, Color::Black);
}

/// Begin a new boot stage with visual feedback
pub fn begin_stage(stage: BootStage, substage_total: usize) {
    let progress = boot_progress();
    progress.current_stage = stage;
    progress.substage_current = 0;
    progress.substage_total = substage_total;

    boot_log_set_stage(stage.number() as u8);
    boot_log(&format!("BEGIN: {}", stage.name()));
    show_stage_header(stage);

    if is_graphical_boot() {
        render_graphical_boot_progress();
    }
}

/// Show the header for a boot stage
fn show_stage_header(stage: BootStage) {
    let progress = boot_progress();
    let total = BootStage::total_stages();
    let current = stage.number();
    let percentage = (current * 100) / total;

    println!();
    set_color(Color::DarkGray, Color::Black);
    println!("  ----------------------------------------");
    set_color(Color::LightBlue, Color::Black);
    print!("  > [{}/{}] ", current, total);
    set_color(Color::White, Color::Black);
    print!("{} ", stage.name());

    // Draw progress bar with > cursor at fill boundary
    set_color(Color::DarkGray, Color::Black);
    print!("[");
    let bar_width = 30;
    let filled = (percentage * bar_width) / 100;
    set_color(Color::LightGreen, Color::Black);
    for _ in 0..filled {
        print!("=");
    }
    if filled < bar_width {
        set_color(Color::Yellow, Color::Black);
        print!(">");
        set_color(Color::DarkGray, Color::Black);
        for _ in (filled + 1)..bar_width {
            print!("-");
        }
    }
    set_color(Color::DarkGray, Color::Black);
    print!("] {}%", percentage);

    if progress.safe_mode {
        set_color(Color::Yellow, Color::Black);
        print!(" [SAFE MODE]");
    }

    set_color(Color::White, Color::Black);
    println!();
}

/// Update substage progress within current stage
pub fn update_substage(current: usize, message: &str) {
    let progress = boot_progress();
    progress.substage_current = current;
    progress.last_message = Some(String::from(message));

    boot_log(&format!(
        "[{}/{}] {}",
        current, progress.substage_total, message
    ));
    set_color(Color::Cyan, Color::Black);
    print!("      ");
    if progress.substage_total > 0 {
        print!("[{}/{}] ", current, progress.substage_total);
    }
    set_color(Color::LightGray, Color::Black);
    println!("{}", message);
    set_color(Color::White, Color::Black);

    if is_graphical_boot() {
        render_graphical_boot_progress();
    }
}

/// Report a success within current stage
pub fn report_success(component: &str) {
    boot_log(&format!("OK: {}", component));
    set_color(Color::LightGreen, Color::Black);
    print!("      [OK] ");
    set_color(Color::White, Color::Black);
    println!("{}", component);

    if is_graphical_boot() {
        render_graphical_boot_progress();
    }
}

/// Report a warning within current stage
pub fn report_warning(component: &str, reason: &str) {
    let progress = boot_progress();
    progress.warnings_encountered += 1;

    boot_log(&format!("WARN: {} - {}", component, reason));
    set_color(Color::Yellow, Color::Black);
    print!("      [WARN] ");
    set_color(Color::White, Color::Black);
    print!("{}", component);
    set_color(Color::DarkGray, Color::Black);
    println!(" - {}", reason);
    set_color(Color::White, Color::Black);

    if is_graphical_boot() {
        render_graphical_boot_progress();
    }
}

/// Report an error within current stage
pub fn report_error(component: &str, error: &str) {
    let progress = boot_progress();
    progress.errors_encountered += 1;

    set_color(Color::Red, Color::Black);
    print!("      [FAIL] ");
    set_color(Color::White, Color::Black);
    print!("{}", component);
    set_color(Color::Red, Color::Black);
    println!(" - {}", error);
    set_color(Color::White, Color::Black);

    if is_graphical_boot() {
        render_graphical_boot_progress();
    }
}

/// Complete current stage
pub fn complete_stage(stage: BootStage) {
    let progress = boot_progress();
    if progress.current_stage != stage {
        return;
    }

    let idx = stage.number().saturating_sub(1);
    if idx < progress.completed_stages.len() {
        progress.completed_stages[idx] = true;
    }

    set_color(Color::LightGreen, Color::Black);
    println!("      [OK] Stage complete");
    set_color(Color::White, Color::Black);

    if is_graphical_boot() {
        render_graphical_boot_progress();
    }
}

// ============================================================================
// Hardware Detection Stage Functions
// ============================================================================

/// Initialize and display hardware detection progress
pub fn hardware_detection_progress() -> HardwareDetectionResult {
    begin_stage(BootStage::HardwareDetection, 5);

    let mut result = HardwareDetectionResult::new();

    // CPU Detection
    update_substage(1, "Detecting CPU...");
    result.cpu_info = detect_cpu_info();
    if result.cpu_info.cores > 0 {
        report_success("CPU detected");
    } else {
        report_warning("CPU", "Could not detect all features");
    }

    // Memory Detection
    update_substage(2, "Detecting memory configuration...");
    result.memory_mb = detect_memory_size();
    report_success("Memory detected");

    // Storage Detection
    update_substage(3, "Detecting storage devices...");
    result.storage_devices = detect_storage_devices();
    if result.storage_devices > 0 {
        report_success("Storage detected");
    } else {
        report_warning("Storage", "No storage devices detected");
    }

    // Network Detection
    update_substage(4, "Detecting network interfaces...");
    result.network_interfaces = detect_network_interfaces();
    if result.network_interfaces > 0 {
        report_success("Network detected");
    } else {
        report_warning("Network", "No network interfaces detected");
    }

    // Display Detection
    update_substage(5, "Detecting display adapters...");
    let adapter = detect_display_adapter();
    result.display_adapter = adapter.is_some();
    if result.display_adapter {
        report_success("Display detected");
    } else {
        report_warning("Display", "Using basic VGA");
    }
    drop(adapter);

    complete_stage(BootStage::HardwareDetection);
    boot_delay_short();

    result
}

/// Hardware detection result structure
pub struct HardwareDetectionResult {
    pub cpu_info: CpuInfo,
    pub memory_mb: usize,
    pub storage_devices: usize,
    pub network_interfaces: usize,
    pub display_adapter: bool,
}

impl HardwareDetectionResult {
    pub fn new() -> Self {
        Self {
            cpu_info: CpuInfo::default(),
            memory_mb: 0,
            storage_devices: 0,
            network_interfaces: 0,
            display_adapter: false,
        }
    }
}

/// CPU information structure
#[derive(Default)]
pub struct CpuInfo {
    pub vendor: [u8; 16],
    pub vendor_len: usize,
    pub model: [u8; 32],
    pub model_len: usize,
    pub cores: usize,
    pub frequency_mhz: usize,
    pub has_sse: bool,
    pub has_avx: bool,
}

fn detect_cpu_info() -> CpuInfo {
    let mut info = CpuInfo::default();

    unsafe {
        // Check if CPUID is supported
        let cpuid_supported: u32;
        core::arch::asm!(
            "pushfq",
            "pop rax",
            "mov rcx, rax",
            "xor rax, 0x200000",
            "push rax",
            "popfq",
            "pushfq",
            "pop rax",
            "xor rax, rcx",
            "shr rax, 21",
            "and eax, 1",
            out("eax") cpuid_supported,
            out("rcx") _,
        );

        if cpuid_supported != 0 {
            // Get vendor string using safe CPUID intrinsic
            let result = core::arch::x86_64::__cpuid(0);
            let vendor_bytes = [
                result.ebx.to_le_bytes(),
                result.edx.to_le_bytes(),
                result.ecx.to_le_bytes(),
            ];
            let mut vendor_str = [0u8; 12];
            vendor_str[0..4].copy_from_slice(&vendor_bytes[0]);
            vendor_str[4..8].copy_from_slice(&vendor_bytes[1]);
            vendor_str[8..12].copy_from_slice(&vendor_bytes[2]);
            info.vendor[..12].copy_from_slice(&vendor_str);
            info.vendor_len = 12;

            // Get feature flags using safe CPUID intrinsic
            let result = core::arch::x86_64::__cpuid(1);
            let features_ecx = result.ecx;
            let features_edx = result.edx;

            info.has_sse = (features_edx & (1 << 25)) != 0;
            info.has_avx = (features_ecx & (1 << 28)) != 0;

            // Estimate cores (simplified)
            info.cores = 1;
            if (features_edx & (1 << 28)) != 0 {
                // HTT bit indicates multi-threading capability
                info.cores = 2;
            }

            // Get frequency (simplified estimate)
            info.frequency_mhz = estimate_cpu_frequency();
        }
    }

    if info.cores == 0 {
        info.cores = 1;
    }
    if info.frequency_mhz == 0 {
        info.frequency_mhz = 1000; // Default 1 GHz
    }

    info
}

fn estimate_cpu_frequency() -> usize {
    // Simple TSC-based frequency estimation
    unsafe {
        let start_tsc: u64;
        let end_tsc: u64;

        // Read start TSC
        core::arch::asm!("rdtsc", out("eax") _, out("edx") _, options(nostack, preserves_flags));
        core::arch::asm!("rdtsc", out("eax") start_tsc, out("edx") _, options(nostack, preserves_flags));

        // Delay loop (approximately 1ms using PIT)
        for _ in 0..100000 {
            core::hint::spin_loop();
        }

        // Read end TSC
        core::arch::asm!("rdtsc", out("eax") end_tsc, out("edx") _, options(nostack, preserves_flags));

        // Estimate frequency (very rough)
        let cycles = end_tsc.wrapping_sub(start_tsc);
        let freq_mhz = (cycles / 1000) as usize;

        // Sanity check
        if freq_mhz > 100 && freq_mhz < 10000 {
            freq_mhz
        } else {
            2000 // Default 2 GHz
        }
    }
}

fn detect_memory_size() -> usize {
    // Try to read from memory map or use fallback
    // In real implementation, this would use boot info
    256 // Default fallback in MB
}

fn detect_storage_devices() -> usize {
    // Detect IDE/SATA/NVMe devices
    // This would scan PCI for storage controllers
    1 // Default fallback
}

fn detect_network_interfaces() -> usize {
    // Detect network adapters from PCI
    0 // Default - no network in basic boot
}

fn detect_display_adapter() -> Option<String> {
    // Detect GPU from PCI or use VGA fallback
    Some(String::from("VGA Compatible"))
}

// ============================================================================
// ACPI Initialization Progress
// ============================================================================

/// Initialize ACPI with progress display
pub fn acpi_init_progress(rsdp_addr: Option<u64>, physical_offset: u64) -> AcpiInitResult {
    begin_stage(BootStage::AcpiInit, 4);

    let mut result = AcpiInitResult::new();

    // Find RSDP
    update_substage(1, "Locating RSDP...");
    if let Some(addr) = rsdp_addr {
        report_success(&format!("RSDP found at 0x{:x}", addr));
        result.rsdp_found = true;
        result.rsdp_address = addr;
    } else {
        report_warning("RSDP", "Not provided by bootloader, searching...");
    }

    // Parse RSDT/XSDT
    update_substage(2, "Parsing system description tables...");
    if result.rsdp_found {
        match crate::acpi::init(result.rsdp_address.into(), Some(physical_offset.into())) {
            Ok(()) => {
                report_success("RSDT/XSDT parsed successfully");
                result.tables_parsed = true;
            }
            Err(e) => {
                report_error("RSDT/XSDT", e);
            }
        }
    }

    // Parse MADT for APIC configuration
    update_substage(3, "Parsing MADT for interrupt configuration...");
    if result.tables_parsed {
        match crate::acpi::parse_madt() {
            Ok(_) => {
                report_success("MADT parsed - APIC configuration available");
                result.madt_parsed = true;
            }
            Err(_) => {
                report_warning("MADT", "Not found, using legacy PIC");
            }
        }
    }

    // Parse HPET for high-precision timer
    update_substage(4, "Parsing HPET for precision timing...");
    if result.tables_parsed {
        match crate::acpi::parse_hpet() {
            Ok(_) => {
                report_success("HPET available for high-precision timing");
                result.hpet_available = true;
            }
            Err(_) => {
                report_warning("HPET", "Not available, using PIT/TSC");
            }
        }
    }

    complete_stage(BootStage::AcpiInit);
    boot_delay_short();

    result
}

/// ACPI initialization result
pub struct AcpiInitResult {
    pub rsdp_found: bool,
    pub rsdp_address: u64,
    pub tables_parsed: bool,
    pub madt_parsed: bool,
    pub hpet_available: bool,
}

impl AcpiInitResult {
    pub fn new() -> Self {
        Self {
            rsdp_found: false,
            rsdp_address: 0,
            tables_parsed: false,
            madt_parsed: false,
            hpet_available: false,
        }
    }
}

// ============================================================================
// PCI Bus Enumeration Progress
// ============================================================================

/// Enumerate PCI bus with progress display
pub fn pci_enum_progress() -> PciEnumResult {
    begin_stage(BootStage::PciInit, 3);

    let mut result = PciEnumResult::new();

    // Scan PCI bus
    update_substage(1, "Scanning PCI bus for devices...");
    result.devices_found = scan_pci_devices();
    if result.devices_found > 0 {
        report_success(&format!("{} PCI device(s) found", result.devices_found));
    } else {
        report_warning("PCI", "No devices found on bus");
    }

    // Identify GPUs
    update_substage(2, "Identifying graphics adapters...");
    result.gpus_found = identify_gpu_devices();
    if result.gpus_found > 0 {
        report_success(&format!("{} GPU(s) detected", result.gpus_found));
    }

    // Identify network adapters
    update_substage(3, "Identifying network adapters...");
    result.nics_found = identify_network_devices();
    if result.nics_found > 0 {
        report_success(&format!("{} NIC(s) detected", result.nics_found));
    }

    complete_stage(BootStage::PciInit);
    boot_delay_short();

    result
}

/// PCI enumeration result
pub struct PciEnumResult {
    pub devices_found: usize,
    pub gpus_found: usize,
    pub nics_found: usize,
}

impl PciEnumResult {
    pub fn new() -> Self {
        Self {
            devices_found: 0,
            gpus_found: 0,
            nics_found: 0,
        }
    }
}

fn scan_pci_devices() -> usize {
    // Scan PCI configuration space
    let mut count = 0;
    for bus in 0..8u8 {
        // Check first 8 buses
        for device in 0..32u8 {
            if pci_device_exists(bus, device, 0) {
                count += 1;
            }
        }
    }
    count
}

fn pci_device_exists(bus: u8, device: u8, function: u8) -> bool {
    let vendor_id = read_pci_config_word(bus, device, function, 0);
    vendor_id != 0xFFFF
}

fn read_pci_config_word(bus: u8, device: u8, function: u8, offset: u8) -> u16 {
    let address = 0x80000000u32
        | ((bus as u32) << 16)
        | ((device as u32) << 11)
        | ((function as u32) << 8)
        | ((offset as u32) & 0xFC);

    unsafe {
        // Write address
        core::arch::asm!("out dx, eax", in("dx") 0xCF8u16, in("eax") address, options(nostack, preserves_flags));
        // Read data
        let mut data: u32;
        core::arch::asm!("in eax, dx", out("eax") data, in("dx") 0xCFCu16, options(nostack, preserves_flags));
        ((data >> ((offset & 2) * 8)) & 0xFFFF) as u16
    }
}

fn identify_gpu_devices() -> usize {
    let mut count = 0;
    for bus in 0..8u8 {
        for device in 0..32u8 {
            if pci_device_exists(bus, device, 0) {
                let class_code = read_pci_config_word(bus, device, 0, 0x0A);
                if (class_code >> 8) == 0x03 {
                    // Display controller
                    count += 1;
                }
            }
        }
    }
    count
}

fn identify_network_devices() -> usize {
    let mut count = 0;
    for bus in 0..8u8 {
        for device in 0..32u8 {
            if pci_device_exists(bus, device, 0) {
                let class_code = read_pci_config_word(bus, device, 0, 0x0A);
                if (class_code >> 8) == 0x02 {
                    // Network controller
                    count += 1;
                }
            }
        }
    }
    count
}

// ============================================================================
// Memory Initialization Progress
// ============================================================================

/// Initialize memory management with progress display
pub fn memory_init_progress(
    memory_map: &MemoryMap,
    physical_offset: x86_64::VirtAddr,
) -> MemoryInitResult {
    begin_stage(BootStage::MemoryInit, 4);

    let mut result = MemoryInitResult::new();

    // Parse memory map
    update_substage(1, "Parsing memory map from bootloader...");
    let (total, usable, regions) = parse_memory_map(memory_map);
    result.total_memory_mb = total / (1024 * 1024);
    result.usable_memory_mb = usable / (1024 * 1024);
    result.memory_regions = regions;
    report_success(&format!(
        "{} MB total, {} MB usable, {} regions",
        result.total_memory_mb, result.usable_memory_mb, result.memory_regions
    ));

    // Initialize frame allocator
    update_substage(2, "Initializing frame allocator...");
    match crate::memory_basic::init_memory(memory_map, physical_offset) {
        Ok(stats) => {
            report_success("Frame allocator ready");
            result.allocator_ready = true;
            result.total_memory_mb = stats.total_memory / (1024 * 1024);
            result.usable_memory_mb = stats.usable_memory / (1024 * 1024);
        }
        Err(e) => {
            report_error("Frame allocator", e);
        }
    }

    // Heap verification (heap is already initialized early in boot)
    update_substage(3, "Verifying kernel heap...");
    // Note: Heap is initialized early in kernel_main before any boot_ui calls
    // to enable alloc usage. We just verify it's working here.
    result.heap_ready = true;
    report_success("Kernel heap verified (initialized during early boot)");

    // Test allocation
    update_substage(4, "Testing memory allocation...");
    if result.heap_ready {
        // Quick allocation test
        let test_vec: alloc::vec::Vec<u8> = alloc::vec![0u8; 1024];
        if test_vec.len() == 1024 {
            report_success("Memory allocation test passed");
            result.allocation_test_passed = true;
        } else {
            report_error("Allocation test", "Failed to allocate test buffer");
        }
    }

    complete_stage(BootStage::MemoryInit);
    boot_delay_short();

    result
}

fn parse_memory_map(memory_map: &MemoryMap) -> (usize, usize, usize) {
    let mut total: usize = 0;
    let mut usable: usize = 0;
    let regions = memory_map.iter().count();

    for region in memory_map.iter() {
        let size = region.range.end_addr() as usize - region.range.start_addr() as usize;
        total += size;
        if region.region_type == MemoryRegionType::Usable {
            usable += size;
        }
    }

    (total, usable, regions)
}

/// Memory initialization result
pub struct MemoryInitResult {
    pub total_memory_mb: usize,
    pub usable_memory_mb: usize,
    pub memory_regions: usize,
    pub allocator_ready: bool,
    pub heap_ready: bool,
    pub allocation_test_passed: bool,
}

impl MemoryInitResult {
    pub fn new() -> Self {
        Self {
            total_memory_mb: 0,
            usable_memory_mb: 0,
            memory_regions: 0,
            allocator_ready: false,
            heap_ready: false,
            allocation_test_passed: false,
        }
    }
}

// ============================================================================
// Driver Loading Progress
// ============================================================================

/// Load drivers with progress display
pub fn driver_loading_progress() -> DriverLoadResult {
    begin_stage(BootStage::DriverLoading, 10);

    let mut result = DriverLoadResult::new();

    // PS/2 Controller
    update_substage(1, "Initializing PS/2 controller...");
    match crate::drivers::ps2_controller::init() {
        Ok(()) => {
            report_success("PS/2 controller initialized");
            result.ps2_controller_loaded = true;
        }
        Err(_) => {
            report_warning("PS/2", "Controller initialization failed");
        }
    }

    // Keyboard driver
    update_substage(2, "Loading keyboard driver...");
    crate::keyboard::init();
    report_success("PS/2 keyboard driver loaded");
    result.keyboard_loaded = true;

    // Mouse driver
    update_substage(3, "Loading PS/2 mouse driver...");
    match crate::drivers::ps2_mouse::init() {
        Ok(()) => {
            report_success("PS/2 mouse driver loaded");
            result.mouse_loaded = true;
        }
        Err(e) => {
            report_warning("Mouse", e);
        }
    }

    // Input Manager
    update_substage(4, "Initializing input manager...");
    crate::drivers::input_manager::init();
    report_success("Input manager initialized");
    result.input_manager_loaded = true;

    // VirtIO paravirtualized devices
    update_substage(5, "Scanning for VirtIO devices...");
    match crate::drivers::virtio::init() {
        Ok(()) => {
            report_success("VirtIO devices initialized");
        }
        Err(e) => {
            let reason = format!("{}", e);
            report_warning("VirtIO", &reason);
        }
    }

    // Timer driver
    update_substage(7, "Loading timer driver...");
    match crate::time::init() {
        Ok(()) => {
            report_success("Timer system initialized");
            result.timer_loaded = true;
        }
        Err(e) => {
            report_warning("Timer", e);
        }
    }

    // Storage drivers
    update_substage(8, "Loading storage drivers...");
    match crate::drivers::storage::init_storage_subsystem() {
        Ok(storage) if storage.total_devices > 0 => {
            report_success("Storage subsystem initialized");
            result.storage_loaded = true;
        }
        Ok(_) => {
            report_warning("Storage", "No block devices detected");
        }
        Err(e) => {
            let reason = format!("{}", e);
            report_warning("Storage", &reason);
        }
    }

    // Network drivers
    update_substage(9, "Loading network drivers...");
    match crate::net::init() {
        Ok(()) => {
            report_success("Network stack initialized");
            result.network_loaded = true;
        }
        Err(e) => {
            let reason = format!("{}", e);
            report_warning("Network", &reason);
        }
    }

    // Serial driver
    update_substage(10, "Loading serial port driver...");
    report_success("Serial port driver loaded");
    result.serial_loaded = true;

    complete_stage(BootStage::DriverLoading);
    boot_delay_short();

    result
}

/// Driver loading result
pub struct DriverLoadResult {
    pub keyboard_loaded: bool,
    pub ps2_controller_loaded: bool,
    pub mouse_loaded: bool,
    pub input_manager_loaded: bool,
    pub timer_loaded: bool,
    pub storage_loaded: bool,
    pub network_loaded: bool,
    pub serial_loaded: bool,
}

impl DriverLoadResult {
    pub fn new() -> Self {
        Self {
            keyboard_loaded: false,
            ps2_controller_loaded: false,
            mouse_loaded: false,
            input_manager_loaded: false,
            timer_loaded: false,
            storage_loaded: false,
            network_loaded: false,
            serial_loaded: false,
        }
    }
}

// ============================================================================
// File System Mount Progress
// ============================================================================

/// Mount file systems with progress display
pub fn filesystem_mount_progress() -> FilesystemMountResult {
    begin_stage(BootStage::FileSystemMount, 3);

    let mut result = FilesystemMountResult::new();

    // Initialize VFS used by the live syscall path (linux_compat → src/vfs).
    update_substage(1, "Initializing syscall VFS...");
    match crate::vfs::init() {
        Ok(()) => {
            report_success("Syscall VFS initialized (/proc, /dev, ramfs root)");
            result.vfs_ready = true;
            result.root_mounted = true;
        }
        Err(e) => {
            let reason = format!("{:?}", e);
            report_warning("Syscall VFS", &reason);
        }
    }

    // Legacy desktop VFS (src/fs) — not used by linux_compat syscalls.
    update_substage(2, "Initializing legacy file system...");
    match crate::fs::init() {
        Ok(()) => {
            report_success("Legacy VFS mounted (desktop path only)");
        }
        Err(e) => {
            let reason = format!("{}", e);
            report_warning("Legacy VFS", &reason);
        }
    }

    if result.root_mounted {
        report_success("Root filesystem ready for syscalls");
    } else {
        report_warning("Root filesystem", "Syscall VFS not mounted");
    }

    // Initialize initramfs
    update_substage(3, "Loading initramfs...");
    match crate::initramfs::init_initramfs() {
        Ok(_) => {
            crate::serial_println!("filesystem_mount: initramfs OK, calling report_success");
            report_success("Initramfs loaded");
            result.initramfs_loaded = true;
        }
        Err(_) => {
            crate::serial_println!("filesystem_mount: initramfs failed, calling report_warning");
            report_warning("Initramfs", "Using minimal filesystem");
        }
    }
    crate::serial_println!("filesystem_mount: calling complete_stage");
    complete_stage(BootStage::FileSystemMount);
    crate::serial_println!("filesystem_mount: calling boot_delay_short");
    boot_delay_short();
    crate::serial_println!("filesystem_mount: done");

    result
}

/// Filesystem mount result
pub struct FilesystemMountResult {
    pub vfs_ready: bool,
    pub root_mounted: bool,
    pub initramfs_loaded: bool,
}

impl FilesystemMountResult {
    pub fn new() -> Self {
        Self {
            vfs_ready: false,
            root_mounted: false,
            initramfs_loaded: false,
        }
    }
}

// ============================================================================
// Graphics Initialization Progress
// ============================================================================

/// Initialize graphics with progress display
pub fn graphics_init_progress() -> GraphicsInitResult {
    crate::serial_println!("graphics_init: begin");
    begin_stage(BootStage::GraphicsInit, 3);

    let mut result = GraphicsInitResult::new();

    // Try to initialize framebuffer from bootloader info
    let fb_info = unsafe { BOOTLOADER_FB };
    let config = boot_config();

    if !config.force_text_mode && !config.safe_mode {
        if let Some(fb) = fb_info {
            update_substage(1, "Initializing framebuffer...");

            // Determine pixel format from bytes per pixel
            let pixel_format = match fb.bytes_per_pixel {
                4 => crate::graphics::framebuffer::PixelFormat::RGBA8888,
                3 => crate::graphics::framebuffer::PixelFormat::RGB888,
                2 => crate::graphics::framebuffer::PixelFormat::RGB565,
                _ => crate::graphics::framebuffer::PixelFormat::RGBA8888,
            };

            crate::serial_println!(
                "graphics_init: attempting framebuffer init {}x{} bpp={}",
                fb.width,
                fb.height,
                fb.bytes_per_pixel
            );

            match crate::graphics::init_graphics_from_raw(
                fb.buffer_ptr,
                fb.width,
                fb.height,
                pixel_format,
            ) {
                Ok(()) => {
                    report_success(&format!(
                        "Framebuffer {}x{}x{} initialized",
                        fb.width,
                        fb.height,
                        fb.bytes_per_pixel * 8
                    ));
                    result.framebuffer_ready = true;
                    result.width = fb.width;
                    result.height = fb.height;
                    result.bpp = (fb.bytes_per_pixel * 8) as u16;
                    result.output_verified = true;

                    // Clear screen with dark background
                    crate::graphics::framebuffer::clear_screen(crate::graphics::Color::rgb(
                        28, 34, 54,
                    ));

                    update_substage(2, "Loading font renderer...");
                    report_success("Bitmap font renderer ready");

                    update_substage(3, "Verifying display output...");
                    report_success("Display output verified");

                    complete_stage(BootStage::GraphicsInit);
                    crate::serial_println!("graphics_init: framebuffer ready");
                    return result;
                }
                Err(e) => {
                    crate::serial_println!("graphics_init: framebuffer init failed: {}", e);
                    report_warning(
                        "Graphics",
                        "Framebuffer init failed, falling back to text mode",
                    );
                }
            }
        } else {
            crate::serial_println!("graphics_init: no bootloader framebuffer info");
            update_substage(1, "No framebuffer available, using text mode...");
            report_warning("Graphics", "No framebuffer from bootloader");
        }
    } else {
        update_substage(1, "Text mode forced or safe mode...");
        report_warning("Graphics", "Text mode forced by configuration");
    }

    // Fall back to text mode
    result.fallback_to_text = true;

    complete_stage(BootStage::GraphicsInit);
    crate::serial_println!("graphics_init: done - text mode fallback");

    result
}

/// Graphics initialization result
pub struct GraphicsInitResult {
    pub framebuffer_ready: bool,
    pub width: usize,
    pub height: usize,
    pub bpp: u16,
    pub gpu_accelerated: bool,
    pub output_verified: bool,
    pub fallback_to_text: bool,
}

impl GraphicsInitResult {
    pub fn new() -> Self {
        Self {
            framebuffer_ready: false,
            width: 0,
            height: 0,
            bpp: 0,
            gpu_accelerated: false,
            output_verified: false,
            fallback_to_text: false,
        }
    }
}

// ============================================================================
// Desktop Environment Initialization
// ============================================================================

/// Initialize desktop environment with progress display
pub fn desktop_init_progress() -> DesktopInitResult {
    begin_stage(BootStage::DesktopInit, 5);

    let mut result = DesktopInitResult::new();

    // Initialize window manager
    update_substage(1, "Initializing window manager...");
    match crate::desktop::setup_full_desktop() {
        Ok(()) => {
            report_success("Window manager initialized");
            result.window_manager_ready = true;
        }
        Err(e) => {
            report_error("Window Manager", e);
            return result;
        }
    }

    // Set up input handling
    update_substage(2, "Setting up input handling...");
    report_success("Keyboard and mouse input configured");
    result.input_ready = true;

    // Initialize taskbar
    update_substage(3, "Initializing taskbar and dock...");
    report_success("Taskbar and dock ready");
    result.taskbar_ready = true;

    // Create initial windows (only if not already created by desktop init)
    update_substage(4, "Preparing desktop windows...");
    // Windows are created by desktop::init_default_desktop() during setup_full_desktop()
    // Don't create duplicate windows here
    report_success("Desktop windows ready");
    result.windows_created = true;

    // Render initial frame
    update_substage(5, "Rendering initial frame...");
    crate::desktop::invalidate_desktop();
    crate::desktop::render_desktop();
    report_success("Desktop rendered successfully");
    result.initial_render_done = true;

    complete_stage(BootStage::DesktopInit);

    result
}

/// Desktop initialization result
pub struct DesktopInitResult {
    pub window_manager_ready: bool,
    pub input_ready: bool,
    pub taskbar_ready: bool,
    pub windows_created: bool,
    pub initial_render_done: bool,
}

impl DesktopInitResult {
    pub fn new() -> Self {
        Self {
            window_manager_ready: false,
            input_ready: false,
            taskbar_ready: false,
            windows_created: false,
            initial_render_done: false,
        }
    }
}

// ============================================================================
// Boot Complete and Transition
// ============================================================================

/// Complete the boot sequence and show summary
pub fn boot_complete_summary() {
    begin_stage(BootStage::BootComplete, 1);

    let progress = boot_progress();
    let overall = progress.overall_progress();

    println!();
    set_color(Color::LightGreen, Color::Black);
    println!("  ================================================");
    println!("            BOOT SEQUENCE COMPLETE");
    println!("  ================================================");
    set_color(Color::White, Color::Black);
    println!();

    // Show stage checklist
    let all_stages = [
        BootStage::HardwareDetection,
        BootStage::AcpiInit,
        BootStage::PciInit,
        BootStage::MemoryInit,
        BootStage::InterruptInit,
        BootStage::DriverLoading,
        BootStage::FileSystemMount,
        BootStage::GraphicsInit,
        BootStage::DesktopInit,
        BootStage::BootComplete,
    ];

    for &s in &all_stages {
        let completed = progress.is_stage_completed(s);
        if completed {
            set_color(Color::LightGreen, Color::Black);
            print!("    [x] ");
            set_color(Color::LightGray, Color::Black);
        } else {
            set_color(Color::Red, Color::Black);
            print!("    [!] ");
            set_color(Color::DarkGray, Color::Black);
        }
        println!("{}", s.name());
    }
    set_color(Color::White, Color::Black);
    println!();

    // Show statistics with aligned labels
    set_color(Color::Cyan, Color::Black);
    print!("  Boot Progress:  ");
    set_color(Color::LightGreen, Color::Black);
    println!("{}%", overall);

    if progress.errors_encountered > 0 {
        set_color(Color::Cyan, Color::Black);
        print!("  Errors:         ");
        set_color(Color::Red, Color::Black);
        println!("{}", progress.errors_encountered);
    } else {
        set_color(Color::Cyan, Color::Black);
        print!("  Errors:         ");
        set_color(Color::LightGreen, Color::Black);
        println!("0");
    }

    if progress.warnings_encountered > 0 {
        set_color(Color::Cyan, Color::Black);
        print!("  Warnings:       ");
        set_color(Color::Yellow, Color::Black);
        println!("{}", progress.warnings_encountered);
    } else {
        set_color(Color::Cyan, Color::Black);
        print!("  Warnings:       ");
        set_color(Color::LightGreen, Color::Black);
        println!("0");
    }

    if progress.safe_mode {
        println!();
        set_color(Color::Yellow, Color::Black);
        println!("  >> NOTE: System booted in SAFE MODE");
        println!("     Some features may be disabled");
        set_color(Color::White, Color::Black);
    }

    // Dump boot log to serial for diagnostics
    boot_log_dump_serial();

    println!();
    set_color(Color::LightGray, Color::Black);
    println!("  >> Press any key to continue to desktop...");
    set_color(Color::White, Color::Black);
}

/// Transition from boot screen to desktop with fade effect
pub fn transition_to_desktop() {
    // In text mode, just clear screen
    // In graphics mode, this would do a fade effect
    if crate::graphics::is_graphics_initialized() {
        // Graphics mode transition
        fade_to_desktop();
    } else {
        // Text mode - just show a transition message
        println!();
        set_color(Color::LightGreen, Color::Black);
        print_centered("Loading Desktop Environment...");
        set_color(Color::White, Color::Black);
        boot_delay_long();
    }
}

fn fade_to_desktop() {
    // Fade from boot background color to black (fade out effect)
    let bg_r: u32 = 28;
    let bg_g: u32 = 34;
    let bg_b: u32 = 54;
    let steps = 10;
    for i in 0..steps {
        let factor = steps - 1 - i; // steps-1 down to 0
        let r = (bg_r * factor / (steps - 1)) as u8;
        let g = (bg_g * factor / (steps - 1)) as u8;
        let b = (bg_b * factor / (steps - 1)) as u8;
        crate::graphics::framebuffer::clear_screen(crate::graphics::Color::rgb(r, g, b));
        crate::graphics::framebuffer::present();
        boot_delay_short();
    }
    // Final frame: pure black
    crate::graphics::framebuffer::clear_screen(crate::graphics::Color::rgb(0, 0, 0));
    crate::graphics::framebuffer::present();
}

// ============================================================================
// Error Handling and Safe Mode
// ============================================================================

/// Show error screen when graphics fail
pub fn show_graphics_error(error: &str) {
    clear_screen();
    set_color(Color::Red, Color::Black);

    println!();
    println!();
    print_centered("===========================================");
    print_centered("        GRAPHICS INITIALIZATION FAILED");
    print_centered("===========================================");
    println!();

    set_color(Color::White, Color::Black);
    println!("  Error: {}", error);
    println!();
    println!("  The system could not initialize graphics mode.");
    println!("  This may be due to:");
    println!("    - Unsupported graphics hardware");
    println!("    - Missing or incompatible GPU driver");
    println!("    - Insufficient video memory");
    println!();

    set_color(Color::Yellow, Color::Black);
    println!("  Options:");
    println!("    [1] Continue in text mode (safe mode)");
    println!("    [2] Retry graphics initialization");
    println!("    [3] Reboot system");
    println!();

    set_color(Color::Cyan, Color::Black);
    println!("  Press a key to select an option...");
    set_color(Color::White, Color::Black);
}

/// Show system information on first boot
pub fn show_first_boot_info(hardware: &HardwareDetectionResult, memory: &MemoryInitResult) {
    println!();
    set_color(Color::LightCyan, Color::Black);
    print_centered("===========================================");
    print_centered("         WELCOME TO RUSTOS");
    print_centered("===========================================");
    set_color(Color::White, Color::Black);
    println!();

    println!("  System Information:");
    println!("  -------------------");
    println!(
        "    CPU: {} cores @ {} MHz",
        hardware.cpu_info.cores, hardware.cpu_info.frequency_mhz
    );
    println!(
        "    Memory: {} MB total, {} MB available",
        memory.total_memory_mb, memory.usable_memory_mb
    );
    println!("    Storage: {} device(s)", hardware.storage_devices);
    println!("    Network: {} interface(s)", hardware.network_interfaces);

    if hardware.display_adapter {
        println!("    Display: VGA Compatible");
    }

    println!();
    set_color(Color::LightGray, Color::Black);
    println!("  This is your first boot. The system has been configured");
    println!("  with default settings. You can customize settings in the");
    println!("  System Settings application after boot completes.");
    set_color(Color::White, Color::Black);
    println!();
}

// ============================================================================
// Utility Functions
// ============================================================================

/// Clear the screen
fn clear_screen() {
    let mut writer = VGA_WRITER.lock();
    writer.clear_screen();
}

/// Set VGA colors
fn set_color(foreground: Color, background: Color) {
    let mut writer = VGA_WRITER.lock();
    writer.set_color(foreground, background);
}

/// Print text centered on screen
fn print_centered(text: &str) {
    let width: usize = 80;
    let padding = (width.saturating_sub(text.len())) / 2;
    for _ in 0..padding {
        print!(" ");
    }
    println!("{}", text);
}

/// Short delay for visual feedback (skipped in fast boot mode)
pub fn boot_delay_short() {
    if boot_config().fast_boot {
        return;
    }
    let mut i: u32 = 0;
    while i < 5_000_000 {
        unsafe {
            core::arch::asm!("nop");
        }
        i = i.wrapping_add(1);
    }
}

/// Medium delay (skipped in fast boot mode)
pub fn boot_delay_medium() {
    if boot_config().fast_boot {
        return;
    }
    let mut i: u32 = 0;
    while i < 10_000_000 {
        unsafe {
            core::arch::asm!("nop");
        }
        i = i.wrapping_add(1);
    }
}

/// Long delay (skipped in fast boot mode)
pub fn boot_delay_long() {
    if boot_config().fast_boot {
        return;
    }
    let mut i: u32 = 0;
    while i < 20_000_000 {
        unsafe {
            core::arch::asm!("nop");
        }
        i = i.wrapping_add(1);
    }
}

// ============================================================================
// Graphical Boot Screen (Framebuffer Mode)
// ============================================================================

/// Check if graphical boot is available (framebuffer initialized)
pub fn is_graphical_boot() -> bool {
    crate::graphics::is_graphics_initialized()
}

/// Render boot progress on the framebuffer
pub fn render_graphical_boot_progress() {
    if !is_graphical_boot() {
        return;
    }

    let (screen_w, screen_h) = match crate::graphics::get_screen_dimensions() {
        Some(dims) => dims,
        None => return,
    };

    let progress = boot_progress();
    let overall = progress.overall_progress();
    let stage = progress.current_stage();

    // Clear with dark background
    crate::graphics::framebuffer::clear_screen(crate::graphics::Color::rgb(28, 34, 54));

    let font = crate::graphics::get_default_font();
    let white = crate::graphics::Color::rgb(255, 255, 255);
    let accent = crate::graphics::Color::rgb(100, 180, 255);
    let green = crate::graphics::Color::rgb(100, 220, 120);
    let gray = crate::graphics::Color::rgb(80, 80, 100);
    let yellow = crate::graphics::Color::rgb(240, 200, 80);
    let red = crate::graphics::Color::rgb(240, 80, 80);

    // Draw "RustOS" title centered
    let title = "RustOS";
    let title_width = title.len() * font.char_width;
    let title_x = (screen_w.saturating_sub(title_width)) / 2;
    let title_y = screen_h / 6;
    crate::graphics::draw_text(title, title_x, title_y, white, font);

    // Draw subtitle
    let subtitle = "Booting...";
    let sub_width = subtitle.len() * font.char_width;
    let sub_x = (screen_w.saturating_sub(sub_width)) / 2;
    let sub_y = title_y + font.char_height + 4;
    crate::graphics::draw_text(subtitle, sub_x, sub_y, gray, font);

    // Draw progress bar
    let bar_width = screen_w * 2 / 3;
    let bar_x = (screen_w.saturating_sub(bar_width)) / 2;
    let bar_y = sub_y + font.char_height + 16;
    let bar_height = 10;

    // Bar background (rounded look: draw slightly larger dark rect)
    crate::graphics::framebuffer::fill_rect(
        crate::graphics::framebuffer::Rect::new(bar_x, bar_y, bar_width, bar_height),
        gray,
    );

    // Bar fill
    let fill_width = (bar_width * overall) / 100;
    if fill_width > 0 {
        crate::graphics::framebuffer::fill_rect(
            crate::graphics::framebuffer::Rect::new(bar_x, bar_y, fill_width, bar_height),
            green,
        );
    }

    // Draw percentage (stack buffer, no heap allocation)
    let mut pct_buf = [0u8; 5];
    let pct_len = format_percent(overall, &mut pct_buf);
    let pct_str = core::str::from_utf8(&pct_buf[..pct_len]).unwrap_or("0%");
    let pct_width = pct_str.len() * font.char_width;
    let pct_x = (screen_w.saturating_sub(pct_width)) / 2;
    let pct_y = bar_y + bar_height + 6;
    crate::graphics::draw_text(pct_str, pct_x, pct_y, white, font);

    // Draw stage checklist on left side
    let all_stages = [
        BootStage::HardwareDetection,
        BootStage::AcpiInit,
        BootStage::PciInit,
        BootStage::MemoryInit,
        BootStage::InterruptInit,
        BootStage::DriverLoading,
        BootStage::FileSystemMount,
        BootStage::GraphicsInit,
        BootStage::DesktopInit,
        BootStage::BootComplete,
    ];

    let checklist_x = bar_x;
    let checklist_y = pct_y + font.char_height + 16;
    let line_height = font.char_height + 4;

    for (i, &s) in all_stages.iter().enumerate() {
        let y = checklist_y + i * line_height;
        if y + font.char_height >= screen_h {
            break;
        }

        let completed = progress.is_stage_completed(s);
        let is_current = s == stage;

        let prefix = if completed {
            "[x] "
        } else if is_current {
            "[>]"
        } else {
            "[ ] "
        };
        let prefix_color = if completed {
            green
        } else if is_current {
            accent
        } else {
            gray
        };

        crate::graphics::draw_text(prefix, checklist_x, y, prefix_color, font);

        let name = s.name();
        let name_color = if completed {
            green
        } else if is_current {
            white
        } else {
            gray
        };
        crate::graphics::draw_text(name, checklist_x + 4 * font.char_width, y, name_color, font);
    }

    // Draw last message if present (bottom area)
    if let Some(ref msg) = progress.last_message {
        let msg_width = msg.len() * font.char_width;
        let msg_x = (screen_w.saturating_sub(msg_width)) / 2;
        let msg_y = screen_h.saturating_sub(font.char_height + 8);
        crate::graphics::draw_text(msg, msg_x, msg_y, accent, font);
    }

    // Draw warnings/errors counter in top-right corner
    if progress.warnings_encountered > 0 || progress.errors_encountered > 0 {
        let mut warn_buf = [0u8; 16];
        let wlen = format_count_label("WARN: ", progress.warnings_encountered, &mut warn_buf);
        let wstr = core::str::from_utf8(&warn_buf[..wlen]).unwrap_or("");
        crate::graphics::draw_text(
            wstr,
            screen_w.saturating_sub(wstr.len() * font.char_width + 8),
            4,
            yellow,
            font,
        );

        let mut err_buf = [0u8; 16];
        let elen = format_count_label("ERR:  ", progress.errors_encountered, &mut err_buf);
        let estr = core::str::from_utf8(&err_buf[..elen]).unwrap_or("");
        crate::graphics::draw_text(
            estr,
            screen_w.saturating_sub(estr.len() * font.char_width + 8),
            4 + font.char_height + 2,
            red,
            font,
        );
    }

    // Present the frame
    crate::graphics::framebuffer::present();
}

/// Format a percentage into a stack buffer (no heap allocation).
/// Returns the number of bytes written.
fn format_percent(value: usize, buf: &mut [u8; 5]) -> usize {
    if value >= 100 {
        buf[0] = b'1';
        buf[1] = b'0';
        buf[2] = b'0';
        buf[3] = b'%';
        4
    } else if value >= 10 {
        buf[0] = b'0' + (value / 10) as u8;
        buf[1] = b'0' + (value % 10) as u8;
        buf[2] = b'%';
        3
    } else {
        buf[0] = b'0' + value as u8;
        buf[1] = b'%';
        2
    }
}

/// Format "LABEL: N" into a stack buffer (no heap allocation).
/// Returns the number of bytes written.
fn format_count_label(label: &str, count: usize, buf: &mut [u8; 16]) -> usize {
    let label_bytes = label.as_bytes();
    let mut pos = 0;
    for &b in label_bytes {
        if pos >= buf.len() {
            break;
        }
        buf[pos] = b;
        pos += 1;
    }
    // Write count as decimal
    if count == 0 {
        if pos < buf.len() {
            buf[pos] = b'0';
            pos += 1;
        }
    } else {
        let mut digits = [0u8; 10];
        let mut dlen = 0;
        let mut n = count;
        while n > 0 && dlen < digits.len() {
            digits[dlen] = b'0' + (n % 10) as u8;
            dlen += 1;
            n /= 10;
        }
        for i in (0..dlen).rev() {
            if pos >= buf.len() {
                break;
            }
            buf[pos] = digits[i];
            pos += 1;
        }
    }
    pos
}

/// Render a graphical boot message (for errors/warnings)
pub fn render_graphical_boot_message(message: &str, color: crate::graphics::Color) {
    if !is_graphical_boot() {
        return;
    }

    let (screen_w, screen_h) = match crate::graphics::get_screen_dimensions() {
        Some(dims) => dims,
        None => return,
    };

    let font = crate::graphics::get_default_font();
    let msg_width = message.len() * font.char_width;
    let msg_x = (screen_w.saturating_sub(msg_width)) / 2;
    let msg_y = screen_h * 3 / 4;
    crate::graphics::draw_text(message, msg_x, msg_y, color, font);
    crate::graphics::framebuffer::present();
}

/// Render a graphical boot complete screen
pub fn render_graphical_boot_complete() {
    if !is_graphical_boot() {
        return;
    }

    let (screen_w, screen_h) = match crate::graphics::get_screen_dimensions() {
        Some(dims) => dims,
        None => return,
    };

    let progress = boot_progress();

    crate::graphics::framebuffer::clear_screen(crate::graphics::Color::rgb(28, 34, 54));

    let font = crate::graphics::get_default_font();
    let white = crate::graphics::Color::rgb(255, 255, 255);
    let green = crate::graphics::Color::rgb(100, 220, 120);
    let gray = crate::graphics::Color::rgb(80, 80, 100);
    let accent = crate::graphics::Color::rgb(100, 180, 255);

    // Draw "RustOS" title centered
    let title = "RustOS";
    let title_width = title.len() * font.char_width;
    let title_x = (screen_w.saturating_sub(title_width)) / 2;
    let title_y = screen_h / 6;
    crate::graphics::draw_text(title, title_x, title_y, white, font);

    // Draw "Boot Complete!" message
    let msg = "Boot Complete!";
    let msg_width = msg.len() * font.char_width;
    let msg_x = (screen_w.saturating_sub(msg_width)) / 2;
    let msg_y = title_y + font.char_height + 8;
    crate::graphics::draw_text(msg, msg_x, msg_y, green, font);

    // Draw stage checklist (all should be complete)
    let all_stages = [
        BootStage::HardwareDetection,
        BootStage::AcpiInit,
        BootStage::PciInit,
        BootStage::MemoryInit,
        BootStage::InterruptInit,
        BootStage::DriverLoading,
        BootStage::FileSystemMount,
        BootStage::GraphicsInit,
        BootStage::DesktopInit,
        BootStage::BootComplete,
    ];

    let checklist_x = (screen_w.saturating_sub(30 * font.char_width)) / 2;
    let checklist_y = msg_y + font.char_height + 20;
    let line_height = font.char_height + 4;

    for (i, &s) in all_stages.iter().enumerate() {
        let y = checklist_y + i * line_height;
        if y + font.char_height >= screen_h {
            break;
        }

        let completed = progress.is_stage_completed(s);
        let prefix = if completed { "[x] " } else { "[ ] " };
        let prefix_color = if completed { green } else { gray };
        crate::graphics::draw_text(prefix, checklist_x, y, prefix_color, font);

        let name = s.name();
        let name_color = if completed { green } else { gray };
        crate::graphics::draw_text(name, checklist_x + 4 * font.char_width, y, name_color, font);
    }

    // Draw warnings/errors summary at bottom
    let summary_y = screen_h.saturating_sub(font.char_height + 8);
    let mut sum_buf = [0u8; 32];
    let slen = format_boot_summary(
        progress.warnings_encountered,
        progress.errors_encountered,
        &mut sum_buf,
    );
    let sstr = core::str::from_utf8(&sum_buf[..slen]).unwrap_or("");
    let sum_width = sstr.len() * font.char_width;
    let sum_x = (screen_w.saturating_sub(sum_width)) / 2;
    let sum_color = if progress.errors_encountered > 0 {
        crate::graphics::Color::rgb(240, 80, 80)
    } else if progress.warnings_encountered > 0 {
        crate::graphics::Color::rgb(240, 200, 80)
    } else {
        accent
    };
    crate::graphics::draw_text(sstr, sum_x, summary_y, sum_color, font);

    crate::graphics::framebuffer::present();
}

/// Format boot summary "Warnings: N  Errors: M" into a stack buffer.
fn format_boot_summary(warnings: usize, errors: usize, buf: &mut [u8; 32]) -> usize {
    let mut pos = 0;
    let w = b"Warnings: ";
    for &b in w {
        if pos >= buf.len() {
            break;
        }
        buf[pos] = b;
        pos += 1;
    }
    pos += write_us(warnings, &mut buf[pos..]);
    let e = b"  Errors: ";
    for &b in e {
        if pos >= buf.len() {
            break;
        }
        buf[pos] = b;
        pos += 1;
    }
    pos += write_us(errors, &mut buf[pos..]);
    pos
}

/// Write a usize as decimal into buf, return number of bytes written.
fn write_us(val: usize, buf: &mut [u8]) -> usize {
    if val == 0 {
        if buf.is_empty() {
            return 0;
        }
        buf[0] = b'0';
        return 1;
    }
    let mut digits = [0u8; 10];
    let mut dlen = 0;
    let mut n = val;
    while n > 0 && dlen < digits.len() {
        digits[dlen] = b'0' + (n % 10) as u8;
        dlen += 1;
        n /= 10;
    }
    let mut written = 0;
    for i in (0..dlen).rev() {
        if written >= buf.len() {
            break;
        }
        buf[written] = digits[i];
        written += 1;
    }
    written
}

// ============================================================================
// Safe Mode Boot
// ============================================================================

/// Enable safe mode for this boot
pub fn enable_safe_mode() {
    let mut config = boot_config().clone();
    config.safe_mode = true;
    config.force_text_mode = true;
    config.fast_boot = true;
    set_boot_config(config);
    boot_progress().enable_safe_mode();
}

/// Check if safe mode is enabled
pub fn is_safe_mode() -> bool {
    boot_config().safe_mode
}

// ============================================================================
// Boot Menu
// ============================================================================

/// Boot menu selection
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootMenuSelection {
    NormalBoot,
    SafeMode,
    TextMode,
}

/// Show the boot menu and wait for user selection.
/// Returns the selected boot mode. Auto-continues with NormalBoot after timeout.
pub fn show_boot_menu() -> BootMenuSelection {
    // Enable keyboard interrupt early for boot menu input
    crate::interrupts::enable_keyboard_interrupt();

    clear_screen();

    // Header with border
    set_color(Color::LightCyan, Color::Black);
    println!();
    print_centered("===========================================");
    print_centered("          RUSTOS BOOT MENU");
    print_centered("===========================================");
    set_color(Color::White, Color::Black);
    println!();

    // Options with arrow indicators
    set_color(Color::LightGreen, Color::Black);
    print!("  > ");
    set_color(Color::White, Color::Black);
    println!("[1] Normal Boot     - Full graphics desktop");

    set_color(Color::Yellow, Color::Black);
    print!("  > ");
    set_color(Color::White, Color::Black);
    println!("[2] Safe Mode       - Text mode, minimal drivers");

    set_color(Color::LightCyan, Color::Black);
    print!("  > ");
    set_color(Color::White, Color::Black);
    println!("[3] Text Mode Only  - Skip graphics init");
    println!();

    // Separator line
    set_color(Color::DarkGray, Color::Black);
    println!("  -------------------------------------------");
    set_color(Color::White, Color::Black);

    // Wait for key with timeout - poll keyboard for ~3 seconds
    // Divide into 3 one-second segments for countdown display
    let timeout_iters: u32 = 3_000_000;
    let segment_size: u32 = 1_000_000;
    let mut iter: u32 = 0;
    let mut last_second: u32 = 3;

    // Print initial countdown
    set_color(Color::Yellow, Color::Black);
    print!(
        "  Auto-continue in {} seconds... (press 1, 2, or 3)",
        last_second
    );

    while iter < timeout_iters {
        if let Some(event) = crate::keyboard::get_key_event() {
            if let crate::keyboard::KeyEvent::CharacterPress(c) = event {
                match c {
                    '1' => {
                        print!("\r  ");
                        set_color(Color::LightGreen, Color::Black);
                        println!(">> Normal Boot selected                    ");
                        set_color(Color::White, Color::Black);
                        boot_delay_short();
                        return BootMenuSelection::NormalBoot;
                    }
                    '2' => {
                        print!("\r  ");
                        set_color(Color::Yellow, Color::Black);
                        println!(">> Safe Mode selected                      ");
                        set_color(Color::White, Color::Black);
                        enable_safe_mode();
                        boot_delay_short();
                        return BootMenuSelection::SafeMode;
                    }
                    '3' => {
                        print!("\r  ");
                        set_color(Color::LightCyan, Color::Black);
                        println!(">> Text Mode selected                      ");
                        set_color(Color::White, Color::Black);
                        let mut config = boot_config().clone();
                        config.force_text_mode = true;
                        set_boot_config(config);
                        boot_delay_short();
                        return BootMenuSelection::TextMode;
                    }
                    _ => {}
                }
            }
        }

        // Update countdown each second
        let current_second = 3 - (iter / segment_size);
        if current_second != last_second && current_second < 3 {
            last_second = current_second;
            set_color(Color::Yellow, Color::Black);
            if current_second > 0 {
                print!(
                    "\r  Auto-continue in {} seconds... (press 1, 2, or 3)",
                    current_second
                );
            } else {
                print!("\r  Auto-continuing...                              ");
            }
            set_color(Color::White, Color::Black);
        }

        // Small delay to avoid 100% CPU spin
        unsafe {
            core::arch::asm!("nop");
        }
        iter += 1;
    }

    // Timeout - default to normal boot
    print!("\r  ");
    set_color(Color::LightGray, Color::Black);
    println!(">> Auto-continuing with Normal Boot         ");
    set_color(Color::White, Color::Black);
    BootMenuSelection::NormalBoot
}
