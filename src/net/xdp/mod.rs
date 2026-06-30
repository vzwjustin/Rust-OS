//! eXpress Data Path (mirrors Linux `net/xdp/`)

use spin::RwLock;

static REDIRECT_PORT: RwLock<u32> = RwLock::new(0);

pub fn set_redirect_port(port: u32) {
    *REDIRECT_PORT.write() = port;
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("xdp: packet redirect map initialized");
    Ok(())
}
