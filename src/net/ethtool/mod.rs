//! Ethtool device configuration (mirrors Linux `net/ethtool/`)

use spin::RwLock;

#[derive(Clone, Copy, Debug)]
pub struct EthtoolSettings {
    pub speed_mbps: u32,
    pub duplex_full: bool,
}

static SETTINGS: RwLock<EthtoolSettings> = RwLock::new(EthtoolSettings {
    speed_mbps: 1000,
    duplex_full: true,
});

pub fn get_settings() -> EthtoolSettings {
    *SETTINGS.read()
}

pub fn set_settings(s: EthtoolSettings) {
    *SETTINGS.write() = s;
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("ethtool: command dispatcher initialized");
    Ok(())
}
