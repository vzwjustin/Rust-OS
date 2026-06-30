//! MemoryStick host/card registry.
//!
//! Rust-owned mirror of Linux `drivers/memstick/` with host registration,
//! card attachment, and block read/write state stored in kernel memory.

use alloc::{collections::BTreeMap, string::String, vec, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemstickCardState {
    Attached,
    Suspended,
    Removed,
}

#[derive(Debug, Clone)]
pub struct MemstickHost {
    pub id: u32,
    pub name: String,
    pub max_cards: u32,
    pub card_ids: Vec<u32>,
}

#[derive(Debug, Clone)]
pub struct MemstickCard {
    pub id: u32,
    pub host_id: u32,
    pub model: String,
    pub sectors: u64,
    pub sector_size: usize,
    pub state: MemstickCardState,
    storage: BTreeMap<u64, Vec<u8>>,
}

static HOST_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CARD_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static MEMSTICK_HOSTS: RwLock<BTreeMap<u32, MemstickHost>> = RwLock::new(BTreeMap::new());
static MEMSTICK_CARDS: RwLock<BTreeMap<u32, MemstickCard>> = RwLock::new(BTreeMap::new());

pub fn register_host(name: &str, max_cards: u32) -> Result<u32, &'static str> {
    if max_cards == 0 {
        return Err("MemoryStick host must accept at least one card");
    }

    let id = HOST_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    MEMSTICK_HOSTS.write().insert(
        id,
        MemstickHost {
            id,
            name: String::from(name),
            max_cards,
            card_ids: Vec::new(),
        },
    );
    Ok(id)
}

pub fn attach_card(
    host_id: u32,
    model: &str,
    sectors: u64,
    sector_size: usize,
) -> Result<u32, &'static str> {
    if sectors == 0 || sector_size == 0 {
        return Err("MemoryStick card geometry is invalid");
    }

    let mut hosts = MEMSTICK_HOSTS.write();
    let host = hosts
        .get_mut(&host_id)
        .ok_or("MemoryStick host not found")?;
    if host.card_ids.len() >= host.max_cards as usize {
        return Err("MemoryStick host is full");
    }

    let id = CARD_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    host.card_ids.push(id);
    MEMSTICK_CARDS.write().insert(
        id,
        MemstickCard {
            id,
            host_id,
            model: String::from(model),
            sectors,
            sector_size,
            state: MemstickCardState::Attached,
            storage: BTreeMap::new(),
        },
    );
    Ok(id)
}

pub fn detach_card(card_id: u32) -> Result<(), &'static str> {
    let card = MEMSTICK_CARDS
        .write()
        .remove(&card_id)
        .ok_or("MemoryStick card not found")?;

    if let Some(host) = MEMSTICK_HOSTS.write().get_mut(&card.host_id) {
        host.card_ids.retain(|id| *id != card_id);
    }
    Ok(())
}

pub fn read_sector(card_id: u32, sector: u64) -> Result<Vec<u8>, &'static str> {
    let cards = MEMSTICK_CARDS.read();
    let card = cards.get(&card_id).ok_or("MemoryStick card not found")?;
    if card.state != MemstickCardState::Attached {
        return Err("MemoryStick card is not attached");
    }
    if sector >= card.sectors {
        return Err("MemoryStick sector out of range");
    }

    Ok(card
        .storage
        .get(&sector)
        .cloned()
        .unwrap_or_else(|| vec![0u8; card.sector_size]))
}

pub fn write_sector(card_id: u32, sector: u64, data: &[u8]) -> Result<(), &'static str> {
    let mut cards = MEMSTICK_CARDS.write();
    let card = cards
        .get_mut(&card_id)
        .ok_or("MemoryStick card not found")?;
    if card.state != MemstickCardState::Attached {
        return Err("MemoryStick card is not attached");
    }
    if sector >= card.sectors {
        return Err("MemoryStick sector out of range");
    }
    if data.len() != card.sector_size {
        return Err("MemoryStick sector write has incorrect size");
    }

    card.storage.insert(sector, data.to_vec());
    Ok(())
}

pub fn list_cards() -> Vec<(u32, u32, String, u64, MemstickCardState)> {
    MEMSTICK_CARDS
        .read()
        .iter()
        .map(|(id, card)| {
            (
                *id,
                card.host_id,
                card.model.clone(),
                card.sectors,
                card.state,
            )
        })
        .collect()
}

pub fn card_count() -> usize {
    MEMSTICK_CARDS.read().len()
}

pub fn init() -> Result<(), &'static str> {
    if !MEMSTICK_HOSTS.read().is_empty() {
        return Ok(());
    }

    let host_id = register_host("sw-memstick", 1)?;
    attach_card(host_id, "MS-128M", 250_000, 512)?;
    crate::serial_println!("memstick: software host registered with 128MB card");
    Ok(())
}
