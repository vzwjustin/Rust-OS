//! Hot-add memory region registration.
//!
//! Tracks pluggable memory blocks and feeds newly online regions into the
//! physical frame allocator (Linux memory hotplug analogue).

extern crate alloc;

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use spin::RwLock;
use x86_64::PhysAddr;

use crate::linux_compat::{LinuxError, LinuxResult};

const PAGE_SIZE: u64 = 4096;

/// State of a hot-pluggable memory block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotplugState {
    Offline,
    Online,
    GoingOnline,
    GoingOffline,
}

/// One registered hotplug memory region.
#[derive(Debug, Clone)]
pub struct HotplugRegion {
    pub id: u64,
    pub start: PhysAddr,
    pub size: u64,
    pub state: HotplugState,
    pub numa_node: u32,
    pub label: String,
}

static NEXT_ID: AtomicU64 = AtomicU64::new(1);
static REGIONS: RwLock<Vec<HotplugRegion>> = RwLock::new(Vec::new());
static ONLINE_BYTES: AtomicUsize = AtomicUsize::new(0);

/// Register a new hot-add memory region (initially offline).
pub fn register_region(start: u64, size: u64, numa_node: u32, label: &str) -> LinuxResult<u64> {
    if size == 0 || start % PAGE_SIZE != 0 || size % PAGE_SIZE != 0 {
        return Err(LinuxError::EINVAL);
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    REGIONS.write().push(HotplugRegion {
        id,
        start: PhysAddr::new(start),
        size,
        state: HotplugState::Offline,
        numa_node,
        label: String::from(label),
    });
    Ok(id)
}

/// Bring a registered region online and extend the frame allocator.
pub fn online_region(id: u64) -> LinuxResult<()> {
    let (start, size, numa_node) = {
        let mut regions = REGIONS.write();
        let region = regions
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or(LinuxError::ENOENT)?;
        if region.state == HotplugState::Online {
            return Ok(());
        }
        region.state = HotplugState::GoingOnline;
        (region.start.as_u64(), region.size, region.numa_node)
    };

    let frames = crate::memory::hotplug_add_usable_range(start, start + size)
        .map_err(|_| LinuxError::ENOMEM)?;

    {
        let mut regions = REGIONS.write();
        if let Some(region) = regions.iter_mut().find(|r| r.id == id) {
            region.state = HotplugState::Online;
        }
    }

    ONLINE_BYTES.fetch_add(size as usize, Ordering::Relaxed);
    if crate::numa::is_valid_node(numa_node) {
        // Refresh node free memory accounting after hot-add.
        let mem_kb = size / 1024;
        crate::numa::register_node(crate::numa::NumaNode {
            id: numa_node,
            online: true,
            mem_total_kb: mem_kb,
            mem_free_kb: mem_kb,
        });
    }

    crate::serial_println!(
        "[memory_hotplug] region {} online: {:#x}..{:#x} (+{} frames)",
        id,
        start,
        start + size,
        frames
    );
    Ok(())
}

/// Take a region offline (frames are not reclaimed — mirrors partial Linux stub).
pub fn offline_region(id: u64) -> LinuxResult<()> {
    let mut regions = REGIONS.write();
    let region = regions
        .iter_mut()
        .find(|r| r.id == id)
        .ok_or(LinuxError::ENOENT)?;
    if region.state != HotplugState::Online {
        return Err(LinuxError::EINVAL);
    }
    region.state = HotplugState::Offline;
    ONLINE_BYTES.fetch_sub(region.size as usize, Ordering::Relaxed);
    crate::serial_println!(
        "[memory_hotplug] region {} marked offline ({})",
        id,
        region.label
    );
    Ok(())
}

pub fn region_count() -> usize {
    REGIONS.read().len()
}

pub fn online_bytes() -> usize {
    ONLINE_BYTES.load(Ordering::Relaxed)
}

pub fn list_regions() -> Vec<HotplugRegion> {
    REGIONS.read().clone()
}

/// Initialize hotplug registry (no regions added until firmware/ACPI reports them).
pub fn init() {
    REGIONS.write().clear();
    ONLINE_BYTES.store(0, Ordering::Relaxed);
    crate::serial_println!("[memory_hotplug] initialized");
}
