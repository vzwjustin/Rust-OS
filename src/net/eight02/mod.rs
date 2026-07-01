//! IEEE 802 LLC (Logical Link Control)
//! (mirrors Linux `net/802/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LlcSap {
    pub dsap: u8,
    pub ssap: u8,
}

static SAPS: RwLock<BTreeMap<u8, LlcSap>> = RwLock::new(BTreeMap::new());

pub fn register_sap(dsap: u8, ssap: u8) {
    SAPS.write().insert(dsap, LlcSap { dsap, ssap });
}

pub fn match_sap(dsap: u8) -> bool {
    SAPS.read().contains_key(&dsap)
}

pub fn init() -> Result<(), &'static str> {
    register_sap(0xaa, 0xaa); // SNAP SAP
    crate::serial_println!("802: LLC SAP registry initialized");
    Ok(())
}
