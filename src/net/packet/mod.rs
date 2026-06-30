//! AF_PACKET raw frame queueing (mirrors Linux `net/packet/`)

use alloc::collections::VecDeque;
use spin::Mutex;

static RAW_PACKETS: Mutex<VecDeque<alloc::vec::Vec<u8>>> = Mutex::new(VecDeque::new());

pub fn enqueue_raw_frame(frame: alloc::vec::Vec<u8>) {
    let mut q = RAW_PACKETS.lock();
    if q.len() < 512 {
        q.push_back(frame);
    }
}

pub fn dequeue_raw_frame() -> Option<alloc::vec::Vec<u8>> {
    RAW_PACKETS.lock().pop_front()
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("packet: raw frame queue initialized");
    Ok(())
}
