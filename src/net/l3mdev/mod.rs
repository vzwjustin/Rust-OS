//! Layer 3 Master Device VRF (mirrors Linux `net/l3mdev/`)

use alloc::collections::BTreeMap;
use spin::RwLock;

static VRF_BINDINGS: RwLock<BTreeMap<u32, u32>> = RwLock::new(BTreeMap::new());

pub fn bind_device_to_vrf(dev_id: u32, vrf_id: u32) {
    VRF_BINDINGS.write().insert(dev_id, vrf_id);
}

pub fn get_device_vrf(dev_id: u32) -> Option<u32> {
    VRF_BINDINGS.read().get(&dev_id).cloned()
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("l3mdev: VRF routing master initialized");
    Ok(())
}
