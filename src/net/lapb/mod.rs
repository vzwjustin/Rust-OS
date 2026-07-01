//! LAPB state machine (mirrors Linux `net/lapb/`)

use core::sync::atomic::{AtomicU8, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LapbState {
    Disconnected,
    Connecting,
    Connected,
}

impl LapbState {
    fn from_u8(v: u8) -> Self {
        match v {
            1 => LapbState::Connecting,
            2 => LapbState::Connected,
            _ => LapbState::Disconnected,
        }
    }
}

static LAPB_STATE: AtomicU8 = AtomicU8::new(0);

pub fn get_state() -> LapbState {
    LapbState::from_u8(LAPB_STATE.load(Ordering::Relaxed))
}

pub fn connect() {
    LAPB_STATE.store(2, Ordering::Relaxed);
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("lapb: link-level state machine initialized");
    Ok(())
}
