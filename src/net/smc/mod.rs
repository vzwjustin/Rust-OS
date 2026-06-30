//! Shared Memory Communications (mirrors Linux `net/smc/`)

use spin::RwLock;

static SMC_LINK_UP: RwLock<bool> = RwLock::new(false);

pub fn set_link_state(up: bool) {
    *SMC_LINK_UP.write() = up;
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("smc: RDMA link group registry initialized");
    Ok(())
}
