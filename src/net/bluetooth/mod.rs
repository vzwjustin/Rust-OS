//! Bluetooth stack (mirrors Linux `net/bluetooth/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

#[derive(Clone)]
pub struct L2capChannel {
    pub psm: u16,
    pub scid: u16,
    pub dcid: u16,
}

static CHANNELS: RwLock<BTreeMap<u16, L2capChannel>> = RwLock::new(BTreeMap::new());

pub fn register_channel(psm: u16, scid: u16, dcid: u16) {
    CHANNELS.write().insert(scid, L2capChannel { psm, scid, dcid });
}

pub fn get_channel(scid: u16) -> Option<L2capChannel> {
    CHANNELS.read().get(&scid).cloned()
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("bluetooth: L2CAP registry initialized");
    Ok(())
}
