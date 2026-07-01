//! I2C bus adapter and device registration framework
//!
//! Provides adapter registration, device binding, and transfer helpers.
//! Includes a platform software bus with in-memory slave devices for systems
//! without dedicated I2C hardware.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum I2cTransferFlags {
    None = 0,
    TenBitAddr = 1,
    NoStart = 2,
}

/// Single I2C message within a transfer.
#[derive(Debug, Clone)]
pub struct I2cMsg {
    pub addr: u16,
    pub flags: u32,
    pub buf: Vec<u8>,
}

impl I2cMsg {
    pub fn write(addr: u16, data: &[u8]) -> Self {
        Self {
            addr,
            flags: 0,
            buf: data.to_vec(),
        }
    }

    pub fn read(addr: u16, len: usize) -> Self {
        Self {
            addr,
            flags: 0x01, // I2C_M_RD
            buf: vec![0u8; len],
        }
    }

    pub fn is_read(&self) -> bool {
        (self.flags & 0x01) != 0
    }
}

/// Device-level read/write handlers for software-backed slaves.
pub struct I2cDeviceOps {
    pub read: fn(offset: u8, buf: &mut [u8]) -> Result<(), &'static str>,
    pub write: fn(offset: u8, buf: &[u8]) -> Result<(), &'static str>,
}

pub struct I2cDevice {
    pub name: String,
    pub addr: u16,
    pub adapter_id: u32,
    pub ops: I2cDeviceOps,
}

pub struct I2cAdapterOps {
    pub transfer: fn(msgs: &mut [I2cMsg]) -> Result<usize, &'static str>,
    pub get_name: fn() -> &'static str,
}

struct I2cAdapter {
    id: u32,
    name: String,
    bus_num: u32,
    ops: I2cAdapterOps,
}

// ── Software platform bus ─────────────────────────────────────────────────

/// In-memory EEPROM-like device at address 0x50 (common SPD/EEPROM address).
struct VirtualEeprom {
    data: [u8; 256],
}

impl VirtualEeprom {
    const fn new() -> Self {
        let mut data = [0u8; 256];
        // Seed with identifiable header bytes.
        data[0] = b'R';
        data[1] = b'O';
        data[2] = b'S';
        data[3] = b'2';
        Self { data }
    }

    fn read(offset: u8, buf: &mut [u8]) -> Result<(), &'static str> {
        let eeprom = VIRTUAL_EEPROM.lock();
        for (i, byte) in buf.iter_mut().enumerate() {
            *byte = eeprom.data[(offset as usize).wrapping_add(i) % 256];
        }
        Ok(())
    }

    fn write(offset: u8, buf: &[u8]) -> Result<(), &'static str> {
        let mut eeprom = VIRTUAL_EEPROM.lock();
        for (i, &byte) in buf.iter().enumerate() {
            eeprom.data[(offset as usize).wrapping_add(i) % 256] = byte;
        }
        Ok(())
    }
}

static VIRTUAL_EEPROM: spin::Mutex<VirtualEeprom> = spin::Mutex::new(VirtualEeprom::new());

fn platform_transfer(msgs: &mut [I2cMsg]) -> Result<usize, &'static str> {
    let devices = I2C_DEVICES.read();
    let mut completed = 0usize;

    for msg in msgs.iter_mut() {
        let addr = msg.addr;
        let device = devices
            .values()
            .find(|d| d.adapter_id == 0 && d.addr == addr)
            .ok_or("I2C NACK: no device at address")?;

        if msg.is_read() {
            let offset = 0u8;
            (device.ops.read)(offset, &mut msg.buf)?;
        } else if msg.buf.is_empty() {
            return Err("I2C write message has empty buffer");
        } else {
            let offset = msg.buf[0];
            let payload = if msg.buf.len() > 1 {
                &msg.buf[1..]
            } else {
                &[]
            };
            (device.ops.write)(offset, payload)?;
        }
        completed += 1;
    }

    Ok(completed)
}

const PLATFORM_ADAPTER_OPS: I2cAdapterOps = I2cAdapterOps {
    transfer: platform_transfer,
    get_name: || "platform-i2c",
};

// ── Registry ────────────────────────────────────────────────────────────

static I2C_ADAPTERS: RwLock<BTreeMap<u32, I2cAdapter>> = RwLock::new(BTreeMap::new());
static I2C_DEVICES: RwLock<BTreeMap<u32, I2cDevice>> = RwLock::new(BTreeMap::new());
static NEXT_ADAPTER_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_DEVICE_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_adapter(name: &str, bus_num: u32, ops: I2cAdapterOps) -> Result<u32, &'static str> {
    let id = NEXT_ADAPTER_ID.fetch_add(1, Ordering::SeqCst);
    I2C_ADAPTERS.write().insert(
        id,
        I2cAdapter {
            id,
            name: String::from(name),
            bus_num,
            ops,
        },
    );
    Ok(id)
}

pub fn register_device(
    adapter_id: u32,
    name: &str,
    addr: u16,
    ops: I2cDeviceOps,
) -> Result<u32, &'static str> {
    if addr > 0x3FF {
        return Err("I2C address out of range");
    }
    if !I2C_ADAPTERS.read().contains_key(&adapter_id) {
        return Err("I2C adapter not found");
    }

    let id = NEXT_DEVICE_ID.fetch_add(1, Ordering::SeqCst);
    I2C_DEVICES.write().insert(
        id,
        I2cDevice {
            name: String::from(name),
            addr,
            adapter_id,
            ops,
        },
    );
    Ok(id)
}

pub fn i2c_transfer(adapter_id: u32, msgs: &mut [I2cMsg]) -> Result<usize, &'static str> {
    let adapters = I2C_ADAPTERS.read();
    let adapter = adapters.get(&adapter_id).ok_or("I2C adapter not found")?;
    (adapter.ops.transfer)(msgs)
}

pub fn i2c_write(adapter_id: u32, addr: u16, offset: u8, data: &[u8]) -> Result<(), &'static str> {
    let mut buf = Vec::with_capacity(1 + data.len());
    buf.push(offset);
    buf.extend_from_slice(data);
    let mut msgs = [I2cMsg::write(addr, &buf)];
    i2c_transfer(adapter_id, &mut msgs)?;
    Ok(())
}

pub fn i2c_read(
    adapter_id: u32,
    addr: u16,
    offset: u8,
    len: usize,
) -> Result<Vec<u8>, &'static str> {
    let mut msgs = [I2cMsg::write(addr, &[offset]), I2cMsg::read(addr, len)];
    i2c_transfer(adapter_id, &mut msgs)?;
    Ok(msgs[1].buf.clone())
}

pub fn adapter_count() -> usize {
    I2C_ADAPTERS.read().len()
}

pub fn device_count() -> usize {
    I2C_DEVICES.read().len()
}

fn register_platform_devices() -> Result<(), &'static str> {
    register_device(
        0,
        "virtual-eeprom",
        0x50,
        I2cDeviceOps {
            read: VirtualEeprom::read,
            write: VirtualEeprom::write,
        },
    )?;
    Ok(())
}

/// Initialize I2C subsystem with platform software bus and virtual EEPROM.
pub fn init() -> Result<(), &'static str> {
    if !I2C_ADAPTERS.read().is_empty() {
        return Ok(());
    }

    register_adapter("platform-i2c", 0, PLATFORM_ADAPTER_OPS)?;
    register_platform_devices()?;
    crate::serial_println!("i2c: platform bus 0, {} device(s)", device_count());
    Ok(())
}
