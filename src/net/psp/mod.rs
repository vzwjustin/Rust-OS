//! PSP encryption context (mirrors Linux `net/psp/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static ENCRYPTION_KEYS: RwLock<BTreeMap<u32, alloc::vec::Vec<u8>>> = RwLock::new(BTreeMap::new());

pub fn set_encryption_key(spi: u32, key: &[u8]) {
    ENCRYPTION_KEYS.write().insert(spi, key.to_vec());
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("psp: crypt context registry initialized");
    Ok(())
}
