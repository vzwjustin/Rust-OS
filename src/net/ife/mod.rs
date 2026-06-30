//! Inter-FE Encapsulation (mirrors Linux `net/ife/`)

use alloc::vec::Vec;

pub fn encapsulate_ife(packet: &mut Vec<u8>, meta_type: u16) {
    let header = [0xED, 0x3E, (meta_type >> 8) as u8, (meta_type & 0xFF) as u8];
    packet.splice(0..0, header.iter().cloned());
}

pub fn decapsulate_ife(packet: &mut Vec<u8>) -> Option<u16> {
    if packet.len() >= 4 && packet[0] == 0xED && packet[1] == 0x3E {
        let meta_type = ((packet[2] as u16) << 8) | (packet[3] as u16);
        packet.drain(0..4);
        Some(meta_type)
    } else {
        None
    }
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("ife: encapsulation helper initialized");
    Ok(())
}
