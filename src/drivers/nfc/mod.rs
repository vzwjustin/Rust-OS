//! NFC (Near Field Communication) subsystem
//!
//! Provides NFC controller framework for NFC tags, targets, and protocols.
//! Mirrors Linux's `net/nfc/nfc.c` and `drivers/nfc/nfc-core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// NFC protocol (Linux `enum nfc_protocol`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NfcProtocol {
    Unspecified,
    Jewel,
    Mifare,
    FeliCa,
    Iso14443,
    Iso14443B,
    Iso15693,
    IsoDep,
    NfcDep,
}

/// NFC target type (Linux `enum nfc_target_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NfcTargetType {
    Unknown,
    Tag,
    Device,
    CardEmulation,
}

/// NFC target (Linux `struct nfc_target`).
#[derive(Debug, Clone)]
pub struct NfcTarget {
    pub idx: u32,
    pub protocols: Vec<NfcProtocol>,
    pub sens_res: u16,
    pub sel_res: u8,
    pub nfcid1: Vec<u8>,
    pub nfcid2: Vec<u8>,
    pub target_type: NfcTargetType,
    pub active: bool,
}

/// NFC device operations (Linux `struct nfc_ops`).
pub struct NfcOps {
    pub dev_up: fn(device_id: u32) -> Result<(), &'static str>,
    pub dev_down: fn(device_id: u32) -> Result<(), &'static str>,
    pub start_poll: fn(device_id: u32, protocols: &[NfcProtocol]) -> Result<(), &'static str>,
    pub stop_poll: fn(device_id: u32) -> Result<(), &'static str>,
    pub activate_target: fn(device_id: u32, target_idx: u32) -> Result<(), &'static str>,
    pub deactivate_target: fn(device_id: u32, target_idx: u32) -> Result<(), &'static str>,
    pub data_exchange:
        fn(device_id: u32, target_idx: u32, data: &[u8]) -> Result<Vec<u8>, &'static str>,
    pub enable_se: fn(device_id: u32, se_idx: u32) -> Result<(), &'static str>,
    pub disable_se: fn(device_id: u32, se_idx: u32) -> Result<(), &'static str>,
}

/// NFC device (Linux `struct nfc_dev`).
pub struct NfcDevice {
    pub id: u32,
    pub name: String,
    pub ops: NfcOps,
    pub supported_protocols: Vec<NfcProtocol>,
    pub targets: Vec<NfcTarget>,
    pub polling: bool,
    pub dev_up: bool,
    pub firmware_name: Option<String>,
}

/// NFC SE (Secure Element).
#[derive(Debug, Clone)]
pub struct NfcSe {
    pub idx: u32,
    pub name: String,
    pub enabled: bool,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static TARGET_IDX_COUNTER: AtomicU32 = AtomicU32::new(0);
static SE_IDX_COUNTER: AtomicU32 = AtomicU32::new(0);

static NFC_DEVICES: RwLock<BTreeMap<u32, NfcDevice>> = RwLock::new(BTreeMap::new());
static NFC_SES: RwLock<BTreeMap<u32, NfcSe>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an NFC device.
pub fn register_device(
    name: &str,
    ops: NfcOps,
    supported_protocols: Vec<NfcProtocol>,
) -> Result<u32, &'static str> {
    let id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = NfcDevice {
        id,
        name: String::from(name),
        ops,
        supported_protocols,
        targets: Vec::new(),
        polling: false,
        dev_up: false,
        firmware_name: None,
    };
    NFC_DEVICES.write().insert(id, dev);
    Ok(id)
}

/// Unregister an NFC device.
pub fn unregister_device(device_id: u32) -> Result<(), &'static str> {
    if NFC_DEVICES.write().remove(&device_id).is_none() {
        return Err("NFC device not found");
    }
    Ok(())
}

/// Power up an NFC device (Linux `nfc_dev_up`).
pub fn dev_up(device_id: u32) -> Result<(), &'static str> {
    let dev_up_fn = {
        let devices = NFC_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("NFC device not found")?;
        dev.ops.dev_up
    };
    (dev_up_fn)(device_id)?;

    let mut devices = NFC_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        dev.dev_up = true;
    }
    Ok(())
}

/// Power down an NFC device (Linux `nfc_dev_down`).
pub fn dev_down(device_id: u32) -> Result<(), &'static str> {
    let dev_down_fn = {
        let devices = NFC_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("NFC device not found")?;
        dev.ops.dev_down
    };
    (dev_down_fn)(device_id)?;

    let mut devices = NFC_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        dev.dev_up = false;
        dev.polling = false;
    }
    Ok(())
}

/// Start polling for NFC targets (Linux `nfc_start_poll`).
pub fn start_poll(device_id: u32, protocols: &[NfcProtocol]) -> Result<(), &'static str> {
    let start_fn = {
        let devices = NFC_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("NFC device not found")?;
        if !dev.dev_up {
            return Err("NFC device not up");
        }
        dev.ops.start_poll
    };
    (start_fn)(device_id, protocols)?;

    let mut devices = NFC_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        dev.polling = true;
    }
    Ok(())
}

/// Stop polling for NFC targets (Linux `nfc_stop_poll`).
pub fn stop_poll(device_id: u32) -> Result<(), &'static str> {
    let stop_fn = {
        let devices = NFC_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("NFC device not found")?;
        dev.ops.stop_poll
    };
    (stop_fn)(device_id)?;

    let mut devices = NFC_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        dev.polling = false;
    }
    Ok(())
}

/// Add a discovered target to the device (called by driver on discovery).
pub fn add_target(
    device_id: u32,
    protocols: Vec<NfcProtocol>,
    sens_res: u16,
    sel_res: u8,
    nfcid1: Vec<u8>,
    target_type: NfcTargetType,
) -> Result<u32, &'static str> {
    let idx = TARGET_IDX_COUNTER.fetch_add(1, Ordering::SeqCst);
    let target = NfcTarget {
        idx,
        protocols,
        sens_res,
        sel_res,
        nfcid1,
        nfcid2: Vec::new(),
        target_type,
        active: false,
    };

    let mut devices = NFC_DEVICES.write();
    let dev = devices.get_mut(&device_id).ok_or("NFC device not found")?;
    dev.targets.push(target);
    Ok(idx)
}

/// Activate a target for data exchange (Linux `nfc_activate_target`).
pub fn activate_target(device_id: u32, target_idx: u32) -> Result<(), &'static str> {
    let activate_fn = {
        let devices = NFC_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("NFC device not found")?;
        dev.ops.activate_target
    };
    (activate_fn)(device_id, target_idx)?;

    let mut devices = NFC_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        for target in &mut dev.targets {
            if target.idx == target_idx {
                target.active = true;
            }
        }
    }
    Ok(())
}

/// Deactivate a target (Linux `nfc_deactivate_target`).
pub fn deactivate_target(device_id: u32, target_idx: u32) -> Result<(), &'static str> {
    let deactivate_fn = {
        let devices = NFC_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("NFC device not found")?;
        dev.ops.deactivate_target
    };
    (deactivate_fn)(device_id, target_idx)?;

    let mut devices = NFC_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        for target in &mut dev.targets {
            if target.idx == target_idx {
                target.active = false;
            }
        }
    }
    Ok(())
}

/// Exchange data with an active target (Linux `nfc_data_exchange`).
pub fn data_exchange(
    device_id: u32,
    target_idx: u32,
    data: &[u8],
) -> Result<Vec<u8>, &'static str> {
    let exchange_fn = {
        let devices = NFC_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("NFC device not found")?;
        let target_active = dev.targets.iter().any(|t| t.idx == target_idx && t.active);
        if !target_active {
            return Err("NFC target not active");
        }
        dev.ops.data_exchange
    };
    (exchange_fn)(device_id, target_idx, data)
}

/// Enable a Secure Element.
pub fn enable_se(device_id: u32, se_idx: u32) -> Result<(), &'static str> {
    let enable_fn = {
        let devices = NFC_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("NFC device not found")?;
        dev.ops.enable_se
    };
    (enable_fn)(device_id, se_idx)?;

    let mut ses = NFC_SES.write();
    if let Some(se) = ses.get_mut(&se_idx) {
        se.enabled = true;
    }
    Ok(())
}

/// Disable a Secure Element.
pub fn disable_se(device_id: u32, se_idx: u32) -> Result<(), &'static str> {
    let disable_fn = {
        let devices = NFC_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("NFC device not found")?;
        dev.ops.disable_se
    };
    (disable_fn)(device_id, se_idx)?;

    let mut ses = NFC_SES.write();
    if let Some(se) = ses.get_mut(&se_idx) {
        se.enabled = false;
    }
    Ok(())
}

/// Register a Secure Element.
pub fn register_se(name: &str) -> Result<u32, &'static str> {
    let idx = SE_IDX_COUNTER.fetch_add(1, Ordering::SeqCst);
    let se = NfcSe {
        idx,
        name: String::from(name),
        enabled: false,
    };
    NFC_SES.write().insert(idx, se);
    Ok(idx)
}

/// List all NFC devices.
pub fn list_devices() -> Vec<(u32, String)> {
    NFC_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone()))
        .collect()
}

/// Get targets for a device.
pub fn get_targets(device_id: u32) -> Result<Vec<(u32, NfcTargetType, bool)>, &'static str> {
    let devices = NFC_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("NFC device not found")?;
    Ok(dev
        .targets
        .iter()
        .map(|t| (t.idx, t.target_type, t.active))
        .collect())
}

/// Count registered devices.
pub fn device_count() -> usize {
    NFC_DEVICES.read().len()
}

// ── Software NFC ────────────────────────────────────────────────────────

fn sw_dev_up(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_dev_down(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_start_poll(_dev_id: u32, _protocols: &[NfcProtocol]) -> Result<(), &'static str> {
    Ok(())
}
fn sw_stop_poll(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_activate_target(_dev_id: u32, _target_idx: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_deactivate_target(_dev_id: u32, _target_idx: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_data_exchange(_dev_id: u32, _target_idx: u32, data: &[u8]) -> Result<Vec<u8>, &'static str> {
    Ok(data.to_vec())
}
fn sw_enable_se(_dev_id: u32, _se_idx: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_disable_se(_dev_id: u32, _se_idx: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software NFC ops (echo data exchange).
pub fn software_nfc_ops() -> NfcOps {
    NfcOps {
        dev_up: sw_dev_up,
        dev_down: sw_dev_down,
        start_poll: sw_start_poll,
        stop_poll: sw_stop_poll,
        activate_target: sw_activate_target,
        deactivate_target: sw_deactivate_target,
        data_exchange: sw_data_exchange,
        enable_se: sw_enable_se,
        disable_se: sw_disable_se,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let mut protocols = Vec::new();
    protocols.push(NfcProtocol::Iso14443);
    protocols.push(NfcProtocol::Mifare);
    protocols.push(NfcProtocol::NfcDep);

    let ops = software_nfc_ops();
    register_device("software-nfc", ops, protocols)?;
    register_se("sw-se0")?;

    crate::serial_println!("nfc: subsystem ready");
    Ok(())
}
