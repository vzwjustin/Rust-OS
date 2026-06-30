//! Qualcomm IPC Router (mirrors Linux `net/qrtr/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static NODE_PORTS: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());

pub fn register_node_port(node: u32, port: u32) {
    NODE_PORTS.write().insert(node, port);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("qrtr: node/port router initialized");
    Ok(())
}
