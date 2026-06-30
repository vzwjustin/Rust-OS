//! IPsec XFRM SA database (mirrors Linux `net/xfrm/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

#[derive(Clone, Debug)]
pub struct XfrmSa {
    pub spi: u32,
    pub mode: u8,
}

static SA_DATABASE: RwLock<BTreeMap<u32, XfrmSa>> = RwLock::new(BTreeMap::new());

pub fn add_sa(sa: XfrmSa) {
    SA_DATABASE.write().insert(sa.spi, sa);
}

pub fn get_sa(spi: u32) -> Option<XfrmSa> {
    SA_DATABASE.read().get(&spi).cloned()
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("xfrm: security association database initialized");
    Ok(())
}
