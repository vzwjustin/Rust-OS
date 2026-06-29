//! OPP (Operating Performance Points) subsystem
//!
//! Provides CPU/device performance state management with frequency/voltage
//! pairs, dynamic voltage scaling, and transition control. Mirrors Linux's
//! `drivers/opp/opp.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// A single Operating Performance Point (Linux `struct dev_pm_opp`).
#[derive(Debug, Clone)]
pub struct Opp {
    pub frequency_hz: u64,
    pub voltage_uv: u32, // microvolts
    pub level: u32,      // performance level
    pub turbo: bool,
    pub suspend: bool,
}

/// OPP table for a device (Linux `struct opp_table`).
struct OppTable {
    id: u32,
    dev_name: String,
    opps: Vec<Opp>,
    current_opp: Option<usize>,
    enabled: bool,
    shared: bool,
}

// ── Default CPU OPP table ───────────────────────────────────────────────

fn default_cpu_opps() -> Vec<Opp> {
    let mut opps = Vec::new();
    opps.push(Opp {
        frequency_hz: 800_000_000,
        voltage_uv: 900_000,
        level: 0,
        turbo: false,
        suspend: false,
    });
    opps.push(Opp {
        frequency_hz: 1_600_000_000,
        voltage_uv: 1_000_000,
        level: 1,
        turbo: false,
        suspend: false,
    });
    opps.push(Opp {
        frequency_hz: 2_400_000_000,
        voltage_uv: 1_100_000,
        level: 2,
        turbo: false,
        suspend: false,
    });
    opps.push(Opp {
        frequency_hz: 3_200_000_000,
        voltage_uv: 1_200_000,
        level: 3,
        turbo: false,
        suspend: false,
    });
    opps.push(Opp {
        frequency_hz: 3_600_000_000,
        voltage_uv: 1_300_000,
        level: 4,
        turbo: true,
        suspend: false,
    });
    opps
}

// ── Registry ────────────────────────────────────────────────────────────

static OPP_TABLES: RwLock<BTreeMap<u32, OppTable>> = RwLock::new(BTreeMap::new());
static NEXT_TABLE_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register an OPP table for a device (Linux `dev_pm_opp_of_add_table`).
pub fn register_table(dev_name: &str, opps: &[Opp]) -> Result<u32, &'static str> {
    if opps.is_empty() {
        return Err("OPP table must contain at least one OPP");
    }
    // Sort by frequency ascending.
    let mut sorted = opps.to_vec();
    sorted.sort_by_key(|o| o.frequency_hz);

    let id = NEXT_TABLE_ID.fetch_add(1, Ordering::SeqCst);
    OPP_TABLES.write().insert(
        id,
        OppTable {
            id,
            dev_name: String::from(dev_name),
            opps: sorted,
            current_opp: Some(0),
            enabled: true,
            shared: false,
        },
    );
    Ok(id)
}

/// Get all OPPs for a device (Linux `dev_pm_opp_get_opp_count`).
pub fn get_opps(table_id: u32) -> Result<Vec<Opp>, &'static str> {
    let tables = OPP_TABLES.read();
    let table = tables.get(&table_id).ok_or("OPP table not found")?;
    Ok(table.opps.clone())
}

/// Get the current OPP (Linux `dev_pm_opp_get_current`).
pub fn get_current_opp(table_id: u32) -> Result<Opp, &'static str> {
    let tables = OPP_TABLES.read();
    let table = tables.get(&table_id).ok_or("OPP table not found")?;
    let idx = table.current_opp.ok_or("No current OPP set")?;
    Ok(table.opps[idx].clone())
}

/// Set the target frequency (Linux `dev_pm_opp_set_rate`).
pub fn set_rate(table_id: u32, target_hz: u64) -> Result<Opp, &'static str> {
    let mut tables = OPP_TABLES.write();
    let table = tables.get_mut(&table_id).ok_or("OPP table not found")?;
    if !table.enabled {
        return Err("OPP table is disabled");
    }

    // Find the OPP closest to (but not exceeding) the target frequency.
    let mut best_idx = 0;
    for (i, opp) in table.opps.iter().enumerate() {
        if opp.frequency_hz <= target_hz {
            best_idx = i;
        } else {
            break;
        }
    }

    // Skip turbo OPPs unless target explicitly requests them.
    if !table.opps[best_idx].turbo || target_hz >= table.opps[best_idx].frequency_hz {
        table.current_opp = Some(best_idx);
    }

    Ok(table.opps[best_idx].clone())
}

/// Set the OPP by performance level (Linux `dev_pm_opp_set_opp`).
pub fn set_opp(table_id: u32, level: u32) -> Result<Opp, &'static str> {
    let mut tables = OPP_TABLES.write();
    let table = tables.get_mut(&table_id).ok_or("OPP table not found")?;
    if !table.enabled {
        return Err("OPP table is disabled");
    }

    let idx = table
        .opps
        .iter()
        .position(|o| o.level == level)
        .ok_or("OPP level not found")?;

    table.current_opp = Some(idx);
    Ok(table.opps[idx].clone())
}

/// Find an OPP by frequency (Linux `dev_pm_opp_find_freq_exact`).
pub fn find_by_freq(table_id: u32, freq_hz: u64) -> Result<Opp, &'static str> {
    let tables = OPP_TABLES.read();
    let table = tables.get(&table_id).ok_or("OPP table not found")?;
    table
        .opps
        .iter()
        .find(|o| o.frequency_hz == freq_hz)
        .cloned()
        .ok_or("OPP frequency not found")
}

/// Find the floor OPP for a frequency (Linux `dev_pm_opp_find_freq_floor`).
pub fn find_floor(table_id: u32, freq_hz: u64) -> Result<Opp, &'static str> {
    let tables = OPP_TABLES.read();
    let table = tables.get(&table_id).ok_or("OPP table not found")?;
    table
        .opps
        .iter()
        .filter(|o| o.frequency_hz <= freq_hz)
        .max_by_key(|o| o.frequency_hz)
        .cloned()
        .ok_or("No OPP at or below frequency")
}

/// Find the ceiling OPP for a frequency (Linux `dev_pm_opp_find_freq_ceil`).
pub fn find_ceil(table_id: u32, freq_hz: u64) -> Result<Opp, &'static str> {
    let tables = OPP_TABLES.read();
    let table = tables.get(&table_id).ok_or("OPP table not found")?;
    table
        .opps
        .iter()
        .filter(|o| o.frequency_hz >= freq_hz)
        .min_by_key(|o| o.frequency_hz)
        .cloned()
        .ok_or("No OPP at or above frequency")
}

/// Enable/disable an OPP table (Linux `dev_pm_opp_enable/disable`).
pub fn set_enabled(table_id: u32, enabled: bool) -> Result<(), &'static str> {
    let mut tables = OPP_TABLES.write();
    let table = tables.get_mut(&table_id).ok_or("OPP table not found")?;
    table.enabled = enabled;
    Ok(())
}

/// Get the maximum frequency OPP.
pub fn get_max_opp(table_id: u32) -> Result<Opp, &'static str> {
    let tables = OPP_TABLES.read();
    let table = tables.get(&table_id).ok_or("OPP table not found")?;
    table
        .opps
        .iter()
        .filter(|o| !o.turbo)
        .max_by_key(|o| o.frequency_hz)
        .or_else(|| table.opps.iter().max_by_key(|o| o.frequency_hz))
        .cloned()
        .ok_or("No OPPs available")
}

/// Get the suspend OPP (Linux `dev_pm_opp_get_suspend`).
pub fn get_suspend_opp(table_id: u32) -> Result<Opp, &'static str> {
    let tables = OPP_TABLES.read();
    let table = tables.get(&table_id).ok_or("OPP table not found")?;
    table
        .opps
        .iter()
        .find(|o| o.suspend)
        .cloned()
        .or_else(|| table.opps.first().cloned())
        .ok_or("No suspend OPP available")
}

/// Number of registered OPP tables.
pub fn table_count() -> usize {
    OPP_TABLES.read().len()
}

/// Initialize OPP subsystem with default CPU OPP table.
pub fn init() -> Result<(), &'static str> {
    if !OPP_TABLES.read().is_empty() {
        return Ok(());
    }

    let cpu_opps = default_cpu_opps();
    let count = cpu_opps.len();
    register_table("cpu", &cpu_opps)?;

    crate::serial_println!("opp: CPU table registered ({} OPPs)", count);
    Ok(())
}
