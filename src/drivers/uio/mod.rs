//! UIO (Userspace I/O) subsystem
//!
//! Provides a framework for exposing device interrupts and memory mappings
//! to userspace. Mirrors Linux's `drivers/uio/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// UIO memory region (Linux `struct uio_mem`).
#[derive(Debug, Clone)]
pub struct UioMem {
    pub name: String,
    pub addr: u64,
    pub size: u64,
    pub memtype: UioMemType,
}

/// UIO memory type (Linux `enum uio_mem_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UioMemType {
    Physical,
    Logical,
    Virtual,
}

/// UIO port region (Linux `struct uio_port`).
#[derive(Debug, Clone)]
pub struct UioPort {
    pub name: String,
    pub start: u16,
    pub size: u16,
    pub porttype: UioPortType,
}

/// UIO port type (Linux `enum uio_port_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UioPortType {
    None,
    X86Ioport,
    Gpio,
}

/// UIO device (Linux `struct uio_device`).
pub struct UioDevice {
    pub id: u32,
    pub name: String,
    pub dev_id: u32,
    pub mem_regions: Vec<UioMem>,
    pub port_regions: Vec<UioPort>,
    pub irq_count: u32,
    pub irq_enabled: bool,
    pub listener_count: u32,
    pub ops: UioOps,
}

/// UIO device operations (Linux `struct uio_device_ops`).
pub struct UioOps {
    pub open: fn(dev_id: u32) -> Result<(), &'static str>,
    pub release: fn(dev_id: u32) -> Result<(), &'static str>,
    pub irq_handler: fn(dev_id: u32) -> Result<IrqReturn, &'static str>,
    pub irqcontrol: fn(dev_id: u32, irq_on: bool) -> Result<(), &'static str>,
}

/// IRQ return value (Linux `enum irqreturn`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IrqReturn {
    None,
    Handled,
    WakeThread,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static UIO_DEVS: RwLock<BTreeMap<u32, UioDevice>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a UIO device.
pub fn register_device(
    name: &str,
    dev_id: u32,
    mem_regions: Vec<UioMem>,
    port_regions: Vec<UioPort>,
    irq_count: u32,
    ops: UioOps,
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = UioDevice {
        id,
        name: String::from(name),
        dev_id,
        mem_regions,
        port_regions,
        irq_count,
        irq_enabled: false,
        listener_count: 0,
        ops,
    };
    UIO_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Unregister a UIO device.
pub fn unregister_device(uio_id: u32) -> Result<(), &'static str> {
    UIO_DEVS
        .write()
        .remove(&uio_id)
        .ok_or("UIO device not found")?;
    Ok(())
}

/// Open a UIO device (userspace open).
pub fn open(uio_id: u32) -> Result<(), &'static str> {
    let ops_fn = {
        let devs = UIO_DEVS.read();
        let dev = devs.get(&uio_id).ok_or("UIO device not found")?;
        dev.ops.open
    };
    (ops_fn)(uio_id)?;
    let mut devs = UIO_DEVS.write();
    if let Some(dev) = devs.get_mut(&uio_id) {
        dev.listener_count += 1;
    }
    Ok(())
}

/// Release a UIO device (userspace close).
pub fn release(uio_id: u32) -> Result<(), &'static str> {
    let ops_fn = {
        let devs = UIO_DEVS.read();
        let dev = devs.get(&uio_id).ok_or("UIO device not found")?;
        dev.ops.release
    };
    (ops_fn)(uio_id)?;
    let mut devs = UIO_DEVS.write();
    if let Some(dev) = devs.get_mut(&uio_id) {
        dev.listener_count = dev.listener_count.saturating_sub(1);
    }
    Ok(())
}

/// Handle an interrupt for a UIO device.
pub fn irq_handler(uio_id: u32) -> Result<IrqReturn, &'static str> {
    let ops_fn = {
        let devs = UIO_DEVS.read();
        let dev = devs.get(&uio_id).ok_or("UIO device not found")?;
        dev.ops.irq_handler
    };
    (ops_fn)(uio_id)
}

/// Enable/disable interrupts for a UIO device.
pub fn irqcontrol(uio_id: u32, irq_on: bool) -> Result<(), &'static str> {
    let ops_fn = {
        let devs = UIO_DEVS.read();
        let dev = devs.get(&uio_id).ok_or("UIO device not found")?;
        dev.ops.irqcontrol
    };
    (ops_fn)(uio_id, irq_on)?;
    let mut devs = UIO_DEVS.write();
    if let Some(dev) = devs.get_mut(&uio_id) {
        dev.irq_enabled = irq_on;
    }
    Ok(())
}

/// Get memory mappings for a UIO device.
pub fn get_mem_regions(uio_id: u32) -> Result<Vec<UioMem>, &'static str> {
    let devs = UIO_DEVS.read();
    let dev = devs.get(&uio_id).ok_or("UIO device not found")?;
    Ok(dev.mem_regions.clone())
}

/// Get port mappings for a UIO device.
pub fn get_port_regions(uio_id: u32) -> Result<Vec<UioPort>, &'static str> {
    let devs = UIO_DEVS.read();
    let dev = devs.get(&uio_id).ok_or("UIO device not found")?;
    Ok(dev.port_regions.clone())
}

/// List all UIO devices.
pub fn list_devices() -> Vec<(u32, String, u32, u32)> {
    UIO_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.irq_count, d.listener_count))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    UIO_DEVS.read().len()
}

// ── Software UIO ────────────────────────────────────────────────────────

fn sw_open(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_release(_dev_id: u32) -> Result<(), &'static str> {
    Ok(())
}

fn sw_irq_handler(_dev_id: u32) -> Result<IrqReturn, &'static str> {
    Ok(IrqReturn::Handled)
}

fn sw_irqcontrol(_dev_id: u32, _irq_on: bool) -> Result<(), &'static str> {
    Ok(())
}

/// Software UIO ops.
pub fn software_uio_ops() -> UioOps {
    UioOps {
        open: sw_open,
        release: sw_release,
        irq_handler: sw_irq_handler,
        irqcontrol: sw_irqcontrol,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !UIO_DEVS.read().is_empty() {
        return Ok(());
    }

    let ops = software_uio_ops();
    let mem_regions = alloc::vec![UioMem {
        name: String::from("sw-uio-mem0"),
        addr: 0,
        size: 4096,
        memtype: UioMemType::Physical,
    }];
    let dev_id = register_device("sw-uio", 0, mem_regions, Vec::new(), 1, ops)?;
    crate::serial_println!(
        "uio: software device registered (id={}, 1 mem region, 1 irq)",
        dev_id
    );
    Ok(())
}
