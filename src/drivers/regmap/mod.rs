//! Register map (regmap) abstraction
//!
//! Provides a generic register access API for device drivers, supporting
//! MMIO and I2C/SPI-backed register maps with endianness conversion,
//! caching, and read/modify/write operations. Mirrors Linux's
//! `drivers/base/regmap/regmap.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Register value size (Linux `enum regmap_endian` / reg_bits).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegmapValSize {
    U8,
    U16,
    U32,
    U64,
}

impl RegmapValSize {
    fn bytes(self) -> usize {
        match self {
            RegmapValSize::U8 => 1,
            RegmapValSize::U16 => 2,
            RegmapValSize::U32 => 4,
            RegmapValSize::U64 => 8,
        }
    }
}

/// Register address size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegmapRegSize {
    U8,
    U16,
    U32,
}

impl RegmapRegSize {
    fn bytes(self) -> usize {
        match self {
            RegmapRegSize::U8 => 1,
            RegmapRegSize::U16 => 2,
            RegmapRegSize::U32 => 4,
        }
    }
}

/// Endianness for register values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegmapEndian {
    Little,
    Big,
    Native,
}

/// Configuration for a regmap instance (Linux `struct regmap_config`).
#[derive(Debug, Clone, Copy)]
pub struct RegmapConfig {
    pub reg_bits: RegmapRegSize,
    pub val_bits: RegmapValSize,
    pub reg_endian: RegmapEndian,
    pub val_endian: RegmapEndian,
    pub max_register: u32,
    pub readable_reg_mask: u32,
    pub writable_reg_mask: u32,
    pub volatile_reg_mask: u32,
    pub cache_type: RegmapCacheType,
}

impl Default for RegmapConfig {
    fn default() -> Self {
        Self {
            reg_bits: RegmapRegSize::U8,
            val_bits: RegmapValSize::U8,
            reg_endian: RegmapEndian::Native,
            val_endian: RegmapEndian::Native,
            max_register: 0xFF,
            readable_reg_mask: 0xFFFF_FFFF,
            writable_reg_mask: 0xFFFF_FFFF,
            volatile_reg_mask: 0xFFFF_FFFF,
            cache_type: RegmapCacheType::None,
        }
    }
}

/// Cache policy (Linux `enum regcache_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegmapCacheType {
    None,
    Flat,
    Rbtree,
}

/// Bus operations for a regmap backend (Linux `struct regmap_bus`).
pub struct RegmapBus {
    pub read: fn(reg: u32, val_size: usize) -> Result<u64, &'static str>,
    pub write: fn(reg: u32, val: u64, val_size: usize) -> Result<(), &'static str>,
    pub get_name: fn() -> &'static str,
}

struct Regmap {
    id: u32,
    name: String,
    config: RegmapConfig,
    bus: &'static RegmapBus,
    cache: BTreeMap<u32, u64>,
}

// ── MMIO backend ────────────────────────────────────────────────────────

/// SAFETY: Reads from a memory-mapped register. Caller must ensure the
/// address is valid and mapped. See docs/SAFETY.md#mmio-access.
unsafe fn mmio_read_raw(addr: u64, size: usize) -> u64 {
    let ptr = addr as *const u8;
    let mut value: u64 = 0;
    for i in 0..size {
        value |= (unsafe { core::ptr::read_volatile(ptr.add(i)) } as u64) << (i * 8);
    }
    value
}

/// SAFETY: Writes to a memory-mapped register. Caller must ensure the
/// address is valid and mapped. See docs/SAFETY.md#mmio-access.
unsafe fn mmio_write_raw(addr: u64, value: u64, size: usize) {
    let ptr = addr as *mut u8;
    for i in 0..size {
        unsafe {
            core::ptr::write_volatile(ptr.add(i), (value >> (i * 8)) as u8);
        }
    }
}

static mut MMIO_BACKEND_BASE: u64 = 0;

fn mmio_read(reg: u32, val_size: usize) -> Result<u64, &'static str> {
    let base = unsafe { MMIO_BACKEND_BASE };
    if base == 0 {
        return Err("MMIO regmap base not configured");
    }
    let addr = base + reg as u64;
    Ok(unsafe { mmio_read_raw(addr, val_size) })
}

fn mmio_write(reg: u32, val: u64, val_size: usize) -> Result<(), &'static str> {
    let base = unsafe { MMIO_BACKEND_BASE };
    if base == 0 {
        return Err("MMIO regmap base not configured");
    }
    let addr = base + reg as u64;
    unsafe { mmio_write_raw(addr, val, val_size) };
    Ok(())
}

fn mmio_name() -> &'static str {
    "mmio"
}

pub static MMIO_BUS: RegmapBus = RegmapBus {
    read: mmio_read,
    write: mmio_write,
    get_name: mmio_name,
};

// ── I2C-backed regmap ───────────────────────────────────────────────────

static mut I2C_REGMAP_ADAPTER: u32 = 0;
static mut I2C_REGMAP_ADDR: u16 = 0;

fn i2c_regmap_read(reg: u32, val_size: usize) -> Result<u64, &'static str> {
    let adapter = unsafe { I2C_REGMAP_ADAPTER };
    let addr = unsafe { I2C_REGMAP_ADDR };
    let data = crate::drivers::i2c::i2c_read(adapter, addr, reg as u8, val_size)?;
    let mut value: u64 = 0;
    for (i, &byte) in data.iter().enumerate() {
        value |= (byte as u64) << (i * 8);
    }
    Ok(value)
}

fn i2c_regmap_write(reg: u32, val: u64, val_size: usize) -> Result<(), &'static str> {
    let adapter = unsafe { I2C_REGMAP_ADAPTER };
    let addr = unsafe { I2C_REGMAP_ADDR };
    let mut buf = Vec::with_capacity(val_size);
    for i in 0..val_size {
        buf.push((val >> (i * 8)) as u8);
    }
    crate::drivers::i2c::i2c_write(adapter, addr, reg as u8, &buf)
}

fn i2c_regmap_name() -> &'static str {
    "i2c"
}

pub static I2C_BUS: RegmapBus = RegmapBus {
    read: i2c_regmap_read,
    write: i2c_regmap_write,
    get_name: i2c_regmap_name,
};

// ── Registry ────────────────────────────────────────────────────────────

static REGMAPS: RwLock<BTreeMap<u32, Regmap>> = RwLock::new(BTreeMap::new());
static NEXT_REGMAP_ID: AtomicU32 = AtomicU32::new(0);

// ── Endianness conversion ───────────────────────────────────────────────

fn to_endian(value: u64, size: usize, endian: RegmapEndian) -> u64 {
    match endian {
        RegmapEndian::Native | RegmapEndian::Little => value,
        RegmapEndian::Big => {
            let bytes = value.to_le_bytes();
            let mut swapped = [0u8; 8];
            for i in 0..size {
                swapped[i] = bytes[size - 1 - i];
            }
            u64::from_le_bytes(swapped)
        }
    }
}

fn from_endian(value: u64, size: usize, endian: RegmapEndian) -> u64 {
    to_endian(value, size, endian)
}

// ── Public API ──────────────────────────────────────────────────────────

/// Initialize an MMIO-backed regmap (Linux devm_regmap_init_mmio).
pub fn init_mmio(name: &str, base_addr: u64, config: RegmapConfig) -> Result<u32, &'static str> {
    unsafe {
        MMIO_BACKEND_BASE = base_addr;
    }
    let id = NEXT_REGMAP_ID.fetch_add(1, Ordering::SeqCst);
    REGMAPS.write().insert(
        id,
        Regmap {
            id,
            name: String::from(name),
            config,
            bus: &MMIO_BUS,
            cache: BTreeMap::new(),
        },
    );
    Ok(id)
}

/// Initialize an I2C-backed regmap (Linux devm_regmap_init_i2c).
pub fn init_i2c(
    name: &str,
    adapter_id: u32,
    device_addr: u16,
    config: RegmapConfig,
) -> Result<u32, &'static str> {
    unsafe {
        I2C_REGMAP_ADAPTER = adapter_id;
        I2C_REGMAP_ADDR = device_addr;
    }
    let id = NEXT_REGMAP_ID.fetch_add(1, Ordering::SeqCst);
    REGMAPS.write().insert(
        id,
        Regmap {
            id,
            name: String::from(name),
            config,
            bus: &I2C_BUS,
            cache: BTreeMap::new(),
        },
    );
    Ok(id)
}

/// Read a register (Linux regmap_read).
pub fn read(regmap_id: u32, reg: u32) -> Result<u64, &'static str> {
    let (bus, config) = {
        let regmaps = REGMAPS.read();
        let rm = regmaps.get(&regmap_id).ok_or("regmap not found")?;
        (rm.bus, rm.config)
    };

    if reg > config.max_register {
        return Err("Register address out of range");
    }

    // Check cache first for non-volatile registers.
    if config.cache_type != RegmapCacheType::None {
        if let Some(cached) = {
            let regmaps = REGMAPS.read();
            regmaps
                .get(&regmap_id)
                .and_then(|rm| rm.cache.get(&reg).copied())
        } {
            return Ok(cached);
        }
    }

    let raw = (bus.read)(reg, config.val_bits.bytes())?;
    let value = from_endian(raw, config.val_bits.bytes(), config.val_endian);

    // Update cache for non-volatile registers.
    if config.cache_type != RegmapCacheType::None {
        let mut regmaps = REGMAPS.write();
        if let Some(rm) = regmaps.get_mut(&regmap_id) {
            rm.cache.insert(reg, value);
        }
    }

    Ok(value)
}

/// Write a register (Linux regmap_write).
pub fn write(regmap_id: u32, reg: u32, val: u64) -> Result<(), &'static str> {
    let (bus, config) = {
        let regmaps = REGMAPS.read();
        let rm = regmaps.get(&regmap_id).ok_or("regmap not found")?;
        (rm.bus, rm.config)
    };

    if reg > config.max_register {
        return Err("Register address out of range");
    }

    let raw = to_endian(val, config.val_bits.bytes(), config.val_endian);
    (bus.write)(reg, raw, config.val_bits.bytes())?;

    // Update cache.
    if config.cache_type != RegmapCacheType::None {
        let mut regmaps = REGMAPS.write();
        if let Some(rm) = regmaps.get_mut(&regmap_id) {
            rm.cache.insert(reg, val);
        }
    }

    Ok(())
}

/// Read-modify-write a register (Linux regmap_update_bits).
pub fn update_bits(regmap_id: u32, reg: u32, mask: u64, val: u64) -> Result<(), &'static str> {
    let current = read(regmap_id, reg)?;
    let new = (current & !mask) | (val & mask);
    write(regmap_id, reg, new)
}

/// Bulk read multiple registers (Linux regmap_bulk_read).
pub fn bulk_read(regmap_id: u32, start_reg: u32, count: usize) -> Result<Vec<u64>, &'static str> {
    let mut values = Vec::with_capacity(count);
    for i in 0..count {
        let reg = start_reg.wrapping_add(i as u32);
        values.push(read(regmap_id, reg)?);
    }
    Ok(values)
}

/// Bulk write multiple registers (Linux regmap_bulk_write).
pub fn bulk_write(regmap_id: u32, start_reg: u32, values: &[u64]) -> Result<(), &'static str> {
    for (i, &val) in values.iter().enumerate() {
        let reg = start_reg.wrapping_add(i as u32);
        write(regmap_id, reg, val)?;
    }
    Ok(())
}

/// Force cache sync (Linux regcache_sync).
pub fn cache_sync(regmap_id: u32) -> Result<(), &'static str> {
    let entries: Vec<(u32, u64)> = {
        let regmaps = REGMAPS.read();
        let rm = regmaps.get(&regmap_id).ok_or("regmap not found")?;
        if rm.config.cache_type == RegmapCacheType::None {
            return Ok(());
        }
        rm.cache.iter().map(|(r, v)| (*r, *v)).collect()
    };

    for (reg, val) in entries {
        write(regmap_id, reg, val)?;
    }
    Ok(())
}

/// Drop all cached values (Linux regcache_cache_only).
pub fn cache_drop(regmap_id: u32) -> Result<(), &'static str> {
    let mut regmaps = REGMAPS.write();
    let rm = regmaps.get_mut(&regmap_id).ok_or("regmap not found")?;
    rm.cache.clear();
    Ok(())
}

/// Number of registered regmaps.
pub fn count() -> usize {
    REGMAPS.read().len()
}

/// Initialize regmap subsystem.
pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("regmap: subsystem ready");
    Ok(())
}
