//! NetLabel security labeling (mirrors Linux `net/netlabel/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static SEC_DOMAINS: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());

pub fn map_sec_domain(domain_id: u32, label: u32) {
    SEC_DOMAINS.write().insert(domain_id, label);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("netlabel: CIPSO/RIPSO mapping initialized");
    Ok(())
}
