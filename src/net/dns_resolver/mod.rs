//! In-kernel DNS resolver cache (mirrors Linux `net/dns_resolver/`)

use alloc::collections::BTreeMap;
use alloc::string::String;
use spin::RwLock;

static RESOLVER_CACHE: RwLock<BTreeMap<String, [u8; 4]>> = RwLock::new(BTreeMap::new());

pub fn cache_resolve(name: &str, ip: [u8; 4]) {
    RESOLVER_CACHE.write().insert(String::from(name), ip);
}

pub fn lookup_cache(name: &str) -> Option<[u8; 4]> {
    RESOLVER_CACHE.read().get(name).cloned()
}

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("dns_resolver: in-kernel cache initialized");
    Ok(())
}
