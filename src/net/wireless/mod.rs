//! cfg80211 wiphy registry (mirrors Linux `net/wireless/`)

use alloc::collections::BTreeMap;
use alloc::string::String;
use spin::RwLock;

static WIPHY_DEVICES: RwLock<BTreeMap<u32, String>> = RwLock::new(BTreeMap::new());

pub fn register_wiphy(id: u32, name: &str) {
    WIPHY_DEVICES.write().insert(id, String::from(name));
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("wireless: wiphy device registry initialized");
    Ok(())
}
