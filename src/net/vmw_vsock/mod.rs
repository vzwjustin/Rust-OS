//! VMware Virtual Sockets (mirrors Linux `net/vmw_vsock/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static VSOCK_CONNECTIONS: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());

pub fn connect_vsock(cid: u32, port: u32) {
    VSOCK_CONNECTIONS.write().insert(cid, port);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("vmw_vsock: connection registry initialized");
    Ok(())
}
