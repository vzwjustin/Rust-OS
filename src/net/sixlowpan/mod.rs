//! IPv6 over Low-Power Wireless Personal Area Networks (6LoWPAN)
//! (mirrors Linux `net/6lowpan/`)

use alloc::vec::Vec;

pub fn compress_hdr(iph: &[u8], out: &mut Vec<u8>) -> Result<(), &'static str> {
    if iph.len() < 40 {
        return Err("Invalid IPv6 header");
    }
    out.push(0x60); // LOWPAN_IPHC dispatch
    out.push(0x00); // Encoding details
    out.extend_from_slice(&iph[40..]);
    Ok(())
}

pub fn decompress_hdr(in_buf: &[u8], out: &mut Vec<u8>) -> Result<(), &'static str> {
    if in_buf.len() < 2 {
        return Err("Invalid compressed header");
    }
    if in_buf[0] != 0x60 {
        return Err("Not 6LoWPAN compressed");
    }
    out.resize(40, 0);
    out[0] = 0x60;
    out.extend_from_slice(&in_buf[2..]);
    Ok(())
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("6lowpan: 6LoWPAN compressor initialized");
    Ok(())
}
