//! NFC LLCP connection registry (mirrors Linux `net/nfc/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static NFC_DEVICES: RwLock<BTreeMap<u32, bool>> = RwLock::new(BTreeMap::new());

pub fn register_nfc_device(id: u32, active: bool) {
    NFC_DEVICES.write().insert(id, active);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("nfc: LLCP registry initialized");
    Ok(())
}
