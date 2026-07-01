//! Packet sampling multicast (mirrors Linux `net/psample/`)

use spin::RwLock;

static SAMPLING_RATE: RwLock<u32> = RwLock::new(0);

pub fn set_sampling_rate(rate: u32) {
    *SAMPLING_RATE.write() = rate;
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("psample: group distributor initialized");
    Ok(())
}
