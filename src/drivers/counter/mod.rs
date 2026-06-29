//! Counter subsystem
//!
//! Provides counter framework for counting events, signals, and positions.
//! Mirrors Linux's `drivers/counter/counter-core.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Counter count direction (Linux `enum counter_count_direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountDirection {
    Forward,
    Backward,
}

/// Counter count mode (Linux `enum counter_count_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountMode {
    Normal,
    RangeLimit,
    NonRecycle,
    Modulo,
}

/// Counter signal (Linux `struct counter_signal`).
pub struct CounterSignal {
    pub id: u32,
    pub name: String,
    pub value: CounterSignalValue,
}

/// Counter signal value (Linux `enum counter_signal_value`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterSignalValue {
    Low,
    High,
}

/// Counter count (Linux `struct counter_count`).
pub struct CounterCount {
    pub id: u32,
    pub name: String,
    pub function: CounterFunction,
    pub value: u64,
    pub direction: CountDirection,
    pub mode: CountMode,
    pub signal_ids: Vec<u32>,
}

/// Counter function (Linux `enum counter_function`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CounterFunction {
    Increase,
    Decrease,
    PulseDirection,
    QuadratureX1,
    QuadratureX2,
    QuadratureX4,
}

/// Counter sync mode (Linux `enum counter_synapse_action`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynapseAction {
    None,
    RisingEdge,
    FallingEdge,
    BothEdges,
}

/// Counter synapse (Linux `struct counter_synapse`).
pub struct CounterSynapse {
    pub signal_id: u32,
    pub action: SynapseAction,
}

/// Counter device operations (Linux `struct counter_ops`).
pub struct CounterOps {
    pub signal_read: fn(device_id: u32, signal_id: u32) -> Result<CounterSignalValue, &'static str>,
    pub count_read: fn(device_id: u32, count_id: u32) -> Result<u64, &'static str>,
    pub count_write: fn(device_id: u32, count_id: u32, value: u64) -> Result<(), &'static str>,
    pub function_set:
        fn(device_id: u32, count_id: u32, function: CounterFunction) -> Result<(), &'static str>,
    pub action_set: fn(
        device_id: u32,
        count_id: u32,
        synapse_index: u32,
        action: SynapseAction,
    ) -> Result<(), &'static str>,
}

/// Counter device (Linux `struct counter_device`).
pub struct CounterDevice {
    pub name: String,
    pub ops: CounterOps,
    pub signals: Vec<CounterSignal>,
    pub counts: Vec<CounterCount>,
    pub synapses: Vec<CounterSynapse>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEVICE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static COUNTER_DEVICES: RwLock<BTreeMap<u32, CounterDevice>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a counter device.
pub fn register_device(device: CounterDevice) -> Result<u32, &'static str> {
    let id = DEVICE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    COUNTER_DEVICES.write().insert(id, device);
    Ok(id)
}

/// Unregister a counter device.
pub fn unregister_device(device_id: u32) -> Result<(), &'static str> {
    if COUNTER_DEVICES.write().remove(&device_id).is_none() {
        return Err("Counter device not found");
    }
    Ok(())
}

/// Read a signal value.
pub fn signal_read(device_id: u32, signal_id: u32) -> Result<CounterSignalValue, &'static str> {
    let read_fn = {
        let devices = COUNTER_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("Counter device not found")?;
        dev.ops.signal_read
    };
    (read_fn)(device_id, signal_id)
}

/// Read a count value.
pub fn count_read(device_id: u32, count_id: u32) -> Result<u64, &'static str> {
    let read_fn = {
        let devices = COUNTER_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("Counter device not found")?;
        dev.ops.count_read
    };
    (read_fn)(device_id, count_id)
}

/// Write a count value.
pub fn count_write(device_id: u32, count_id: u32, value: u64) -> Result<(), &'static str> {
    let write_fn = {
        let devices = COUNTER_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("Counter device not found")?;
        dev.ops.count_write
    };
    (write_fn)(device_id, count_id, value)?;

    let mut devices = COUNTER_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        if let Some(count) = dev.counts.get_mut(count_id as usize) {
            count.value = value;
        }
    }
    Ok(())
}

/// Set the counting function.
pub fn function_set(
    device_id: u32,
    count_id: u32,
    function: CounterFunction,
) -> Result<(), &'static str> {
    let set_fn = {
        let devices = COUNTER_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("Counter device not found")?;
        dev.ops.function_set
    };
    (set_fn)(device_id, count_id, function)?;

    let mut devices = COUNTER_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        if let Some(count) = dev.counts.get_mut(count_id as usize) {
            count.function = function;
        }
    }
    Ok(())
}

/// Set a synapse action.
pub fn action_set(
    device_id: u32,
    count_id: u32,
    synapse_index: u32,
    action: SynapseAction,
) -> Result<(), &'static str> {
    let set_fn = {
        let devices = COUNTER_DEVICES.read();
        let dev = devices.get(&device_id).ok_or("Counter device not found")?;
        dev.ops.action_set
    };
    (set_fn)(device_id, count_id, synapse_index, action)?;

    let mut devices = COUNTER_DEVICES.write();
    if let Some(dev) = devices.get_mut(&device_id) {
        if let Some(syn) = dev.synapses.get_mut(synapse_index as usize) {
            syn.action = action;
        }
    }
    Ok(())
}

/// Get a snapshot of a counter device's counts.
pub fn get_counts(device_id: u32) -> Result<Vec<(u32, String, u64)>, &'static str> {
    let devices = COUNTER_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("Counter device not found")?;
    Ok(dev
        .counts
        .iter()
        .map(|c| (c.id, c.name.clone(), c.value))
        .collect())
}

/// Get a snapshot of a counter device's signals.
pub fn get_signals(device_id: u32) -> Result<Vec<(u32, String, CounterSignalValue)>, &'static str> {
    let devices = COUNTER_DEVICES.read();
    let dev = devices.get(&device_id).ok_or("Counter device not found")?;
    Ok(dev
        .signals
        .iter()
        .map(|s| (s.id, s.name.clone(), s.value))
        .collect())
}

/// List all registered counter devices.
pub fn list_devices() -> Vec<(u32, String)> {
    COUNTER_DEVICES
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone()))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    COUNTER_DEVICES.read().len()
}

// ── Software counter ────────────────────────────────────────────────────

fn sw_signal_read(_dev_id: u32, _sig_id: u32) -> Result<CounterSignalValue, &'static str> {
    Ok(CounterSignalValue::Low)
}

fn sw_count_read(_dev_id: u32, count_id: u32) -> Result<u64, &'static str> {
    let devices = COUNTER_DEVICES.read();
    let dev = devices.iter().next().ok_or("No counter device")?;
    dev.1
        .counts
        .get(count_id as usize)
        .map(|c| c.value)
        .ok_or("Count not found")
}

fn sw_count_write(_dev_id: u32, _count_id: u32, _value: u64) -> Result<(), &'static str> {
    Ok(())
}
fn sw_function_set(
    _dev_id: u32,
    _count_id: u32,
    _func: CounterFunction,
) -> Result<(), &'static str> {
    Ok(())
}
fn sw_action_set(
    _dev_id: u32,
    _count_id: u32,
    _syn_idx: u32,
    _action: SynapseAction,
) -> Result<(), &'static str> {
    Ok(())
}

/// Software counter ops.
pub fn software_counter_ops() -> CounterOps {
    CounterOps {
        signal_read: sw_signal_read,
        count_read: sw_count_read,
        count_write: sw_count_write,
        function_set: sw_function_set,
        action_set: sw_action_set,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_counter_ops();

    let mut signals = Vec::new();
    signals.push(CounterSignal {
        id: 0,
        name: String::from("SIG0"),
        value: CounterSignalValue::Low,
    });
    signals.push(CounterSignal {
        id: 1,
        name: String::from("SIG1"),
        value: CounterSignalValue::Low,
    });

    let mut counts = Vec::new();
    let mut sig_ids = Vec::new();
    sig_ids.push(0);
    sig_ids.push(1);
    counts.push(CounterCount {
        id: 0,
        name: String::from("COUNT0"),
        function: CounterFunction::Increase,
        value: 0,
        direction: CountDirection::Forward,
        mode: CountMode::Normal,
        signal_ids: sig_ids,
    });

    let mut synapses = Vec::new();
    synapses.push(CounterSynapse {
        signal_id: 0,
        action: SynapseAction::RisingEdge,
    });
    synapses.push(CounterSynapse {
        signal_id: 1,
        action: SynapseAction::RisingEdge,
    });

    let dev = CounterDevice {
        name: String::from("sw-counter"),
        ops,
        signals,
        counts,
        synapses,
    };
    register_device(dev)?;
    Ok(())
}
