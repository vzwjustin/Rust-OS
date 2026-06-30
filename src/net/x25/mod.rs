//! X.25 PLP virtual circuit (mirrors Linux `net/x25/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static VIRTUAL_CIRCUITS: RwLock<BTreeMap<u16, bool>> = RwLock::new(BTreeMap::new());

pub fn establish_vc(lcn: u16) {
    VIRTUAL_CIRCUITS.write().insert(lcn, true);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("x25: virtual circuit multiplexer initialized");
    Ok(())
}
