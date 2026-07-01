//! Core network stack (mirrors Linux `net/core/`)

use alloc::collections::VecDeque;
use spin::Mutex;

static RX_QUEUE: Mutex<VecDeque<alloc::vec::Vec<u8>>> = Mutex::new(VecDeque::new());

pub fn enqueue_rx(packet: alloc::vec::Vec<u8>) {
    let mut q = RX_QUEUE.lock();
    if q.len() < 1024 {
        q.push_back(packet);
    }
}

pub fn dequeue_rx() -> Option<alloc::vec::Vec<u8>> {
    RX_QUEUE.lock().pop_front()
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("core: socket buffer queue initialized");
    Ok(())
}
