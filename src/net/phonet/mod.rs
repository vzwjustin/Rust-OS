//! Nokia Phonet protocol pep socket (mirrors Linux `net/phonet/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static PHONET_PORTS: RwLock<BTreeMap<u8, u32>> = RwLock::new(BTreeMap::new());

pub fn bind_phonet_port(port: u8, sock_id: u32) {
    PHONET_PORTS.write().insert(port, sock_id);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("phonet: pep socket pipe initialized");
    Ok(())
}
