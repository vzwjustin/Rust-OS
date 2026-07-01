//! HWRNG (Hardware Random Number Generator) subsystem
//!
//! Provides framework for hardware RNG devices.
//! Mirrors Linux's `drivers/char/hw_random/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// HWRNG device (Linux `struct hwrng`).
pub struct HwrngDevice {
    pub id: u32,
    pub name: String,
    pub ops: HwrngOps,
    pub quality: u32,
    pub active: bool,
    pub bytes_read: u64,
    pub seed_present: bool,
}

/// HWRNG operations (Linux `struct hwrng` callbacks).
pub struct HwrngOps {
    pub init: fn(dev_id: u32) -> Result<(), &'static str>,
    pub cleanup: fn(dev_id: u32) -> Result<(), &'static str>,
    pub data_present: fn(dev_id: u32) -> Result<bool, &'static str>,
    pub data_read: fn(dev_id: u32, data: &mut [u8]) -> Result<usize, &'static str>,
    pub read: fn(dev_id: u32, data: &mut [u8], wait: bool) -> Result<usize, &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static HWRNG_DEVS: RwLock<BTreeMap<u32, HwrngDevice>> = RwLock::new(BTreeMap::new());
static CURRENT_RNG: RwLock<Option<u32>> = RwLock::new(None);

// ── Public API ──────────────────────────────────────────────────────────

/// Register an HWRNG device (Linux `hwrng_register`).
pub fn register_device(name: &str, quality: u32, ops: HwrngOps) -> Result<u32, &'static str> {
    if name.is_empty() {
        return Err("HWRNG device name is empty");
    }
    if quality > 1024 {
        return Err("HWRNG quality out of range");
    }
    if HWRNG_DEVS.read().values().any(|dev| dev.name == name) {
        return Err("HWRNG device already registered");
    }

    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = HwrngDevice {
        id,
        name: String::from(name),
        ops,
        quality,
        active: false,
        bytes_read: 0,
        seed_present: false,
    };
    HWRNG_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Initialize an HWRNG device (Linux `hwrng_init`).
pub fn init_device(dev_id: u32) -> Result<(), &'static str> {
    let init_fn = {
        let devs = HWRNG_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("HWRNG device not found")?;
        dev.ops.init
    };
    (init_fn)(dev_id)?;

    let mut devs = HWRNG_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.active = true;
        dev.seed_present = true;
    }
    drop(devs);

    let mut current = CURRENT_RNG.write();
    if current.is_none() {
        *current = Some(dev_id);
    }
    Ok(())
}

/// Select the current RNG (Linux `hwrng_select`).
pub fn select_rng(dev_id: u32) -> Result<(), &'static str> {
    {
        let devs = HWRNG_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("HWRNG device not found")?;
        if !dev.active {
            return Err("HWRNG device not active");
        }
    }
    *CURRENT_RNG.write() = Some(dev_id);
    Ok(())
}

/// Read random data from the current RNG (Linux `hwrng_data_read`).
pub fn read_random(buf: &mut [u8]) -> Result<usize, &'static str> {
    if buf.is_empty() {
        return Err("HWRNG read buffer is empty");
    }
    let dev_id = CURRENT_RNG.read().ok_or("No HWRNG selected")?;
    let (read_fn, data_present_fn) = {
        let devs = HWRNG_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("HWRNG device not found")?;
        if !dev.active {
            return Err("HWRNG device not active");
        }
        (dev.ops.read, dev.ops.data_present)
    };

    // Check if data is available
    let present = (data_present_fn)(dev_id)?;
    if !present {
        return Err("HWRNG data not available");
    }

    let n = (read_fn)(dev_id, buf, true)?;

    let mut devs = HWRNG_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.bytes_read = dev
            .bytes_read
            .checked_add(n as u64)
            .ok_or("HWRNG byte counter overflow")?;
    }
    Ok(n)
}

/// Read a specific number of random bytes (Linux `rng_get_data`).
pub fn read_bytes(count: usize) -> Result<Vec<u8>, &'static str> {
    if count == 0 {
        return Err("HWRNG read length is zero");
    }
    let mut buf = Vec::new();
    buf.resize(count, 0);
    let mut total = 0;
    while total < count {
        let n = read_random(&mut buf[total..])?;
        if n == 0 {
            break;
        }
        total += n;
    }
    buf.truncate(total);
    Ok(buf)
}

/// Get the current RNG name.
pub fn current_rng_name() -> Result<String, &'static str> {
    let dev_id = CURRENT_RNG.read().ok_or("No HWRNG selected")?;
    let devs = HWRNG_DEVS.read();
    let dev = devs.get(&dev_id).ok_or("HWRNG device not found")?;
    Ok(dev.name.clone())
}

/// List all HWRNG devices.
pub fn list_devices() -> Vec<(u32, String, u32, bool, u64)> {
    let current = *CURRENT_RNG.read();
    HWRNG_DEVS
        .read()
        .iter()
        .map(|(id, d)| {
            (
                *id,
                d.name.clone(),
                d.quality,
                current == Some(*id),
                d.bytes_read,
            )
        })
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    HWRNG_DEVS.read().len()
}

// ── Software HWRNG ──────────────────────────────────────────────────────

fn sw_init(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_cleanup(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_data_present(_dev_id: u32) -> Result<bool, &'static str> {
    Ok(true)
}
fn sw_data_read(_dev_id: u32, data: &mut [u8]) -> Result<usize, &'static str> {
    static SEED: AtomicU32 = AtomicU32::new(0xDEAD_BEEF);
    for b in data.iter_mut() {
        let mut s = SEED.load(Ordering::Relaxed);
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        SEED.store(s, Ordering::Relaxed);
        *b = s as u8;
    }
    Ok(data.len())
}
fn sw_read(_dev_id: u32, data: &mut [u8], _wait: bool) -> Result<usize, &'static str> {
    sw_data_read(_dev_id, data)
}

/// Software HWRNG ops.
pub fn software_hwrng_ops() -> HwrngOps {
    HwrngOps {
        init: sw_init,
        cleanup: sw_cleanup,
        data_present: sw_data_present,
        data_read: sw_data_read,
        read: sw_read,
    }
}

// ── VirtIO RNG bridge ───────────────────────────────────────────────────

fn virtio_rng_init(_dev_id: u32) -> Result<(), &'static str> {
    if !crate::drivers::virtio::rng::is_available() {
        return Err("virtio-rng not initialized");
    }
    Ok(())
}

fn virtio_rng_cleanup(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn virtio_rng_data_present(_dev_id: u32) -> Result<bool, &'static str> {
    Ok(crate::drivers::virtio::rng::data_available())
}

fn virtio_rng_data_read(_dev_id: u32, data: &mut [u8]) -> Result<usize, &'static str> {
    crate::drivers::virtio::rng::read_random(data)
}

fn virtio_rng_read(dev_id: u32, data: &mut [u8], _wait: bool) -> Result<usize, &'static str> {
    virtio_rng_data_read(dev_id, data)
}

fn virtio_rng_ops() -> HwrngOps {
    HwrngOps {
        init: virtio_rng_init,
        cleanup: virtio_rng_cleanup,
        data_present: virtio_rng_data_present,
        data_read: virtio_rng_data_read,
        read: virtio_rng_read,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !HWRNG_DEVS.read().is_empty() {
        return Ok(());
    }

    let sw_id = register_device("software-rng", 512, software_hwrng_ops())?;
    init_device(sw_id)?;
    crate::serial_println!("hwrng: software RNG registered (id={}, quality=512)", sw_id);

    if crate::drivers::virtio::rng::is_available() {
        let v_id = register_device("virtio-rng", 1000, virtio_rng_ops())?;
        init_device(v_id)?;
        select_rng(v_id)?;
        crate::serial_println!("hwrng: virtio-rng registered and selected (id={})", v_id);
    }
    Ok(())
}
