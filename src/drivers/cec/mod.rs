//! CEC (Consumer Electronics Control) subsystem
//!
//! Provides CEC framework for HDMI CEC device communication.
//! Mirrors Linux's `drivers/media/cec/cec-core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// CEC adapter (Linux `struct cec_adapter`).
pub struct CecAdapter {
    pub id: u32,
    pub name: String,
    pub ops: CecOps,
    pub capabilities: u32,
    pub available_log_addrs: u8,
    pub phys_addr: u16,
    pub log_addrs: Vec<CecLogAddr>,
    pub state: CecState,
    pub monitor: bool,
    pub monitor_all: bool,
    pub transmit_in_progress: bool,
    pub msg_queue: Vec<CecMessage>,
}

/// CEC logical address (Linux `struct cec_log_addrs`).
#[derive(Debug, Clone)]
pub struct CecLogAddr {
    pub log_addr: u8,
    pub primary_device_type: u8,
    pub log_addr_type: u8,
    pub all_device_types: u8,
    pub features: [u8; 4],
}

/// CEC state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CecState {
    Unconfigured,
    Configuring,
    Configured,
    Adapting,
}

/// CEC message (Linux `struct cec_msg`).
#[derive(Debug, Clone)]
pub struct CecMessage {
    pub tx_ts: u64,
    pub rx_ts: u64,
    pub len: u8,
    pub msg: [u8; 16],
    pub reply: u8,
    pub timeout_ms: u32,
    pub flags: u32,
    pub tx_status: u32,
    pub rx_status: u32,
    pub sequence: u32,
}

/// CEC operations (Linux `struct cec_adap_ops`).
pub struct CecOps {
    pub adap_enable: fn(adap_id: u32, enable: bool) -> Result<(), &'static str>,
    pub adap_log_addr: fn(adap_id: u32, log_addr: u8) -> Result<(), &'static str>,
    pub adap_transmit: fn(adap_id: u32, msg: &CecMessage) -> Result<(), &'static str>,
    pub adap_status: fn(adap_id: u32) -> Result<String, &'static str>,
    pub adap_monitor_all_enable: fn(adap_id: u32, enable: bool) -> Result<(), &'static str>,
}

/// CEC notifier (Linux `struct cec_notifier`).
pub struct CecNotifier {
    pub id: u32,
    pub adap_id: Option<u32>,
    pub phys_addr: u16,
}

// ── Registry ────────────────────────────────────────────────────────────

static ADAP_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static NOTIFIER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static SEQ_COUNTER: AtomicU32 = AtomicU32::new(0);

static CEC_ADAPS: RwLock<BTreeMap<u32, CecAdapter>> = RwLock::new(BTreeMap::new());
static CEC_NOTIFIERS: RwLock<BTreeMap<u32, CecNotifier>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a CEC adapter (Linux `cec_allocate_adapter` + `cec_register_adapter`).
pub fn register_adapter(
    name: &str,
    ops: CecOps,
    capabilities: u32,
    available_log_addrs: u8,
) -> Result<u32, &'static str> {
    let id = ADAP_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let adap = CecAdapter {
        id,
        name: String::from(name),
        ops,
        capabilities,
        available_log_addrs,
        phys_addr: 0xFFFF, // Not connected
        log_addrs: Vec::new(),
        state: CecState::Unconfigured,
        monitor: false,
        monitor_all: false,
        transmit_in_progress: false,
        msg_queue: Vec::new(),
    };
    CEC_ADAPS.write().insert(id, adap);
    Ok(id)
}

/// Enable a CEC adapter (Linux `cec_adap_enable`).
pub fn enable_adapter(adap_id: u32, enable: bool) -> Result<(), &'static str> {
    let enable_fn = {
        let adaps = CEC_ADAPS.read();
        let adap = adaps.get(&adap_id).ok_or("CEC adapter not found")?;
        adap.ops.adap_enable
    };
    (enable_fn)(adap_id, enable)?;
    Ok(())
}

/// Set physical address (Linux `cec_s_phys_addr`).
pub fn set_phys_addr(adap_id: u32, phys_addr: u16) -> Result<(), &'static str> {
    let mut adaps = CEC_ADAPS.write();
    let adap = adaps.get_mut(&adap_id).ok_or("CEC adapter not found")?;
    adap.phys_addr = phys_addr;
    if phys_addr == 0xFFFF {
        adap.state = CecState::Unconfigured;
        adap.log_addrs.clear();
    }
    Ok(())
}

/// Configure logical addresses (Linux `cec_s_log_addrs`).
pub fn set_log_addrs(adap_id: u32, log_addrs: Vec<CecLogAddr>) -> Result<(), &'static str> {
    let (log_addr_fn, max_addrs) = {
        let adaps = CEC_ADAPS.read();
        let adap = adaps.get(&adap_id).ok_or("CEC adapter not found")?;
        if log_addrs.len() > adap.available_log_addrs as usize {
            return Err("Too many logical addresses");
        }
        (adap.ops.adap_log_addr, adap.available_log_addrs)
    };
    let _ = max_addrs;

    {
        let mut adaps = CEC_ADAPS.write();
        let adap = adaps.get_mut(&adap_id).ok_or("CEC adapter not found")?;
        adap.state = CecState::Configuring;
    }

    for la in &log_addrs {
        (log_addr_fn)(adap_id, la.log_addr)?;
    }

    let mut adaps = CEC_ADAPS.write();
    if let Some(adap) = adaps.get_mut(&adap_id) {
        adap.log_addrs = log_addrs;
        adap.state = CecState::Configured;
    }
    Ok(())
}

/// Transmit a CEC message (Linux `cec_transmit_msg`).
pub fn transmit_msg(adap_id: u32, mut msg: CecMessage) -> Result<u32, &'static str> {
    let transmit_fn = {
        let adaps = CEC_ADAPS.read();
        let adap = adaps.get(&adap_id).ok_or("CEC adapter not found")?;
        if adap.state != CecState::Configured {
            return Err("CEC adapter not configured");
        }
        adap.ops.adap_transmit
    };

    msg.sequence = SEQ_COUNTER.fetch_add(1, Ordering::SeqCst);

    {
        let mut adaps = CEC_ADAPS.write();
        if let Some(adap) = adaps.get_mut(&adap_id) {
            adap.transmit_in_progress = true;
            adap.msg_queue.push(msg.clone());
        }
    }

    (transmit_fn)(adap_id, &msg)?;

    let mut adaps = CEC_ADAPS.write();
    if let Some(adap) = adaps.get_mut(&adap_id) {
        adap.transmit_in_progress = false;
    }

    Ok(msg.sequence)
}

/// Receive a CEC message (called by hardware driver, Linux `cec_received_msg`).
pub fn receive_msg(adap_id: u32, msg: CecMessage) -> Result<(), &'static str> {
    let mut adaps = CEC_ADAPS.write();
    let adap = adaps.get_mut(&adap_id).ok_or("CEC adapter not found")?;
    adap.msg_queue.push(msg);
    Ok(())
}

/// Enable monitor-all mode (Linux `cec_monitor_all_enable`).
pub fn enable_monitor_all(adap_id: u32, enable: bool) -> Result<(), &'static str> {
    let enable_fn = {
        let adaps = CEC_ADAPS.read();
        let adap = adaps.get(&adap_id).ok_or("CEC adapter not found")?;
        adap.ops.adap_monitor_all_enable
    };
    (enable_fn)(adap_id, enable)?;

    let mut adaps = CEC_ADAPS.write();
    if let Some(adap) = adaps.get_mut(&adap_id) {
        adap.monitor_all = enable;
    }
    Ok(())
}

/// Get adapter status (Linux `cec_adap_status`).
pub fn get_status(adap_id: u32) -> Result<String, &'static str> {
    let status_fn = {
        let adaps = CEC_ADAPS.read();
        let adap = adaps.get(&adap_id).ok_or("CEC adapter not found")?;
        adap.ops.adap_status
    };
    (status_fn)(adap_id)
}

/// Register a CEC notifier (Linux `cec_notifier_conn_register`).
pub fn register_notifier(phys_addr: u16) -> Result<u32, &'static str> {
    let id = NOTIFIER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let notifier = CecNotifier {
        id,
        adap_id: None,
        phys_addr,
    };
    CEC_NOTIFIERS.write().insert(id, notifier);
    Ok(id)
}

/// Set notifier physical address (Linux `cec_notifier_set_phys_addr`).
pub fn notifier_set_phys_addr(notifier_id: u32, phys_addr: u16) -> Result<(), &'static str> {
    let mut notifiers = CEC_NOTIFIERS.write();
    let n = notifiers
        .get_mut(&notifier_id)
        .ok_or("CEC notifier not found")?;
    n.phys_addr = phys_addr;

    // Propagate to associated adapter
    if let Some(adap_id) = n.adap_id {
        drop(notifiers);
        set_phys_addr(adap_id, phys_addr)?;
    }
    Ok(())
}

/// List all CEC adapters.
pub fn list_adapters() -> Vec<(u32, String, CecState, u16)> {
    CEC_ADAPS
        .read()
        .iter()
        .map(|(id, a)| (*id, a.name.clone(), a.state, a.phys_addr))
        .collect()
}

/// Count registered adapters.
pub fn adapter_count() -> usize {
    CEC_ADAPS.read().len()
}

// ── Software CEC ────────────────────────────────────────────────────────

fn sw_adap_enable(_adap_id: u32, _enable: bool) -> Result<(), &'static str> {
    Ok(())
}
fn sw_adap_log_addr(_adap_id: u32, _log_addr: u8) -> Result<(), &'static str> {
    Ok(())
}
fn sw_adap_transmit(_adap_id: u32, _msg: &CecMessage) -> Result<(), &'static str> {
    Ok(())
}
fn sw_adap_status(adap_id: u32) -> Result<String, &'static str> {
    let adaps = CEC_ADAPS.read();
    let adap = adaps.get(&adap_id).ok_or("CEC adapter not found")?;
    Ok(alloc::format!(
        "CEC adapter: {} (phys_addr={:04x})",
        adap.name,
        adap.phys_addr
    ))
}
fn sw_adap_monitor_all_enable(_adap_id: u32, _enable: bool) -> Result<(), &'static str> {
    Ok(())
}

/// Software CEC ops.
pub fn software_cec_ops() -> CecOps {
    CecOps {
        adap_enable: sw_adap_enable,
        adap_log_addr: sw_adap_log_addr,
        adap_transmit: sw_adap_transmit,
        adap_status: sw_adap_status,
        adap_monitor_all_enable: sw_adap_monitor_all_enable,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !CEC_ADAPS.read().is_empty() {
        return Ok(());
    }

    let ops = software_cec_ops();
    let adap_id = register_adapter("sw-cec", ops, 0, 1)?;
    crate::serial_println!("cec: software adapter registered (id={})", adap_id);
    Ok(())
}
