//! ISDB-T (Integrated Services Digital Broadcasting - Terrestrial) subsystem
//!
//! Provides ISDB-T digital TV demod and frontend interface.
//! Mirrors Linux's `drivers/media/dvb-frontends/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// ISDB-T demod device (Linux `struct dvb_frontend` ISDB-T specific).
pub struct IsdbtDemod {
    pub id: u32,
    pub name: String,
    pub ops: IsdbtOps,
    pub state: IsdbtState,
    pub frequency: u32,
    pub bandwidth: IsdbtBandwidth,
    pub transmission_mode: IsdbtTransmissionMode,
    pub guard_interval: IsdbtGuardInterval,
    pub partial_reception: bool,
    pub layer_a: IsdbtLayer,
    pub layer_b: IsdbtLayer,
    pub layer_c: IsdbtLayer,
    pub stats: IsdbtStats,
}

/// ISDB-T state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsdbtState {
    Sleep,
    Active,
    Tuning,
    Locked,
    LostLock,
}

/// ISDB-T bandwidth (Linux `enum fe_bandwidth`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsdbtBandwidth {
    Bw6Mhz,
    Bw7Mhz,
    Bw8Mhz,
    Auto,
}

/// ISDB-T transmission mode (Linux `enum fe_transmit_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsdbtTransmissionMode {
    Mode1, // 2K
    Mode2, // 4K
    Mode3, // 8K
    Auto,
}

/// ISDB-T guard interval (Linux `enum fe_guard_interval`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsdbtGuardInterval {
    Gi1_4,
    Gi1_8,
    Gi1_16,
    Gi1_32,
    Auto,
}

/// ISDB-T layer configuration (one of 13 segments).
#[derive(Debug, Clone)]
pub struct IsdbtLayer {
    pub enabled: bool,
    pub segments: u8,
    pub modulation: IsdbtModulation,
    pub fec: IsdbtFec,
    pub interleaving: u8,
}

/// ISDB-T modulation (Linux `enum fe_modulation`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsdbtModulation {
    Qpsk,
    Qam16,
    Qam64,
    Qam256,
    Auto,
}

/// ISDB-T FEC (Linux `enum fe_code_rate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsdbtFec {
    Rate1_2,
    Rate2_3,
    Rate3_4,
    Rate5_6,
    Rate7_8,
    Auto,
}

/// ISDB-T reception statistics.
#[derive(Debug, Clone)]
pub struct IsdbtStats {
    pub signal_strength: u16,
    pub snr: u16,
    pub ber: u32,
    pub per: u32, // Packet Error Rate
    pub ucblocks: u32,
    pub locked: bool,
}

/// ISDB-T operations.
pub struct IsdbtOps {
    pub init: fn(demod_id: u32) -> Result<(), &'static str>,
    pub sleep: fn(demod_id: u32) -> Result<(), &'static str>,
    pub tune: fn(demod_id: u32, params: &IsdbtTuneParams) -> Result<(), &'static str>,
    pub read_status: fn(demod_id: u32) -> Result<IsdbtStats, &'static str>,
    pub set_layer: fn(demod_id: u32, layer: u8, config: &IsdbtLayer) -> Result<(), &'static str>,
}

/// ISDB-T tune parameters.
#[derive(Debug, Clone)]
pub struct IsdbtTuneParams {
    pub frequency: u32,
    pub bandwidth: IsdbtBandwidth,
    pub transmission_mode: IsdbtTransmissionMode,
    pub guard_interval: IsdbtGuardInterval,
    pub partial_reception: bool,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEMOD_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static ISDBT_DEMODS: RwLock<BTreeMap<u32, IsdbtDemod>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an ISDB-T demod.
pub fn register_demod(name: &str, ops: IsdbtOps) -> Result<u32, &'static str> {
    let id = DEMOD_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let demod = IsdbtDemod {
        id,
        name: String::from(name),
        ops,
        state: IsdbtState::Sleep,
        frequency: 0,
        bandwidth: IsdbtBandwidth::Auto,
        transmission_mode: IsdbtTransmissionMode::Auto,
        guard_interval: IsdbtGuardInterval::Auto,
        partial_reception: false,
        layer_a: IsdbtLayer {
            enabled: true,
            segments: 13,
            modulation: IsdbtModulation::Qam64,
            fec: IsdbtFec::Rate3_4,
            interleaving: 4,
        },
        layer_b: IsdbtLayer {
            enabled: false,
            segments: 0,
            modulation: IsdbtModulation::Auto,
            fec: IsdbtFec::Auto,
            interleaving: 0,
        },
        layer_c: IsdbtLayer {
            enabled: false,
            segments: 0,
            modulation: IsdbtModulation::Auto,
            fec: IsdbtFec::Auto,
            interleaving: 0,
        },
        stats: IsdbtStats {
            signal_strength: 0,
            snr: 0,
            ber: 0,
            per: 0,
            ucblocks: 0,
            locked: false,
        },
    };
    ISDBT_DEMODS.write().insert(id, demod);
    Ok(id)
}

/// Initialize the demod (Linux `dvb_frontend_init`).
pub fn init_demod(demod_id: u32) -> Result<(), &'static str> {
    let init_fn = {
        let demods = ISDBT_DEMODS.read();
        let demod = demods.get(&demod_id).ok_or("ISDB-T demod not found")?;
        demod.ops.init
    };
    (init_fn)(demod_id)?;

    let mut demods = ISDBT_DEMODS.write();
    if let Some(demod) = demods.get_mut(&demod_id) {
        demod.state = IsdbtState::Active;
    }
    Ok(())
}

/// Put the demod to sleep.
pub fn sleep_demod(demod_id: u32) -> Result<(), &'static str> {
    let sleep_fn = {
        let demods = ISDBT_DEMODS.read();
        let demod = demods.get(&demod_id).ok_or("ISDB-T demod not found")?;
        demod.ops.sleep
    };
    (sleep_fn)(demod_id)?;

    let mut demods = ISDBT_DEMODS.write();
    if let Some(demod) = demods.get_mut(&demod_id) {
        demod.state = IsdbtState::Sleep;
    }
    Ok(())
}

/// Tune the demod (Linux `dvb_frontend_tune`).
pub fn tune_demod(demod_id: u32, params: &IsdbtTuneParams) -> Result<(), &'static str> {
    let tune_fn = {
        let demods = ISDBT_DEMODS.read();
        let demod = demods.get(&demod_id).ok_or("ISDB-T demod not found")?;
        if demod.state == IsdbtState::Sleep {
            return Err("ISDB-T demod is sleeping");
        }
        demod.ops.tune
    };
    (tune_fn)(demod_id, params)?;

    let mut demods = ISDBT_DEMODS.write();
    if let Some(demod) = demods.get_mut(&demod_id) {
        demod.frequency = params.frequency;
        demod.bandwidth = params.bandwidth;
        demod.transmission_mode = params.transmission_mode;
        demod.guard_interval = params.guard_interval;
        demod.partial_reception = params.partial_reception;
        demod.state = IsdbtState::Tuning;
    }
    Ok(())
}

/// Read demod status (Linux `dvb_frontend_read_status`).
pub fn read_status(demod_id: u32) -> Result<IsdbtStats, &'static str> {
    let status_fn = {
        let demods = ISDBT_DEMODS.read();
        let demod = demods.get(&demod_id).ok_or("ISDB-T demod not found")?;
        demod.ops.read_status
    };
    let stats = (status_fn)(demod_id)?;

    let mut demods = ISDBT_DEMODS.write();
    if let Some(demod) = demods.get_mut(&demod_id) {
        demod.stats = stats.clone();
        if stats.locked {
            demod.state = IsdbtState::Locked;
        } else if demod.state == IsdbtState::Locked {
            demod.state = IsdbtState::LostLock;
        }
    }
    Ok(stats)
}

/// Configure a layer (Linux `ISDBT_LAYERS` ioctl).
pub fn set_layer(demod_id: u32, layer: u8, config: &IsdbtLayer) -> Result<(), &'static str> {
    let set_fn = {
        let demods = ISDBT_DEMODS.read();
        let demod = demods.get(&demod_id).ok_or("ISDB-T demod not found")?;
        demod.ops.set_layer
    };
    (set_fn)(demod_id, layer, config)?;

    let mut demods = ISDBT_DEMODS.write();
    if let Some(demod) = demods.get_mut(&demod_id) {
        match layer {
            0 => demod.layer_a = config.clone(),
            1 => demod.layer_b = config.clone(),
            2 => demod.layer_c = config.clone(),
            _ => return Err("Invalid layer index"),
        }
    }
    Ok(())
}

/// List all ISDB-T demods.
pub fn list_demods() -> Vec<(u32, String, IsdbtState, u32)> {
    ISDBT_DEMODS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.state, d.frequency))
        .collect()
}

/// Count registered demods.
pub fn demod_count() -> usize {
    ISDBT_DEMODS.read().len()
}

// ── Software ISDB-T ─────────────────────────────────────────────────────

fn sw_init(_demod_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_sleep(_demod_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_tune(_demod_id: u32, _params: &IsdbtTuneParams) -> Result<(), &'static str> {
    Ok(())
}
fn sw_read_status(demod_id: u32) -> Result<IsdbtStats, &'static str> {
    let demods = ISDBT_DEMODS.read();
    let demod = demods.get(&demod_id).ok_or("ISDB-T demod not found")?;
    let locked = demod.state == IsdbtState::Tuning || demod.state == IsdbtState::Locked;
    Ok(IsdbtStats {
        signal_strength: if locked { 0x8000 } else { 0 },
        snr: if locked { 0x4000 } else { 0 },
        ber: 0,
        per: 0,
        ucblocks: 0,
        locked,
    })
}
fn sw_set_layer(_demod_id: u32, _layer: u8, _config: &IsdbtLayer) -> Result<(), &'static str> {
    Ok(())
}

/// Software ISDB-T ops.
pub fn software_isdbt_ops() -> IsdbtOps {
    IsdbtOps {
        init: sw_init,
        sleep: sw_sleep,
        tune: sw_tune,
        read_status: sw_read_status,
        set_layer: sw_set_layer,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_isdbt_ops();
    let demod_id = register_demod("sw-isdbt-demod", ops)?;

    // Initialize
    init_demod(demod_id)?;

    // Configure layer A (13 segments, QAM64, 3/4 FEC)
    let layer_a = IsdbtLayer {
        enabled: true,
        segments: 13,
        modulation: IsdbtModulation::Qam64,
        fec: IsdbtFec::Rate3_4,
        interleaving: 4,
    };
    set_layer(demod_id, 0, &layer_a)?;

    // Tune to 521 MHz (Japan ISDB-T channel 24)
    let params = IsdbtTuneParams {
        frequency: 521_000_000,
        bandwidth: IsdbtBandwidth::Bw6Mhz,
        transmission_mode: IsdbtTransmissionMode::Mode3,
        guard_interval: IsdbtGuardInterval::Gi1_4,
        partial_reception: false,
    };
    tune_demod(demod_id, &params)?;

    // Read status
    let stats = read_status(demod_id)?;
    if stats.locked {
        let _ = stats.snr;
    }

    Ok(())
}
