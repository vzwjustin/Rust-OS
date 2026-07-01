//! VM Inter-User Communication Vehicle (mirrors Linux `net/iucv/`)

use alloc::collections::VecDeque;
use spin::Mutex;

static IUCV_INBOX: Mutex<VecDeque<alloc::vec::Vec<u8>>> = Mutex::new(VecDeque::new());

pub fn send_iucv_message(data: alloc::vec::Vec<u8>) {
    IUCV_INBOX.lock().push_back(data);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("iucv: path queueing initialized");
    Ok(())
}
