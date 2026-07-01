//! High-availability Seamless Redundancy (mirrors Linux `net/hsr/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static SEEN_FRAMES: RwLock<BTreeMap<[u8; 6], u16>> = RwLock::new(BTreeMap::new());

pub fn is_duplicate(src: [u8; 6], seq: u16) -> bool {
    let mut seen = SEEN_FRAMES.write();
    if let Some(&last_seq) = seen.get(&src) {
        if seq <= last_seq {
            return true;
        }
    }
    seen.insert(src, seq);
    false
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("hsr: duplicate elimination initialized");
    Ok(())
}
