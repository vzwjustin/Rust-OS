//! PCMCIA subsystem
//!
//! Provides PCMCIA/CardBus socket framework for PC Card devices.
//! Mirrors Linux's `drivers/pcmcia/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// PCMCIA socket (Linux `struct pcmcia_socket`).
pub struct PcmciaSocket {
    pub id: u32,
    pub name: String,
    pub socket_type: SocketType,
    pub state: SocketState,
    pub ops: SocketOps,
    pub device_id: Option<u32>,
    pub features: SocketFeatures,
    pub irq_count: u32,
    pub card_irq: u32,
    pub card_config: CardConfig,
}

/// Socket type (Linux `enum pcmcia_socket_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketType {
    PcCard,  // 16-bit PC Card
    CardBus, // 32-bit CardBus
    ExpressCard,
}

/// Socket state (Linux `struct pcmcia_socket.state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketState {
    pub present: bool,
    pub ready: bool,
    pub busy: bool,
    pub suspended: bool,
    pub voltage_3v: bool,
    pub voltage_5v: bool,
    pub voltage_xv: bool,
    pub battery_dead: bool,
    pub battery_low: bool,
    pub write_protect: bool,
    pub configured: bool,
}

impl Default for SocketState {
    fn default() -> Self {
        Self {
            present: false,
            ready: false,
            busy: false,
            suspended: false,
            voltage_3v: true,
            voltage_5v: true,
            voltage_xv: false,
            battery_dead: false,
            battery_low: false,
            write_protect: false,
            configured: false,
        }
    }
}

/// Socket features (Linux `struct pcmcia_socket.features`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketFeatures {
    pub card_3v: bool,
    pub card_5v: bool,
    pub xv_card: bool,
    pub pci_bus: bool,
    pub pci_irq: bool,
    pub mem_win: u8,
    pub io_win: u8,
}

impl Default for SocketFeatures {
    fn default() -> Self {
        Self {
            card_3v: true,
            card_5v: true,
            xv_card: false,
            pci_bus: true,
            pci_irq: true,
            mem_win: 4,
            io_win: 2,
        }
    }
}

/// Socket operations (Linux `struct pccard_operations`).
pub struct SocketOps {
    pub init: fn(socket_id: u32) -> Result<(), &'static str>,
    pub suspend: fn(socket_id: u32) -> Result<(), &'static str>,
    pub resume: fn(socket_id: u32) -> Result<(), &'static str>,
    pub get_status: fn(socket_id: u32) -> Result<SocketState, &'static str>,
    pub set_socket: fn(socket_id: u32, state: SocketState) -> Result<(), &'static str>,
    pub set_io_map: fn(socket_id: u32, map: IoMap) -> Result<(), &'static str>,
    pub set_mem_map: fn(socket_id: u32, map: MemMap) -> Result<(), &'static str>,
    pub register_callback: fn(
        socket_id: u32,
        callback: fn(socket_id: u32, event: SocketEvent),
    ) -> Result<(), &'static str>,
}

/// I/O map (Linux `struct resource`).
#[derive(Debug, Clone)]
pub struct IoMap {
    pub start: u64,
    pub stop: u64,
    pub flags: u16,
    pub speed: u32,
}

/// Memory map (Linux `struct resource`).
#[derive(Debug, Clone)]
pub struct MemMap {
    pub card_start: u64,
    pub sys_start: u64,
    pub sys_stop: u64,
    pub flags: u16,
    pub speed: u32,
}

/// Socket event (Linux `enum cs_event`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketEvent {
    CardInsertion,
    CardRemoval,
    CardReset,
    BatteryLow,
    BatteryDead,
    ReadyTimeout,
    PMSuspend,
    PMResume,
    ResetPhysical,
    ResetComplete,
}

/// PCMCIA device (Linux `struct pcmcia_device`).
pub struct PcmciaDevice {
    pub id: u32,
    pub socket_id: u32,
    pub name: String,
    pub dev_type: PcmciaDevType,
    pub config_idx: u8,
    pub config_base: u32,
    pub irq: u32,
    pub io_start: u64,
    pub io_len: u64,
    pub state: PcmciaDevState,
    pub manf_id: u16,
    pub card_id: u16,
    pub func_id: u8,
}

/// PCMCIA device type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmciaDevType {
    Network,
    Storage,
    Serial,
    Parallel,
    Modem,
    Multifunction,
    Memory,
    Other,
}

/// PCMCIA device state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PcmciaDevState {
    Unbound,
    Bound,
    Configured,
    Suspended,
    Removed,
}

/// Card configuration (Linux `struct pcmcia_socket.card_config`).
#[derive(Debug, Clone, Default)]
pub struct CardConfig {
    pub manf_id: u16,
    pub card_id: u16,
    pub func_id: u8,
    pub prod_v1: String,
    pub prod_v2: String,
}

// ── Registry ────────────────────────────────────────────────────────────

static SOCKET_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static PCMCIA_SOCKETS: RwLock<BTreeMap<u32, PcmciaSocket>> = RwLock::new(BTreeMap::new());
static PCMCIA_DEVS: RwLock<BTreeMap<u32, PcmciaDevice>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a PCMCIA socket (Linux `pcmcia_register_socket`).
pub fn register_socket(
    name: &str,
    socket_type: SocketType,
    ops: SocketOps,
) -> Result<u32, &'static str> {
    let id = SOCKET_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let socket = PcmciaSocket {
        id,
        name: String::from(name),
        socket_type,
        state: SocketState::default(),
        ops,
        device_id: None,
        features: SocketFeatures::default(),
        irq_count: 0,
        card_irq: 0,
        card_config: CardConfig::default(),
    };
    PCMCIA_SOCKETS.write().insert(id, socket);
    Ok(id)
}

/// Initialize a socket (Linux `socket->ops->init`).
pub fn init_socket(socket_id: u32) -> Result<(), &'static str> {
    let init_fn = {
        let sockets = PCMCIA_SOCKETS.read();
        let socket = sockets.get(&socket_id).ok_or("PCMCIA socket not found")?;
        socket.ops.init
    };
    (init_fn)(socket_id)
}

/// Get socket status (Linux `socket->ops->get_status`).
pub fn get_status(socket_id: u32) -> Result<SocketState, &'static str> {
    let status_fn = {
        let sockets = PCMCIA_SOCKETS.read();
        let socket = sockets.get(&socket_id).ok_or("PCMCIA socket not found")?;
        socket.ops.get_status
    };
    let state = (status_fn)(socket_id)?;

    let mut sockets = PCMCIA_SOCKETS.write();
    if let Some(socket) = sockets.get_mut(&socket_id) {
        socket.state = state;
    }
    Ok(state)
}

/// Insert a card (Linux `pcmcia_insert_card`).
pub fn insert_card(
    socket_id: u32,
    manf_id: u16,
    card_id: u16,
    func_id: u8,
    name: &str,
    dev_type: PcmciaDevType,
) -> Result<u32, &'static str> {
    // Update socket state
    {
        let mut sockets = PCMCIA_SOCKETS.write();
        let socket = sockets
            .get_mut(&socket_id)
            .ok_or("PCMCIA socket not found")?;
        socket.state.present = true;
        socket.state.ready = true;
        socket.card_config = CardConfig {
            manf_id,
            card_id,
            func_id,
            prod_v1: String::from(name),
            prod_v2: String::new(),
        };
    }

    // Create device
    let dev_id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dev = PcmciaDevice {
        id: dev_id,
        socket_id,
        name: String::from(name),
        dev_type,
        config_idx: 1,
        config_base: 0,
        irq: 0,
        io_start: 0,
        io_len: 0,
        state: PcmciaDevState::Bound,
        manf_id,
        card_id,
        func_id,
    };
    PCMCIA_DEVS.write().insert(dev_id, dev);

    let mut sockets = PCMCIA_SOCKETS.write();
    if let Some(socket) = sockets.get_mut(&socket_id) {
        socket.device_id = Some(dev_id);
    }

    Ok(dev_id)
}

/// Remove a card (Linux `pcmcia_remove_card`).
pub fn remove_card(socket_id: u32) -> Result<(), &'static str> {
    let dev_id = {
        let mut sockets = PCMCIA_SOCKETS.write();
        let socket = sockets
            .get_mut(&socket_id)
            .ok_or("PCMCIA socket not found")?;
        socket.state.present = false;
        socket.state.ready = false;
        socket.state.configured = false;
        socket.device_id.take()
    };

    if let Some(dev_id) = dev_id {
        let mut devs = PCMCIA_DEVS.write();
        if let Some(dev) = devs.get_mut(&dev_id) {
            dev.state = PcmciaDevState::Removed;
        }
    }
    Ok(())
}

/// Configure a device (Linux `pcmcia_request_configuration`).
pub fn configure_device(
    dev_id: u32,
    config_idx: u8,
    irq: u32,
    io_start: u64,
    io_len: u64,
) -> Result<(), &'static str> {
    let socket_id = {
        let mut devs = PCMCIA_DEVS.write();
        let dev = devs.get_mut(&dev_id).ok_or("PCMCIA device not found")?;
        dev.config_idx = config_idx;
        dev.irq = irq;
        dev.io_start = io_start;
        dev.io_len = io_len;
        dev.state = PcmciaDevState::Configured;
        dev.socket_id
    };

    let mut sockets = PCMCIA_SOCKETS.write();
    if let Some(socket) = sockets.get_mut(&socket_id) {
        socket.state.configured = true;
        socket.card_irq = irq;
    }
    Ok(())
}

/// Set I/O map (Linux `socket->ops->set_io_map`).
pub fn set_io_map(socket_id: u32, map: IoMap) -> Result<(), &'static str> {
    let set_fn = {
        let sockets = PCMCIA_SOCKETS.read();
        let socket = sockets.get(&socket_id).ok_or("PCMCIA socket not found")?;
        socket.ops.set_io_map
    };
    (set_fn)(socket_id, map)
}

/// Set memory map (Linux `socket->ops->set_mem_map`).
pub fn set_mem_map(socket_id: u32, map: MemMap) -> Result<(), &'static str> {
    let set_fn = {
        let sockets = PCMCIA_SOCKETS.read();
        let socket = sockets.get(&socket_id).ok_or("PCMCIA socket not found")?;
        socket.ops.set_mem_map
    };
    (set_fn)(socket_id, map)
}

/// Suspend a socket (Linux `socket->ops->suspend`).
pub fn suspend_socket(socket_id: u32) -> Result<(), &'static str> {
    let suspend_fn = {
        let sockets = PCMCIA_SOCKETS.read();
        let socket = sockets.get(&socket_id).ok_or("PCMCIA socket not found")?;
        socket.ops.suspend
    };
    (suspend_fn)(socket_id)?;

    let mut sockets = PCMCIA_SOCKETS.write();
    if let Some(socket) = sockets.get_mut(&socket_id) {
        socket.state.suspended = true;
    }

    if let Some(dev_id) = sockets.get(&socket_id).and_then(|s| s.device_id) {
        let mut devs = PCMCIA_DEVS.write();
        if let Some(dev) = devs.get_mut(&dev_id) {
            dev.state = PcmciaDevState::Suspended;
        }
    }
    Ok(())
}

/// Resume a socket (Linux `socket->ops->resume`).
pub fn resume_socket(socket_id: u32) -> Result<(), &'static str> {
    let resume_fn = {
        let sockets = PCMCIA_SOCKETS.read();
        let socket = sockets.get(&socket_id).ok_or("PCMCIA socket not found")?;
        socket.ops.resume
    };
    (resume_fn)(socket_id)?;

    let mut sockets = PCMCIA_SOCKETS.write();
    if let Some(socket) = sockets.get_mut(&socket_id) {
        socket.state.suspended = false;
    }

    if let Some(dev_id) = sockets.get(&socket_id).and_then(|s| s.device_id) {
        let mut devs = PCMCIA_DEVS.write();
        if let Some(dev) = devs.get_mut(&dev_id) {
            dev.state = PcmciaDevState::Configured;
        }
    }
    Ok(())
}

/// List all sockets.
pub fn list_sockets() -> Vec<(u32, String, SocketType, bool, bool)> {
    PCMCIA_SOCKETS
        .read()
        .iter()
        .map(|(id, s)| {
            (
                *id,
                s.name.clone(),
                s.socket_type,
                s.state.present,
                s.state.configured,
            )
        })
        .collect()
}

/// Count registered sockets.
pub fn socket_count() -> usize {
    PCMCIA_SOCKETS.read().len()
}

// ── Software PCMCIA ─────────────────────────────────────────────────────

fn sw_init(_socket_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_suspend(_socket_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_resume(_socket_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_get_status(socket_id: u32) -> Result<SocketState, &'static str> {
    let sockets = PCMCIA_SOCKETS.read();
    let socket = sockets.get(&socket_id).ok_or("PCMCIA socket not found")?;
    Ok(socket.state)
}
fn sw_set_socket(_socket_id: u32, _state: SocketState) -> Result<(), &'static str> {
    Ok(())
}
fn sw_set_io_map(_socket_id: u32, _map: IoMap) -> Result<(), &'static str> {
    Ok(())
}
fn sw_set_mem_map(_socket_id: u32, _map: MemMap) -> Result<(), &'static str> {
    Ok(())
}
fn sw_register_callback(
    _socket_id: u32,
    _callback: fn(socket_id: u32, event: SocketEvent),
) -> Result<(), &'static str> {
    Ok(())
}

/// Software PCMCIA socket ops.
pub fn software_socket_ops() -> SocketOps {
    SocketOps {
        init: sw_init,
        suspend: sw_suspend,
        resume: sw_resume,
        get_status: sw_get_status,
        set_socket: sw_set_socket,
        set_io_map: sw_set_io_map,
        set_mem_map: sw_set_mem_map,
        register_callback: sw_register_callback,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    let ops = software_socket_ops();
    let socket_id = register_socket("sw-pcmcia0", SocketType::PcCard, ops)?;

    // Initialize socket
    init_socket(socket_id)?;

    // Insert a network card
    let dev_id = insert_card(
        socket_id,
        0x0101,
        0x0565,
        0x06,
        "sw-eth-pccard",
        PcmciaDevType::Network,
    )?;

    // Configure the device
    configure_device(dev_id, 1, 10, 0x300, 0x20)?;

    // Check status
    let status = get_status(socket_id)?;
    if !status.present || !status.configured {
        return Err("PCMCIA: card not configured");
    }

    // Set I/O map
    let io_map = IoMap {
        start: 0x300,
        stop: 0x31F,
        flags: 0x0001,
        speed: 0,
    };
    set_io_map(socket_id, io_map)?;

    // Set memory map
    let mem_map = MemMap {
        card_start: 0,
        sys_start: 0xD0000,
        sys_stop: 0xD3FFF,
        flags: 0x0001,
        speed: 0,
    };
    set_mem_map(socket_id, mem_map)?;

    // Suspend and resume
    suspend_socket(socket_id)?;
    resume_socket(socket_id)?;

    // Remove card
    remove_card(socket_id)?;

    Ok(())
}
