//! Layer 2 Tunneling Protocol (mirrors Linux `net/l2tp/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

struct L2tpSession {
    tunnel_id: u32,
    session_id: u32,
}

static SESSIONS: RwLock<BTreeMap<u32, L2tpSession>> = RwLock::new(BTreeMap::new());

pub fn register_session(tunnel_id: u32, session_id: u32) {
    SESSIONS.write().insert(session_id, L2tpSession { tunnel_id, session_id });
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("l2tp: tunnel session registry initialized");
    Ok(())
}
