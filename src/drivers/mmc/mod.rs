//! MMC/SD/SDIO card subsystem
//!
//! Provides host controller registration, card detection, and block
//! access for MMC, SD, and SDIO cards. Mirrors Linux's `drivers/mmc/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Card type (Linux `enum mmc_card_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmcCardType {
    Mmc,
    Sd,
    Sdio,
    SdCombo,
}

/// Bus width (Linux `enum mmc_bus_width`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmcBusWidth {
    One,
    Four,
    Eight,
}

impl MmcBusWidth {
    pub fn bits(self) -> u32 {
        match self {
            MmcBusWidth::One => 1,
            MmcBusWidth::Four => 4,
            MmcBusWidth::Eight => 8,
        }
    }
}

/// Card status (Linux `enum mmc_card_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MmcCardState {
    Idle,
    Ready,
    Ident,
    Standby,
    Tran,
    Data,
    Rcv,
    Prg,
    Dis,
}

/// Host controller operations (Linux `struct mmc_host_ops`).
pub struct MmcHostOps {
    pub request: fn(cmd: u32, arg: u32, buf: Option<&mut [u8]>) -> Result<u32, &'static str>,
    pub set_ios: fn(clock_hz: u32, bus_width: MmcBusWidth) -> Result<(), &'static str>,
    pub get_ro: fn() -> bool,
    pub get_cd: fn() -> bool, // Card detect
    pub get_name: fn() -> &'static str,
    pub get_max_clk: fn() -> u32,
}

struct MmcHost {
    id: u32,
    name: String,
    ops: &'static MmcHostOps,
    max_clk: u32,
    card_present: bool,
}

/// MMC/SD card descriptor.
#[derive(Debug, Clone)]
pub struct MmcCard {
    pub host_id: u32,
    pub card_type: MmcCardType,
    pub rca: u32,
    pub cid: [u32; 4], // Card IDentification register
    pub csd: [u32; 4], // Card Specific Data register
    pub capacity_bytes: u64,
    pub bus_width: MmcBusWidth,
    pub clock_hz: u32,
    pub state: MmcCardState,
    pub read_only: bool,
}

// ── Software SD card host ───────────────────────────────────────────────

static mut SW_SD_DATA: Vec<u8> = Vec::new();
const SW_SD_SIZE: u64 = 2 * 1024 * 1024 * 1024; // 2 GiB
const SW_SD_BLOCK: usize = 512;

fn sw_request(cmd: u32, arg: u32, buf: Option<&mut [u8]>) -> Result<u32, &'static str> {
    match cmd {
        17 => {
            // READ_SINGLE_BLOCK
            let block = arg as usize;
            let data = unsafe { &SW_SD_DATA };
            let offset = block * SW_SD_BLOCK;
            if offset + SW_SD_BLOCK > data.len() {
                return Err("SD read: block out of range");
            }
            if let Some(buf) = buf {
                if buf.len() < SW_SD_BLOCK {
                    return Err("SD read: buffer too small");
                }
                buf[..SW_SD_BLOCK].copy_from_slice(&data[offset..offset + SW_SD_BLOCK]);
            }
            Ok(0)
        }
        24 => {
            // WRITE_SINGLE_BLOCK
            let block = arg as usize;
            let data = unsafe { &mut SW_SD_DATA };
            let offset = block * SW_SD_BLOCK;
            if offset + SW_SD_BLOCK > data.len() {
                return Err("SD write: block out of range");
            }
            if let Some(buf) = buf {
                if buf.len() < SW_SD_BLOCK {
                    return Err("SD write: buffer too small");
                }
                data[offset..offset + SW_SD_BLOCK].copy_from_slice(&buf[..SW_SD_BLOCK]);
            }
            Ok(0)
        }
        41 => Ok(0x80000000),    // ACMD41: OCR (3.3V supported)
        2 | 3 | 9 | 10 => Ok(0), // CID/CSD commands
        _ => Ok(0),
    }
}

fn sw_set_ios(_clock: u32, _width: MmcBusWidth) -> Result<(), &'static str> {
    Ok(())
}
fn sw_get_ro() -> bool {
    false
}
fn sw_get_cd() -> bool {
    true
}
fn sw_name() -> &'static str {
    "software-sd-host"
}
fn sw_max_clk() -> u32 {
    50_000_000
} // 50 MHz

pub static SW_SD_HOST_OPS: MmcHostOps = MmcHostOps {
    request: sw_request,
    set_ios: sw_set_ios,
    get_ro: sw_get_ro,
    get_cd: sw_get_cd,
    get_name: sw_name,
    get_max_clk: sw_max_clk,
};

// ── Registry ────────────────────────────────────────────────────────────

static MMC_HOSTS: RwLock<BTreeMap<u32, MmcHost>> = RwLock::new(BTreeMap::new());
static MMC_CARDS: RwLock<BTreeMap<u32, MmcCard>> = RwLock::new(BTreeMap::new());
static NEXT_HOST_ID: AtomicU32 = AtomicU32::new(0);
static NEXT_CARD_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

/// Register an MMC host controller (Linux `mmc_alloc_host`).
pub fn register_host(name: &str, ops: &'static MmcHostOps) -> Result<u32, &'static str> {
    let max_clk = (ops.get_max_clk)();
    let card_present = (ops.get_cd)();
    let id = NEXT_HOST_ID.fetch_add(1, Ordering::SeqCst);
    MMC_HOSTS.write().insert(
        id,
        MmcHost {
            id,
            name: String::from(name),
            ops,
            max_clk,
            card_present,
        },
    );
    Ok(id)
}

/// Detect and initialize a card on a host (Linux `mmc_detect_card`).
pub fn detect_card(host_id: u32) -> Result<u32, &'static str> {
    let (ops, _max_clk) = {
        let hosts = MMC_HOSTS.read();
        let host = hosts.get(&host_id).ok_or("MMC host not found")?;
        if !(host.ops.get_cd)() {
            return Err("No card present");
        }
        (host.ops, host.max_clk)
    };

    // Send CMD0 (reset), CMD8 (interface condition), ACMD41 (init).
    let _ = (ops.request)(0, 0, None)?;
    let _ = (ops.request)(8, 0x1AA, None)?;
    let _ = (ops.request)(41, 0x40300000, None)?;

    // Read CID (CMD2) and set RCA (CMD3).
    let _ = (ops.request)(2, 0, None)?;
    let rca = (ops.request)(3, 0, None)?;

    // Read CSD (CMD9).
    let _ = (ops.request)(9, rca, None)?;

    let card_id = NEXT_CARD_ID.fetch_add(1, Ordering::SeqCst);
    let card = MmcCard {
        host_id,
        card_type: MmcCardType::Sd,
        rca,
        cid: [0x12345678; 4],
        csd: [0xAABBCCDD; 4],
        capacity_bytes: SW_SD_SIZE,
        bus_width: MmcBusWidth::Four,
        clock_hz: 50_000_000,
        state: MmcCardState::Tran,
        read_only: (ops.get_ro)(),
    };

    MMC_CARDS.write().insert(card_id, card);

    // Update host card_present.
    let mut hosts = MMC_HOSTS.write();
    if let Some(host) = hosts.get_mut(&host_id) {
        host.card_present = true;
    }

    Ok(card_id)
}

/// Read a block from an MMC/SD card (Linux `mmc_read_block`).
pub fn read_block(card_id: u32, block: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let (host_id, _capacity) = {
        let cards = MMC_CARDS.read();
        let card = cards.get(&card_id).ok_or("MMC card not found")?;
        if block as u64 * 512 >= card.capacity_bytes {
            return Err("SD read: block beyond capacity");
        }
        (card.host_id, card.capacity_bytes)
    };

    let ops = {
        let hosts = MMC_HOSTS.read();
        let host = hosts.get(&host_id).ok_or("MMC host not found")?;
        host.ops
    };

    (ops.request)(17, block, Some(buf))?;
    Ok(SW_SD_BLOCK)
}

/// Write a block to an MMC/SD card (Linux `mmc_write_block`).
pub fn write_block(card_id: u32, block: u32, buf: &mut [u8]) -> Result<usize, &'static str> {
    let (host_id, read_only) = {
        let cards = MMC_CARDS.read();
        let card = cards.get(&card_id).ok_or("MMC card not found")?;
        if card.read_only {
            return Err("SD card is read-only");
        }
        (card.host_id, card.read_only)
    };
    let _ = read_only;

    let ops = {
        let hosts = MMC_HOSTS.read();
        let host = hosts.get(&host_id).ok_or("MMC host not found")?;
        host.ops
    };

    (ops.request)(24, block, Some(buf))?;
    Ok(SW_SD_BLOCK)
}

/// Get card info.
pub fn get_card_info(card_id: u32) -> Result<MmcCard, &'static str> {
    let cards = MMC_CARDS.read();
    let card = cards.get(&card_id).ok_or("MMC card not found")?;
    Ok(card.clone())
}

/// Number of registered hosts.
pub fn host_count() -> usize {
    MMC_HOSTS.read().len()
}

/// Number of detected cards.
pub fn card_count() -> usize {
    MMC_CARDS.read().len()
}

/// Initialize MMC subsystem with software SD host.
pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("mmc: subsystem ready");
    Ok(())
}
