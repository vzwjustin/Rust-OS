//! Thermal zone and cooling device framework
//!
//! Registers thermal zones with trip points and cooling devices. Integrates with
//! ACPI when available (DSDT thermal zone name discovery) and provides an
//! IA32_THERM_STATUS-backed CPU zone as a software sensor.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalTripType {
    Hot,
    Passive,
    Active,
    Critical,
}

#[derive(Debug, Clone, Copy)]
pub struct ThermalTripPoint {
    pub trip_type: ThermalTripType,
    pub temp_celsius: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoolingState {
    Off,
    Low,
    Medium,
    High,
    Max,
}

pub struct ThermalZoneOps {
    pub get_temp: fn() -> Result<i32, &'static str>,
    pub get_name: fn() -> &'static str,
}

pub struct CoolingDeviceOps {
    pub set_state: fn(CoolingState) -> Result<(), &'static str>,
    pub get_state: fn() -> CoolingState,
    pub get_max_state: fn() -> u8,
    pub get_name: fn() -> &'static str,
}

struct ThermalZone {
    id: u32,
    name: String,
    trips: Vec<ThermalTripPoint>,
    ops: ThermalZoneOps,
    last_temp: i32,
}

struct CoolingDevice {
    id: u32,
    name: String,
    zone_id: Option<u32>,
    ops: CoolingDeviceOps,
    current_state: CoolingState,
}

// ── MSR CPU temperature (software sensor) ───────────────────────────────

#[inline]
unsafe fn rdmsr(msr: u32) -> u64 {
    let low: u32;
    let high: u32;
    core::arch::asm!(
        "rdmsr",
        in("ecx") msr,
        lateout("eax") low,
        lateout("edx") high,
        options(nomem, nostack, preserves_flags)
    );
    ((high as u64) << 32) | (low as u64)
}

fn read_cpu_msr_temp() -> Result<i32, &'static str> {
    const MSR_THERM_STATUS: u32 = 0x19C;
    const MSR_TEMP_TARGET: u32 = 0x1A2;
    const TJMAX_FALLBACK: i32 = 100;

    let therm_status = unsafe { rdmsr(MSR_THERM_STATUS) };
    if (therm_status >> 31) & 1 == 0 {
        return Err("CPU thermal MSR not valid");
    }

    let digital_reading = ((therm_status >> 16) & 0x7F) as i32;
    if digital_reading == 0 {
        return Ok(TJMAX_FALLBACK);
    }

    let tjmax = match unsafe { rdmsr(MSR_TEMP_TARGET) } {
        target if target != 0 => {
            let t = ((target >> 16) & 0xFF) as i32;
            if (70..=130).contains(&t) {
                t
            } else {
                TJMAX_FALLBACK
            }
        }
        _ => TJMAX_FALLBACK,
    };

    Ok(tjmax - digital_reading)
}

fn cpu_zone_get_temp() -> Result<i32, &'static str> {
    read_cpu_msr_temp()
}

fn cpu_zone_name() -> &'static str {
    "acpi-cpu"
}

const CPU_ZONE_OPS: ThermalZoneOps = ThermalZoneOps {
    get_temp: cpu_zone_get_temp,
    get_name: cpu_zone_name,
};

// ── Processor cooling device ────────────────────────────────────────────

static mut PROCESSOR_COOLING_STATE: CoolingState = CoolingState::Off;

fn processor_set_state(state: CoolingState) -> Result<(), &'static str> {
    unsafe {
        PROCESSOR_COOLING_STATE = state;
    }
    crate::serial_println!("thermal: processor cooling -> {:?}", state);
    Ok(())
}

fn processor_get_state() -> CoolingState {
    unsafe { PROCESSOR_COOLING_STATE }
}

fn processor_max_state() -> u8 {
    4
}

fn processor_cooling_name() -> &'static str {
    "processor-cooling"
}

const PROCESSOR_COOLING_OPS: CoolingDeviceOps = CoolingDeviceOps {
    set_state: processor_set_state,
    get_state: processor_get_state,
    get_max_state: processor_max_state,
    get_name: processor_cooling_name,
};

// ── ACPI DSDT thermal zone discovery ────────────────────────────────────

fn scan_dsdt_for_thermal_zones() -> Vec<String> {
    let mut zones = Vec::new();

    if !crate::acpi::acpi_available() {
        return zones;
    }

    let fadt = match crate::acpi::fadt() {
        Some(f) => f,
        None => {
            let _ = crate::acpi::parse_fadt();
            crate::acpi::fadt().unwrap_or_default()
        }
    };

    let dsdt_phys = match fadt.dsdt {
        Some(addr) if addr != 0 => addr as u64,
        _ => return zones,
    };

    let info = match crate::acpi::acpi_info() {
        Some(i) => i,
        None => return zones,
    };
    let offset = match info.physical_memory_offset {
        Some(o) => o,
        None => return zones,
    };

    let virt = match offset.checked_add(dsdt_phys) {
        Some(v) => v as usize,
        None => return zones,
    };

    // Read DSDT length from SDT header (offset 4, u32 LE).
    let header_slice = unsafe { core::slice::from_raw_parts(virt as *const u8, 36) };
    let length = u32::from_le_bytes([
        header_slice[4],
        header_slice[5],
        header_slice[6],
        header_slice[7],
    ]) as usize;
    if length < 36 || length > 256 * 1024 {
        return zones;
    }

    let table = unsafe { core::slice::from_raw_parts(virt as *const u8, length) };

    // Scan for AML NameSeg patterns like "TZ00", "TZ01" (Thermal Zone devices).
    let mut i = 0usize;
    while i + 4 <= table.len() {
        if table[i] == b'T' && table[i + 1] == b'Z' && table[i + 2].is_ascii_digit() {
            let name = alloc::format!(
                "TZ{}{}",
                table[i + 2] as char,
                if i + 3 < table.len() && table[i + 3].is_ascii_digit() {
                    alloc::format!("{}", table[i + 3] as char)
                } else {
                    String::new()
                }
            );
            if !zones.iter().any(|z: &String| z == &name) {
                zones.push(name);
            }
        }
        i += 1;
    }

    zones
}

fn dsdt_zone_get_temp() -> Result<i32, &'static str> {
    // DSDT zones without AML _TMP evaluation fall back to CPU MSR sensor.
    read_cpu_msr_temp()
}

// ── Registry ────────────────────────────────────────────────────────────

static THERMAL_ZONES: RwLock<BTreeMap<u32, ThermalZone>> = RwLock::new(BTreeMap::new());
static COOLING_DEVICES: RwLock<BTreeMap<u32, CoolingDevice>> = RwLock::new(BTreeMap::new());
static NEXT_ZONE_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_COOLING_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_zone(
    name: &str,
    trips: &[ThermalTripPoint],
    ops: ThermalZoneOps,
) -> Result<u32, &'static str> {
    let id = NEXT_ZONE_ID.fetch_add(1, Ordering::SeqCst);
    THERMAL_ZONES.write().insert(
        id,
        ThermalZone {
            id,
            name: String::from(name),
            trips: trips.to_vec(),
            ops,
            last_temp: 0,
        },
    );
    Ok(id)
}

pub fn register_cooling_device(
    name: &str,
    zone_id: Option<u32>,
    ops: CoolingDeviceOps,
) -> Result<u32, &'static str> {
    if let Some(zid) = zone_id {
        if !THERMAL_ZONES.read().contains_key(&zid) {
            return Err("Thermal zone not found for cooling device");
        }
    }

    let id = NEXT_COOLING_ID.fetch_add(1, Ordering::SeqCst);
    COOLING_DEVICES.write().insert(
        id,
        CoolingDevice {
            id,
            name: String::from(name),
            zone_id,
            ops,
            current_state: CoolingState::Off,
        },
    );
    Ok(id)
}

pub fn read_zone_temp(zone_id: u32) -> Result<i32, &'static str> {
    let mut zones = THERMAL_ZONES.write();
    let zone = zones.get_mut(&zone_id).ok_or("Thermal zone not found")?;
    let temp = (zone.ops.get_temp)()?;
    zone.last_temp = temp;
    Ok(temp)
}

pub fn poll_thermal() {
    let zone_ids: Vec<u32> = THERMAL_ZONES.read().keys().copied().collect();
    for zone_id in zone_ids {
        if let Ok(temp) = read_zone_temp(zone_id) {
            apply_thermal_policy(zone_id, temp);
        }
    }
}

fn apply_thermal_policy(zone_id: u32, temp: i32) {
    let zones = THERMAL_ZONES.read();
    let zone = match zones.get(&zone_id) {
        Some(z) => z,
        None => return,
    };

    let mut target_state = CoolingState::Off;
    for trip in &zone.trips {
        if temp >= trip.temp_celsius {
            target_state = match trip.trip_type {
                ThermalTripType::Critical | ThermalTripType::Hot => CoolingState::Max,
                ThermalTripType::Passive => CoolingState::Medium,
                ThermalTripType::Active => CoolingState::High,
            };
        }
    }
    drop(zones);

    let cooling_ids: Vec<u32> = COOLING_DEVICES
        .read()
        .iter()
        .filter(|(_, d)| d.zone_id == Some(zone_id))
        .map(|(id, _)| *id)
        .collect();

    for cid in cooling_ids {
        let _ = set_cooling_state(cid, target_state);
    }
}

pub fn set_cooling_state(device_id: u32, state: CoolingState) -> Result<(), &'static str> {
    let mut devices = COOLING_DEVICES.write();
    let dev = devices
        .get_mut(&device_id)
        .ok_or("Cooling device not found")?;
    (dev.ops.set_state)(state)?;
    dev.current_state = state;
    Ok(())
}

pub fn zone_count() -> usize {
    THERMAL_ZONES.read().len()
}

pub fn cooling_device_count() -> usize {
    COOLING_DEVICES.read().len()
}

fn register_acpi_zones() -> Result<(), &'static str> {
    let default_trips = [
        ThermalTripPoint {
            trip_type: ThermalTripType::Passive,
            temp_celsius: 70,
        },
        ThermalTripPoint {
            trip_type: ThermalTripType::Active,
            temp_celsius: 85,
        },
        ThermalTripPoint {
            trip_type: ThermalTripType::Critical,
            temp_celsius: 95,
        },
    ];

    // Always register CPU MSR zone when ACPI is available or as platform fallback.
    let cpu_id = register_zone("acpi-cpu", &default_trips, CPU_ZONE_OPS)?;

    register_cooling_device("processor-cooling", Some(cpu_id), PROCESSOR_COOLING_OPS)?;

    if crate::acpi::acpi_available() {
        let dsdt_zones = scan_dsdt_for_thermal_zones();
        for name in dsdt_zones {
            let trips = default_trips;
            let zone_name = name.clone();
            // Each DSDT zone uses MSR fallback until AML _TMP is evaluated.
            let _ = register_zone(
                &zone_name,
                &trips,
                ThermalZoneOps {
                    get_temp: dsdt_zone_get_temp,
                    get_name: || "dsdt-thermal",
                },
            );
            crate::serial_println!("thermal: registered DSDT zone {}", zone_name);
        }
    }

    Ok(())
}

/// Initialize thermal subsystem and register ACPI/MSR-backed zones.
pub fn init() -> Result<(), &'static str> {
    if !THERMAL_ZONES.read().is_empty() {
        return Ok(());
    }

    register_acpi_zones()?;

    // Perform an initial temperature poll.
    poll_thermal();

    crate::serial_println!(
        "thermal: {} zone(s), {} cooling device(s)",
        zone_count(),
        cooling_device_count()
    );
    Ok(())
}
