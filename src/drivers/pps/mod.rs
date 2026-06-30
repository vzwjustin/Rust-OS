//! PPS (Pulse Per Second) driver subsystem
//!
//! Provides PPS signal source framework for time synchronization.
//! Mirrors Linux's `drivers/pps/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// PPS source mode (Linux `struct pps_source_info` flags).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PpsMode {
    Assert,
    Clear,
    AssertClear,
}

/// PPS source (Linux `struct pps_source_info`).
pub struct PpsSource {
    pub id: u32,
    pub name: String,
    pub path: String,
    pub mode: PpsMode,
    pub assert_count: AtomicU64,
    pub clear_count: AtomicU64,
    pub last_assert_ns: AtomicU64,
    pub last_clear_ns: AtomicU64,
}

/// PPS event (Linux `struct pps_event`).
#[derive(Debug, Clone, Copy)]
pub struct PpsEvent {
    pub timestamp_ns: u64,
    pub assert_sequence: u64,
    pub clear_sequence: u64,
    pub mode: PpsMode,
}

// ── Registry ────────────────────────────────────────────────────────────

static SOURCE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static PPS_SOURCES: RwLock<BTreeMap<u32, PpsSource>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a PPS source (Linux `pps_register_source`).
pub fn register_source(name: &str, path: &str, mode: PpsMode) -> Result<u32, &'static str> {
    let id = SOURCE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let source = PpsSource {
        id,
        name: String::from(name),
        path: String::from(path),
        mode,
        assert_count: AtomicU64::new(0),
        clear_count: AtomicU64::new(0),
        last_assert_ns: AtomicU64::new(0),
        last_clear_ns: AtomicU64::new(0),
    };
    PPS_SOURCES.write().insert(id, source);
    Ok(id)
}

/// Unregister a PPS source (Linux `pps_unregister_source`).
pub fn unregister_source(id: u32) -> Result<(), &'static str> {
    PPS_SOURCES
        .write()
        .remove(&id)
        .ok_or("PPS source not found")?;
    Ok(())
}

/// Fire a PPS assert event (Linux `pps_event`).
pub fn fire_assert(source_id: u32, timestamp_ns: u64) -> Result<(), &'static str> {
    let sources = PPS_SOURCES.read();
    let source = sources.get(&source_id).ok_or("PPS source not found")?;
    source.assert_count.fetch_add(1, Ordering::SeqCst);
    source.last_assert_ns.store(timestamp_ns, Ordering::SeqCst);
    Ok(())
}

/// Fire a PPS clear event.
pub fn fire_clear(source_id: u32, timestamp_ns: u64) -> Result<(), &'static str> {
    let sources = PPS_SOURCES.read();
    let source = sources.get(&source_id).ok_or("PPS source not found")?;
    source.clear_count.fetch_add(1, Ordering::SeqCst);
    source.last_clear_ns.store(timestamp_ns, Ordering::SeqCst);
    Ok(())
}

/// Get the latest PPS event for a source.
pub fn get_event(source_id: u32) -> Result<PpsEvent, &'static str> {
    let sources = PPS_SOURCES.read();
    let source = sources.get(&source_id).ok_or("PPS source not found")?;
    Ok(PpsEvent {
        timestamp_ns: source.last_assert_ns.load(Ordering::SeqCst),
        assert_sequence: source.assert_count.load(Ordering::SeqCst),
        clear_sequence: source.clear_count.load(Ordering::SeqCst),
        mode: source.mode,
    })
}

/// List all PPS sources.
pub fn list_sources() -> Vec<(u32, String, String, PpsMode, u64)> {
    PPS_SOURCES
        .read()
        .iter()
        .map(|(id, s)| {
            (
                *id,
                s.name.clone(),
                s.path.clone(),
                s.mode,
                s.assert_count.load(Ordering::SeqCst),
            )
        })
        .collect()
}

/// Count sources.
pub fn source_count() -> usize {
    PPS_SOURCES.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    if !PPS_SOURCES.read().is_empty() {
        return Ok(());
    }

    let id = register_source("sw-pps", "/dev/pps0", PpsMode::Assert)?;
    crate::serial_println!("pps: software PPS source registered (id={})", id);
    Ok(())
}
