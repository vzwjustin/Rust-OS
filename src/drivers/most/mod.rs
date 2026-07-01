//! MOST (Media Oriented Systems Transport) subsystem
//! (mirrors Linux `drivers/most/`)
//!
//! Registers MOST interface devices and their channels (control, async,
//! synchronous, isochronous) and moves buffers across a channel in the
//! configured direction.

use alloc::collections::BTreeMap;
use alloc::collections::VecDeque;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataType {
    Control,
    Async,
    Sync,
    Isoc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Rx,
    Tx,
}

#[derive(Clone)]
struct Channel {
    index: u16,
    data_type: DataType,
    direction: Direction,
    buffer_size: u16,
    fifo: VecDeque<Vec<u8>>,
}

struct Interface {
    id: u32,
    name: String,
    channels: Vec<Channel>,
}

// ── Registry ──────────────────────────────────────────────────────────────

static IFACES: RwLock<BTreeMap<u32, Interface>> = RwLock::new(BTreeMap::new());
static NEXT_ID: AtomicU32 = AtomicU32::new(0);

// ── Public API ──────────────────────────────────────────────────────────

pub fn register_interface(name: &str) -> u32 {
    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    IFACES.write().insert(
        id,
        Interface {
            id,
            name: String::from(name),
            channels: Vec::new(),
        },
    );
    id
}

pub fn add_channel(
    iface_id: u32,
    index: u16,
    data_type: DataType,
    direction: Direction,
    buffer_size: u16,
) -> Result<(), &'static str> {
    let mut ifaces = IFACES.write();
    let iface = ifaces
        .get_mut(&iface_id)
        .ok_or("most: interface not found")?;
    if iface.channels.iter().any(|c| c.index == index) {
        return Err("most: channel index in use");
    }
    iface.channels.push(Channel {
        index,
        data_type,
        direction,
        buffer_size,
        fifo: VecDeque::new(),
    });
    Ok(())
}

fn channel_mut<'a>(iface: &'a mut Interface, index: u16) -> Result<&'a mut Channel, &'static str> {
    iface
        .channels
        .iter_mut()
        .find(|c| c.index == index)
        .ok_or("most: channel not found")
}

pub fn write(iface_id: u32, channel: u16, data: &[u8]) -> Result<(), &'static str> {
    let mut ifaces = IFACES.write();
    let iface = ifaces
        .get_mut(&iface_id)
        .ok_or("most: interface not found")?;
    let ch = channel_mut(iface, channel)?;
    if ch.direction != Direction::Tx {
        return Err("most: channel is not TX");
    }
    if data.len() > ch.buffer_size as usize {
        return Err("most: buffer exceeds channel size");
    }
    ch.fifo.push_back(data.to_vec());
    Ok(())
}

pub fn read(iface_id: u32, channel: u16) -> Result<Option<Vec<u8>>, &'static str> {
    let mut ifaces = IFACES.write();
    let iface = ifaces
        .get_mut(&iface_id)
        .ok_or("most: interface not found")?;
    let ch = channel_mut(iface, channel)?;
    Ok(ch.fifo.pop_front())
}

pub fn channel_count(iface_id: u32) -> usize {
    IFACES
        .read()
        .get(&iface_id)
        .map(|i| i.channels.len())
        .unwrap_or(0)
}

pub fn interface_count() -> usize {
    IFACES.read().len()
}

/// Initialize MOST with a software interface exposing control + sync channels.
pub fn init() -> Result<(), &'static str> {
    if !IFACES.read().is_empty() {
        return Ok(());
    }
    let iface = register_interface("most0");
    add_channel(iface, 0, DataType::Control, Direction::Tx, 64)?;
    add_channel(iface, 1, DataType::Sync, Direction::Rx, 1024)?;
    crate::serial_println!("most: interface most0, {} channel(s)", channel_count(iface));
    Ok(())
}
