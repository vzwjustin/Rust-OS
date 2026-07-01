//! Direct Cache Access provider registry.
//!
//! Rust-owned mirror of Linux `drivers/dca/`. Providers expose CPU masks and
//! cache tags; consumers allocate and release tags through this module.

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

#[derive(Debug, Clone)]
pub struct DcaProvider {
    pub id: u32,
    pub name: String,
    pub cpu_mask: u64,
    pub tag_base: u32,
    pub tag_count: u32,
    pub active_tags: u32,
    pub next_tag_offset: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DcaTag {
    pub id: u32,
    pub provider_id: u32,
    pub cpu_id: u32,
    pub cache_tag: u32,
}

static PROVIDER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static TAG_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DCA_PROVIDERS: RwLock<BTreeMap<u32, DcaProvider>> = RwLock::new(BTreeMap::new());
static DCA_TAGS: RwLock<BTreeMap<u32, DcaTag>> = RwLock::new(BTreeMap::new());

pub fn register_provider(
    name: &str,
    cpu_mask: u64,
    tag_base: u32,
    tag_count: u32,
) -> Result<u32, &'static str> {
    if cpu_mask == 0 {
        return Err("DCA provider requires at least one CPU");
    }
    if tag_count == 0 {
        return Err("DCA provider requires at least one tag");
    }

    let id = PROVIDER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    DCA_PROVIDERS.write().insert(
        id,
        DcaProvider {
            id,
            name: String::from(name),
            cpu_mask,
            tag_base,
            tag_count,
            active_tags: 0,
            next_tag_offset: 0,
        },
    );
    Ok(id)
}

pub fn unregister_provider(provider_id: u32) -> Result<(), &'static str> {
    if DCA_TAGS
        .read()
        .values()
        .any(|tag| tag.provider_id == provider_id)
    {
        return Err("DCA provider still has active tags");
    }

    DCA_PROVIDERS
        .write()
        .remove(&provider_id)
        .map(|_| ())
        .ok_or("DCA provider not found")
}

pub fn request_tag(cpu_id: u32) -> Result<DcaTag, &'static str> {
    if cpu_id >= u64::BITS {
        return Err("DCA CPU id outside mask range");
    }

    let cpu_bit = 1u64 << cpu_id;
    let mut providers = DCA_PROVIDERS.write();
    let provider = providers
        .values_mut()
        .find(|provider| {
            (provider.cpu_mask & cpu_bit) != 0 && provider.active_tags < provider.tag_count
        })
        .ok_or("no DCA provider available for CPU")?;

    let id = TAG_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let tag = DcaTag {
        id,
        provider_id: provider.id,
        cpu_id,
        cache_tag: provider.tag_base + provider.next_tag_offset,
    };
    provider.active_tags += 1;
    provider.next_tag_offset += 1;
    DCA_TAGS.write().insert(id, tag);
    Ok(tag)
}

pub fn release_tag(tag_id: u32) -> Result<(), &'static str> {
    let tag = DCA_TAGS
        .write()
        .remove(&tag_id)
        .ok_or("DCA tag not found")?;

    if let Some(provider) = DCA_PROVIDERS.write().get_mut(&tag.provider_id) {
        provider.active_tags = provider.active_tags.saturating_sub(1);
    }
    Ok(())
}

pub fn list_providers() -> Vec<DcaProvider> {
    DCA_PROVIDERS.read().values().cloned().collect()
}

pub fn active_tag_count() -> usize {
    DCA_TAGS.read().len()
}

pub fn init() -> Result<(), &'static str> {
    if !DCA_PROVIDERS.read().is_empty() {
        return Ok(());
    }

    register_provider("sw-dca", 0x01, 0, 8)?;
    crate::serial_println!("dca: software DCA provider registered (cpu_mask=0x1, 8 tags)");
    Ok(())
}
