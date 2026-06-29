//! NVDIMM region registration with ACPI NFIT table parsing.
//!
//! On init, scans ACPI for an NFIT table; when absent, registers the
//! conventional DRAM span as node-local persistent-capable memory metadata.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use spin::RwLock;

/// NFIT table type constants (ACPI 6.x).
const NFIT_TYPE_SYSTEM_ADDRESS: u16 = 0;
const NFIT_TYPE_MEMORY_MAP: u16 = 1;

/// One persistent memory region.
#[derive(Debug, Clone)]
pub struct NvdimmRegion {
    pub handle: u32,
    pub uuid: String,
    pub phys_start: u64,
    pub size_bytes: u64,
    pub numa_node: u32,
    pub persistent: bool,
}

static REGIONS: RwLock<BTreeMap<u32, NvdimmRegion>> = RwLock::new(BTreeMap::new());
static NFIT_PRESENT: RwLock<bool> = RwLock::new(false);

/// Register an NVDIMM region in the global table.
pub fn register_region(region: NvdimmRegion) -> bool {
    let mut regions = REGIONS.write();
    if regions.contains_key(&region.handle) {
        return false;
    }
    regions.insert(region.handle, region);
    true
}

/// Lookup region by handle.
pub fn get_region(handle: u32) -> Option<NvdimmRegion> {
    REGIONS.read().get(&handle).cloned()
}

/// List all registered regions.
pub fn list_regions() -> Vec<NvdimmRegion> {
    REGIONS.read().values().cloned().collect()
}

/// Whether ACPI NFIT was discovered during boot scan.
pub fn nfit_present() -> bool {
    *NFIT_PRESENT.read()
}

/// NFIT parser: validates the ACPI table header and extracts SPA/memory-map
/// structures to build NvdimmRegion entries.
fn parse_nfit(virt: usize, length: u32) -> Vec<NvdimmRegion> {
    let mut out = Vec::new();
    if length < 40 {
        return out;
    }

    let data = unsafe { core::slice::from_raw_parts(virt as *const u8, length as usize) };
    let header_len = u32::from_le_bytes([data[4], data[5], data[6], data[7]]) as usize;
    if header_len < 40 || header_len > data.len() {
        return out;
    }

    let mut offset = header_len;
    let mut handle = 1u32;
    while offset + 4 <= data.len() {
        let struct_type = u16::from_le_bytes([data[offset], data[offset + 1]]);
        let struct_len = u16::from_le_bytes([data[offset + 2], data[offset + 3]]) as usize;
        if struct_len < 4 || offset + struct_len > data.len() {
            break;
        }

        match struct_type {
            NFIT_TYPE_SYSTEM_ADDRESS | NFIT_TYPE_MEMORY_MAP => {
                if struct_len >= 40 {
                    let addr = u64::from_le_bytes([
                        data[offset + 8],
                        data[offset + 9],
                        data[offset + 10],
                        data[offset + 11],
                        data[offset + 12],
                        data[offset + 13],
                        data[offset + 14],
                        data[offset + 15],
                    ]);
                    let size = u64::from_le_bytes([
                        data[offset + 16],
                        data[offset + 17],
                        data[offset + 18],
                        data[offset + 19],
                        data[offset + 20],
                        data[offset + 21],
                        data[offset + 22],
                        data[offset + 23],
                    ]);
                    if size > 0 {
                        out.push(NvdimmRegion {
                            handle,
                            uuid: format!("nfit-spa-{handle:04x}"),
                            phys_start: addr,
                            size_bytes: size,
                            numa_node: 0,
                            persistent: struct_type == NFIT_TYPE_SYSTEM_ADDRESS,
                        });
                        handle += 1;
                    }
                }
            }
            _ => {}
        }
        offset += struct_len;
    }
    out
}

/// Scan ACPI for NFIT and populate the region registry.
pub fn scan_acpi_nfit() -> usize {
    let Some(desc) = crate::acpi::find_table(b"NFIT") else {
        return 0;
    };

    *NFIT_PRESENT.write() = true;

    let virt = desc.virt_addr.or_else(|| {
        crate::acpi::acpi_info().and_then(|info| {
            info.physical_memory_offset
                .and_then(|off| off.checked_add(desc.phys_addr).map(|v| v as usize))
        })
    });

    let Some(virt) = virt else {
        crate::serial_println!("[nvdimm] NFIT found but not mapped");
        return 0;
    };

    let length = unsafe {
        let ptr = virt as *const u8;
        u32::from_le_bytes([*ptr.add(4), *ptr.add(5), *ptr.add(6), *ptr.add(7)])
    };

    let parsed = parse_nfit(virt, length);
    let count = parsed.len();
    for region in parsed {
        register_region(region);
    }
    count
}

fn register_fallback_dram() {
    let mem_bytes = crate::memory::get_memory_manager()
        .map(|m| m.memory_stats().total_memory as u64)
        .unwrap_or(512 * 1024 * 1024);
    register_region(NvdimmRegion {
        handle: 0,
        uuid: String::from("dram-node0"),
        phys_start: 0,
        size_bytes: mem_bytes,
        numa_node: 0,
        persistent: false,
    });
}

/// Initialize NVDIMM subsystem: NFIT scan + fallback DRAM region.
pub fn init() {
    REGIONS.write().clear();
    *NFIT_PRESENT.write() = false;

    let nfit_count = scan_acpi_nfit();
    if nfit_count == 0 {
        register_fallback_dram();
        crate::serial_println!("[nvdimm] no NFIT regions; registered fallback DRAM metadata");
    } else {
        crate::serial_println!("[nvdimm] registered {} NFIT region(s)", nfit_count);
    }
}
