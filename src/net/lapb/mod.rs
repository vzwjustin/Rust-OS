//! LAPB state machine (mirrors Linux `net/lapb/`)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LapbState {
    Disconnected,
    Connecting,
    Connected,
}

static mut LAPB_STATE: LapbState = LapbState::Disconnected;

pub fn get_state() -> LapbState {
    unsafe { LAPB_STATE }
}

pub fn connect() {
    unsafe {
        LAPB_STATE = LapbState::Connected;
    }
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("lapb: link-level state machine initialized");
    Ok(())
}
