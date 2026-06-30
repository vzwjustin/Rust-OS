//! Multipath TCP subflow sequence mapping (mirrors Linux `net/mptcp/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static SUBFLOW_SEQS: RwLock<BTreeMap<u32, u64>> = RwLock::new(BTreeMap::new());

pub fn map_sequence(subflow_id: u32, seq: u64) {
    SUBFLOW_SEQS.write().insert(subflow_id, seq);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("mptcp: subflow manager initialized");
    Ok(())
}
