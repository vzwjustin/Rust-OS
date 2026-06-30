//! MCTP packet assembly (mirrors Linux `net/mctp/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static EID_ROUTING: RwLock<BTreeMap<u8, u32>> = RwLock::new(BTreeMap::new());

pub fn add_route(eid: u8, port: u32) {
    EID_ROUTING.write().insert(eid, port);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("mctp: EID-to-port router initialized");
    Ok(())
}
