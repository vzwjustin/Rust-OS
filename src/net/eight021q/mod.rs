//! IEEE 802.1Q VLAN tagging
//! (mirrors Linux `net/8021q/`)

use alloc::vec::Vec;

pub fn add_vlan_tag(packet: &mut Vec<u8>, vlan_id: u16) {
    let tci = vlan_id & 0x0FFF;
    let tag = [0x81, 0x00, (tci >> 8) as u8, (tci & 0xFF) as u8];
    if packet.len() >= 12 {
        packet.splice(12..12, tag.iter().cloned());
    }
}

pub fn remove_vlan_tag(packet: &mut Vec<u8>) -> Option<u16> {
    if packet.len() >= 16 && packet[12] == 0x81 && packet[13] == 0x00 {
        let tci = ((packet[14] as u16) << 8) | (packet[15] as u16);
        packet.drain(12..16);
        Some(tci & 0x0FFF)
    } else {
        None
    }
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("8021q: VLAN tagger initialized");
    Ok(())
}
