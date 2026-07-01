//! IEEE 802.15.4 MAC (mirrors Linux `net/mac802154/`)

use spin::RwLock;

static PAN_ID: RwLock<u16> = RwLock::new(0xFFFF);

pub fn set_pan_id(id: u16) {
    *PAN_ID.write() = id;
}

pub fn get_pan_id() -> u16 {
    *PAN_ID.read()
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("mac802154: PAN coordinator helper initialized");
    Ok(())
}
