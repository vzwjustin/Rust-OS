//! IPv6 Neighbor Discovery (mirrors Linux `net/ipv6/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static NEIGHBOR_CACHE: RwLock<BTreeMap<[u8; 16], [u8; 6]>> = RwLock::new(BTreeMap::new());

pub fn update_neighbor(ip: [u8; 16], mac: [u8; 6]) {
    NEIGHBOR_CACHE.write().insert(ip, mac);
}

pub fn lookup_neighbor(ip: &[u8; 16]) -> Option<[u8; 6]> {
    NEIGHBOR_CACHE.read().get(ip).cloned()
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("ipv6: neighbor discovery cache initialized");
    Ok(())
}
