//! UEFI runtime services table parsing.
//!
//! When boot firmware leaves a discoverable EFI system table (typical on UEFI
//! boots), we locate it from the bootloader memory map and parse the runtime
//! services function pointers. On legacy BIOS boots this cleanly no-ops.

extern crate alloc;

use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::RwLock;

use bootloader::bootinfo::{BootInfo, MemoryRegionType};

/// EFI system table signature (`"IBI SYST"` little-endian).
const EFI_SYSTEM_TABLE_SIGNATURE: u64 = 0x5453_5920_4942_4953;

/// Minimum supported EFI spec revision we accept (2.0).
const EFI_SPEC_MIN: u32 = (2 << 16) | 0;

/// Parsed UEFI runtime services table (x86_64 layout).
#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct EfiRuntimeServices {
    pub header: EfiTableHeader,
    pub get_time: u64,
    pub set_time: u64,
    pub get_wakeup_time: u64,
    pub set_wakeup_time: u64,
    pub set_virtual_address_map: u64,
    pub convert_pointer: u64,
    pub get_variable: u64,
    pub get_next_variable_name: u64,
    pub set_variable: u64,
    pub get_next_high_monotonic_count: u64,
    pub reset_system: u64,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct EfiTableHeader {
    pub signature: u64,
    pub revision: u32,
    pub header_size: u32,
    pub crc32: u32,
    pub reserved: u32,
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct EfiSystemTable {
    pub header: EfiTableHeader,
    pub firmware_vendor: u64,
    pub firmware_revision: u32,
    _pad0: u32,
    pub console_in_handle: u64,
    pub con_in: u64,
    pub console_out_handle: u64,
    pub con_out: u64,
    pub standard_error_handle: u64,
    pub stderr: u64,
    pub runtime_services: u64,
    pub boot_services: u64,
    pub number_of_table_entries: u64,
    pub configuration_table: u64,
}

#[derive(Debug, Clone)]
pub struct EfiFirmwareInfo {
    pub system_table_phys: u64,
    pub revision: u32,
    pub vendor: String,
    pub runtime_services_phys: u64,
    pub get_time: u64,
    pub set_virtual_address_map: u64,
    pub reset_system: u64,
}

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static AVAILABLE: AtomicBool = AtomicBool::new(false);
static INFO: RwLock<Option<EfiFirmwareInfo>> = RwLock::new(None);

/// True when a runtime services table was parsed from boot firmware.
pub fn is_available() -> bool {
    AVAILABLE.load(Ordering::Relaxed)
}

pub fn firmware_info() -> Option<EfiFirmwareInfo> {
    INFO.read().clone()
}

/// Scan boot information for a UEFI system table and parse runtime services.
pub fn init_from_boot_info(boot_info: &BootInfo) -> bool {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return is_available();
    }

    let phys_off = boot_info.physical_memory_offset;
    if phys_off == 0 {
        crate::serial_println!("[efi] no physical memory mapping — skipping UEFI parse");
        return false;
    }

    for region in boot_info.memory_map.iter() {
        let scan = matches!(
            region.region_type,
            MemoryRegionType::Reserved
                | MemoryRegionType::Bootloader
                | MemoryRegionType::AcpiNvs
                | MemoryRegionType::AcpiReclaimable
        );
        if !scan {
            continue;
        }

        let start = region.range.start_addr();
        let end = region.range.end_addr();
        if end <= start || end - start < core::mem::size_of::<EfiSystemTable>() as u64 {
            continue;
        }

        // EFI system tables are always placed near the start of a reserved
        // region by firmware.  Cap the per-region scan at 256 KiB so we do
        // not spend seconds walking multi-megabyte reserved areas on BIOS
        // boots where no EFI table exists.
        const SCAN_LIMIT: u64 = 256 * 1024;
        let scan_end = end.min(start.saturating_add(SCAN_LIMIT));

        let mut addr = start;
        while addr + core::mem::size_of::<EfiSystemTable>() as u64 <= scan_end {
            if let Some(info) = try_parse_system_table(addr, phys_off) {
                AVAILABLE.store(true, Ordering::Relaxed);
                *INFO.write() = Some(info.clone());
                crate::serial_println!(
                    "[efi] UEFI system table at {:#x} (rev {:#x}, vendor \"{}\")",
                    info.system_table_phys,
                    info.revision,
                    info.vendor
                );
                return true;
            }
            addr = addr.saturating_add(8);
        }
    }

    crate::serial_println!("[efi] no UEFI system table found (BIOS/legacy boot)");
    false
}

fn try_parse_system_table(phys: u64, phys_off: u64) -> Option<EfiFirmwareInfo> {
    let virt = phys_off + phys;
    let table = unsafe { core::ptr::read_unaligned(virt as *const EfiSystemTable) };
    if table.header.signature != EFI_SYSTEM_TABLE_SIGNATURE {
        return None;
    }
    if table.header.revision < EFI_SPEC_MIN {
        return None;
    }
    if table.runtime_services == 0 {
        return None;
    }

    let rs =
        unsafe { core::ptr::read_unaligned((table.runtime_services) as *const EfiRuntimeServices) };
    if rs.header.signature != 0 {
        // Runtime services table uses a different header signature in some firmware;
        // accept if GetTime looks like a canonical kernel pointer.
        if rs.get_time < 0xFFFF_0000_0000_0000 {
            return None;
        }
    }

    let vendor = read_uefi_string(table.firmware_vendor, phys_off);

    Some(EfiFirmwareInfo {
        system_table_phys: phys,
        revision: table.header.revision,
        vendor,
        runtime_services_phys: table.runtime_services,
        get_time: rs.get_time,
        set_virtual_address_map: rs.set_virtual_address_map,
        reset_system: rs.reset_system,
    })
}

fn read_uefi_string(wide_ptr: u64, phys_off: u64) -> String {
    if wide_ptr == 0 {
        return String::from("(unknown)");
    }
    let mut out = String::new();
    let mut offset = 0usize;
    loop {
        let ch_ptr = (wide_ptr + offset as u64) as *const u16;
        let unit = unsafe { core::ptr::read_unaligned(ch_ptr) };
        if unit == 0 {
            break;
        }
        if unit <= 0x7F {
            out.push(unit as u8 as char);
        } else {
            out.push('?');
        }
        offset += 2;
        if offset > 512 {
            break;
        }
    }
    if out.is_empty() {
        String::from("(unknown)")
    } else {
        out
    }
}

pub fn init() {
    // Boot-info parsing happens earlier in main; this marks the subsystem ready.
    if !INITIALIZED.load(Ordering::Relaxed) {
        INITIALIZED.store(true, Ordering::Relaxed);
    }
}
