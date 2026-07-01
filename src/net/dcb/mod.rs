//! Data Center Bridging (mirrors Linux `net/dcb/`)

use spin::RwLock;

static PFC_ENABLED: RwLock<bool> = RwLock::new(false);

pub fn set_pfc_enabled(enabled: bool) {
    *PFC_ENABLED.write() = enabled;
}

pub fn is_pfc_enabled() -> bool {
    *PFC_ENABLED.read()
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("dcb: PFC configuration initialized");
    Ok(())
}
