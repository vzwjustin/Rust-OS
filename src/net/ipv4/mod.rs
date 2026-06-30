//! IPv4 fragmentation and reassembly (mirrors Linux `net/ipv4/`)

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;
use spin::RwLock;

struct FragQueue {
    buffer: Vec<u8>,
    received_bytes: usize,
    total_bytes: usize,
}

static FRAG_QUEUES: RwLock<BTreeMap<u16, FragQueue>> = RwLock::new(BTreeMap::new());

pub fn reassemble_fragment(id: u16, offset: usize, data: &[u8], total: usize) -> Option<Vec<u8>> {
    let mut queues = FRAG_QUEUES.write();
    let q = queues.entry(id).or_insert_with(|| FragQueue {
        buffer: vec![0; total],
        received_bytes: 0,
        total_bytes: total,
    });

    if offset + data.len() <= q.buffer.len() {
        q.buffer[offset..offset + data.len()].copy_from_slice(data);
        q.received_bytes += data.len();
    }

    if q.received_bytes >= q.total_bytes {
        let res = q.buffer.clone();
        queues.remove(&id);
        Some(res)
    } else {
        None
    }
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("ipv4: fragment reassembler initialized");
    Ok(())
}
