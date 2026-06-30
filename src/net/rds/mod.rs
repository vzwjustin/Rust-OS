//! Reliable Datagram Sockets (mirrors Linux `net/rds/`)

use spin::RwLock;

static RDS_TRANSPORT_UP: RwLock<bool> = RwLock::new(false);

pub fn set_transport_state(up: bool) {
    *RDS_TRANSPORT_UP.write() = up;
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("rds: transport layer initialized");
    Ok(())
}
