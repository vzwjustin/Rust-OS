//! MPLS label switching (mirrors Linux `net/mpls/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static MPLS_TABLE: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());

pub fn add_switching_route(in_label: u32, out_label: u32) {
    MPLS_TABLE.write().insert(in_label, out_label);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("mpls: label switching router initialized");
    Ok(())
}
