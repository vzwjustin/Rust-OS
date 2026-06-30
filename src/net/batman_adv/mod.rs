//! B.A.T.M.A.N. Advanced routing (mirrors Linux `net/batman-adv/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

struct BatmanRoute {
    orig: [u8; 6],
    router: [u8; 6],
    tq: u8,
}

static ROUTING_TABLE: RwLock<BTreeMap<[u8; 6], BatmanRoute>> = RwLock::new(BTreeMap::new());

pub fn update_route(orig: [u8; 6], router: [u8; 6], tq: u8) {
    let mut table = ROUTING_TABLE.write();
    if let Some(r) = table.get_mut(&orig) {
        if tq > r.tq {
            r.router = router;
            r.tq = tq;
        }
    } else {
        table.insert(orig, BatmanRoute { orig, router, tq });
    }
}

pub fn find_router(orig: &[u8; 6]) -> Option<[u8; 6]> {
    ROUTING_TABLE.read().get(orig).map(|r| r.router)
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("batman-adv: mesh routing initialized");
    Ok(())
}
