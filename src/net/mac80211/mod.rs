//! IEEE 802.11 wireless MLME (mirrors Linux `net/mac80211/`)

use spin::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WlanState {
    Scanning,
    Associating,
    Associated,
}

static WLAN_STATE: RwLock<WlanState> = RwLock::new(WlanState::Scanning);

pub fn set_wlan_state(s: WlanState) {
    *WLAN_STATE.write() = s;
}

pub fn get_wlan_state() -> WlanState {
    *WLAN_STATE.read()
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("mac80211: MLME wireless stack initialized");
    Ok(())
}
