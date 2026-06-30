//! DVB (Digital Video Broadcasting) subsystem
//!
//! Provides DVB framework for digital TV frontend, demux, and streaming.
//! Mirrors Linux's `drivers/media/dvb-core/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// DVB adapter (Linux `struct dvb_adapter`).
pub struct DvbAdapter {
    pub id: u32,
    pub name: String,
    pub num: u32,
    pub frontend_ids: Vec<u32>,
    pub demux_ids: Vec<u32>,
    pub net_ids: Vec<u32>,
}

/// DVB frontend (Linux `struct dvb_frontend`).
pub struct DvbFrontend {
    pub id: u32,
    pub adapter_id: u32,
    pub name: String,
    pub ops: DvbFeOps,
    pub delivery_system: DvbDelSys,
    pub state: DvbFeState,
    pub frequency: u32,
    pub bandwidth_hz: u32,
    pub symbol_rate: u32,
    pub inversion: DvbInversion,
    pub stream_id: u32,
}

/// DVB delivery system (Linux `enum fe_delivery_system`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DvbDelSys {
    Undefined,
    DvbC,
    DvbCAnnexB,
    DvbT,
    Dss,
    DvbS,
    DvbS2,
    DvbH,
    IsdbT,
    IsdbS,
    DmbT,
    Cqam,
    DvbC2,
    DvbT2,
    Turbo,
    DvbCc,
    DvbSc,
    Atsc,
    AtscMh,
    Dtb,
    CqamAnnexB,
}

/// DVB frontend state (Linux `enum dvb_frontend_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DvbFeState {
    Idle,
    Tuning,
    Locked,
    LostLock,
}

/// DVB inversion (Linux `enum fe_spectral_inversion`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DvbInversion {
    Off,
    On,
    Auto,
}

/// DVB frontend operations (Linux `struct dvb_frontend_ops`).
pub struct DvbFeOps {
    pub init: fn(fe_id: u32) -> Result<(), &'static str>,
    pub sleep: fn(fe_id: u32) -> Result<(), &'static str>,
    pub tune: fn(fe_id: u32, params: &DvbTuneParams) -> Result<(), &'static str>,
    pub get_frontend: fn(fe_id: u32) -> Result<DvbTuneParams, &'static str>,
    pub read_status: fn(fe_id: u32) -> Result<DvbFeStatus, &'static str>,
    pub read_signal_strength: fn(fe_id: u32) -> Result<u16, &'static str>,
    pub read_snr: fn(fe_id: u32) -> Result<u16, &'static str>,
    pub read_ber: fn(fe_id: u32) -> Result<u32, &'static str>,
    pub read_ucblocks: fn(fe_id: u32) -> Result<u32, &'static str>,
}

/// DVB tune parameters (Linux `struct dtv_frontend_properties`).
#[derive(Debug, Clone)]
pub struct DvbTuneParams {
    pub frequency: u32,
    pub bandwidth_hz: u32,
    pub symbol_rate: u32,
    pub delivery_system: DvbDelSys,
    pub inversion: DvbInversion,
    pub stream_id: u32,
    pub modulation: u32,
    pub fec: u32,
    pub code_rate_hp: u32,
    pub code_rate_lp: u32,
    pub guard_interval: u32,
    pub transmission_mode: u32,
    pub hierarchy: u32,
}

/// DVB frontend status flags (Linux `enum fe_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DvbFeStatus(pub u32);

impl DvbFeStatus {
    pub const NONE: Self = DvbFeStatus(0);
    pub const HAS_SIGNAL: Self = DvbFeStatus(1);
    pub const HAS_CARRIER: Self = DvbFeStatus(2);
    pub const HAS_VITERBI: Self = DvbFeStatus(4);
    pub const HAS_SYNC: Self = DvbFeStatus(8);
    pub const HAS_LOCK: Self = DvbFeStatus(16);
    pub const TIMEDOUT: Self = DvbFeStatus(32);
    pub const REINIT: Self = DvbFeStatus(64);

    pub fn has(self, flag: Self) -> bool {
        (self.0 & flag.0) != 0
    }
}

/// DVB demux (Linux `struct dvb_demux`).
pub struct DvbDemux {
    pub id: u32,
    pub adapter_id: u32,
    pub name: String,
    pub filter_ids: Vec<u32>,
    pub feed_count: u32,
}

/// DVB filter (Linux `struct dvb_demux_filter`).
pub struct DvbFilter {
    pub id: u32,
    pub demux_id: u32,
    pub pid: u16,
    pub filter_value: [u8; 8],
    pub filter_mask: [u8; 8],
    pub filter_mode: [u8; 8],
    pub active: bool,
}

/// DVB net (Linux `struct dvb_net`).
pub struct DvbNet {
    pub id: u32,
    pub adapter_id: u32,
    pub name: String,
    pub pid: u16,
    pub active: bool,
}

// ── Registry ────────────────────────────────────────────────────────────

static ADAP_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static FE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEMUX_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static FILTER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static NET_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static DVB_ADAPS: RwLock<BTreeMap<u32, DvbAdapter>> = RwLock::new(BTreeMap::new());
static DVB_FES: RwLock<BTreeMap<u32, DvbFrontend>> = RwLock::new(BTreeMap::new());
static DVB_DEMUXES: RwLock<BTreeMap<u32, DvbDemux>> = RwLock::new(BTreeMap::new());
static DVB_FILTERS: RwLock<BTreeMap<u32, DvbFilter>> = RwLock::new(BTreeMap::new());
static DVB_NETS: RwLock<BTreeMap<u32, DvbNet>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a DVB adapter (Linux `dvb_register_adapter`).
pub fn register_adapter(name: &str) -> Result<u32, &'static str> {
    let id = ADAP_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let adap = DvbAdapter {
        id,
        name: String::from(name),
        num: id,
        frontend_ids: Vec::new(),
        demux_ids: Vec::new(),
        net_ids: Vec::new(),
    };
    DVB_ADAPS.write().insert(id, adap);
    Ok(id)
}

/// Register a DVB frontend (Linux `dvb_register_frontend`).
pub fn register_frontend(
    adapter_id: u32,
    name: &str,
    ops: DvbFeOps,
    delivery_system: DvbDelSys,
) -> Result<u32, &'static str> {
    let id = FE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let fe = DvbFrontend {
        id,
        adapter_id,
        name: String::from(name),
        ops,
        delivery_system,
        state: DvbFeState::Idle,
        frequency: 0,
        bandwidth_hz: 0,
        symbol_rate: 0,
        inversion: DvbInversion::Auto,
        stream_id: 0,
    };
    DVB_FES.write().insert(id, fe);

    let mut adaps = DVB_ADAPS.write();
    if let Some(adap) = adaps.get_mut(&adapter_id) {
        adap.frontend_ids.push(id);
    }
    Ok(id)
}

/// Tune the frontend (Linux `dvb_frontend_tune`).
pub fn tune_frontend(fe_id: u32, params: &DvbTuneParams) -> Result<(), &'static str> {
    let tune_fn = {
        let fes = DVB_FES.read();
        let fe = fes.get(&fe_id).ok_or("DVB frontend not found")?;
        fe.ops.tune
    };
    (tune_fn)(fe_id, params)?;

    let mut fes = DVB_FES.write();
    if let Some(fe) = fes.get_mut(&fe_id) {
        fe.frequency = params.frequency;
        fe.bandwidth_hz = params.bandwidth_hz;
        fe.symbol_rate = params.symbol_rate;
        fe.inversion = params.inversion;
        fe.stream_id = params.stream_id;
        fe.state = DvbFeState::Tuning;
    }
    Ok(())
}

/// Read frontend status (Linux `dvb_frontend_read_status`).
pub fn read_status(fe_id: u32) -> Result<DvbFeStatus, &'static str> {
    let status_fn = {
        let fes = DVB_FES.read();
        let fe = fes.get(&fe_id).ok_or("DVB frontend not found")?;
        fe.ops.read_status
    };
    let status = (status_fn)(fe_id)?;

    let mut fes = DVB_FES.write();
    if let Some(fe) = fes.get_mut(&fe_id) {
        if status.has(DvbFeStatus::HAS_LOCK) {
            fe.state = DvbFeState::Locked;
        } else if fe.state == DvbFeState::Locked {
            fe.state = DvbFeState::LostLock;
        }
    }
    Ok(status)
}

/// Read signal strength (Linux `dvb_frontend_read_signal_strength`).
pub fn read_signal_strength(fe_id: u32) -> Result<u16, &'static str> {
    let read_fn = {
        let fes = DVB_FES.read();
        let fe = fes.get(&fe_id).ok_or("DVB frontend not found")?;
        fe.ops.read_signal_strength
    };
    (read_fn)(fe_id)
}

/// Read SNR (Linux `dvb_frontend_read_snr`).
pub fn read_snr(fe_id: u32) -> Result<u16, &'static str> {
    let read_fn = {
        let fes = DVB_FES.read();
        let fe = fes.get(&fe_id).ok_or("DVB frontend not found")?;
        fe.ops.read_snr
    };
    (read_fn)(fe_id)
}

/// Register a DVB demux (Linux `dvb_dmx_init`).
pub fn register_demux(adapter_id: u32, name: &str) -> Result<u32, &'static str> {
    let id = DEMUX_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let demux = DvbDemux {
        id,
        adapter_id,
        name: String::from(name),
        filter_ids: Vec::new(),
        feed_count: 0,
    };
    DVB_DEMUXES.write().insert(id, demux);

    let mut adaps = DVB_ADAPS.write();
    if let Some(adap) = adaps.get_mut(&adapter_id) {
        adap.demux_ids.push(id);
    }
    Ok(id)
}

/// Add a PID filter (Linux `dvb_dmx_swfilter`).
pub fn add_filter(demux_id: u32, pid: u16) -> Result<u32, &'static str> {
    let id = FILTER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let filter = DvbFilter {
        id,
        demux_id,
        pid,
        filter_value: [0; 8],
        filter_mask: [0; 8],
        filter_mode: [0; 8],
        active: true,
    };
    DVB_FILTERS.write().insert(id, filter);

    let mut demuxes = DVB_DEMUXES.write();
    if let Some(demux) = demuxes.get_mut(&demux_id) {
        demux.filter_ids.push(id);
        demux.feed_count += 1;
    }
    Ok(id)
}

/// Remove a filter.
pub fn remove_filter(filter_id: u32) -> Result<(), &'static str> {
    let demux_id = {
        let mut filters = DVB_FILTERS.write();
        let filter = filters.remove(&filter_id).ok_or("DVB filter not found")?;
        filter.demux_id
    };

    let mut demuxes = DVB_DEMUXES.write();
    if let Some(demux) = demuxes.get_mut(&demux_id) {
        demux.filter_ids.retain(|&id| id != filter_id);
        if demux.feed_count > 0 {
            demux.feed_count -= 1;
        }
    }
    Ok(())
}

/// Register a DVB net interface (Linux `dvb_net_init`).
pub fn register_net(adapter_id: u32, name: &str, pid: u16) -> Result<u32, &'static str> {
    let id = NET_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let net = DvbNet {
        id,
        adapter_id,
        name: String::from(name),
        pid,
        active: true,
    };
    DVB_NETS.write().insert(id, net);

    let mut adaps = DVB_ADAPS.write();
    if let Some(adap) = adaps.get_mut(&adapter_id) {
        adap.net_ids.push(id);
    }
    Ok(id)
}

/// List all DVB adapters.
pub fn list_adapters() -> Vec<(u32, String, usize, usize)> {
    DVB_ADAPS
        .read()
        .iter()
        .map(|(id, a)| (*id, a.name.clone(), a.frontend_ids.len(), a.demux_ids.len()))
        .collect()
}

/// List frontends on an adapter.
pub fn list_frontends(
    adapter_id: u32,
) -> Result<Vec<(u32, String, DvbDelSys, DvbFeState)>, &'static str> {
    let adaps = DVB_ADAPS.read();
    let adap = adaps.get(&adapter_id).ok_or("DVB adapter not found")?;
    let fes = DVB_FES.read();
    let mut result = Vec::new();
    for &fe_id in &adap.frontend_ids {
        if let Some(fe) = fes.get(&fe_id) {
            result.push((fe.id, fe.name.clone(), fe.delivery_system, fe.state));
        }
    }
    Ok(result)
}

/// Count registered adapters.
pub fn adapter_count() -> usize {
    DVB_ADAPS.read().len()
}

// ── Software DVB ────────────────────────────────────────────────────────

fn sw_fe_init(_fe_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_fe_sleep(_fe_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_fe_tune(_fe_id: u32, _params: &DvbTuneParams) -> Result<(), &'static str> {
    Ok(())
}
fn sw_fe_get_frontend(fe_id: u32) -> Result<DvbTuneParams, &'static str> {
    let fes = DVB_FES.read();
    let fe = fes.get(&fe_id).ok_or("DVB frontend not found")?;
    Ok(DvbTuneParams {
        frequency: fe.frequency,
        bandwidth_hz: fe.bandwidth_hz,
        symbol_rate: fe.symbol_rate,
        delivery_system: fe.delivery_system,
        inversion: fe.inversion,
        stream_id: fe.stream_id,
        modulation: 0,
        fec: 0,
        code_rate_hp: 0,
        code_rate_lp: 0,
        guard_interval: 0,
        transmission_mode: 0,
        hierarchy: 0,
    })
}
fn sw_fe_read_status(fe_id: u32) -> Result<DvbFeStatus, &'static str> {
    let fes = DVB_FES.read();
    let fe = fes.get(&fe_id).ok_or("DVB frontend not found")?;
    if fe.state == DvbFeState::Tuning || fe.state == DvbFeState::Locked {
        Ok(DvbFeStatus(
            DvbFeStatus::HAS_SIGNAL.0
                | DvbFeStatus::HAS_CARRIER.0
                | DvbFeStatus::HAS_VITERBI.0
                | DvbFeStatus::HAS_SYNC.0
                | DvbFeStatus::HAS_LOCK.0,
        ))
    } else {
        Ok(DvbFeStatus::NONE)
    }
}
fn sw_fe_read_signal_strength(_fe_id: u32) -> Result<u16, &'static str> {
    Ok(0x8000)
}
fn sw_fe_read_snr(_fe_id: u32) -> Result<u16, &'static str> {
    Ok(0x4000)
}
fn sw_fe_read_ber(_fe_id: u32) -> Result<u32, &'static str> {
    Ok(0)
}
fn sw_fe_read_ucblocks(_fe_id: u32) -> Result<u32, &'static str> {
    Ok(0)
}

/// Software DVB frontend ops.
pub fn software_dvb_fe_ops() -> DvbFeOps {
    DvbFeOps {
        init: sw_fe_init,
        sleep: sw_fe_sleep,
        tune: sw_fe_tune,
        get_frontend: sw_fe_get_frontend,
        read_status: sw_fe_read_status,
        read_signal_strength: sw_fe_read_signal_strength,
        read_snr: sw_fe_read_snr,
        read_ber: sw_fe_read_ber,
        read_ucblocks: sw_fe_read_ucblocks,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !DVB_ADAPS.read().is_empty() {
        return Ok(());
    }

    let adap_id = register_adapter("software-dvb")?;
    let fe_ops = software_dvb_fe_ops();
    let fe_id = register_frontend(adap_id, "sw-dvb-frontend", fe_ops, DvbDelSys::DvbT)?;

    crate::serial_println!(
        "dvb: adapter {} registered with frontend {} (DVB-T)",
        adap_id,
        fe_id
    );
    Ok(())
}
