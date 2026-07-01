//! COMEDI data-acquisition subsystem (mirrors Linux `drivers/comedi/`)
//!
//! Registers DAQ devices composed of subdevices (analog-in, analog-out,
//! digital-io) and performs single-sample reads/writes against them.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubdevKind {
    AnalogInput,
    AnalogOutput,
    DigitalIo,
}

#[derive(Clone)]
struct Subdevice {
    index: u32,
    kind: SubdevKind,
    channels: u16,
    maxdata: u32,
    /// Last value latched per channel.
    samples: Vec<u32>,
}

struct ComediDevice {
    id: u32,
    name: String,
    subdevices: Vec<Subdevice>,
}

// ── Registry ──────────────────────────────────────────────────────────────

static DEVS: RwLock<BTreeMap<u32, ComediDevice>> = RwLock::new(BTreeMap::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_device(name: &str) -> u32 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    DEVS.write().insert(
        id,
        ComediDevice {
            id,
            name: String::from(name),
            subdevices: Vec::new(),
        },
    );
    id
}

pub fn add_subdevice(
    dev_id: u32,
    kind: SubdevKind,
    channels: u16,
    maxdata: u32,
) -> Result<u32, &'static str> {
    let mut devs = DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("comedi: device not found")?;
    let index = dev.subdevices.len() as u32;
    dev.subdevices.push(Subdevice {
        index,
        kind,
        channels,
        maxdata,
        samples: alloc::vec![0u32; channels as usize],
    });
    Ok(index)
}

pub fn data_write(dev_id: u32, subdev: u32, channel: u16, value: u32) -> Result<(), &'static str> {
    let mut devs = DEVS.write();
    let dev = devs.get_mut(&dev_id).ok_or("comedi: device not found")?;
    let sd = dev
        .subdevices
        .get_mut(subdev as usize)
        .ok_or("comedi: subdevice not found")?;
    if matches!(sd.kind, SubdevKind::AnalogInput) {
        return Err("comedi: cannot write to analog input");
    }
    if channel >= sd.channels {
        return Err("comedi: channel out of range");
    }
    if value > sd.maxdata {
        return Err("comedi: value exceeds maxdata");
    }
    sd.samples[channel as usize] = value;
    Ok(())
}

pub fn data_read(dev_id: u32, subdev: u32, channel: u16) -> Result<u32, &'static str> {
    let devs = DEVS.read();
    let dev = devs.get(&dev_id).ok_or("comedi: device not found")?;
    let sd = dev
        .subdevices
        .get(subdev as usize)
        .ok_or("comedi: subdevice not found")?;
    if channel >= sd.channels {
        return Err("comedi: channel out of range");
    }
    Ok(sd.samples[channel as usize])
}

pub fn subdevice_count(dev_id: u32) -> usize {
    DEVS.read()
        .get(&dev_id)
        .map(|d| d.subdevices.len())
        .unwrap_or(0)
}

pub fn device_count() -> usize {
    DEVS.read().len()
}

/// Initialize COMEDI with a software multifunction DAQ device.
pub fn init() -> Result<(), &'static str> {
    if !DEVS.read().is_empty() {
        return Ok(());
    }
    let dev = register_device("comedi0");
    add_subdevice(dev, SubdevKind::AnalogInput, 16, 0xFFFF)?;
    add_subdevice(dev, SubdevKind::AnalogOutput, 2, 0xFFFF)?;
    add_subdevice(dev, SubdevKind::DigitalIo, 8, 1)?;
    crate::serial_println!(
        "comedi: device comedi0 with {} subdevice(s)",
        subdevice_count(dev)
    );
    Ok(())
}
