//! 9P2000 protocol framing
//! (mirrors Linux `net/9p/`)

use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Msg9p {
    Tversion {
        msize: u32,
        version: alloc::string::String,
    },
    Rversion {
        msize: u32,
        version: alloc::string::String,
    },
}

pub fn encode_msg(msg: &Msg9p, out: &mut Vec<u8>) {
    match msg {
        Msg9p::Tversion { msize, version } => {
            let len = 4 + 1 + 2 + 4 + 2 + version.len() as u32;
            out.extend_from_slice(&len.to_le_bytes());
            out.push(100); // Tversion
            out.extend_from_slice(&0xFFFFu16.to_le_bytes());
            out.extend_from_slice(&msize.to_le_bytes());
            out.extend_from_slice(&(version.len() as u16).to_le_bytes());
            out.extend_from_slice(version.as_bytes());
        }
        Msg9p::Rversion { msize, version } => {
            let len = 4 + 1 + 2 + 4 + 2 + version.len() as u32;
            out.extend_from_slice(&len.to_le_bytes());
            out.push(101); // Rversion
            out.extend_from_slice(&0xFFFFu16.to_le_bytes());
            out.extend_from_slice(&msize.to_le_bytes());
            out.extend_from_slice(&(version.len() as u16).to_le_bytes());
            out.extend_from_slice(version.as_bytes());
        }
    }
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("9p: 9P2000 transport layer initialized");
    Ok(())
}
