//! ACPI subsystem scaffolding for RustOS
//!
//! This module stores bootloader-provided ACPI discovery information so the
//! kernel can parse ACPI tables once physical memory mappings are established.

use alloc::{collections::BTreeSet, vec::Vec};
use core::{mem, slice};
use lazy_static::lazy_static;
use spin::RwLock;

mod acpica;
pub use acpica::AcpicaError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmlEngine {
    Acpica,
    RustStaticScanner,
}

/// ACPI discovery information captured during boot
#[derive(Debug, Clone)]
pub struct AcpiInfo {
    /// Physical address of the ACPI Root System Description Pointer (RSDP)
    pub rsdp_physical: u64,
    /// Optional virtual address where the RSDP can be accessed (requires physical offset)
    pub rsdp_virtual: Option<usize>,
    /// Physical memory offset supplied by the bootloader for identity mappings
    pub physical_memory_offset: Option<u64>,
    /// Whether the full ACPI tables have been parsed and cached
    pub tables_initialized: bool,
    /// Cached system description tables discovered during enumeration
    pub tables: Option<AcpiTables>,
    /// Cached MADT information
    pub madt: Option<MadtInfo>,
    /// Cached FADT information
    pub fadt: Option<FadtInfo>,
    /// Cached MCFG information
    pub mcfg: Option<McfgInfo>,
    /// Cached HPET information
    pub hpet: Option<HpetInfo>,
}

impl AcpiInfo {
    fn new(rsdp_physical: u64, physical_memory_offset: Option<u64>) -> Result<Self, &'static str> {
        let rsdp_virtual = if let Some(offset) = physical_memory_offset {
            match offset.checked_add(rsdp_physical) {
                Some(virt) => Some(virt as usize),
                None => return Err("Physical memory offset + RSDP address overflowed"),
            }
        } else {
            None
        };

        Ok(Self {
            rsdp_physical,
            rsdp_virtual,
            physical_memory_offset,
            tables_initialized: false,
            tables: None,
            madt: None,
            fadt: None,
            mcfg: None,
            hpet: None,
        })
    }
}

/// Parsed RSDP information that downstream subsystems can use
#[derive(Debug, Clone)]
pub struct RsdpInfo {
    pub oem_id: [u8; 6],
    pub revision: u8,
    pub rsdt_address: u32,
    pub xsdt_address: Option<u64>,
}

/// Parsed ACPI system description tables
#[derive(Debug, Clone, Default)]
pub struct AcpiTables {
    pub rsdt_entries: Vec<u64>,
    pub xsdt_entries: Vec<u64>,
    pub descriptors: Vec<AcpiTableDescriptor>,
}

impl AcpiTables {
    /// Check if the ACPI tables collection is empty
    pub fn is_empty(&self) -> bool {
        self.rsdt_entries.is_empty() && self.xsdt_entries.is_empty() && self.descriptors.is_empty()
    }
}

/// Metadata for a discovered ACPI system description table
#[derive(Debug, Clone)]
pub struct AcpiTableDescriptor {
    pub signature: [u8; 4],
    pub phys_addr: u64,
    pub virt_addr: Option<usize>,
}

/// Multiprocessor APIC configuration extracted from the MADT
#[derive(Debug, Clone, Default)]
pub struct MadtInfo {
    pub local_apic_address: u32,
    pub flags: u32,
    pub io_apics: Vec<IoApic>,
    pub interrupt_overrides: Vec<InterruptOverride>,
    pub processors: Vec<ProcessorInfo>,
}

/// Processor information from MADT
#[derive(Debug, Clone)]
pub struct ProcessorInfo {
    pub processor_id: u8,
    pub apic_id: u8,
    pub flags: u32,
}

#[derive(Debug, Clone)]
pub struct IoApic {
    pub id: u8,
    pub address: u32,
    pub global_system_interrupt_base: u32,
}

#[derive(Debug, Clone)]
pub struct InterruptOverride {
    pub bus_source: u8,
    pub irq_source: u8,
    pub global_system_interrupt: u32,
    pub flags: u16,
}

/// Fixed ACPI Description Table (FADT / FACP) summary
#[derive(Debug, Clone, Default)]
pub struct FadtInfo {
    pub firmware_ctrl: Option<u32>,
    pub dsdt: Option<u32>,
    pub sci_interrupt: Option<u16>,
    pub smi_command: Option<u32>,
    pub acpi_enable: Option<u8>,
    pub acpi_disable: Option<u8>,
    pub pm1a_control_block: Option<u32>,
    pub pm1b_control_block: Option<u32>,
    pub pm_timer_block: Option<u32>,
    pub flags: Option<u32>,
    pub x_pm_timer_block: Option<u64>,
    /// SLP_TYP value for S3 read from DSDT _S3_ package; None = use spec default (3).
    pub slp_typ_s3: Option<u8>,
    /// SLP_TYP value for S4 read from DSDT _S4_ package; None = use spec default (4).
    pub slp_typ_s4: Option<u8>,
}

/// Memory Mapped Configuration (MCFG) table entry
#[derive(Debug, Clone)]
pub struct McfgEntry {
    pub base_address: u64,
    pub segment_group: u16,
    pub start_bus: u8,
    pub end_bus: u8,
}

/// MCFG table information for PCIe MMCONFIG
#[derive(Debug, Clone, Default)]
pub struct McfgInfo {
    pub entries: Vec<McfgEntry>,
}

/// HPET (High Precision Event Timer) information
#[derive(Debug, Clone)]
pub struct HpetInfo {
    pub base_address: u64,
    pub sequence_number: u16,
    pub minimum_tick: u16,
    pub page_protection: u8,
}

/// HPET table header structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct HpetTable {
    pub header: SdtHeader,
    pub event_timer_block_id: u32,
    pub base_address: u64,
    pub hpet_number: u8,
    pub minimum_tick: u16,
    pub page_protection: u8,
}

/// MADT (Multiple APIC Description Table) header structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct MadtHeader {
    pub local_apic_address: u32,
    pub flags: u32,
}

/// MADT Processor Entry structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct MadtProcessorEntry {
    entry_type: u8,
    length: u8,
    processor_id: u8,
    apic_id: u8,
    flags: u32,
}

/// MADT IO APIC Entry structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct MadtIoApicEntry {
    entry_type: u8,
    length: u8,
    id: u8,
    reserved: u8,
    address: u32,
    global_system_interrupt_base: u32,
}

/// MADT Interrupt Override Entry structure
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
struct MadtInterruptOverrideEntry {
    entry_type: u8,
    length: u8,
    bus_source: u8,
    irq_source: u8,
    global_system_interrupt: u32,
    flags: u16,
}

lazy_static! {
    static ref ACPI_STATE: RwLock<Option<AcpiInfo>> = RwLock::new(None);
}

/// Initialize the ACPI subsystem with the provided RSDP pointer
pub fn init(rsdp_physical: u64, physical_memory_offset: Option<u64>) -> Result<(), &'static str> {
    let mut state = ACPI_STATE.write();

    if state.is_some() {
        return Ok(());
    }

    let info = AcpiInfo::new(rsdp_physical, physical_memory_offset)?;
    if acpica::available() {
        acpica::initialize(rsdp_physical).map_err(|_| "ACPICA initialization failed")?;
    }

    *state = Some(info);
    Ok(())
}

/// Retrieve a snapshot of the ACPI discovery information
pub fn acpi_info() -> Option<AcpiInfo> {
    ACPI_STATE.read().clone()
}

pub fn acpica_available() -> bool {
    acpica::available()
}

pub fn aml_engine() -> AmlEngine {
    if acpica::available() {
        AmlEngine::Acpica
    } else {
        AmlEngine::RustStaticScanner
    }
}

pub fn evaluate_aml_integer(path: &[u8], method: &[u8]) -> Result<u64, AcpicaError> {
    acpica::evaluate_integer(path, method)
}

/// Mark that ACPI tables have been fully parsed
pub fn mark_tables_initialized() {
    let mut state = ACPI_STATE.write();
    if let Some(info) = state.as_mut() {
        info.tables_initialized = true;
    }
}

/// Check if the ACPI subsystem has been initialized
pub fn is_initialized() -> bool {
    ACPI_STATE.read().is_some()
}

/// Attempt to parse the RSDP structure pointed to by the bootloader
pub fn parse_rsdp() -> Result<RsdpInfo, &'static str> {
    let state = ACPI_STATE.read();
    let state = state.as_ref().ok_or("ACPI subsystem not initialized")?;

    let rsdp_addr = state
        .rsdp_virtual
        .ok_or("Physical memory offset unavailable; cannot access ACPI tables")?;

    unsafe {
        let rsdp_v1 = &*(rsdp_addr as *const RsdpDescriptorV1);

        if &rsdp_v1.signature != b"RSD PTR " {
            return Err("Invalid RSDP signature");
        }

        if !checksum_bytes(slice::from_raw_parts(
            rsdp_addr as *const u8,
            mem::size_of::<RsdpDescriptorV1>(),
        )) {
            return Err("RSDP checksum validation failed");
        }

        let revision = rsdp_v1.revision;
        let mut result = RsdpInfo {
            oem_id: rsdp_v1.oem_id,
            revision,
            rsdt_address: rsdp_v1.rsdt_address,
            xsdt_address: None,
        };

        if revision >= 2 {
            let rsdp_v2 = &*(rsdp_addr as *const RsdpDescriptorV2);

            if rsdp_v2.length as usize >= mem::size_of::<RsdpDescriptorV2>() {
                if !checksum_bytes(slice::from_raw_parts(
                    rsdp_addr as *const u8,
                    rsdp_v2.length as usize,
                )) {
                    return Err("Extended RSDP checksum validation failed");
                }

                result.xsdt_address = Some(rsdp_v2.xsdt_address);
            }
        }

        Ok(result)
    }
}

/// Enumerate the ACPI system description tables referenced by the RSDP
pub fn enumerate_system_description_tables() -> Result<AcpiTables, &'static str> {
    let rsdp = parse_rsdp()?;

    let state_guard = ACPI_STATE.read();
    let state = state_guard
        .as_ref()
        .ok_or("ACPI subsystem not initialized")?;

    let physical_offset = state
        .physical_memory_offset
        .ok_or("Physical memory offset unavailable; cannot access ACPI tables")?;

    let mut tables = AcpiTables::default();

    if rsdp.rsdt_address != 0 {
        let rsdt_entries = unsafe {
            read_sdt_entries(
                phys_to_virt(rsdp.rsdt_address as u64, physical_offset)
                    .ok_or("Failed to translate RSDT physical address")?,
                4,
            )?
        };
        tables.rsdt_entries = rsdt_entries;
    }

    if let Some(xsdt_phys) = rsdp.xsdt_address {
        if xsdt_phys != 0 {
            let xsdt_entries = unsafe {
                read_sdt_entries(
                    phys_to_virt(xsdt_phys, physical_offset)
                        .ok_or("Failed to translate XSDT physical address")?,
                    8,
                )?
            };
            tables.xsdt_entries = xsdt_entries;
        }
    }

    let mut unique_entries: BTreeSet<u64> = BTreeSet::new();
    unique_entries.extend(tables.rsdt_entries.iter().copied());
    unique_entries.extend(tables.xsdt_entries.iter().copied());

    let mut descriptors = Vec::new();

    for phys in unique_entries {
        if phys == 0 {
            continue;
        }

        let virt = phys_to_virt(phys, physical_offset)
            .ok_or("Failed to translate ACPI SDT physical address")?;

        let header = unsafe { &*(virt as *const SdtHeader) };

        if header.length as usize >= mem::size_of::<SdtHeader>() {
            // Validate table checksum before accepting it
            let table_slice =
                unsafe { slice::from_raw_parts(virt as *const u8, header.length as usize) };

            if !checksum_bytes(table_slice) {
                continue;
            }

            descriptors.push(AcpiTableDescriptor {
                signature: header.signature,
                phys_addr: phys,
                virt_addr: Some(virt),
            });
        }
    }

    tables.descriptors = descriptors;

    drop(state_guard);

    {
        let mut state_write = ACPI_STATE.write();
        if let Some(info) = state_write.as_mut() {
            info.tables = Some(tables.clone());
            info.tables_initialized = true;
        }
    }

    Ok(tables)
}

/// ACPI RSDP descriptor for revision 1.0
#[repr(C, packed)]
struct RsdpDescriptorV1 {
    signature: [u8; 8],
    checksum: u8,
    oem_id: [u8; 6],
    revision: u8,
    rsdt_address: u32,
}

/// ACPI RSDP descriptor for revision 2.0+
#[repr(C, packed)]
struct RsdpDescriptorV2 {
    v1: RsdpDescriptorV1,
    length: u32,
    xsdt_address: u64,
    extended_checksum: u8,
    reserved: [u8; 3],
}

fn checksum_bytes(bytes: &[u8]) -> bool {
    bytes.iter().fold(0u8, |acc, b| acc.wrapping_add(*b)) == 0
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct SdtHeader {
    signature: [u8; 4],
    length: u32,
    revision: u8,
    checksum: u8,
    oem_id: [u8; 6],
    oem_table_id: [u8; 8],
    oem_revision: u32,
    creator_id: u32,
    creator_revision: u32,
}

unsafe fn read_sdt_entries(virt_addr: usize, entry_size: usize) -> Result<Vec<u64>, &'static str> {
    let header = &*(virt_addr as *const SdtHeader);
    let total_length = header.length as usize;
    let header_size = mem::size_of::<SdtHeader>();

    // ponytail: firmware-provided ACPI tables are only semi-trusted. `length` drives the
    // raw slice construction below, so cap it to a sane ceiling. Real SDTs are far smaller
    // than this; the bound just prevents a bogus length from triggering a runaway read.
    const MAX_SDT_LENGTH: usize = 64 * 1024;

    if total_length < header_size {
        return Err("ACPI SDT length shorter than header");
    }

    if total_length > MAX_SDT_LENGTH {
        return Err("ACPI SDT length exceeds maximum");
    }

    let entries_length = total_length - header_size;

    if entries_length % entry_size != 0 {
        return Err("ACPI SDT entry area misaligned");
    }

    let entry_count = entries_length / entry_size;
    let entries_ptr = (virt_addr + header_size) as *const u8;
    let entries_slice = slice::from_raw_parts(entries_ptr, entries_length);

    let mut entries = Vec::with_capacity(entry_count);

    for chunk in entries_slice.chunks(entry_size) {
        let value = match entry_size {
            4 => {
                let mut array = [0u8; 4];
                array.copy_from_slice(chunk);
                u32::from_le_bytes(array) as u64
            }
            8 => {
                let mut array = [0u8; 8];
                array.copy_from_slice(chunk);
                u64::from_le_bytes(array)
            }
            _ => return Err("Unsupported SDT entry size"),
        };
        entries.push(value);
    }
    if !checksum_bytes(slice::from_raw_parts(virt_addr as *const u8, total_length)) {
        return Err("ACPI SDT checksum validation failed");
    }

    Ok(entries)
}

const MADT_ENTRY_PROCESSOR: u8 = 0;
const MADT_ENTRY_IO_APIC: u8 = 1;
const MADT_ENTRY_INTERRUPT_OVERRIDE: u8 = 2;
const MADT_PROCESSOR_LEN: usize = 8;
const MADT_IO_APIC_LEN: usize = 12;
const MADT_INTERRUPT_OVERRIDE_LEN: usize = 10;

fn phys_to_virt(phys: u64, offset: u64) -> Option<usize> {
    offset.checked_add(phys).map(|addr| addr as usize)
}

unsafe fn sdt_slice_from_virt(
    virt_addr: usize,
    expected_signature: &[u8; 4],
) -> Option<&'static [u8]> {
    let header = &*(virt_addr as *const SdtHeader);
    if &header.signature != expected_signature {
        return None;
    }

    let len = header.length as usize;
    if len < mem::size_of::<SdtHeader>() {
        return None;
    }

    let bytes = slice::from_raw_parts(virt_addr as *const u8, len);
    checksum_bytes(bytes).then_some(bytes)
}

unsafe fn sdt_slice_from_phys(
    phys_addr: u64,
    expected_signature: &[u8; 4],
) -> Option<&'static [u8]> {
    let offset = acpi_info()?.physical_memory_offset?;
    let virt = phys_to_virt(phys_addr, offset)?;
    sdt_slice_from_virt(virt, expected_signature)
}

unsafe fn sdt_slice_from_descriptor(
    descriptor: &AcpiTableDescriptor,
    expected_signature: &[u8; 4],
) -> Option<&'static [u8]> {
    let virt = descriptor.virt_addr.or_else(|| {
        acpi_info()
            .and_then(|info| info.physical_memory_offset)
            .and_then(|offset| phys_to_virt(descriptor.phys_addr, offset))
    })?;
    sdt_slice_from_virt(virt, expected_signature)
}

/// Retrieve a clone of the cached ACPI tables (if enumeration has completed)
pub fn tables() -> Option<AcpiTables> {
    ACPI_STATE.read().as_ref()?.tables.clone()
}

/// Find a specific ACPI table by its four-character signature
pub fn find_table(signature: &[u8; 4]) -> Option<AcpiTableDescriptor> {
    let state = ACPI_STATE.read();
    let info = state.as_ref()?;
    let tables = info.tables.as_ref()?;
    tables
        .descriptors
        .iter()
        .find(|desc| &desc.signature == signature)
        .cloned()
}

/// Get cached MADT information if previously parsed
pub fn madt() -> Option<MadtInfo> {
    ACPI_STATE.read().as_ref()?.madt.clone()
}

/// Get cached FADT information if previously parsed
pub fn fadt() -> Option<FadtInfo> {
    ACPI_STATE.read().as_ref()?.fadt.clone()
}

/// Get cached MCFG information if previously parsed
pub fn mcfg() -> Option<McfgInfo> {
    ACPI_STATE.read().as_ref()?.mcfg.clone()
}

/// Get cached HPET information if previously parsed
pub fn hpet() -> Option<HpetInfo> {
    ACPI_STATE.read().as_ref()?.hpet.clone()
}

/// Parse the Multiple APIC Description Table (MADT) to extract interrupt controller topology
pub fn parse_madt() -> Result<MadtInfo, &'static str> {
    let descriptor = find_table(b"APIC").ok_or("MADT (APIC) table not found")?;

    let virt = descriptor
        .virt_addr
        .or_else(|| {
            acpi_info()
                .and_then(|info| info.physical_memory_offset)
                .and_then(|offset| phys_to_virt(descriptor.phys_addr, offset))
        })
        .ok_or("Failed to map MADT virtual address")?;

    // Get the length from the MADT header
    let header = unsafe { &*(virt as *const SdtHeader) };
    let table_length = header.length as usize;

    let info = unsafe { parse_madt_from_address(virt, table_length) }?;

    {
        let mut state = ACPI_STATE.write();
        if let Some(acpi) = state.as_mut() {
            acpi.madt = Some(info.clone());
        }
    }

    Ok(info)
}

unsafe fn parse_madt_from_address(
    virt_addr: usize,
    table_length: usize,
) -> Result<MadtInfo, &'static str> {
    if table_length < mem::size_of::<SdtHeader>() + mem::size_of::<MadtHeader>() {
        return Err("MADT shorter than expected header size");
    }

    // Validate checksum
    let table_slice = slice::from_raw_parts(virt_addr as *const u8, table_length);
    if !checksum_bytes(table_slice) {
        return Err("MADT checksum validation failed");
    }

    let mut info = MadtInfo::default();

    // Skip SDT header to get to MADT-specific data
    let madt_data_start = virt_addr + mem::size_of::<SdtHeader>();

    // Read MADT header (local APIC address and flags)
    let madt_header = &*(madt_data_start as *const MadtHeader);
    info.local_apic_address = madt_header.local_apic_address;
    info.flags = madt_header.flags;

    // Parse MADT entries
    let entries_start = madt_data_start + mem::size_of::<MadtHeader>();
    let entries_length = table_length - mem::size_of::<SdtHeader>() - mem::size_of::<MadtHeader>();

    let mut offset = 0;
    while offset < entries_length {
        if offset + 2 > entries_length {
            break; // Not enough space for entry header
        }

        let entry_ptr = (entries_start + offset) as *const u8;
        let entry_type = *entry_ptr;
        let entry_length = *(entry_ptr.add(1)) as usize;

        if entry_length < 2 || offset + entry_length > entries_length {
            break; // Invalid entry length
        }

        match entry_type {
            MADT_ENTRY_PROCESSOR => {
                if entry_length >= MADT_PROCESSOR_LEN {
                    let processor_entry = &*(entry_ptr as *const MadtProcessorEntry);
                    info.processors.push(ProcessorInfo {
                        processor_id: processor_entry.processor_id,
                        apic_id: processor_entry.apic_id,
                        flags: processor_entry.flags,
                    });
                }
            }
            MADT_ENTRY_IO_APIC => {
                if entry_length >= MADT_IO_APIC_LEN {
                    let ioapic_entry = &*(entry_ptr as *const MadtIoApicEntry);
                    info.io_apics.push(IoApic {
                        id: ioapic_entry.id,
                        address: ioapic_entry.address,
                        global_system_interrupt_base: ioapic_entry.global_system_interrupt_base,
                    });
                }
            }
            MADT_ENTRY_INTERRUPT_OVERRIDE => {
                if entry_length >= MADT_INTERRUPT_OVERRIDE_LEN {
                    let override_entry = &*(entry_ptr as *const MadtInterruptOverrideEntry);
                    info.interrupt_overrides.push(InterruptOverride {
                        bus_source: override_entry.bus_source,
                        irq_source: override_entry.irq_source,
                        global_system_interrupt: override_entry.global_system_interrupt,
                        flags: override_entry.flags,
                    });
                }
            }
            _ => {
                // Unknown entry type, skip it
            }
        }

        offset += entry_length;
    }

    Ok(info)
}

/// Parse the Fixed ACPI Description Table (FADT)
pub fn parse_fadt() -> Result<FadtInfo, &'static str> {
    let descriptor = find_table(b"FACP").ok_or("FADT (FACP) table not found")?;

    let virt = descriptor
        .virt_addr
        .or_else(|| {
            acpi_info()
                .and_then(|info| info.physical_memory_offset)
                .and_then(|offset| phys_to_virt(descriptor.phys_addr, offset))
        })
        .ok_or("Failed to map FADT virtual address")?;

    // Get the length from the FADT header
    let header = unsafe { &*(virt as *const SdtHeader) };
    let table_length = header.length as usize;

    let info = unsafe { parse_fadt_from_address(virt, table_length) }?;

    {
        let mut state = ACPI_STATE.write();
        if let Some(acpi) = state.as_mut() {
            acpi.fadt = Some(info.clone());
        }
    }

    Ok(info)
}

unsafe fn parse_fadt_from_address(
    virt_addr: usize,
    table_length: usize,
) -> Result<FadtInfo, &'static str> {
    if table_length < mem::size_of::<SdtHeader>() + 44 {
        return Err("FADT shorter than minimum required size");
    }

    let table_slice = slice::from_raw_parts(virt_addr as *const u8, table_length);
    if !checksum_bytes(table_slice) {
        return Err("FADT checksum validation failed");
    }

    let mut info = FadtInfo::default();

    // Parse basic FADT fields
    info.firmware_ctrl = read_u32(table_slice, FADT_FIRMWARE_CTRL_OFFSET);
    info.dsdt = read_u32(table_slice, FADT_DSDT_OFFSET);
    info.sci_interrupt = read_u16(table_slice, FADT_SCI_INTERRUPT_OFFSET);
    info.smi_command = read_u32(table_slice, FADT_SMI_CMD_OFFSET);
    info.acpi_enable = read_u8(table_slice, FADT_ACPI_ENABLE_OFFSET);
    info.acpi_disable = read_u8(table_slice, FADT_ACPI_DISABLE_OFFSET);
    info.pm1a_control_block = read_u32(table_slice, FADT_PM1A_CONTROL_OFFSET);
    info.pm1b_control_block = read_u32(table_slice, FADT_PM1B_CONTROL_OFFSET).filter(|&v| v != 0);
    info.pm_timer_block = read_u32(table_slice, FADT_PM_TIMER_BLOCK_OFFSET);
    info.flags = read_u32(table_slice, FADT_FLAGS_OFFSET);

    // Parse extended fields if table is long enough
    if table_length >= FADT_X_PM_TIMER_BLOCK_OFFSET + 8 {
        info.x_pm_timer_block = read_u64(table_slice, FADT_X_PM_TIMER_BLOCK_OFFSET);
    }

    // Validate critical fields
    if let Some(sci_int) = info.sci_interrupt {
        if sci_int > 255 {
            return Err("Invalid SCI interrupt number in FADT");
        }
    }

    // Scan DSDT for _S3_/_S4_ sleep type packages
    if let Some(dsdt_phys) = info.dsdt {
        if let Some(mem_offset) = acpi_info().and_then(|i| i.physical_memory_offset) {
            if let Some((s3, s4)) = scan_dsdt_sleep_types(dsdt_phys, mem_offset) {
                info.slp_typ_s3 = s3;
                info.slp_typ_s4 = s4;
            }
        }
    }

    Ok(info)
}

const FADT_FIRMWARE_CTRL_OFFSET: usize = mem::size_of::<SdtHeader>();
const FADT_DSDT_OFFSET: usize = FADT_FIRMWARE_CTRL_OFFSET + 4;
const FADT_SCI_INTERRUPT_OFFSET: usize = FADT_FIRMWARE_CTRL_OFFSET + 8 + 1 + 1;
const FADT_SMI_CMD_OFFSET: usize = FADT_FIRMWARE_CTRL_OFFSET + 12;
const FADT_ACPI_ENABLE_OFFSET: usize = FADT_SMI_CMD_OFFSET + 4;
const FADT_ACPI_DISABLE_OFFSET: usize = FADT_ACPI_ENABLE_OFFSET + 1;
const FADT_PM1A_CONTROL_OFFSET: usize = FADT_FIRMWARE_CTRL_OFFSET + 24;
const FADT_PM1B_CONTROL_OFFSET: usize = FADT_FIRMWARE_CTRL_OFFSET + 28;
const FADT_PM_TIMER_BLOCK_OFFSET: usize = FADT_FIRMWARE_CTRL_OFFSET + 40;
const FADT_FLAGS_OFFSET: usize = FADT_FIRMWARE_CTRL_OFFSET + 44;
const FADT_X_PM_TIMER_BLOCK_OFFSET: usize = FADT_FIRMWARE_CTRL_OFFSET + 76;

fn read_u8(data: &[u8], offset: usize) -> Option<u8> {
    data.get(offset).copied()
}

fn read_u16(data: &[u8], offset: usize) -> Option<u16> {
    if offset + 2 <= data.len() {
        Some(u16::from_le_bytes([data[offset], data[offset + 1]]))
    } else {
        None
    }
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    if offset + 4 <= data.len() {
        Some(u32::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
        ]))
    } else {
        None
    }
}

fn read_u64(data: &[u8], offset: usize) -> Option<u64> {
    if offset + 8 <= data.len() {
        Some(u64::from_le_bytes([
            data[offset],
            data[offset + 1],
            data[offset + 2],
            data[offset + 3],
            data[offset + 4],
            data[offset + 5],
            data[offset + 6],
            data[offset + 7],
        ]))
    } else {
        None
    }
}

/// Scan the raw DSDT for `_S3_` and `_S4_` AML Name objects and extract
/// the SLP_TYP byte (first element of the Package value).
///
/// Returns `(slp_typ_s3, slp_typ_s4)` — either or both may be `None` if the
/// object is absent or malformed.  No AML interpreter is needed: the objects
/// have a fixed encoding that every known firmware uses.
fn scan_dsdt_sleep_types(dsdt_phys: u32, phys_mem_offset: u64) -> Option<(Option<u8>, Option<u8>)> {
    // Map the DSDT header to read its Length field.
    let header_virt = phys_mem_offset + dsdt_phys as u64;
    // SAFETY: phys_mem_offset maps all physical RAM; dsdt_phys comes from FADT.
    let header_bytes = unsafe {
        core::slice::from_raw_parts(header_virt as *const u8, core::mem::size_of::<SdtHeader>())
    };
    if header_bytes.len() < 8 {
        return None;
    }
    let table_len = u32::from_le_bytes([
        header_bytes[4],
        header_bytes[5],
        header_bytes[6],
        header_bytes[7],
    ]) as usize;
    if table_len < core::mem::size_of::<SdtHeader>() || table_len > 0x40_0000 {
        return None; // Sanity cap: DSDT should not exceed 4 MiB
    }

    // SAFETY: Same mapping; length validated above.
    let dsdt = unsafe { core::slice::from_raw_parts(header_virt as *const u8, table_len) };

    let mut s3: Option<u8> = None;
    let mut s4: Option<u8> = None;

    // AML encoding of Name(_S3_, Package(…){slp_typ, …}):
    //   08          NameOp
    //   5F 53 33 5F  "_S3_"
    //   12          PackageOp
    //   <PkgLength>  variable-length (1–4 bytes)
    //   <NumElems>  1 byte
    //   0A <byte>   ByteDataPrefix + SLP_TYP value   (most firmware)
    //
    // We scan for the 4-byte name and then parse the package.
    let body = &dsdt[core::mem::size_of::<SdtHeader>()..];
    let mut i = 0;
    while i + 6 < body.len() {
        let name_tag = &body[i..i + 4];
        let is_s3 = name_tag == b"\x5F\x53\x33\x5F"; // _S3_
        let is_s4 = name_tag == b"\x5F\x53\x34\x5F"; // _S4_

        if (is_s3 || is_s4) && i >= 1 && body[i - 1] == 0x08 {
            // NameOp confirmed one byte before the name string.
            // Parse PkgLength after the 4-byte name and PackageOp (0x12).
            let after_name = i + 4;
            if after_name < body.len() && body[after_name] == 0x12 {
                let pkg_start = after_name + 1;
                if pkg_start < body.len() {
                    let pkg_len_byte = body[pkg_start];
                    // PkgLength: bits [7:6] encode extra bytes (0..3).
                    let extra = ((pkg_len_byte >> 6) & 0x03) as usize;
                    let num_elems_off = pkg_start + 1 + extra;
                    let first_elem_off = num_elems_off + 1;
                    if first_elem_off + 1 < body.len() {
                        let typ_val = if body[first_elem_off] == 0x0A {
                            // ByteDataPrefix
                            Some(body[first_elem_off + 1])
                        } else if body[first_elem_off] < 0x0A {
                            // ZeroOp(0x00) or OneOp(0x01) or a small literal
                            Some(body[first_elem_off])
                        } else {
                            None
                        };
                        if is_s3 {
                            s3 = typ_val;
                        } else {
                            s4 = typ_val;
                        }
                    }
                }
            }
            i += 5; // skip past the 4-byte name + 1
        } else {
            i += 1;
        }
    }

    Some((s3, s4))
}

/// Parse the Memory Mapped Configuration (MCFG) table for PCIe MMCONFIG
pub fn parse_mcfg() -> Result<McfgInfo, &'static str> {
    let descriptor = find_table(b"MCFG").ok_or("MCFG table not found")?;

    let virt = descriptor
        .virt_addr
        .or_else(|| {
            acpi_info()
                .and_then(|info| info.physical_memory_offset)
                .and_then(|offset| phys_to_virt(descriptor.phys_addr, offset))
        })
        .ok_or("Failed to map MCFG virtual address")?;

    // Get the length from the MCFG header
    let header = unsafe { &*(virt as *const SdtHeader) };
    let table_length = header.length as usize;

    let info = unsafe { parse_mcfg_from_address(virt, table_length) }?;

    {
        let mut state = ACPI_STATE.write();
        if let Some(acpi) = state.as_mut() {
            acpi.mcfg = Some(info.clone());
        }
    }

    Ok(info)
}

unsafe fn parse_mcfg_from_address(
    virt_addr: usize,
    table_length: usize,
) -> Result<McfgInfo, &'static str> {
    if table_length < mem::size_of::<SdtHeader>() + 8 {
        return Err("MCFG table too short");
    }

    let table_slice = slice::from_raw_parts(virt_addr as *const u8, table_length);
    if !checksum_bytes(table_slice) {
        return Err("MCFG checksum validation failed");
    }

    let header_size = mem::size_of::<SdtHeader>();
    let reserved_size = 8; // 8 bytes reserved after header
    let entry_start = header_size + reserved_size;

    if table_length < entry_start {
        return Err("MCFG has no entries");
    }

    let entries_data = &table_slice[entry_start..];
    let entry_size = 16; // Each MCFG entry is 16 bytes
    let entry_count = entries_data.len() / entry_size;

    let mut entries = Vec::with_capacity(entry_count);

    for i in 0..entry_count {
        let entry_offset = i * entry_size;
        if entry_offset + entry_size > entries_data.len() {
            break;
        }

        // Parse MCFG entry - each entry is 16 bytes:
        // - 8 bytes: Base address
        // - 2 bytes: PCI segment group number
        // - 1 byte: Start bus number
        // - 1 byte: End bus number
        // - 4 bytes: Reserved
        let base_address = u64::from_le_bytes([
            entries_data[entry_offset],
            entries_data[entry_offset + 1],
            entries_data[entry_offset + 2],
            entries_data[entry_offset + 3],
            entries_data[entry_offset + 4],
            entries_data[entry_offset + 5],
            entries_data[entry_offset + 6],
            entries_data[entry_offset + 7],
        ]);

        let segment_group = u16::from_le_bytes([
            entries_data[entry_offset + 8],
            entries_data[entry_offset + 9],
        ]);

        let start_bus = entries_data[entry_offset + 10];
        let end_bus = entries_data[entry_offset + 11];

        entries.push(McfgEntry {
            base_address,
            segment_group,
            start_bus,
            end_bus,
        });
    }

    Ok(McfgInfo { entries })
}

/// Parse the HPET (High Precision Event Timer) table
pub fn parse_hpet() -> Result<HpetInfo, &'static str> {
    let descriptor = find_table(b"HPET").ok_or("HPET table not found")?;

    let virt = descriptor
        .virt_addr
        .or_else(|| {
            acpi_info()
                .and_then(|info| info.physical_memory_offset)
                .and_then(|offset| phys_to_virt(descriptor.phys_addr, offset))
        })
        .ok_or("Failed to map HPET virtual address")?;

    let hpet_table = unsafe { &*(virt as *const HpetTable) };

    // Validate table signature
    if &hpet_table.header.signature != b"HPET" {
        return Err("Invalid HPET table signature");
    }

    // Validate table length
    if hpet_table.header.length < mem::size_of::<HpetTable>() as u32 {
        return Err("HPET table too short");
    }

    // Validate base address
    if hpet_table.base_address == 0 {
        return Err("Invalid HPET base address");
    }

    let info = HpetInfo {
        base_address: hpet_table.base_address,
        sequence_number: hpet_table.hpet_number as u16,
        minimum_tick: hpet_table.minimum_tick,
        page_protection: hpet_table.page_protection,
    };

    // Cache the parsed information
    {
        let mut state = ACPI_STATE.write();
        if let Some(acpi) = state.as_mut() {
            acpi.hpet = Some(info.clone());
        }
    }

    Ok(info)
}

/// Initialize and parse all available ACPI tables
pub fn init_acpi_tables() -> Result<(), &'static str> {
    // First enumerate all system description tables
    let _tables = enumerate_system_description_tables()?;

    // Parse MADT for interrupt controller information
    if let Err(e) = parse_madt() {
        crate::serial_println!("Warning: Failed to parse MADT: {}", e);
    }

    // Parse FADT for power management information
    if let Err(e) = parse_fadt() {
        crate::serial_println!("Warning: Failed to parse FADT: {}", e);
    }

    // Parse MCFG for PCIe configuration
    if let Err(e) = parse_mcfg() {
        crate::serial_println!("Warning: Failed to parse MCFG: {}", e);
    }

    // Parse HPET for high precision timer
    if let Err(e) = parse_hpet() {
        crate::serial_println!("Warning: Failed to parse HPET: {}", e);
    }

    // Mark tables as fully initialized
    mark_tables_initialized();

    Ok(())
}

/// Validate ACPI table integrity
pub fn validate_acpi_integrity() -> Result<(), &'static str> {
    let state = ACPI_STATE.read();
    let info = state.as_ref().ok_or("ACPI not initialized")?;

    if !info.tables_initialized {
        return Err("ACPI tables not fully initialized");
    }

    let tables = info.tables.as_ref().ok_or("ACPI tables not enumerated")?;

    // Validate we have at least some basic tables
    if tables.descriptors.is_empty() {
        return Err("No ACPI tables found");
    }

    // Check for critical tables
    let has_madt = find_table(b"APIC").is_some();
    let has_fadt = find_table(b"FACP").is_some();

    if !has_fadt {
        return Err("Critical FADT table missing");
    }

    if !has_madt {
        crate::serial_println!("Warning: MADT table not found - interrupt routing may be limited");
    }

    Ok(())
}

/// Get the virtual address of a specific ACPI table by its signature
///
/// This function finds an ACPI table by its four-character signature and returns
/// its virtual address if available. This is commonly used by device drivers and
/// subsystems that need to access ACPI table data directly.
///
/// # Arguments
/// * `signature` - Four-character ACPI table signature (e.g., b"MCFG", b"APIC")
///
/// # Returns
/// * `Ok(usize)` - Virtual address of the table
/// * `Err(&'static str)` - Error message if table not found or not mapped
pub fn get_table_address(signature: &[u8; 4]) -> Result<usize, &'static str> {
    let descriptor = find_table(signature).ok_or("ACPI table not found")?;

    // Try to get virtual address first
    if let Some(virt_addr) = descriptor.virt_addr {
        return Ok(virt_addr);
    }

    // If virtual address not available, try to map it using physical memory offset
    let state = ACPI_STATE.read();
    let info = state.as_ref().ok_or("ACPI not initialized")?;
    let physical_offset = info
        .physical_memory_offset
        .ok_or("Physical memory offset not available")?;

    let virt_addr = phys_to_virt(descriptor.phys_addr, physical_offset)
        .ok_or("Failed to map ACPI table virtual address")?;

    Ok(virt_addr)
}

const AML_EXT_OP_PREFIX: u8 = 0x5B;
const AML_DEVICE_OP: u8 = 0x82;
const AML_NAME_OP: u8 = 0x08;
const AML_ROOT_PREFIX: u8 = b'\\';
const AML_PARENT_PREFIX: u8 = b'^';
const AML_NULL_NAME: u8 = 0x00;
const AML_DUAL_NAME_PREFIX: u8 = 0x2E;
const AML_MULTI_NAME_PREFIX: u8 = 0x2F;
const AML_STRING_PREFIX: u8 = 0x0D;
const AML_BYTE_PREFIX: u8 = 0x0A;
const AML_WORD_PREFIX: u8 = 0x0B;
const AML_DWORD_PREFIX: u8 = 0x0C;
const AML_QWORD_PREFIX: u8 = 0x0E;

fn parse_aml_pkg_length(data: &[u8], offset: usize) -> Option<(usize, usize)> {
    let first = *data.get(offset)?;
    let byte_count = (first >> 6) as usize;
    if offset + 1 + byte_count > data.len() {
        return None;
    }

    if byte_count == 0 {
        return Some(((first & 0x3F) as usize, 1));
    }

    let mut length = (first & 0x0F) as usize;
    for idx in 0..byte_count {
        length |= (data[offset + 1 + idx] as usize) << (4 + idx * 8);
    }
    Some((length, 1 + byte_count))
}

fn is_aml_nameseg_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_uppercase() || byte.is_ascii_digit()
}

fn append_aml_nameseg(out: &mut alloc::string::String, segment: &[u8]) -> Option<()> {
    if segment.len() != 4 || !segment.iter().all(|&byte| is_aml_nameseg_byte(byte)) {
        return None;
    }

    if !out.is_empty() && !out.ends_with('\\') {
        out.push('.');
    }
    for &byte in segment {
        out.push(byte as char);
    }
    Some(())
}

fn parse_aml_name_string(data: &[u8], mut offset: usize) -> Option<(alloc::string::String, usize)> {
    let mut out = alloc::string::String::new();

    if data.get(offset).copied() == Some(AML_ROOT_PREFIX) {
        out.push('\\');
        offset += 1;
    }

    while data.get(offset).copied() == Some(AML_PARENT_PREFIX) {
        if !out.is_empty() && !out.ends_with('\\') {
            out.push('.');
        }
        out.push('^');
        offset += 1;
    }

    match data.get(offset).copied()? {
        AML_NULL_NAME => Some((out, offset + 1)),
        AML_DUAL_NAME_PREFIX => {
            offset += 1;
            for _ in 0..2 {
                append_aml_nameseg(&mut out, data.get(offset..offset + 4)?)?;
                offset += 4;
            }
            Some((out, offset))
        }
        AML_MULTI_NAME_PREFIX => {
            let count = *data.get(offset + 1)? as usize;
            offset += 2;
            for _ in 0..count {
                append_aml_nameseg(&mut out, data.get(offset..offset + 4)?)?;
                offset += 4;
            }
            Some((out, offset))
        }
        _ => {
            append_aml_nameseg(&mut out, data.get(offset..offset + 4)?)?;
            Some((out, offset + 4))
        }
    }
}

fn parse_aml_string(data: &[u8], offset: usize) -> Option<(alloc::string::String, usize)> {
    if data.get(offset).copied()? != AML_STRING_PREFIX {
        return None;
    }

    let mut end = offset + 1;
    while end < data.len() && data[end] != 0 {
        end += 1;
    }
    if end >= data.len() {
        return None;
    }

    let mut value = alloc::string::String::new();
    for &byte in &data[offset + 1..end] {
        if !byte.is_ascii() {
            return None;
        }
        value.push(byte as char);
    }
    Some((value, end + 1))
}

fn parse_aml_integer(data: &[u8], offset: usize) -> Option<(u64, usize)> {
    match data.get(offset).copied()? {
        0x00 => Some((0, offset + 1)),
        0x01 => Some((1, offset + 1)),
        0xFF => Some((u64::MAX, offset + 1)),
        AML_BYTE_PREFIX => Some((*data.get(offset + 1)? as u64, offset + 2)),
        AML_WORD_PREFIX => {
            let bytes = data.get(offset + 1..offset + 3)?;
            Some((u16::from_le_bytes([bytes[0], bytes[1]]) as u64, offset + 3))
        }
        AML_DWORD_PREFIX => {
            let bytes = data.get(offset + 1..offset + 5)?;
            Some((
                u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as u64,
                offset + 5,
            ))
        }
        AML_QWORD_PREFIX => {
            let bytes = data.get(offset + 1..offset + 9)?;
            Some((
                u64::from_le_bytes([
                    bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
                ]),
                offset + 9,
            ))
        }
        _ => None,
    }
}

fn decode_eisa_id(encoded: u32) -> alloc::string::String {
    let c1 = (((encoded >> 26) & 0x1F) as u8 + b'@') as char;
    let c2 = (((encoded >> 21) & 0x1F) as u8 + b'@') as char;
    let c3 = (((encoded >> 16) & 0x1F) as u8 + b'@') as char;
    alloc::format!("{}{}{}{:04X}", c1, c2, c3, encoded & 0xFFFF)
}

fn parse_decimal_u32(text: &str) -> Option<u32> {
    let mut value = 0u32;
    for byte in text.bytes() {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as u32)?;
    }
    Some(value)
}

fn find_aml_name_value(data: &[u8], target: &str) -> Option<usize> {
    let mut offset = 0usize;
    while offset < data.len() {
        if data[offset] == AML_NAME_OP {
            if let Some((name, value_offset)) = parse_aml_name_string(data, offset + 1) {
                if name.ends_with(target) {
                    return Some(value_offset);
                }
                offset = value_offset;
                continue;
            }
        }
        offset += 1;
    }
    None
}

fn parse_aml_hid(data: &[u8], offset: usize) -> Option<alloc::string::String> {
    if let Some((value, _)) = parse_aml_string(data, offset) {
        return Some(value);
    }

    if data.get(offset).copied()? == AML_DWORD_PREFIX {
        let bytes = data.get(offset + 1..offset + 5)?;
        let encoded = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        return Some(decode_eisa_id(encoded));
    }

    None
}

fn parse_aml_uid(data: &[u8], offset: usize) -> Option<u32> {
    if let Some((value, _)) = parse_aml_integer(data, offset) {
        return u32::try_from(value).ok();
    }

    let (value, _) = parse_aml_string(data, offset)?;
    parse_decimal_u32(&value)
}

fn parse_aml_device(data: &[u8], op_offset: usize) -> Option<(AcpiDevice, usize)> {
    if data.get(op_offset).copied()? != AML_EXT_OP_PREFIX
        || data.get(op_offset + 1).copied()? != AML_DEVICE_OP
    {
        return None;
    }

    let pkg_offset = op_offset + 2;
    let (pkg_len, pkg_len_bytes) = parse_aml_pkg_length(data, pkg_offset)?;
    let pkg_end = core::cmp::min(pkg_offset.checked_add(pkg_len)?, data.len());
    let name_offset = pkg_offset + pkg_len_bytes;
    if name_offset >= pkg_end {
        return None;
    }

    let (name, body_offset) = parse_aml_name_string(data, name_offset)?;
    if body_offset > pkg_end {
        return None;
    }

    let body = &data[body_offset..pkg_end];
    let hid = find_aml_name_value(body, "_HID").and_then(|offset| parse_aml_hid(body, offset));
    let uid = find_aml_name_value(body, "_UID").and_then(|offset| parse_aml_uid(body, offset));

    Some((
        AcpiDevice { name, hid, uid },
        core::cmp::max(pkg_end, op_offset + 2),
    ))
}

fn scan_aml_table_devices(
    table: &[u8],
    devices: &mut Vec<AcpiDevice>,
    seen: &mut BTreeSet<alloc::string::String>,
) {
    if table.len() <= mem::size_of::<SdtHeader>() {
        return;
    }

    let aml = &table[mem::size_of::<SdtHeader>()..];
    let mut offset = 0usize;
    while offset + 1 < aml.len() {
        if aml[offset] == AML_EXT_OP_PREFIX && aml[offset + 1] == AML_DEVICE_OP {
            if let Some((device, next_offset)) = parse_aml_device(aml, offset) {
                if seen.insert(device.name.clone()) {
                    devices.push(device);
                }
                offset = next_offset;
                continue;
            }
        }
        offset += 1;
    }
}

fn append_aml_namespace_devices(devices: &mut Vec<AcpiDevice>) {
    let mut seen = BTreeSet::new();
    for device in devices.iter() {
        seen.insert(device.name.clone());
    }

    if let Some(fadt) = fadt() {
        if let Some(dsdt) = fadt.dsdt {
            if let Some(table) = unsafe { sdt_slice_from_phys(dsdt as u64, b"DSDT") } {
                scan_aml_table_devices(table, devices, &mut seen);
            }
        }
    }

    if let Some(tables) = tables() {
        for descriptor in &tables.descriptors {
            if &descriptor.signature == b"SSDT" {
                if let Some(table) = unsafe { sdt_slice_from_descriptor(descriptor, b"SSDT") } {
                    scan_aml_table_devices(table, devices, &mut seen);
                }
            }
        }
    }
}

// =============================================================================
// Wrapper functions for legacy API compatibility
// =============================================================================

/// Enumerate ACPI tables (alias for enumerate_system_description_tables)
pub fn enumerate_tables() -> Result<AcpiTables, &'static str> {
    enumerate_system_description_tables()
}

/// Enumerate ACPI devices from the MADT and other available tables.
///
/// This walks the cached MADT to extract processor (CPU) and IO-APIC
/// devices, then scans DSDT/SSDT AML for literal `DeviceOp` namespace
/// objects with `_HID` / `_UID` names. AML methods are not executed; only
/// static device objects with literal identifiers are returned. Each device
/// is given an ACPI-style name and, where applicable, a hardware ID (`hid`)
/// and unit ID (`uid`).
pub fn enumerate_devices() -> Result<Vec<AcpiDevice>, &'static str> {
    let mut devices = Vec::new();

    // Enumerate processors from the MADT.
    if let Some(madt) = madt() {
        for proc in &madt.processors {
            // The ACPI HID for a local APIC processor is "ACPI0007" (Processor),
            // but legacy MADT entries use the processor ID rather than an ACPI
            // HID. We synthesize a descriptive name and use the APIC ID as the
            // unit ID.
            let name = alloc::format!("CPU{}", proc.apic_id);
            let enabled = (proc.flags & 0x1) != 0;
            let hid = if enabled {
                Some(alloc::string::String::from("ACPI0007"))
            } else {
                // Disabled processors are still enumerated but flagged.
                Some(alloc::string::String::from("ACPI0007"))
            };
            devices.push(AcpiDevice {
                name,
                hid,
                uid: Some(proc.apic_id as u32),
            });
        }

        // Enumerate IO-APIC controllers.
        for ioapic in &madt.io_apics {
            let name = alloc::format!("IOAPIC{}", ioapic.id);
            // IO-APIC devices use "ACPI000E" (IO-APIC) as a synthetic HID.
            devices.push(AcpiDevice {
                name,
                hid: Some(alloc::string::String::from("ACPI000E")),
                uid: Some(ioapic.id as u32),
            });
        }
    }

    // If the FADT is present, enumerate the PM timer and RTC as devices.
    if fadt().is_some() {
        devices.push(AcpiDevice {
            name: alloc::string::String::from("PNP0C00"),
            hid: Some(alloc::string::String::from("PNP0C00")), // Real-time clock / AT
            uid: Some(0),
        });
    }

    append_aml_namespace_devices(&mut devices);

    Ok(devices)
}

/// An ACPI device discovered from the ACPI tables.
#[derive(Debug, Clone)]
pub struct AcpiDevice {
    pub name: alloc::string::String,
    pub hid: Option<alloc::string::String>,
    pub uid: Option<u32>,
}

/// Check if power management is available via ACPI
pub fn power_management_available() -> bool {
    // Check if FADT exists, which contains power management information
    fadt().is_some()
}

/// Check if ACPI is available and initialized
pub fn acpi_available() -> bool {
    is_initialized()
}
/// ACPI sleep state targets (S3/S4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcpiSleepState {
    SuspendToRam,
    SuspendToDisk,
}

/// Attempt ACPI sleep transition for suspend-to-RAM (S3) or suspend-to-disk (S4).
///
/// RustOS does not execute AML to resolve `_S3`/`_S4` sleep types yet. When FADT
/// PM registers are unavailable this returns `ENOSYS`; callers still receive a
/// full notifier-driven PM state machine from `power`.
pub fn enter_sleep_state(state: AcpiSleepState) -> Result<(), i32> {
    use crate::linux_compat::LinuxError;

    if !power_management_available() {
        return Err(LinuxError::ENOSYS as i32);
    }

    let fadt = match fadt() {
        Some(info) => info,
        None => return Err(LinuxError::ENOSYS as i32),
    };

    let pm1a = match fadt.pm1a_control_block {
        Some(block) if block != 0 => block as u64,
        _ => return Err(LinuxError::ENOSYS as i32),
    };

    // Use DSDT-derived SLP_TYP if available; fall back to ACPI spec defaults.
    let sleep_type = match state {
        AcpiSleepState::SuspendToRam => fadt.slp_typ_s3.map(|v| v as u16).unwrap_or(3u16),
        AcpiSleepState::SuspendToDisk => fadt.slp_typ_s4.map(|v| v as u16).unwrap_or(4u16),
    };

    // Write the sleep type and SLP_EN to the PM1a control register.
    // The PM1a control block is an I/O port address from the FADT.
    // Bits [12:10] = SLP_TYP (sleep state selector), bit [13] = SLP_EN (enable).
    // Both must be written together; SLP_EN alone triggers nothing.
    // See ACPI spec §4.7.3 "PM1 Control Registers".
    if pm1a > 0xFFFF {
        // x86 I/O ports are 16-bit; a FADT address above 0xFFFF is invalid.
        return Err(LinuxError::ENOSYS as i32);
    }
    let pm_val = (sleep_type << 10) | (1u16 << 13); // SLP_TYP[12:10] | SLP_EN[13]
    crate::serial_println!(
        "[acpi] sleep request {:?} via PM1a 0x{:x} value 0x{:04x}",
        state,
        pm1a,
        pm_val
    );

    // SAFETY: Writing to PM1 I/O ports triggers the hardware sleep transition.
    // Per ACPI §4.7.3, PM1a and PM1b must be written simultaneously (or as close
    // as possible). See docs/SAFETY.md#io-port-access.
    unsafe {
        let mut port_a = x86_64::instructions::port::Port::<u16>::new(pm1a as u16);
        port_a.write(pm_val);

        // Write PM1b if present (ACPI §4.7.3 requires both registers).
        if let Some(pm1b) = fadt.pm1b_control_block {
            let pm1b_addr = pm1b as u64;
            if pm1b_addr <= 0xFFFF {
                let mut port_b = x86_64::instructions::port::Port::<u16>::new(pm1b_addr as u16);
                port_b.write(pm_val);
            }
        }
    }

    // CPU should be halted by the hardware after the sleep transition.
    // If we reach here, the sleep didn't take effect (hardware ignored the write).
    crate::serial_println!("[acpi] sleep transition did not take effect");
    Err(LinuxError::EIO as i32)
}
