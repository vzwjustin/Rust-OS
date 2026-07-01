//! LLC Type 1 connection (mirrors Linux `net/llc/`)

use spin::RwLock;

static LLC_CONNECTED: RwLock<bool> = RwLock::new(false);

pub fn set_connected(c: bool) {
    *LLC_CONNECTED.write() = c;
}

pub fn is_connected() -> bool {
    *LLC_CONNECTED.read()
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("llc: Type 1 connection state initialized");
    Ok(())
}
