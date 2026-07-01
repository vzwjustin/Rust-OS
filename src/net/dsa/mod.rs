//! Distributed Switch Architecture tagging (mirrors Linux `net/dsa/`)

use alloc::vec::Vec;

pub fn add_dsa_tag(packet: &mut Vec<u8>, port_id: u8) {
    if packet.len() >= 12 {
        packet.insert(12, port_id); // Simple 1-byte DSA tag
    }
}

pub fn remove_dsa_tag(packet: &mut Vec<u8>) -> Option<u8> {
    if packet.len() >= 13 {
        Some(packet.remove(12))
    } else {
        None
    }
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("dsa: switch tagger initialized");
    Ok(())
}
