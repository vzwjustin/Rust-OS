//! SPI bus master framework
//!
//! Provides master controller registration, chip-select aware device binding,
//! and transfer helpers. Includes a software master with an in-memory flash
//! slave for platform use.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpiMode {
    Mode0,
    Mode1,
    Mode2,
    Mode3,
}

impl SpiMode {
    pub fn cpol(&self) -> u8 {
        match self {
            SpiMode::Mode2 | SpiMode::Mode3 => 1,
            _ => 0,
        }
    }

    pub fn cpha(&self) -> u8 {
        match self {
            SpiMode::Mode1 | SpiMode::Mode3 => 1,
            _ => 0,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SpiDeviceConfig {
    pub mode: SpiMode,
    pub max_speed_hz: u32,
    pub bits_per_word: u8,
    pub chip_select: u8,
}

impl Default for SpiDeviceConfig {
    fn default() -> Self {
        Self {
            mode: SpiMode::Mode0,
            max_speed_hz: 1_000_000,
            bits_per_word: 8,
            chip_select: 0,
        }
    }
}

pub struct SpiMasterOps {
    pub transfer: fn(cs: u8, tx: &[u8], rx: &mut [u8]) -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
}

pub struct SpiDeviceOps {
    pub transfer: fn(tx: &[u8], rx: &mut [u8]) -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
}

struct SpiMaster {
    id: u32,
    name: String,
    bus_num: u32,
    num_chipselect: u8,
    ops: SpiMasterOps,
}

struct SpiDevice {
    id: u32,
    name: String,
    master_id: u32,
    config: SpiDeviceConfig,
    ops: SpiDeviceOps,
}

// ── Software flash slave (CS0) ────────────────────────────────────────────

struct VirtualSpiFlash {
    storage: [u8; 4096],
    write_enabled: bool,
}

impl VirtualSpiFlash {
    const fn new() -> Self {
        let mut storage = [0xFFu8; 4096];
        storage[0] = 0xEF; // JEDEC manufacturer continuation
        storage[1] = 0x40;
        storage[2] = 0x18; // JEDEC capacity code
        Self {
            storage,
            write_enabled: false,
        }
    }

    fn handle_command(
        flash: &mut VirtualSpiFlash,
        tx: &[u8],
        rx: &mut [u8],
    ) -> Result<(), &'static str> {
        if tx.is_empty() {
            return Ok(());
        }

        match tx[0] {
            0x03 => {
                // READ: opcode, addr24, then data out
                if tx.len() >= 4 {
                    let addr = ((tx[1] as usize) << 16) | ((tx[2] as usize) << 8) | tx[3] as usize;
                    for (i, byte) in rx.iter_mut().enumerate().skip(4) {
                        *byte = flash.storage[(addr + i - 4) % flash.storage.len()];
                    }
                }
            }
            0x06 => flash.write_enabled = true,
            0x04 => flash.write_enabled = false,
            0x02 => {
                // PAGE PROGRAM
                if flash.write_enabled && tx.len() >= 4 {
                    let addr = ((tx[1] as usize) << 16) | ((tx[2] as usize) << 8) | tx[3] as usize;
                    for (i, &byte) in tx.iter().enumerate().skip(4) {
                        flash.storage[(addr + i - 4) % flash.storage.len()] = byte;
                    }
                }
            }
            0x9F => {
                // JEDEC ID
                if rx.len() >= 4 {
                    rx[1] = flash.storage[0];
                    rx[2] = flash.storage[1];
                    rx[3] = flash.storage[2];
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn transfer(tx: &[u8], rx: &mut [u8]) -> Result<(), &'static str> {
        let mut flash = VIRTUAL_SPI_FLASH.lock();
        Self::handle_command(&mut flash, tx, rx)
    }

    fn name() -> &'static str {
        "virtual-spi-flash"
    }
}

static VIRTUAL_SPI_FLASH: spin::Mutex<VirtualSpiFlash> = spin::Mutex::new(VirtualSpiFlash::new());

fn software_master_transfer(cs: u8, tx: &[u8], rx: &mut [u8]) -> Result<(), &'static str> {
    if tx.len() != rx.len() {
        return Err("SPI transfer: tx/rx length mismatch");
    }

    let devices = SPI_DEVICES.read();
    let device = devices
        .values()
        .find(|d| d.master_id == 0 && d.config.chip_select == cs)
        .ok_or("SPI: no device on chip select")?;

    (device.ops.transfer)(tx, rx)
}

const SOFTWARE_MASTER_OPS: SpiMasterOps = SpiMasterOps {
    transfer: software_master_transfer,
    get_name: || "software-spi",
};

const VIRTUAL_FLASH_OPS: SpiDeviceOps = SpiDeviceOps {
    transfer: VirtualSpiFlash::transfer,
    get_name: VirtualSpiFlash::name,
};

// ── Registry ────────────────────────────────────────────────────────────

static SPI_MASTERS: RwLock<BTreeMap<u32, SpiMaster>> = RwLock::new(BTreeMap::new());
static SPI_DEVICES: RwLock<BTreeMap<u32, SpiDevice>> = RwLock::new(BTreeMap::new());
static NEXT_MASTER_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_DEVICE_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_master(
    name: &str,
    bus_num: u32,
    num_chipselect: u8,
    ops: SpiMasterOps,
) -> Result<u32, &'static str> {
    let id = NEXT_MASTER_ID.fetch_add(1, Ordering::SeqCst);
    SPI_MASTERS.write().insert(
        id,
        SpiMaster {
            id,
            name: String::from(name),
            bus_num,
            num_chipselect,
            ops,
        },
    );
    Ok(id)
}

pub fn register_device(
    master_id: u32,
    name: &str,
    config: SpiDeviceConfig,
    ops: SpiDeviceOps,
) -> Result<u32, &'static str> {
    if !SPI_MASTERS.read().contains_key(&master_id) {
        return Err("SPI master not found");
    }

    let id = NEXT_DEVICE_ID.fetch_add(1, Ordering::SeqCst);
    SPI_DEVICES.write().insert(
        id,
        SpiDevice {
            id,
            name: String::from(name),
            master_id,
            config,
            ops,
        },
    );
    Ok(id)
}

pub fn spi_transfer(master_id: u32, cs: u8, tx: &[u8], rx: &mut [u8]) -> Result<(), &'static str> {
    let masters = SPI_MASTERS.read();
    let master = masters.get(&master_id).ok_or("SPI master not found")?;
    (master.ops.transfer)(cs, tx, rx)
}

pub fn spi_write_read(master_id: u32, cs: u8, tx: &[u8]) -> Result<Vec<u8>, &'static str> {
    let mut rx = vec![0u8; tx.len()];
    spi_transfer(master_id, cs, tx, &mut rx)?;
    Ok(rx)
}

pub fn master_count() -> usize {
    SPI_MASTERS.read().len()
}

pub fn device_count() -> usize {
    SPI_DEVICES.read().len()
}

fn register_platform_devices() -> Result<(), &'static str> {
    register_device(
        0,
        "virtual-spi-flash",
        SpiDeviceConfig {
            chip_select: 0,
            ..Default::default()
        },
        VIRTUAL_FLASH_OPS,
    )?;
    Ok(())
}

/// Initialize SPI subsystem with software master and virtual flash.
pub fn init() -> Result<(), &'static str> {
    if !SPI_MASTERS.read().is_empty() {
        return Ok(());
    }

    register_master("software-spi", 0, 4, SOFTWARE_MASTER_OPS)?;
    register_platform_devices()?;
    crate::serial_println!("spi: software master bus 0, {} device(s)", device_count());
    Ok(())
}
