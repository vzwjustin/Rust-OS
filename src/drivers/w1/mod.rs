//! W1 (One-Wire) bus subsystem
//!
//! Provides one-wire bus framework for device discovery, ROM reads, and data transfers.
//! Mirrors Linux's `drivers/w1/w1.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// W1 family ID (Linux `enum w1_family_ids` subset).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum W1Family {
    Unknown,
    Ds18s20, // 0x10
    Ds18b20, // 0x28
    Ds2408,  // 0x29
    Ds2413,  // 0x3A
    Ds2431,  // 0x2D
    Ds2433,  // 0x23
    Ds28e04, // 0x1C
    Custom(u8),
}

impl W1Family {
    pub fn from_id(id: u8) -> Self {
        match id {
            0x10 => W1Family::Ds18s20,
            0x28 => W1Family::Ds18b20,
            0x29 => W1Family::Ds2408,
            0x3A => W1Family::Ds2413,
            0x2D => W1Family::Ds2431,
            0x23 => W1Family::Ds2433,
            0x1C => W1Family::Ds28e04,
            _ => W1Family::Custom(id),
        }
    }
}

/// W1 ROM (64-bit unique ID: 8-bit family + 48-bit serial + 8-bit CRC).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct W1Rom(pub u64);

impl W1Rom {
    pub fn family(&self) -> W1Family {
        W1Family::from_id((self.0 & 0xFF) as u8)
    }
    pub fn serial(&self) -> u64 {
        (self.0 >> 8) & 0xFFFF_FFFF_FFFF
    }
    pub fn crc(&self) -> u8 {
        ((self.0 >> 56) & 0xFF) as u8
    }
}

/// W1 master operations (Linux `struct w1_bus_master_ops`).
pub struct W1MasterOps {
    pub read_byte: fn(master_id: u32) -> Result<u8, &'static str>,
    pub write_byte: fn(master_id: u32, byte: u8) -> Result<(), &'static str>,
    pub touch_bit: fn(master_id: u32, bit: bool) -> Result<bool, &'static str>,
    pub reset_bus: fn(master_id: u32) -> Result<bool, &'static str>,
    pub set_pullup: fn(master_id: u32, delay_ms: u32) -> Result<(), &'static str>,
}

/// W1 master (Linux `struct w1_master`).
pub struct W1Master {
    pub id: u32,
    pub name: String,
    pub ops: W1MasterOps,
    pub device_ids: Vec<u32>,
    pub max_slave_count: u32,
    pub bus_timeout_ms: u32,
}

/// W1 slave device (Linux `struct w1_slave`).
pub struct W1Slave {
    pub id: u32,
    pub master_id: u32,
    pub rom: W1Rom,
    pub name: String,
    pub registered: bool,
}

/// W1 family driver (Linux `struct w1_family`).
pub struct W1FamilyDriver {
    pub family: W1Family,
    pub name: String,
    pub add_slave: fn(slave_id: u32) -> Result<(), &'static str>,
    pub remove_slave: fn(slave_id: u32) -> Result<(), &'static str>,
    pub read_data: fn(slave_id: u32, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub write_data: fn(slave_id: u32, data: &[u8]) -> Result<usize, &'static str>,
}

// ── Registry ────────────────────────────────────────────────────────────

static MASTER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static SLAVE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static W1_MASTERS: RwLock<BTreeMap<u32, W1Master>> = RwLock::new(BTreeMap::new());
static W1_SLAVES: RwLock<BTreeMap<u32, W1Slave>> = RwLock::new(BTreeMap::new());
static W1_FAMILIES: RwLock<BTreeMap<u8, W1FamilyDriver>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a W1 bus master.
pub fn register_master(
    name: &str,
    ops: W1MasterOps,
    max_slave_count: u32,
) -> Result<u32, &'static str> {
    let id = MASTER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let master = W1Master {
        id,
        name: String::from(name),
        ops,
        device_ids: Vec::new(),
        max_slave_count,
        bus_timeout_ms: 1000,
    };
    W1_MASTERS.write().insert(id, master);
    Ok(id)
}

/// Register a W1 family driver.
pub fn register_family(family_id: u8, driver: W1FamilyDriver) -> Result<(), &'static str> {
    W1_FAMILIES.write().insert(family_id, driver);
    Ok(())
}

/// Reset the W1 bus and detect presence of devices.
pub fn reset_bus(master_id: u32) -> Result<bool, &'static str> {
    let reset_fn = {
        let masters = W1_MASTERS.read();
        let master = masters.get(&master_id).ok_or("W1 master not found")?;
        master.ops.reset_bus
    };
    (reset_fn)(master_id)
}

/// Write a byte to the W1 bus.
pub fn write_byte(master_id: u32, byte: u8) -> Result<(), &'static str> {
    let write_fn = {
        let masters = W1_MASTERS.read();
        let master = masters.get(&master_id).ok_or("W1 master not found")?;
        master.ops.write_byte
    };
    (write_fn)(master_id, byte)
}

/// Read a byte from the W1 bus.
pub fn read_byte(master_id: u32) -> Result<u8, &'static str> {
    let read_fn = {
        let masters = W1_MASTERS.read();
        let master = masters.get(&master_id).ok_or("W1 master not found")?;
        master.ops.read_byte
    };
    (read_fn)(master_id)
}

/// Touch a single bit on the W1 bus.
pub fn touch_bit(master_id: u32, bit: bool) -> Result<bool, &'static str> {
    let touch_fn = {
        let masters = W1_MASTERS.read();
        let master = masters.get(&master_id).ok_or("W1 master not found")?;
        master.ops.touch_bit
    };
    (touch_fn)(master_id, bit)
}

/// Discover and register slave devices on the bus (Linux `w1_search`).
pub fn search_slaves(master_id: u32) -> Result<Vec<u32>, &'static str> {
    let present = reset_bus(master_id)?;
    if !present {
        return Ok(Vec::new());
    }

    let (write_fn, read_fn) = {
        let masters = W1_MASTERS.read();
        let master = masters.get(&master_id).ok_or("W1 master not found")?;
        (master.ops.write_byte, master.ops.read_byte)
    };

    let write_byte = |byte: u8| -> Result<(), &'static str> {
        (write_fn)(master_id, byte).map_err(|_| "W1 write failed")
    };
    let read_byte =
        || -> Result<u8, &'static str> { (read_fn)(master_id).map_err(|_| "W1 read failed") };

    let mut found_roms: Vec<u64> = Vec::new();
    let mut last_discrepancy: i32 = -1;
    let max_roms = 64u32;

    loop {
        if found_roms.len() >= max_roms as usize {
            break;
        }

        if !reset_bus(master_id)? {
            break;
        }

        write_byte(0xF0)?;

        let mut rom: u64 = 0;
        let mut last_zero: i32 = -1;
        let mut search_done = false;

        for bit_idx in 0..64u32 {
            let read_bit = read_byte()? & 1;
            let comp_bit = read_byte()? & 1;

            if read_bit == 1 && comp_bit == 1 {
                break;
            }

            let search_dir: u8 = if read_bit != comp_bit {
                read_bit as u8
            } else if bit_idx as i32 == last_discrepancy {
                1
            } else if bit_idx as i32 > last_discrepancy {
                0
            } else {
                ((rom >> bit_idx) & 1) as u8
            };

            if read_bit == 0 && comp_bit == 0 && search_dir == 0 {
                last_zero = bit_idx as i32;
            }

            write_byte(search_dir)?;

            if search_dir == 1 {
                rom |= 1u64 << bit_idx;
            }
        }

        if last_zero == -1 {
            search_done = true;
        }

        last_discrepancy = last_zero;

        let crc = (rom >> 56) as u8;
        let mut calc_crc: u8 = 0;
        for i in 0..7u32 {
            let b = ((rom >> (i * 8)) & 0xFF) as u8;
            calc_crc ^= b;
            for _ in 0..8 {
                if calc_crc & 1 != 0 {
                    calc_crc = (calc_crc >> 1) ^ 0x8C;
                } else {
                    calc_crc >>= 1;
                }
            }
        }

        if calc_crc == crc && !found_roms.contains(&rom) {
            found_roms.push(rom);
        }

        if search_done {
            break;
        }
    }

    let mut new_slave_ids = Vec::new();
    for rom_val in &found_roms {
        let rom_struct = W1Rom(*rom_val);
        let already = {
            let slaves = W1_SLAVES.read();
            slaves
                .values()
                .any(|s| s.master_id == master_id && s.rom.0 == *rom_val)
        };
        if !already {
            if let Ok(sid) = register_slave(master_id, rom_struct) {
                new_slave_ids.push(sid);
            }
        }
    }

    let masters = W1_MASTERS.read();
    let master = masters.get(&master_id).ok_or("W1 master not found")?;
    let mut all_ids = master.device_ids.clone();
    for sid in &new_slave_ids {
        if !all_ids.contains(sid) {
            all_ids.push(*sid);
        }
    }
    Ok(all_ids)
}

/// Register a discovered slave device.
pub fn register_slave(master_id: u32, rom: W1Rom) -> Result<u32, &'static str> {
    let slave_id = SLAVE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let family = rom.family();
    let name = match family {
        W1Family::Ds18s20 => String::from("ds18s20"),
        W1Family::Ds18b20 => String::from("ds18b20"),
        W1Family::Ds2408 => String::from("ds2408"),
        W1Family::Ds2413 => String::from("ds2413"),
        W1Family::Ds2431 => String::from("ds2431"),
        W1Family::Ds2433 => String::from("ds2433"),
        W1Family::Ds28e04 => String::from("ds28e04"),
        W1Family::Custom(id) => {
            let mut s = String::from("w1-");
            s.push_str(&alloc::format!("{:02x}", id));
            s
        }
        W1Family::Unknown => String::from("w1-unknown"),
    };

    let slave = W1Slave {
        id: slave_id,
        master_id,
        rom,
        name: name.clone(),
        registered: true,
    };
    W1_SLAVES.write().insert(slave_id, slave);

    let mut masters = W1_MASTERS.write();
    if let Some(master) = masters.get_mut(&master_id) {
        master.device_ids.push(slave_id);
    }

    // Call family add_slave if registered
    let family_id = (rom.0 & 0xFF) as u8;
    let add_fn = {
        let families = W1_FAMILIES.read();
        families.get(&family_id).map(|f| f.add_slave)
    };
    if let Some(add) = add_fn {
        (add)(slave_id)?;
    }

    Ok(slave_id)
}

/// Read data from a slave device (family-specific).
pub fn read_slave(slave_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let (_family_id, read_fn) = {
        let slaves = W1_SLAVES.read();
        let slave = slaves.get(&slave_id).ok_or("W1 slave not found")?;
        let fid = (slave.rom.0 & 0xFF) as u8;
        let families = W1_FAMILIES.read();
        let family = families.get(&fid).ok_or("No family driver for slave")?;
        (fid, family.read_data)
    };
    (read_fn)(slave_id, buf)
}

/// Write data to a slave device (family-specific).
pub fn write_slave(slave_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    let family_id = {
        let slaves = W1_SLAVES.read();
        let slave = slaves.get(&slave_id).ok_or("W1 slave not found")?;
        (slave.rom.0 & 0xFF) as u8
    };
    let write_fn = {
        let families = W1_FAMILIES.read();
        let family = families
            .get(&family_id)
            .ok_or("No family driver for slave")?;
        family.write_data
    };
    (write_fn)(slave_id, data)
}

/// List all W1 masters.
pub fn list_masters() -> Vec<(u32, String, usize)> {
    W1_MASTERS
        .read()
        .iter()
        .map(|(id, m)| (*id, m.name.clone(), m.device_ids.len()))
        .collect()
}

/// List slaves on a master.
pub fn list_slaves(master_id: u32) -> Result<Vec<(u32, String, W1Rom)>, &'static str> {
    let masters = W1_MASTERS.read();
    let master = masters.get(&master_id).ok_or("W1 master not found")?;
    let slaves = W1_SLAVES.read();
    let mut result = Vec::new();
    for &sid in &master.device_ids {
        if let Some(slave) = slaves.get(&sid) {
            result.push((slave.id, slave.name.clone(), slave.rom));
        }
    }
    Ok(result)
}

/// Count registered masters.
pub fn master_count() -> usize {
    W1_MASTERS.read().len()
}

// ── Software W1 ─────────────────────────────────────────────────────────

fn sw_read_byte(_master_id: u32) -> Result<u8, &'static str> {
    Ok(0)
}
fn sw_write_byte(_master_id: u32, _byte: u8) -> Result<(), &'static str> {
    Ok(())
}
fn sw_touch_bit(_master_id: u32, _bit: bool) -> Result<bool, &'static str> {
    Ok(false)
}
fn sw_reset_bus(_master_id: u32) -> Result<bool, &'static str> {
    Ok(true)
}
fn sw_set_pullup(_master_id: u32, _delay_ms: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software W1 master ops.
pub fn software_w1_ops() -> W1MasterOps {
    W1MasterOps {
        read_byte: sw_read_byte,
        write_byte: sw_write_byte,
        touch_bit: sw_touch_bit,
        reset_bus: sw_reset_bus,
        set_pullup: sw_set_pullup,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

fn ds18b20_add(_slave_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn ds18b20_remove(_slave_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn ds18b20_read(_slave_id: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    // Return a fixed temperature of 25.0°C in 16-bit fixed point (0x0190 = 400 = 25.000°C)
    if buf.len() >= 2 {
        buf[0] = 0x90;
        buf[1] = 0x01;
        Ok(2)
    } else {
        Ok(0)
    }
}
fn ds18b20_write(_slave_id: u32, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}

pub fn init() -> Result<(), &'static str> {
    if !W1_MASTERS.read().is_empty() {
        return Ok(());
    }

    let ops = software_w1_ops();
    let master_id = register_master("sw-w1-master", ops, 16)?;

    register_family(
        0x28,
        W1FamilyDriver {
            family: W1Family::Ds18b20,
            name: String::from("ds18b20"),
            add_slave: ds18b20_add,
            remove_slave: ds18b20_remove,
            read_data: ds18b20_read,
            write_data: ds18b20_write,
        },
    )?;

    crate::serial_println!("w1: software master registered (id={})", master_id);
    Ok(())
}
