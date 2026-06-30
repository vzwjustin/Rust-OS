//! Kernel Connection Multiplexor (mirrors Linux `net/kcm/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static KCM_MULTIPLEXERS: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());

pub fn attach_tcp_to_kcm(kcm_fd: u32, tcp_fd: u32) {
    KCM_MULTIPLEXERS.write().insert(kcm_fd, tcp_fd);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("kcm: connection multiplexer initialized");
    Ok(())
}
