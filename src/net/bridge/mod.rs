//! Ethernet bridge FDB (mirrors Linux `net/bridge/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

struct FdbEntry {
    mac: [u8; 6],
    port_id: u32,
    updated: u64,
}

static FDB: RwLock<BTreeMap<[u8; 6], FdbEntry>> = RwLock::new(BTreeMap::new());

pub fn update_fdb(mac: [u8; 6], port_id: u32) {
    FDB.write().insert(mac, FdbEntry { mac, port_id, updated: 1 });
}

pub fn lookup_port(mac: &[u8; 6]) -> Option<u32> {
    FDB.read().get(mac).map(|e| e.port_id)
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("bridge: forwarding database initialized");
    Ok(())
}
