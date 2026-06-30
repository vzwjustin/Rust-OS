//! ATM net layer (mirrors Linux `net/atm/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AtmAddr {
    pub vpi: u16,
    pub vci: u16,
}

static ATM_SOCKETS: RwLock<BTreeMap<u32, AtmAddr>> = RwLock::new(BTreeMap::new());

pub fn bind_socket(sock_id: u32, addr: AtmAddr) {
    ATM_SOCKETS.write().insert(sock_id, addr);
}

pub fn get_socket_addr(sock_id: u32) -> Option<AtmAddr> {
    ATM_SOCKETS.read().get(&sock_id).cloned()
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("atm: net layer initialized");
    Ok(())
}
