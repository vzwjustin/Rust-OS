//! Switchdev port offloading (mirrors Linux `net/switchdev/`)

use spin::RwLock;

static OFFLOAD_ENABLED: RwLock<bool> = RwLock::new(false);

pub fn set_offload_enabled(enabled: bool) {
    *OFFLOAD_ENABLED.write() = enabled;
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("switchdev: attribute offloader initialized");
    Ok(())
}
