//! Key management service (mirrors Linux `net/key/`)

use alloc::collections::BTreeMap;
use alloc::string::String;
use spin::RwLock;

static KEY_RING: RwLock<BTreeMap<String, alloc::vec::Vec<u8>>> = RwLock::new(BTreeMap::new());

pub fn add_key(name: &str, secret: &[u8]) {
    KEY_RING.write().insert(String::from(name), secret.to_vec());
}

pub fn get_key(name: &str) -> Option<alloc::vec::Vec<u8>> {
    KEY_RING.read().get(name).cloned()
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("key: retention keyring initialized");
    Ok(())
}
