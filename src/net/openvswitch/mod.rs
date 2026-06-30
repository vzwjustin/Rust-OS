//! Open vSwitch flow table (mirrors Linux `net/openvswitch/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static FLOW_TABLE: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());

pub fn add_flow_rule(match_val: u32, action: u32) {
    FLOW_TABLE.write().insert(match_val, action);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("openvswitch: wildcard flow table initialized");
    Ok(())
}
