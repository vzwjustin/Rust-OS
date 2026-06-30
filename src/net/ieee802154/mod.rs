//! IEEE 802.15.4 frame parsing (mirrors Linux `net/ieee802154/`)

pub struct Ieee802154Header {
    pub fcf: u16,
    pub seq: u8,
}

pub fn parse_header(data: &[u8]) -> Option<Ieee802154Header> {
    if data.len() < 3 { return None; }
    let fcf = u16::from_le_bytes([data[0], data[1]]);
    let seq = data[2];
    Some(Ieee802154Header { fcf, seq })
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("ieee802154: wireless frame parser initialized");
    Ok(())
}
