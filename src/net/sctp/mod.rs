//! SCTP association chunk validation (mirrors Linux `net/sctp/`)

use spin::RwLock;

static SCTP_CHECKSUM_VAL: RwLock<bool> = RwLock::new(true);

pub fn set_checksum_validation(val: bool) {
    *SCTP_CHECKSUM_VAL.write() = val;
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("sctp: chunk validator initialized");
    Ok(())
}
