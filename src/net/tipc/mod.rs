//! Transparent Inter-Process Communication (mirrors Linux `net/tipc/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static TOPOLOGY_REGS: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());

pub fn register_topology_port(port: u32, node: u32) {
    TOPOLOGY_REGS.write().insert(port, node);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("tipc: node topology registry initialized");
    Ok(())
}
