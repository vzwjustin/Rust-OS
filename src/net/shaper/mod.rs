//! Traffic shaper queue (mirrors Linux `net/shaper/`)

use spin::RwLock;

static BANDWIDTH_LIMIT_KBPS: RwLock<u32> = RwLock::new(0); // 0 means unlimited

pub fn set_bandwidth_limit(limit: u32) {
    *BANDWIDTH_LIMIT_KBPS.write() = limit;
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("shaper: queue rate limiter initialized");
    Ok(())
}
