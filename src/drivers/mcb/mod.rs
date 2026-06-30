//! MEN Chameleon Bus (MCB) subsystem (mirrors Linux `drivers/mcb/`)
//!
//! Parses the Chameleon descriptor table presented by a FPGA carrier and
//! registers the IP-core devices it advertises, each with an MMIO window and
//! interrupt assignment.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct McbDevice {
    /// Chameleon device id (the IP-core type).
    device_id: u16,
    instance: u8,
    rev: u8,
    irq: u8,
    mem_base: u64,
    mem_size: u32,
}

struct McbBus {
    id: u32,
    name: String,
    revision: u8,
    devices: Vec<McbDevice>,
}

// ── Registry ──────────────────────────────────────────────────────────────

static BUSES: RwLock<BTreeMap<u32, McbBus>> = RwLock::new(BTreeMap::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_bus(name: &str, revision: u8) -> u32 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    BUSES.write().insert(
        id,
        McbBus {
            id,
            name: String::from(name),
            revision,
            devices: Vec::new(),
        },
    );
    id
}

pub fn add_device(
    bus_id: u32,
    device_id: u16,
    rev: u8,
    irq: u8,
    mem_base: u64,
    mem_size: u32,
) -> Result<u8, &'static str> {
    let mut buses = BUSES.write();
    let bus = buses.get_mut(&bus_id).ok_or("mcb: bus not found")?;
    let instance = bus
        .devices
        .iter()
        .filter(|d| d.device_id == device_id)
        .count() as u8;
    bus.devices.push(McbDevice {
        device_id,
        instance,
        rev,
        irq,
        mem_base,
        mem_size,
    });
    Ok(instance)
}

pub fn find_device(bus_id: u32, device_id: u16) -> Option<u64> {
    BUSES
        .read()
        .get(&bus_id)
        .and_then(|b| b.devices.iter().find(|d| d.device_id == device_id))
        .map(|d| d.mem_base)
}

pub fn device_count(bus_id: u32) -> usize {
    BUSES
        .read()
        .get(&bus_id)
        .map(|b| b.devices.len())
        .unwrap_or(0)
}

pub fn bus_count() -> usize {
    BUSES.read().len()
}

/// Initialize MCB with a sample Chameleon carrier and IP cores.
pub fn init() -> Result<(), &'static str> {
    if !BUSES.read().is_empty() {
        return Ok(());
    }
    let bus = register_bus("mcb-pci0", 2);
    add_device(bus, 0x0009, 1, 5, 0xF000_0000, 0x1000)?; // 16z029 CAN
    add_device(bus, 0x0023, 1, 6, 0xF000_1000, 0x1000)?; // 16z035 SRAM
    crate::serial_println!(
        "mcb: chameleon carrier rev 2, {} core(s)",
        device_count(bus)
    );
    Ok(())
}
