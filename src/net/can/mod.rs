//! CAN socket filtering (mirrors Linux `net/can/`)

use alloc::vec::Vec;
use spin::RwLock;

#[derive(Clone, Copy, Debug)]
pub struct CanFilter {
    pub can_id: u32,
    pub can_mask: u32,
}

static FILTERS: RwLock<Vec<CanFilter>> = RwLock::new(Vec::new());

pub fn add_filter(filter: CanFilter) {
    FILTERS.write().push(filter);
}

pub fn match_id(id: u32) -> bool {
    let filters = FILTERS.read();
    if filters.is_empty() {
        return true;
    }
    filters
        .iter()
        .any(|f| (id & f.can_mask) == (f.can_id & f.can_mask))
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("can: filter registry initialized");
    Ok(())
}
