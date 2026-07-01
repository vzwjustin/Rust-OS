//! Ceph messenger v2 (mirrors Linux `net/ceph/`)

pub struct CephFrame {
    pub tag: u8,
    pub len: u32,
}

pub fn parse_frame_header(data: &[u8]) -> Option<CephFrame> {
    if data.len() < 5 {
        return None;
    }
    let tag = data[0];
    let len = u32::from_le_bytes([data[1], data[2], data[3], data[4]]);
    Some(CephFrame { tag, len })
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("ceph: client messenger initialized");
    Ok(())
}
