//! Netlink multicast groups (mirrors Linux `net/netlink/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static SUBSCRIPTIONS: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());

pub fn subscribe_group(sock_id: u32, group_id: u32) {
    SUBSCRIPTIONS.write().insert(sock_id, group_id);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("netlink: multicast router initialized");
    Ok(())
}
