//! Devlink port registry (mirrors Linux `net/devlink/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

#[derive(Clone, Debug)]
pub struct DevlinkPort {
    pub index: u32,
    pub flavour: u16,
}

static PORTS: RwLock<BTreeMap<u32, DevlinkPort>> = RwLock::new(BTreeMap::new());

pub fn register_port(index: u32, flavour: u16) {
    PORTS.write().insert(index, DevlinkPort { index, flavour });
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("devlink: parameter registry initialized");
    Ok(())
}
