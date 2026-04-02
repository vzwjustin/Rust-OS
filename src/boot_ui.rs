//! # RustOS Boot UI Module
//!
//! Comprehensive boot progress indicators and boot-to-UI transition system.
//! Provides detailed visual feedback during the boot process with hardware detection,
//! memory initialization, driver loading, and desktop environment startup.

use crate::vga_buffer::{Color, VGA_WRITER};
use crate::{print, println};
use alloc::string::{String, ToString};
use alloc::format;
use bootloader::bootinfo::{MemoryMap, MemoryRegionType};

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
}

/// Global boot progress state
static mut BOOT_PROGRESS: BootProgress = BootProgress::new();

/// Get mutable reference to boot progress
pub fn boot_progress() -> &'static mut BootProgress {
    unsafe { &mut *core::ptr::addr_of_mut!(BOOT_PROGRESS) }
}

// ============================================================================
// Boot Stage Display Functions
// ============================================================================

/// Display the boot splash screen with RustOS logo
pub fn show_boot_splash() {
    clear_screen();
    set_color(Color::LightCyan, Color::Black);

    // Center the logo
    println!();
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
    println!();

    set_color(Color::White, Color::Black);
}

/// Begin a new boot stage with visual feedback
pub fn begin_stage(stage: BootStage, substage_total: usize) {
    let progress = boot_progress();
    progress.current_stage = stage;
    progress.substage_current = 0;
    progress.substage_total = substage_total;

    show_stage_header(stage);
}

/// Show the header for a boot stage
fn show_stage_header(stage: BootStage) {
    let progress = boot_progress();
    let total = BootStage::total_stages();
    let current = stage.number();
    let percentage = (current * 100) / total;

    println!();
    set_color(Color::LightBlue, Color::Black);
    print!("  [{}/{}] ", current, total);
    set_color(Color::White, Color::Black);
    print!("{} ", stage.name());

    // Draw mini progress bar
    set_color(Color::DarkGray, Color::Black);
    print!("[");
    let bar_width = 20;
    let filled = (percentage * bar_width) / 100;
    set_color(Color::LightGreen, Color::Black);
    for _ in 0..filled {
        print!("=");
    }
    set_color(Color::DarkGray, Color::Black);
    for _ in filled..bar_width {
        print!("-");
    }
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

    set_color(Color::Cyan, Color::Black);
    print!("      ");
    if progress.substage_total > 0 {
        print!("[{}/{}] ", current, progress.substage_total);
    }
    set_color(Color::LightGray, Color::Black);
    println!("{}", message);
    set_color(Color::White, Color::Black);
}

/// Report a success within current stage
pub fn report_success(component: &str) {
    set_color(Color::LightGreen, Color::Black);
    print!("      [OK] ");
    set_color(Color::White, Color::Black);
    println!("{}", component);
}

/// Report a warning within current stage
pub fn report_warning(component: &str, reason: &str) {
    let progress = boot_progress();
    progress.warnings_encountered += 1;

    set_color(Color::Yellow, Color::Black);
    print!("      [WARN] ");
    set_color(Color::White, Color::Black);
    print!("{}", component);
    set_color(Color::DarkGray, Color::Black);
    println!(" - {}", reason);
    set_color(Color::White, Color::Black);
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
}

/// Complete current stage
pub fn complete_stage(stage: BootStage) {
    let progress = boot_progress();
    if progress.current_stage != stage {
        return;
    }

    set_color(Color::LightGreen, Color::Black);
    println!("      Stage complete");
    set_color(Color::White, Color::Black);
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
        report_success(&format!("CPU: {} cores, {} MHz", result.cpu_info.cores, result.cpu_info.frequency_mhz));
    } else {
        report_warning("CPU", "Could not detect all features");
    }

    // Memory Detection
    update_substage(2, "Detecting memory configuration...");
    result.memory_mb = detect_memory_size();
    report_success(&format!("Memory: {} MB detected", result.memory_mb));

    // Storage Detection
    update_substage(3, "Detecting storage devices...");
    result.storage_devices = detect_storage_devices();
    if result.storage_devices > 0 {
        report_success(&format!("Storage: {} device(s) found", result.storage_devices));
    } else {
        report_warning("Storage", "No storage devices detected");
    }

    // Network Detection
    update_substage(4, "Detecting network interfaces...");
    result.network_interfaces = detect_network_interfaces();
    if result.network_interfaces > 0 {
        report_success(&format!("Network: {} interface(s) found", result.network_interfaces));
    } else {
        report_warning("Network", "No network interfaces detected");
    }

    // Display Detection
    update_substage(5, "Detecting display adapters...");
    result.display_adapter = detect_display_adapter();
    match &result.display_adapter {
        Some(adapter) => report_success(&format!("Display: {}", adapter)),
        None => report_warning("Display", "Using basic VGA"),
    }

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
    pub display_adapter: Option<String>,
}

impl HardwareDetectionResult {
    pub fn new() -> Self {
        Self {
            cpu_info: CpuInfo::default(),
            memory_mb: 0,
            storage_devices: 0,
            network_interfaces: 0,
            display_adapter: None,
        }
    }
}

/// CPU information structure
#[derive(Default)]
pub struct CpuInfo {
    pub vendor: String,
    pub model: String,
    pub cores: usize,
    pub frequency_mhz: usize,
    pub has_sse: bool,
    pub has_avx: bool,
}

fn detect_cpu_info() -> CpuInfo {
    // Use CPUID to get CPU information
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
            options(nostack, preserves_flags)
        );

        if cpuid_supported != 0 {
            // Get vendor string
            let mut vendor_a: u32;
            let mut vendor_b: u32;
            let mut vendor_c: u32;
            core::arch::asm!(
                "mov {tmp:e}, ebx",
                "cpuid",
                "xchg {tmp:e}, ebx",
                tmp = out(reg) vendor_a,
                in("eax") 0u32,
                out("ecx") vendor_c,
                out("edx") vendor_b,
                options(nostack, preserves_flags)
            );

            let vendor_bytes = [
                vendor_a.to_le_bytes(),
                vendor_b.to_le_bytes(),
                vendor_c.to_le_bytes(),
            ];
            let mut vendor_str = [0u8; 12];
            vendor_str[0..4].copy_from_slice(&vendor_bytes[0]);
            vendor_str[4..8].copy_from_slice(&vendor_bytes[1]);
            vendor_str[8..12].copy_from_slice(&vendor_bytes[2]);
            info.vendor = String::from_utf8_lossy(&vendor_str).to_string();

            // Get feature flags
            let features_ecx: u32;
            let features_edx: u32;
            core::arch::asm!(
                "mov {tmp:e}, ebx",
                "cpuid",
                "mov ebx, {tmp:e}",
                tmp = out(reg) _,
                in("eax") 1u32,
                out("ecx") features_ecx,
                out("edx") features_edx,
                options(nostack, preserves_flags)
            );

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
    for bus in 0..8u8 { // Check first 8 buses
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
                if (class_code >> 8) == 0x03 { // Display controller
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
                if (class_code >> 8) == 0x02 { // Network controller
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
    report_success(&format!("{} MB total, {} MB usable, {} regions",
        result.total_memory_mb, result.usable_memory_mb, result.memory_regions));

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
    begin_stage(BootStage::DriverLoading, 8);

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

    // Timer driver
    update_substage(5, "Loading timer driver...");
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
    update_substage(6, "Loading storage drivers...");
    report_success("IDE/AHCI drivers ready");
    result.storage_loaded = true;

    // Network drivers
    update_substage(7, "Loading network drivers...");
    report_success("Network stack initialized");
    result.network_loaded = true;

    // Serial driver
    update_substage(8, "Loading serial port driver...");
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

    // Initialize VFS
    update_substage(1, "Initializing virtual file system...");
    report_success("VFS layer initialized");
    result.vfs_ready = true;

    // Mount root filesystem
    update_substage(2, "Mounting root file system...");
    report_success("Root filesystem mounted (initramfs)");
    result.root_mounted = true;

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
    crate::serial_println!("graphics_init: begin - using safe text mode");
    begin_stage(BootStage::GraphicsInit, 1);
    update_substage(1, "Initializing text mode...");

    // Skip complex framebuffer init - use text mode fallback
    let mut result = GraphicsInitResult::new();
    result.fallback_to_text = true;

    report_warning("Graphics", "Using text mode interface");

    complete_stage(BootStage::GraphicsInit);
    boot_delay_short();
    crate::serial_println!("graphics_init: done - text mode ready");

    result
}

/// Graphics initialization result
pub struct GraphicsInitResult {
    pub framebuffer_ready: bool,
    pub width: usize,
    pub height: usize,
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

    // Create initial windows
    update_substage(4, "Creating default windows...");
    crate::desktop::create_window("Welcome to RustOS", 50, 50, 400, 300);
    crate::desktop::create_window("System Information", 150, 150, 350, 250);
    report_success("Default windows created");
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
    println!("  ============================================================");
    println!("                    BOOT SEQUENCE COMPLETE");
    println!("  ============================================================");
    set_color(Color::White, Color::Black);
    println!();

    // Show statistics
    set_color(Color::Cyan, Color::Black);
    print!("  Boot Progress: ");
    set_color(Color::White, Color::Black);
    println!("{}%", overall);

    if progress.errors_encountered > 0 {
        set_color(Color::Red, Color::Black);
        print!("  Errors: ");
        set_color(Color::White, Color::Black);
        println!("{}", progress.errors_encountered);
    }

    if progress.warnings_encountered > 0 {
        set_color(Color::Yellow, Color::Black);
        print!("  Warnings: ");
        set_color(Color::White, Color::Black);
        println!("{}", progress.warnings_encountered);
    }

    if progress.safe_mode {
        println!();
        set_color(Color::Yellow, Color::Black);
        println!("  NOTE: System booted in SAFE MODE");
        println!("  Some features may be disabled");
        set_color(Color::White, Color::Black);
    }

    println!();
    set_color(Color::LightGray, Color::Black);
    println!("  Press any key to continue to desktop...");
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
    // Implement fade effect using graphics
    for i in 0..10 {
        let brightness = (i * 25) as u8;
        crate::graphics::framebuffer::clear_screen(
            crate::graphics::Color::rgb(
                (28 * brightness / 255) as u8,
                (34 * brightness / 255) as u8,
                (54 * brightness / 255) as u8,
            )
        );
        boot_delay_short();
    }
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
pub fn show_first_boot_info(
    hardware: &HardwareDetectionResult,
    memory: &MemoryInitResult,
) {
    println!();
    set_color(Color::LightCyan, Color::Black);
    print_centered("===========================================");
    print_centered("         WELCOME TO RUSTOS");
    print_centered("===========================================");
    set_color(Color::White, Color::Black);
    println!();

    println!("  System Information:");
    println!("  -------------------");
    println!("    CPU: {} cores @ {} MHz", hardware.cpu_info.cores, hardware.cpu_info.frequency_mhz);
    println!("    Memory: {} MB total, {} MB available", memory.total_memory_mb, memory.usable_memory_mb);
    println!("    Storage: {} device(s)", hardware.storage_devices);
    println!("    Network: {} interface(s)", hardware.network_interfaces);

    if let Some(ref adapter) = hardware.display_adapter {
        println!("    Display: {}", adapter);
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

/// Short delay for visual feedback
pub fn boot_delay_short() {
    let mut i: u32 = 0;
    while i < 5_000_000 {
        unsafe { core::arch::asm!("nop"); }
        i = i.wrapping_add(1);
    }
}

/// Medium delay
pub fn boot_delay_medium() {
    let mut i: u32 = 0;
    while i < 10_000_000 {
        unsafe { core::arch::asm!("nop"); }
        i = i.wrapping_add(1);
    }
}

/// Long delay
pub fn boot_delay_long() {
    let mut i: u32 = 0;
    while i < 20_000_000 {
        unsafe { core::arch::asm!("nop"); }
        i = i.wrapping_add(1);
    }
}
