//! Advanced Programmable Interrupt Controller (APIC) Support
//!
//! This module implements Local APIC and IO APIC configuration using ACPI MADT data.
//! It provides modern interrupt handling capabilities beyond the legacy PIC.

use crate::acpi::{InterruptOverride, MadtInfo};
use core::ptr;
use x86_64::VirtAddr;

/// Local APIC register offsets
#[repr(u32)]
#[derive(Debug, Clone, Copy)]
pub enum LocalApicRegister {
    Id = 0x20,
    Version = 0x30,
    TaskPriority = 0x80,
    ArbitrationPriority = 0x90,
    ProcessorPriority = 0xA0,
    EndOfInterrupt = 0xB0,
    RemoteRead = 0xC0,
    LogicalDestination = 0xD0,
    DestinationFormat = 0xE0,
    SpuriousInterruptVector = 0xF0,
    InService = 0x100,
    TriggerMode = 0x180,
    InterruptRequest = 0x200,
    ErrorStatus = 0x280,
    InterruptCommandLow = 0x300,
    InterruptCommandHigh = 0x310,
    TimerLocalVectorTable = 0x320,
    ThermalLocalVectorTable = 0x330,
    PerformanceCounterLocalVectorTable = 0x340,
    LocalInterrupt0VectorTable = 0x350,
    LocalInterrupt1VectorTable = 0x360,
    ErrorVectorTable = 0x370,
    TimerInitialCount = 0x380,
    TimerCurrentCount = 0x390,
    TimerDivideConfiguration = 0x3E0,
}

/// IO APIC register offsets
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum IoApicRegister {
    Id = 0x00,
    Version = 0x01,
    ArbitrationId = 0x02,
    RedirectionTableBase = 0x10,
}

/// Local APIC interface
pub struct LocalApic {
    base_address: VirtAddr,
}

impl LocalApic {
    /// Create a new Local APIC interface
    pub unsafe fn new(base_address: VirtAddr) -> Self {
        Self { base_address }
    }

    /// Read from a Local APIC register
    pub fn read(&self, register: LocalApicRegister) -> u32 {
        unsafe {
            let addr = self.base_address.as_u64() + register as u64;
            ptr::read_volatile(addr as *const u32)
        }
    }

    /// Write to a Local APIC register
    pub fn write(&mut self, register: LocalApicRegister, value: u32) {
        unsafe {
            let addr = self.base_address.as_u64() + register as u64;
            ptr::write_volatile(addr as *mut u32, value);
        }
    }

    /// Initialize the Local APIC
    pub fn init(&mut self) -> Result<(), &'static str> {
        // Validate APIC is present by checking version register
        let version = self.read(LocalApicRegister::Version);
        if version == 0 || version == 0xFFFFFFFF {
            return Err("Local APIC not present or not accessible");
        }

        // Clear error status register
        self.write(LocalApicRegister::ErrorStatus, 0);

        // Clear any pending interrupts
        self.write(LocalApicRegister::EndOfInterrupt, 0);

        // Set spurious interrupt vector and enable APIC
        // Use vector 255 for spurious interrupts and enable APIC (bit 8)
        self.write(LocalApicRegister::SpuriousInterruptVector, 0x1FF);

        // Set task priority to 0 (accept all interrupts)
        self.write(LocalApicRegister::TaskPriority, 0);

        // Configure timer to be masked initially
        self.write(LocalApicRegister::TimerLocalVectorTable, 0x10000); // Masked

        // Configure performance counter to be masked
        self.write(
            LocalApicRegister::PerformanceCounterLocalVectorTable,
            0x10000,
        ); // Masked

        // Configure thermal sensor to be masked
        self.write(LocalApicRegister::ThermalLocalVectorTable, 0x10000); // Masked

        // Configure LINT0 and LINT1 to be masked initially
        self.write(LocalApicRegister::LocalInterrupt0VectorTable, 0x10000); // Masked
        self.write(LocalApicRegister::LocalInterrupt1VectorTable, 0x10000); // Masked

        // Configure error interrupt
        self.write(LocalApicRegister::ErrorVectorTable, 0x10000); // Masked for now

        Ok(())
    }

    /// Send End of Interrupt signal
    pub fn end_of_interrupt(&mut self) {
        self.write(LocalApicRegister::EndOfInterrupt, 0);
    }

    /// Get Local APIC ID
    pub fn id(&self) -> u8 {
        ((self.read(LocalApicRegister::Id) >> 24) & 0xFF) as u8
    }

    /// Get Local APIC version
    pub fn version(&self) -> u32 {
        self.read(LocalApicRegister::Version)
    }
}

/// IO APIC interface
pub struct IoApic {
    base_address: VirtAddr,
    id: u8,
    gsi_base: u32,
    max_redirections: u8,
}

impl IoApic {
    /// Create a new IO APIC interface
    pub unsafe fn new(base_address: VirtAddr, id: u8, gsi_base: u32) -> Result<Self, &'static str> {
        let mut ioapic = Self {
            base_address,
            id,
            gsi_base,
            max_redirections: 0,
        };

        // Validate IO APIC is accessible
        let version = ioapic.read_register(IoApicRegister::Version);
        if version == 0 || version == 0xFFFFFFFF {
            return Err("IO APIC not accessible");
        }

        // Extract max redirection entries (bits 16-23)
        ioapic.max_redirections = ((version >> 16) & 0xFF) as u8;

        // Validate reasonable number of redirection entries
        if ioapic.max_redirections == 0 || ioapic.max_redirections > 240 {
            return Err("Invalid IO APIC redirection entry count");
        }

        // Validate and set IO APIC ID
        let current_id = (ioapic.read_register(IoApicRegister::Id) >> 24) & 0xFF;
        if current_id as u8 != id {
            // Try to set the correct ID
            ioapic.write_register(IoApicRegister::Id, (id as u32) << 24);

            // Verify the ID was set correctly
            let new_id = (ioapic.read_register(IoApicRegister::Id) >> 24) & 0xFF;
            if new_id as u8 != id {
                crate::serial_println!(
                    "Warning: IO APIC ID mismatch - expected {}, got {}",
                    id,
                    new_id
                );
            }
        }

        // Initialize all redirection entries to masked state
        ioapic.init_redirection_table()?;

        Ok(ioapic)
    }

    /// Initialize redirection table with all entries masked
    fn init_redirection_table(&mut self) -> Result<(), &'static str> {
        for irq in 0..=self.max_redirections {
            // Set entry to masked (bit 16 = 1), edge-triggered, active-high, vector 0
            let masked_entry = 0x10000u64; // Masked bit set
            self.write_redirection_entry(irq, masked_entry);
        }
        Ok(())
    }

    /// Read from IO APIC register
    fn read_register(&self, register: IoApicRegister) -> u32 {
        unsafe {
            // Write register selector
            let selector_addr = self.base_address.as_u64();
            ptr::write_volatile(selector_addr as *mut u32, register as u32);

            // Read data
            let data_addr = self.base_address.as_u64() + 0x10;
            ptr::read_volatile(data_addr as *const u32)
        }
    }

    /// Write to IO APIC register
    fn write_register(&mut self, register: IoApicRegister, value: u32) {
        unsafe {
            // Write register selector
            let selector_addr = self.base_address.as_u64();
            ptr::write_volatile(selector_addr as *mut u32, register as u32);

            // Write data
            let data_addr = self.base_address.as_u64() + 0x10;
            ptr::write_volatile(data_addr as *mut u32, value);
        }
    }

    /// Read redirection table entry
    pub fn read_redirection_entry(&self, irq: u8) -> u64 {
        if irq > self.max_redirections {
            return 0;
        }

        let reg_low = IoApicRegister::RedirectionTableBase as u32 + (irq as u32 * 2);
        let reg_high = reg_low + 1;

        unsafe {
            // Read low 32 bits
            ptr::write_volatile(self.base_address.as_u64() as *mut u32, reg_low);
            let low = ptr::read_volatile((self.base_address.as_u64() + 0x10) as *const u32);

            // Read high 32 bits
            ptr::write_volatile(self.base_address.as_u64() as *mut u32, reg_high);
            let high = ptr::read_volatile((self.base_address.as_u64() + 0x10) as *const u32);

            ((high as u64) << 32) | (low as u64)
        }
    }

    /// Write redirection table entry
    pub fn write_redirection_entry(&mut self, irq: u8, entry: u64) {
        if irq > self.max_redirections {
            return;
        }

        let reg_low = IoApicRegister::RedirectionTableBase as u32 + (irq as u32 * 2);
        let reg_high = reg_low + 1;

        let low = (entry & 0xFFFFFFFF) as u32;
        let high = ((entry >> 32) & 0xFFFFFFFF) as u32;

        unsafe {
            // Write low 32 bits
            ptr::write_volatile(self.base_address.as_u64() as *mut u32, reg_low);
            ptr::write_volatile((self.base_address.as_u64() + 0x10) as *mut u32, low);

            // Write high 32 bits
            ptr::write_volatile(self.base_address.as_u64() as *mut u32, reg_high);
            ptr::write_volatile((self.base_address.as_u64() + 0x10) as *mut u32, high);
        }
    }

    /// Get IO APIC ID
    pub fn id(&self) -> u8 {
        self.id
    }

    /// Get GSI base
    pub fn gsi_base(&self) -> u32 {
        self.gsi_base
    }

    /// Get maximum redirection entries
    pub fn max_redirections(&self) -> u8 {
        self.max_redirections
    }
}

/// APIC system manager
pub struct ApicSystem {
    local_apic: Option<LocalApic>,
    io_apics: alloc::vec::Vec<IoApic>,
    interrupt_overrides: alloc::vec::Vec<InterruptOverride>,
}

impl ApicSystem {
    /// Create a new APIC system
    pub fn new() -> Self {
        Self {
            local_apic: None,
            io_apics: alloc::vec::Vec::new(),
            interrupt_overrides: alloc::vec::Vec::new(),
        }
    }

    /// Initialize APIC system from MADT data
    pub fn init_from_madt(
        &mut self,
        madt: &MadtInfo,
        physical_offset: u64,
    ) -> Result<(), &'static str> {
        // Initialize Local APIC
        let local_apic_virt = VirtAddr::new(physical_offset + madt.local_apic_address as u64);
        let mut local_apic = unsafe { LocalApic::new(local_apic_virt) };
        local_apic.init()?;

        self.local_apic = Some(local_apic);

        // Initialize IO APICs
        for acpi_ioapic in &madt.io_apics {
            let ioapic_virt = VirtAddr::new(physical_offset + acpi_ioapic.address as u64);
            let ioapic = unsafe {
                IoApic::new(
                    ioapic_virt,
                    acpi_ioapic.id,
                    acpi_ioapic.global_system_interrupt_base,
                )?
            };

            self.io_apics.push(ioapic);
        }

        // Store interrupt overrides
        self.interrupt_overrides = madt.interrupt_overrides.clone();

        for _override_entry in &self.interrupt_overrides {}

        Ok(())
    }

    /// Send End of Interrupt to Local APIC
    pub fn end_of_interrupt(&mut self) {
        if let Some(ref mut local_apic) = self.local_apic {
            local_apic.end_of_interrupt();
        }
    }

    /// Configure interrupt routing for a specific IRQ
    pub fn configure_irq(&mut self, irq: u8, vector: u8, cpu_id: u8) -> Result<(), &'static str> {
        // Validate vector is in valid range (32-255)
        if vector < 32 {
            return Err("Interrupt vector must be >= 32");
        }

        // Check for interrupt overrides first
        let (gsi, flags) = self.resolve_irq_to_gsi(irq);

        // Find the appropriate IO APIC for this GSI
        // max_redirections() returns the maximum redirection entry index (0-based)
        // So we need to add 1 to get the count of entries
        let ioapic = self
            .io_apics
            .iter_mut()
            .find(|ioapic| {
                gsi >= ioapic.gsi_base()
                    && gsi <= ioapic.gsi_base() + ioapic.max_redirections() as u32
            })
            .ok_or("No IO APIC found for GSI")?;

        let local_irq = (gsi - ioapic.gsi_base()) as u8;

        // Validate local IRQ is within range
        if local_irq > ioapic.max_redirections() {
            return Err("IRQ exceeds IO APIC redirection table size");
        }

        // Build redirection entry
        let mut entry = vector as u64;

        // Set destination mode to physical (bit 11 = 0) and destination CPU
        entry |= (cpu_id as u64) << 56; // Destination CPU in bits 56-63

        // Set delivery mode to fixed (bits 8-10 = 000)
        // entry |= 0 << 8; // Fixed delivery mode (default)

        // Apply polarity and trigger mode from interrupt override flags
        if flags & 0x02 != 0 {
            // Active low polarity
            entry |= 1 << 13;
        }
        // Default is active high (bit 13 = 0)

        if flags & 0x08 != 0 {
            // Level triggered
            entry |= 1 << 15;
        }
        // Default is edge triggered (bit 15 = 0)

        // Ensure interrupt is not masked (bit 16 = 0)
        // entry &= !(1 << 16); // Already 0 by default

        // Write the redirection entry
        ioapic.write_redirection_entry(local_irq, entry);

        crate::serial_println!(
            "Configured IRQ {} -> GSI {} -> Vector {} on CPU {}",
            irq,
            gsi,
            vector,
            cpu_id
        );

        Ok(())
    }

    /// Resolve IRQ to GSI using interrupt overrides
    fn resolve_irq_to_gsi(&self, irq: u8) -> (u32, u16) {
        for override_entry in &self.interrupt_overrides {
            if override_entry.irq_source == irq {
                return (override_entry.global_system_interrupt, override_entry.flags);
            }
        }
        // Default mapping: IRQ == GSI
        (irq as u32, 0)
    }

    /// Check if APIC system is initialized
    pub fn is_initialized(&self) -> bool {
        self.local_apic.is_some() && !self.io_apics.is_empty()
    }
}

use lazy_static::lazy_static;
use spin::Mutex;

lazy_static! {
    static ref APIC_SYSTEM: Mutex<ApicSystem> = Mutex::new(ApicSystem::new());
}

/// Initialize the global APIC system
pub fn init_apic_system() -> Result<(), &'static str> {
    let madt = crate::acpi::madt().ok_or("MADT not available")?;
    let acpi_info = crate::acpi::acpi_info().ok_or("ACPI not initialized")?;
    let physical_offset = acpi_info
        .physical_memory_offset
        .ok_or("Physical memory offset not available")?;

    let mut apic_system = APIC_SYSTEM.lock();
    apic_system.init_from_madt(&madt, physical_offset)?;

    Ok(())
}

/// Get reference to the global APIC system
pub fn apic_system() -> &'static Mutex<ApicSystem> {
    &APIC_SYSTEM
}

/// Send End of Interrupt to the APIC system
pub fn end_of_interrupt() {
    APIC_SYSTEM.lock().end_of_interrupt();
}

/// Configure an IRQ with the APIC system
pub fn configure_irq(irq: u8, vector: u8, cpu_id: u8) -> Result<(), &'static str> {
    APIC_SYSTEM.lock().configure_irq(irq, vector, cpu_id)
}

/// Check if APIC is available and initialized
pub fn is_apic_available() -> bool {
    APIC_SYSTEM.lock().is_initialized()
}

// =============================================================================
// Wrapper functions for legacy API compatibility
// =============================================================================

/// Alias for is_apic_available - checks if local APIC is available
pub fn local_apic_available() -> bool {
    is_apic_available()
}

/// Alias for is_apic_available - checks if I/O APIC is available
pub fn io_apic_available() -> bool {
    is_apic_available()
}

/// Alias for init_apic_system
pub fn init_apic() -> Result<(), &'static str> {
    init_apic_system()
}

/// Get a reference to the local APIC (returns the APIC system)
pub fn get_local_apic() -> Option<&'static Mutex<ApicSystem>> {
    if is_apic_available() {
        Some(&APIC_SYSTEM)
    } else {
        None
    }
}
