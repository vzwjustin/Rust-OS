//! Sun RPC client call serialization (mirrors Linux `net/sunrpc/`)

use alloc::vec::Vec;

pub fn encode_xdr_header(xid: u32, proc_id: u32, out: &mut Vec<u8>) {
    out.extend_from_slice(&xid.to_be_bytes());
    out.extend_from_slice(&0u32.to_be_bytes()); // CALL type
    out.extend_from_slice(&2u32.to_be_bytes()); // RPC version 2
    out.extend_from_slice(&100003u32.to_be_bytes()); // Program (NFS)
    out.extend_from_slice(&proc_id.to_be_bytes()); // Procedure
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("sunrpc: RPC client serializer initialized");
    Ok(())
}
