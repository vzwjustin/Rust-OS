//! SoundWire subsystem
//!
//! Provides SoundWire (SDW) bus framework for audio peripheral communication.
//! Mirrors Linux's `drivers/soundwire/soundwire.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// SoundWire device (Linux `struct sdw_slave`).
pub struct SdwSlave {
    pub id: u32,
    pub bus_id: u32,
    pub name: String,
    pub dev_num: u8,
    pub dev_id: SdwDevId,
    pub prop: SdwProp,
    pub state: SdwDevState,
    pub driver_name: Option<String>,
}

/// SoundWire device ID (Linux `struct sdw_slave_id`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SdwDevId {
    pub mfg_id: u16,
    pub part_id: u16,
    pub class: u8,
    pub unique_id: u8,
}

/// SoundWire device properties (Linux `struct sdw_slave_prop`).
#[derive(Debug, Clone)]
pub struct SdwProp {
    pub mcp_behaviour: u8,
    pub wake_capable: bool,
    pub test_mode_capable: bool,
    pub clk_stop_mode1: bool,
    pub simple_clk_stop_capable: bool,
    pub clk_stop_timeout: u32,
    pub ch_prep_timeout: u32,
    pub reset_behaviour: u8,
    pub high_PHY_capable: bool,
    pub paging_support: bool,
    pub bank_delay_support: bool,
    pub p15_behaviour: bool,
    pub lane_control_support: bool,
    pub max_clk_freq: u32,
    pub num_ports: u32,
}

/// SoundWire device state (Linux `enum sdw_slave_status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SdwDevState {
    Offline,
    Online,
    Alert,
    Unattached,
    Attached,
}

/// SoundWire bus/master (Linux `struct sdw_bus` / `struct sdw_master`).
pub struct SdwBus {
    pub id: u32,
    pub name: String,
    pub ops: SdwBusOps,
    pub slave_ids: Vec<u32>,
    pub link_sync_mask: u32,
    pub clk_freq: u32,
    pub active: bool,
}

/// SoundWire bus operations (Linux `struct sdw_master_ops`).
pub struct SdwBusOps {
    pub read_prop: fn(bus_id: u32) -> Result<(), &'static str>,
    pub xfer_msg: fn(bus_id: u32, slave_id: u32, msg: &SdwMsg) -> Result<(), &'static str>,
    pub set_bus_conf: fn(bus_id: u32, conf: &SdwBusConf) -> Result<(), &'static str>,
    pub prep_bank_switch: fn(bus_id: u32) -> Result<(), &'static str>,
    pub enable_clk_stop: fn(bus_id: u32) -> Result<(), &'static str>,
    pub disable_clk_stop: fn(bus_id: u32) -> Result<(), &'static str>,
}

/// SoundWire message (Linux `struct sdw_msg`).
#[derive(Debug, Clone)]
pub struct SdwMsg {
    pub addr: u32,
    pub len: u32,
    pub dev_num: u8,
    pub read: bool,
    pub data: Vec<u8>,
}

/// SoundWire bus configuration (Linux `struct sdw_bus_params`).
#[derive(Debug, Clone)]
pub struct SdwBusConf {
    pub curr_bank: u8,
    pub next_bank: u8,
    pub clk_freq: u32,
    pub col: u16,
    pub row: u16,
}

// ── Registry ────────────────────────────────────────────────────────────

static BUS_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static SLAVE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static SDW_BUSES: RwLock<BTreeMap<u32, SdwBus>> = RwLock::new(BTreeMap::new());
static SDW_SLAVES: RwLock<BTreeMap<u32, SdwSlave>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a SoundWire bus/master.
pub fn register_bus(name: &str, ops: SdwBusOps, clk_freq: u32) -> Result<u32, &'static str> {
    let id = BUS_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let bus = SdwBus {
        id,
        name: String::from(name),
        ops,
        slave_ids: Vec::new(),
        link_sync_mask: 0,
        clk_freq,
        active: false,
    };
    SDW_BUSES.write().insert(id, bus);
    Ok(id)
}

/// Initialize a SoundWire bus (Linux `sdw_bus_startup`).
pub fn init_bus(bus_id: u32) -> Result<(), &'static str> {
    let read_fn = {
        let buses = SDW_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("SoundWire bus not found")?;
        bus.ops.read_prop
    };
    (read_fn)(bus_id)?;

    let mut buses = SDW_BUSES.write();
    if let Some(bus) = buses.get_mut(&bus_id) {
        bus.active = true;
    }
    Ok(())
}

/// Register a SoundWire slave device.
pub fn register_slave(
    bus_id: u32,
    name: &str,
    dev_id: SdwDevId,
    prop: SdwProp,
) -> Result<u32, &'static str> {
    let slave_id = SLAVE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let slave = SdwSlave {
        id: slave_id,
        bus_id,
        name: String::from(name),
        dev_num: 0,
        dev_id,
        prop,
        state: SdwDevState::Unattached,
        driver_name: None,
    };
    SDW_SLAVES.write().insert(slave_id, slave);

    let mut buses = SDW_BUSES.write();
    if let Some(bus) = buses.get_mut(&bus_id) {
        bus.slave_ids.push(slave_id);
    }
    Ok(slave_id)
}

/// Assign a device number to a slave (Linux `sdw_assign_device_num`).
pub fn assign_device_num(slave_id: u32, dev_num: u8) -> Result<(), &'static str> {
    let mut slaves = SDW_SLAVES.write();
    let slave = slaves
        .get_mut(&slave_id)
        .ok_or("SoundWire slave not found")?;
    slave.dev_num = dev_num;
    slave.state = SdwDevState::Attached;
    Ok(())
}

/// Transfer a message to/from a SoundWire slave.
pub fn xfer_msg(bus_id: u32, slave_id: u32, msg: &SdwMsg) -> Result<(), &'static str> {
    let xfer_fn = {
        let buses = SDW_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("SoundWire bus not found")?;
        bus.ops.xfer_msg
    };
    (xfer_fn)(bus_id, slave_id, msg)
}

/// Set bus configuration.
pub fn set_bus_conf(bus_id: u32, conf: &SdwBusConf) -> Result<(), &'static str> {
    let set_fn = {
        let buses = SDW_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("SoundWire bus not found")?;
        bus.ops.set_bus_conf
    };
    (set_fn)(bus_id, conf)
}

/// Prepare for bank switch.
pub fn prep_bank_switch(bus_id: u32) -> Result<(), &'static str> {
    let prep_fn = {
        let buses = SDW_BUSES.read();
        let bus = buses.get(&bus_id).ok_or("SoundWire bus not found")?;
        bus.ops.prep_bank_switch
    };
    (prep_fn)(bus_id)
}

/// List all SoundWire buses.
pub fn list_buses() -> Vec<(u32, String, bool)> {
    SDW_BUSES
        .read()
        .iter()
        .map(|(id, b)| (*id, b.name.clone(), b.active))
        .collect()
}

/// List slaves on a bus.
pub fn list_slaves(bus_id: u32) -> Result<Vec<(u32, String, u8, SdwDevState)>, &'static str> {
    let buses = SDW_BUSES.read();
    let bus = buses.get(&bus_id).ok_or("SoundWire bus not found")?;
    let slaves = SDW_SLAVES.read();
    let mut result = Vec::new();
    for &sid in &bus.slave_ids {
        if let Some(slave) = slaves.get(&sid) {
            result.push((slave.id, slave.name.clone(), slave.dev_num, slave.state));
        }
    }
    Ok(result)
}

/// Count registered buses.
pub fn bus_count() -> usize {
    SDW_BUSES.read().len()
}

// ── Software SoundWire ──────────────────────────────────────────────────

fn sw_read_prop(_bus_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_xfer_msg(_bus_id: u32, _slave_id: u32, _msg: &SdwMsg) -> Result<(), &'static str> {
    Ok(())
}
fn sw_set_bus_conf(_bus_id: u32, _conf: &SdwBusConf) -> Result<(), &'static str> {
    Ok(())
}
fn sw_prep_bank_switch(_bus_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_enable_clk_stop(_bus_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_disable_clk_stop(_bus_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software SoundWire bus ops.
pub fn software_sdw_ops() -> SdwBusOps {
    SdwBusOps {
        read_prop: sw_read_prop,
        xfer_msg: sw_xfer_msg,
        set_bus_conf: sw_set_bus_conf,
        prep_bank_switch: sw_prep_bank_switch,
        enable_clk_stop: sw_enable_clk_stop,
        disable_clk_stop: sw_disable_clk_stop,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_sdw_ops();
    let bus_id = register_bus("sw-sdw-0", ops, 9_600_000)?;
    init_bus(bus_id)?;

    // Register a codec slave
    let dev_id = SdwDevId {
        mfg_id: 0x01FA,
        part_id: 0xAAAA,
        class: 0x00,
        unique_id: 0x00,
    };
    let prop = SdwProp {
        mcp_behaviour: 0,
        wake_capable: false,
        test_mode_capable: false,
        clk_stop_mode1: false,
        simple_clk_stop_capable: true,
        clk_stop_timeout: 100,
        ch_prep_timeout: 100,
        reset_behaviour: 0,
        high_PHY_capable: false,
        paging_support: false,
        bank_delay_support: false,
        p15_behaviour: false,
        lane_control_support: false,
        max_clk_freq: 9_600_000,
        num_ports: 4,
    };
    let slave_id = register_slave(bus_id, "sw-sdw-codec", dev_id, prop)?;
    assign_device_num(slave_id, 1)?;

    Ok(())
}
