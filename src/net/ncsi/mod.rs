//! NC-SI channel management (mirrors Linux `net/ncsi/`)

use spin::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NcsiChannelState {
    Inactive,
    Configuring,
    Active,
}

static CHANNEL_STATE: RwLock<NcsiChannelState> = RwLock::new(NcsiChannelState::Inactive);

pub fn set_channel_state(s: NcsiChannelState) {
    *CHANNEL_STATE.write() = s;
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("ncsi: channel state machine initialized");
    Ok(())
}
