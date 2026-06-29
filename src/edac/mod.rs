//! EDAC (Error Detection And Correction) reporting framework.
//!
//! Memory controllers register with EDAC; correctable and uncorrectable
//! errors are counted and logged per device.

extern crate alloc;

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::RwLock;

/// One EDAC-registered memory controller / DIMM bank.
#[derive(Debug, Clone)]
pub struct EdacDevice {
    pub name: String,
    pub dimm_label: String,
    pub ce_count: u64,
    pub ue_count: u64,
    pub last_ce_addr: Option<u64>,
    pub last_ue_addr: Option<u64>,
}

static DEVICES: RwLock<BTreeMap<String, EdacDevice>> = RwLock::new(BTreeMap::new());
static TOTAL_CE: AtomicU64 = AtomicU64::new(0);
static TOTAL_UE: AtomicU64 = AtomicU64::new(0);

/// Register an EDAC device (memory controller slice).
pub fn register_device(name: &str, dimm_label: &str) -> bool {
    let mut devs = DEVICES.write();
    if devs.contains_key(name) {
        return false;
    }
    devs.insert(
        String::from(name),
        EdacDevice {
            name: String::from(name),
            dimm_label: String::from(dimm_label),
            ce_count: 0,
            ue_count: 0,
            last_ce_addr: None,
            last_ue_addr: None,
        },
    );
    true
}

/// Report a correctable error (CE).
pub fn report_ce(device: &str, addr: u64) {
    TOTAL_CE.fetch_add(1, Ordering::Relaxed);
    let mut devs = DEVICES.write();
    if let Some(dev) = devs.get_mut(device) {
        dev.ce_count += 1;
        dev.last_ce_addr = Some(addr);
        crate::serial_println!(
            "[edac] CE on {} ({}) at {:#x}",
            dev.name,
            dev.dimm_label,
            addr
        );
    }
}

/// Report an uncorrectable error (UE).
pub fn report_ue(device: &str, addr: u64) {
    TOTAL_UE.fetch_add(1, Ordering::Relaxed);
    let mut devs = DEVICES.write();
    if let Some(dev) = devs.get_mut(device) {
        dev.ue_count += 1;
        dev.last_ue_addr = Some(addr);
        crate::serial_println!(
            "[edac] UE on {} ({}) at {:#x}",
            dev.name,
            dev.dimm_label,
            addr
        );
        crate::audit::audit_log(
            crate::audit::AuditType::Kernel,
            0,
            0,
            false,
            &format!("edac_ue device={} addr={:#x}", device, addr),
        );
    }
}

/// Aggregate CE/UE totals across all devices.
pub fn totals() -> (u64, u64) {
    (
        TOTAL_CE.load(Ordering::Relaxed),
        TOTAL_UE.load(Ordering::Relaxed),
    )
}

/// Snapshot all registered devices.
pub fn devices() -> Vec<EdacDevice> {
    DEVICES.read().values().cloned().collect()
}

/// Initialize EDAC and register the boot memory controller.
pub fn init() {
    DEVICES.write().clear();
    TOTAL_CE.store(0, Ordering::Release);
    TOTAL_UE.store(0, Ordering::Release);
    register_device("mc0", "DIMM0");
    crate::serial_println!("[edac] initialized ({} devices)", DEVICES.read().len());
}
