//! RF switch state (mirrors Linux `net/rfkill/`)

use spin::RwLock;

static RF_BLOCKED: RwLock<bool> = RwLock::new(false);

pub fn set_rf_blocked(blocked: bool) {
    *RF_BLOCKED.write() = blocked;
}

pub fn is_rf_blocked() -> bool {
    *RF_BLOCKED.read()
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("rfkill: state registry initialized");
    Ok(())
}
