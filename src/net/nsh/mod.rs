//! Network Service Header forwarding (mirrors Linux `net/nsh/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static NSH_FORWARDING: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());

pub fn set_nsh_route(path_id: u32, next_hop: u32) {
    NSH_FORWARDING.write().insert(path_id, next_hop);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("nsh: path forwarding table initialized");
    Ok(())
}
