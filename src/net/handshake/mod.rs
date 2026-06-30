//! Netlink handshake tracker (mirrors Linux `net/handshake/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static ACTIVE_HANDSHAKES: RwLock<BTreeMap<u32, &'static str>> = RwLock::new(BTreeMap::new());

pub fn register_handshake(sock_id: u32, proto: &'static str) {
    ACTIVE_HANDSHAKES.write().insert(sock_id, proto);
}

pub fn complete_handshake(sock_id: u32) {
    ACTIVE_HANDSHAKES.write().remove(&sock_id);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("handshake: TLS agent tracker initialized");
    Ok(())
}
