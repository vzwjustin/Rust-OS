//! AGP (Accelerated Graphics Port) subsystem
//!
//! Provides AGP bus framework for AGP graphics cards.
//! Mirrors Linux's `drivers/char/agp/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// AGP bridge (Linux `struct agp_bridge_data`).
pub struct AgpBridge {
    pub id: u32,
    pub name: String,
    pub vendor: u16,
    pub device: u16,
    pub mode: AgpMode,
    pub aperture_base: u64,
    pub aperture_size: u64,
    pub aperture_page_count: u32,
    pub max_memory: u32,
    pub current_memory: u32,
    pub state: AgpState,
    pub ops: AgpOps,
    pub aperture_list: Vec<AgpMemory>,
}

/// AGP mode (Linux `struct agp_mode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgpMode {
    pub rate: AgpRate,
    pub sba: bool, // Sideband Addressing
    pub agp3: bool,
    pub fw: bool, // Fast Writes
}

/// AGP rate (Linux `enum agp_rate`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgpRate {
    X1,
    X2,
    X4,
    X8,
}

/// AGP state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgpState {
    Uninitialized,
    Initialized,
    Enabled,
    Disabled,
}

/// AGP operations (Linux `struct agp_bridge_driver`).
pub struct AgpOps {
    pub configure: fn(bridge_id: u32) -> Result<(), &'static str>,
    pub enable: fn(bridge_id: u32, mode: AgpMode) -> Result<(), &'static str>,
    pub disable: fn(bridge_id: u32) -> Result<(), &'static str>,
    pub fetch_size: fn(bridge_id: u32) -> Result<u64, &'static str>,
    pub configure_size: fn(bridge_id: u32, size: u64) -> Result<(), &'static str>,
    pub alloc_memory: fn(bridge_id: u32, size: u32, type_: AgpMemType) -> Result<u32, &'static str>,
    pub free_memory: fn(bridge_id: u32, mem_id: u32) -> Result<(), &'static str>,
    pub bind_memory: fn(bridge_id: u32, mem_id: u32, offset: u32) -> Result<(), &'static str>,
    pub unbind_memory: fn(bridge_id: u32, mem_id: u32) -> Result<(), &'static str>,
    pub remove_memory: fn(bridge_id: u32, mem_id: u32) -> Result<(), &'static str>,
    pub tlb_flush: fn(bridge_id: u32) -> Result<(), &'static str>,
}

/// AGP memory type (Linux `enum agp_mem_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgpMemType {
    Normal,
    WC, // Write-Combining
    UC, // Uncacheable
}

/// AGP memory (Linux `struct agp_memory`).
pub struct AgpMemory {
    pub id: u32,
    pub bridge_id: u32,
    pub page_count: u32,
    pub key: u32,
    pub type_: AgpMemType,
    pub bound: bool,
    pub offset: u32,
    pub pages: Vec<u64>,
}

/// AGP frontend client (Linux `struct agp_front_data`).
pub struct AgpClient {
    pub id: u32,
    pub bridge_id: u32,
    pub pid: u32,
    pub acquired: bool,
    pub mem_ids: Vec<u32>,
}

// ── Registry ────────────────────────────────────────────────────────────

static BRIDGE_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static MEM_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CLIENT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static AGP_BRIDGES: RwLock<BTreeMap<u32, AgpBridge>> = RwLock::new(BTreeMap::new());
static AGP_CLIENTS: RwLock<BTreeMap<u32, AgpClient>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an AGP bridge (Linux `agp_add_bridge`).
pub fn register_bridge(
    name: &str,
    vendor: u16,
    device: u16,
    aperture_base: u64,
    aperture_size: u64,
    max_memory: u32,
    ops: AgpOps,
) -> Result<u32, &'static str> {
    let id = BRIDGE_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let bridge = AgpBridge {
        id,
        name: String::from(name),
        vendor,
        device,
        mode: AgpMode {
            rate: AgpRate::X8,
            sba: true,
            agp3: true,
            fw: true,
        },
        aperture_base,
        aperture_size,
        aperture_page_count: (aperture_size / 4096) as u32,
        max_memory,
        current_memory: 0,
        state: AgpState::Uninitialized,
        ops,
        aperture_list: Vec::new(),
    };
    AGP_BRIDGES.write().insert(id, bridge);
    Ok(id)
}

/// Configure an AGP bridge (Linux `agp_bridge->configure`).
pub fn configure_bridge(bridge_id: u32) -> Result<(), &'static str> {
    let configure_fn = {
        let bridges = AGP_BRIDGES.read();
        let bridge = bridges.get(&bridge_id).ok_or("AGP bridge not found")?;
        bridge.ops.configure
    };
    (configure_fn)(bridge_id)?;

    let mut bridges = AGP_BRIDGES.write();
    if let Some(bridge) = bridges.get_mut(&bridge_id) {
        bridge.state = AgpState::Initialized;
    }
    Ok(())
}

/// Enable AGP (Linux `agp_bridge->enable`).
pub fn enable_agp(bridge_id: u32, mode: AgpMode) -> Result<(), &'static str> {
    let enable_fn = {
        let bridges = AGP_BRIDGES.read();
        let bridge = bridges.get(&bridge_id).ok_or("AGP bridge not found")?;
        bridge.ops.enable
    };
    (enable_fn)(bridge_id, mode)?;

    let mut bridges = AGP_BRIDGES.write();
    if let Some(bridge) = bridges.get_mut(&bridge_id) {
        bridge.mode = mode;
        bridge.state = AgpState::Enabled;
    }
    Ok(())
}

/// Disable AGP (Linux `agp_bridge->disable`).
pub fn disable_agp(bridge_id: u32) -> Result<(), &'static str> {
    let disable_fn = {
        let bridges = AGP_BRIDGES.read();
        let bridge = bridges.get(&bridge_id).ok_or("AGP bridge not found")?;
        bridge.ops.disable
    };
    (disable_fn)(bridge_id)?;

    let mut bridges = AGP_BRIDGES.write();
    if let Some(bridge) = bridges.get_mut(&bridge_id) {
        bridge.state = AgpState::Disabled;
    }
    Ok(())
}

/// Allocate AGP memory (Linux `agp_allocate_memory`).
pub fn allocate_memory(
    bridge_id: u32,
    page_count: u32,
    type_: AgpMemType,
) -> Result<u32, &'static str> {
    let (alloc_fn, max_mem, cur_mem) = {
        let bridges = AGP_BRIDGES.read();
        let bridge = bridges.get(&bridge_id).ok_or("AGP bridge not found")?;
        if bridge.state != AgpState::Enabled {
            return Err("AGP bridge not enabled");
        }
        (
            bridge.ops.alloc_memory,
            bridge.max_memory,
            bridge.current_memory,
        )
    };

    if cur_mem + page_count > max_mem {
        return Err("AGP memory limit exceeded");
    }

    let mem_id = (alloc_fn)(bridge_id, page_count, type_)?;

    let mut bridges = AGP_BRIDGES.write();
    if let Some(bridge) = bridges.get_mut(&bridge_id) {
        let mut pages = Vec::new();
        for i in 0..page_count {
            pages.push(bridge.aperture_base + (i as u64) * 4096);
        }
        bridge.aperture_list.push(AgpMemory {
            id: mem_id,
            bridge_id,
            page_count,
            key: MEM_ID_COUNTER.fetch_add(1, Ordering::SeqCst),
            type_,
            bound: false,
            offset: 0,
            pages,
        });
        bridge.current_memory += page_count;
    }
    Ok(mem_id)
}

/// Free AGP memory (Linux `agp_free_memory`).
pub fn free_memory(bridge_id: u32, mem_id: u32) -> Result<(), &'static str> {
    let free_fn = {
        let bridges = AGP_BRIDGES.read();
        let bridge = bridges.get(&bridge_id).ok_or("AGP bridge not found")?;
        bridge.ops.free_memory
    };
    (free_fn)(bridge_id, mem_id)?;

    let mut bridges = AGP_BRIDGES.write();
    if let Some(bridge) = bridges.get_mut(&bridge_id) {
        if let Some(pos) = bridge.aperture_list.iter().position(|m| m.id == mem_id) {
            let mem = bridge.aperture_list.remove(pos);
            bridge.current_memory = bridge.current_memory.saturating_sub(mem.page_count);
        }
    }
    Ok(())
}

/// Bind AGP memory to aperture (Linux `agp_bind_memory`).
pub fn bind_memory(bridge_id: u32, mem_id: u32, offset: u32) -> Result<(), &'static str> {
    let bind_fn = {
        let bridges = AGP_BRIDGES.read();
        let bridge = bridges.get(&bridge_id).ok_or("AGP bridge not found")?;
        bridge.ops.bind_memory
    };
    (bind_fn)(bridge_id, mem_id, offset)?;

    let mut bridges = AGP_BRIDGES.write();
    if let Some(bridge) = bridges.get_mut(&bridge_id) {
        if let Some(mem) = bridge.aperture_list.iter_mut().find(|m| m.id == mem_id) {
            mem.bound = true;
            mem.offset = offset;
        }
    }
    Ok(())
}

/// Unbind AGP memory (Linux `agp_unbind_memory`).
pub fn unbind_memory(bridge_id: u32, mem_id: u32) -> Result<(), &'static str> {
    let unbind_fn = {
        let bridges = AGP_BRIDGES.read();
        let bridge = bridges.get(&bridge_id).ok_or("AGP bridge not found")?;
        bridge.ops.unbind_memory
    };
    (unbind_fn)(bridge_id, mem_id)?;

    let mut bridges = AGP_BRIDGES.write();
    if let Some(bridge) = bridges.get_mut(&bridge_id) {
        if let Some(mem) = bridge.aperture_list.iter_mut().find(|m| m.id == mem_id) {
            mem.bound = false;
            mem.offset = 0;
        }
    }
    Ok(())
}

/// Flush TLB (Linux `agp_bridge->tlb_flush`).
pub fn tlb_flush(bridge_id: u32) -> Result<(), &'static str> {
    let flush_fn = {
        let bridges = AGP_BRIDGES.read();
        let bridge = bridges.get(&bridge_id).ok_or("AGP bridge not found")?;
        bridge.ops.tlb_flush
    };
    (flush_fn)(bridge_id)
}

/// Acquire AGP for a client (Linux `agp_frontend_acquire`).
pub fn acquire(bridge_id: u32, pid: u32) -> Result<u32, &'static str> {
    let client_id = CLIENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let client = AgpClient {
        id: client_id,
        bridge_id,
        pid,
        acquired: true,
        mem_ids: Vec::new(),
    };
    AGP_CLIENTS.write().insert(client_id, client);
    Ok(client_id)
}

/// Release AGP (Linux `agp_frontend_release`).
pub fn release(client_id: u32) -> Result<(), &'static str> {
    let mut clients = AGP_CLIENTS.write();
    if let Some(client) = clients.get_mut(&client_id) {
        client.acquired = false;
    }
    Ok(())
}

/// List all AGP bridges.
pub fn list_bridges() -> Vec<(u32, String, u16, u16, AgpState, u32, u32)> {
    AGP_BRIDGES
        .read()
        .iter()
        .map(|(id, b)| {
            (
                *id,
                b.name.clone(),
                b.vendor,
                b.device,
                b.state,
                b.current_memory,
                b.max_memory,
            )
        })
        .collect()
}

/// Count registered bridges.
pub fn bridge_count() -> usize {
    AGP_BRIDGES.read().len()
}

// ── Software AGP ────────────────────────────────────────────────────────

fn sw_configure(_bridge_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_enable(_bridge_id: u32, _mode: AgpMode) -> Result<(), &'static str> {
    Ok(())
}
fn sw_disable(_bridge_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_fetch_size(bridge_id: u32) -> Result<u64, &'static str> {
    let bridges = AGP_BRIDGES.read();
    let bridge = bridges.get(&bridge_id).ok_or("AGP bridge not found")?;
    Ok(bridge.aperture_size)
}
fn sw_configure_size(_bridge_id: u32, _size: u64) -> Result<(), &'static str> {
    Ok(())
}
fn sw_alloc_memory(_bridge_id: u32, _size: u32, _type_: AgpMemType) -> Result<u32, &'static str> {
    Ok(MEM_ID_COUNTER.fetch_add(1, Ordering::SeqCst))
}
fn sw_free_memory(_bridge_id: u32, _mem_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_bind_memory(_bridge_id: u32, _mem_id: u32, _offset: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_unbind_memory(_bridge_id: u32, _mem_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_remove_memory(_bridge_id: u32, _mem_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_tlb_flush(_bridge_id: u32) -> Result<(), &'static str> {
    Ok(())
}

/// Software AGP ops.
pub fn software_agp_ops() -> AgpOps {
    AgpOps {
        configure: sw_configure,
        enable: sw_enable,
        disable: sw_disable,
        fetch_size: sw_fetch_size,
        configure_size: sw_configure_size,
        alloc_memory: sw_alloc_memory,
        free_memory: sw_free_memory,
        bind_memory: sw_bind_memory,
        unbind_memory: sw_unbind_memory,
        remove_memory: sw_remove_memory,
        tlb_flush: sw_tlb_flush,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_agp_ops();

    let bridge_id = register_bridge(
        "agp0",
        0x8086,
        0x2560,
        0xE000_0000,
        256 * 1024 * 1024,
        16384,
        ops,
    )?;

    // Configure
    configure_bridge(bridge_id)?;

    // Enable AGP 8x with SBA and fast writes
    let mode = AgpMode {
        rate: AgpRate::X8,
        sba: true,
        agp3: true,
        fw: true,
    };
    enable_agp(bridge_id, mode)?;

    // Allocate some AGP memory
    let mem_id = allocate_memory(bridge_id, 256, AgpMemType::Normal)?;

    // Bind it
    bind_memory(bridge_id, mem_id, 0)?;

    // Flush TLB
    tlb_flush(bridge_id)?;

    // Acquire as a client
    let client_id = acquire(bridge_id, 1)?;

    // Unbind and free
    unbind_memory(bridge_id, mem_id)?;
    free_memory(bridge_id, mem_id)?;

    // Release
    release(client_id)?;

    // Disable
    disable_agp(bridge_id)?;

    Ok(())
}
