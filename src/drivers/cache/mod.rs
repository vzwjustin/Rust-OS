//! Cache (CPU cache) driver subsystem
//!
//! Provides CPU cache topology and management framework.
//! Mirrors Linux's `drivers/cache/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Cache level (L1, L2, L3, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheLevel {
    L1i,
    L1d,
    L2,
    L3,
    L4,
}

/// Cache type (Linux `enum cache_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheType {
    Unified,
    Instruction,
    Data,
}

/// Cache information (Linux `struct cacheinfo`).
pub struct CacheInfo {
    pub id: u32,
    pub cpu: u32,
    pub level: CacheLevel,
    pub cache_type: CacheType,
    pub size: u32,
    pub ways: u32,
    pub line_size: u32,
    pub sets: u32,
    pub shared: u32,
    pub inclusive: bool,
}

// ── Registry ────────────────────────────────────────────────────────────

static CACHE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static CACHES: RwLock<BTreeMap<u32, CacheInfo>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register cache information for a CPU.
pub fn register_cache(
    cpu: u32,
    level: CacheLevel,
    cache_type: CacheType,
    size: u32,
    ways: u32,
    line_size: u32,
    sets: u32,
    shared: u32,
    inclusive: bool,
) -> Result<u32, &'static str> {
    let id = CACHE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let cache = CacheInfo {
        id,
        cpu,
        level,
        cache_type,
        size,
        ways,
        line_size,
        sets,
        shared,
        inclusive,
    };
    CACHES.write().insert(id, cache);
    Ok(id)
}

/// Get cache info for a specific CPU.
pub fn get_cpu_caches(cpu: u32) -> Vec<(u32, CacheLevel, CacheType, u32)> {
    CACHES
        .read()
        .iter()
        .filter(|(_, c)| c.cpu == cpu)
        .map(|(id, c)| (*id, c.level, c.cache_type, c.size))
        .collect()
}

/// Get all cache info.
pub fn list_caches() -> Vec<(u32, u32, CacheLevel, CacheType, u32, u32)> {
    CACHES
        .read()
        .iter()
        .map(|(id, c)| (*id, c.cpu, c.level, c.cache_type, c.size, c.line_size))
        .collect()
}

/// Count registered caches.
pub fn cache_count() -> usize {
    CACHES.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !CACHES.read().is_empty() {
        return Ok(());
    }

    register_cache(
        0,
        CacheLevel::L1i,
        CacheType::Instruction,
        32 * 1024,
        8,
        64,
        64,
        1,
        false,
    )?;
    register_cache(
        0,
        CacheLevel::L1d,
        CacheType::Data,
        32 * 1024,
        8,
        64,
        64,
        1,
        false,
    )?;
    register_cache(
        0,
        CacheLevel::L2,
        CacheType::Unified,
        256 * 1024,
        8,
        64,
        512,
        1,
        false,
    )?;
    register_cache(
        0,
        CacheLevel::L3,
        CacheType::Unified,
        8 * 1024 * 1024,
        16,
        64,
        8192,
        0,
        true,
    )?;

    crate::serial_println!("bus: registered 4 caches for CPU 0 (L1i=32K L1d=32K L2=256K L3=8M)");
    Ok(())
}
