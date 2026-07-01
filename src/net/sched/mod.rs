//! Traffic control Qdisc (mirrors Linux `net/sched/`)

use spin::RwLock;

static QDISC_LIMIT: RwLock<u32> = RwLock::new(1000);

pub fn set_qdisc_limit(limit: u32) {
    *QDISC_LIMIT.write() = limit;
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("sched: queueing discipline initialized");
    Ok(())
}
