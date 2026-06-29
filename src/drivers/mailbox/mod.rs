//! Mailbox subsystem
//!
//! Provides inter-processor communication (IPC) via hardware mailbox controllers.
//! Mirrors Linux's `drivers/mailbox/mailbox.c`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// Mailbox direction (Linux `enum mbox_direction`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MboxDirection {
    Tx,
    Rx,
    TxRx,
}

/// Mailbox client (Linux `struct mbox_client`).
pub struct MboxClient {
    pub name: String,
    pub tx_block: bool,
    pub tx_tout: u32,
    pub knows_txdone: bool,
    pub rx_callback: Option<fn(channel_id: u32, data: &[u8])>,
    pub tx_done: Option<fn(channel_id: u32, data: &[u8], result: MboxTxResult)>,
    pub tx_prepare: Option<fn(channel_id: u32, data: &mut [u8])>,
}

/// TX completion result (Linux `enum mbox_tx_result`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MboxTxResult {
    Ok,
    Timeout,
    Busy,
    Error,
}

/// Mailbox channel operations (Linux `struct mbox_chan_ops`).
pub struct MboxChanOps {
    pub startup: fn(channel_id: u32) -> Result<(), &'static str>,
    pub shutdown: fn(channel_id: u32) -> Result<(), &'static str>,
    pub send_data: fn(channel_id: u32, data: &[u8]) -> Result<(), &'static str>,
    pub last_tx_done: fn(channel_id: u32) -> bool,
    pub peek_data: fn(channel_id: u32) -> Result<Option<Vec<u8>>, &'static str>,
}

/// Mailbox controller (Linux `struct mbox_controller`).
pub struct MboxController {
    pub name: String,
    pub ops: MboxChanOps,
    pub num_channels: u32,
    pub txdone_irq: bool,
    pub txdone_poll: bool,
    pub txpoll_period: u32,
}

/// Mailbox channel instance.
pub struct MboxChannel {
    pub controller_id: u32,
    pub index: u32,
    pub direction: MboxDirection,
    pub client: Option<u32>,
    pub active: bool,
    pub tx_pending: bool,
}

// ── Registry ────────────────────────────────────────────────────────────

static CONTROLLER_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CHANNEL_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CLIENT_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static MBOX_CONTROLLERS: RwLock<BTreeMap<u32, MboxController>> = RwLock::new(BTreeMap::new());
static MBOX_CHANNELS: RwLock<BTreeMap<u32, MboxChannel>> = RwLock::new(BTreeMap::new());
static MBOX_CLIENTS: RwLock<BTreeMap<u32, MboxClient>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register a mailbox controller.
pub fn register_controller(
    name: &str,
    ops: MboxChanOps,
    num_channels: u32,
    txdone_irq: bool,
    txdone_poll: bool,
    txpoll_period: u32,
) -> Result<u32, &'static str> {
    let id = CONTROLLER_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let ctrl = MboxController {
        name: String::from(name),
        ops,
        num_channels,
        txdone_irq,
        txdone_poll,
        txpoll_period,
    };
    MBOX_CONTROLLERS.write().insert(id, ctrl);

    // Pre-create channels
    for i in 0..num_channels {
        let ch_id = CHANNEL_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let ch = MboxChannel {
            controller_id: id,
            index: i,
            direction: MboxDirection::TxRx,
            client: None,
            active: false,
            tx_pending: false,
        };
        MBOX_CHANNELS.write().insert(ch_id, ch);
    }

    Ok(id)
}

/// Register a mailbox client.
pub fn register_client(client: MboxClient) -> Result<u32, &'static str> {
    let id = CLIENT_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    MBOX_CLIENTS.write().insert(id, client);
    Ok(id)
}

/// Request a mailbox channel for a client.
pub fn request_channel(
    controller_id: u32,
    channel_index: u32,
    client_id: u32,
) -> Result<u32, &'static str> {
    let ch_id = {
        let channels = MBOX_CHANNELS.read();
        let mut found: Option<u32> = None;
        for (id, ch) in channels.iter() {
            if ch.controller_id == controller_id && ch.index == channel_index && ch.client.is_none()
            {
                found = Some(*id);
                break;
            }
        }
        found.ok_or("Mailbox channel not available")?
    };

    let startup_fn = {
        let ctrls = MBOX_CONTROLLERS.read();
        let ctrl = ctrls
            .get(&controller_id)
            .ok_or("Mailbox controller not found")?;
        ctrl.ops.startup
    };

    {
        let mut channels = MBOX_CHANNELS.write();
        let ch = channels
            .get_mut(&ch_id)
            .ok_or("Mailbox channel not found")?;
        ch.client = Some(client_id);
        ch.active = true;
    }

    (startup_fn)(ch_id)?;
    Ok(ch_id)
}

/// Free a mailbox channel.
pub fn free_channel(channel_id: u32) -> Result<(), &'static str> {
    let (shutdown_fn, _controller_id) = {
        let channels = MBOX_CHANNELS.read();
        let ch = channels
            .get(&channel_id)
            .ok_or("Mailbox channel not found")?;
        if !ch.active {
            return Err("Mailbox channel not active");
        }
        let ctrls = MBOX_CONTROLLERS.read();
        let ctrl = ctrls
            .get(&ch.controller_id)
            .ok_or("Mailbox controller not found")?;
        (ctrl.ops.shutdown, ch.controller_id)
    };

    (shutdown_fn)(channel_id)?;

    let mut channels = MBOX_CHANNELS.write();
    let ch = channels
        .get_mut(&channel_id)
        .ok_or("Mailbox channel not found")?;
    ch.client = None;
    ch.active = false;
    ch.tx_pending = false;
    Ok(())
}

/// Send data through a mailbox channel.
pub fn send_data(channel_id: u32, data: &[u8]) -> Result<(), &'static str> {
    let (send_fn, _controller_id) = {
        let channels = MBOX_CHANNELS.read();
        let ch = channels
            .get(&channel_id)
            .ok_or("Mailbox channel not found")?;
        if !ch.active {
            return Err("Mailbox channel not active");
        }
        let ctrls = MBOX_CONTROLLERS.read();
        let ctrl = ctrls
            .get(&ch.controller_id)
            .ok_or("Mailbox controller not found")?;
        (ctrl.ops.send_data, ch.controller_id)
    };

    (send_fn)(channel_id, data)?;

    let mut channels = MBOX_CHANNELS.write();
    if let Some(ch) = channels.get_mut(&channel_id) {
        ch.tx_pending = true;
    }
    Ok(())
}

/// Check if the last TX on a channel is done.
pub fn last_tx_done(channel_id: u32) -> Result<bool, &'static str> {
    let (txdone_fn, _) = {
        let channels = MBOX_CHANNELS.read();
        let ch = channels
            .get(&channel_id)
            .ok_or("Mailbox channel not found")?;
        let ctrls = MBOX_CONTROLLERS.read();
        let ctrl = ctrls
            .get(&ch.controller_id)
            .ok_or("Mailbox controller not found")?;
        (ctrl.ops.last_tx_done, ch.controller_id)
    };
    Ok((txdone_fn)(channel_id))
}

/// Peek at received data on a channel.
pub fn peek_data(channel_id: u32) -> Result<Option<Vec<u8>>, &'static str> {
    let peek_fn = {
        let channels = MBOX_CHANNELS.read();
        let ch = channels
            .get(&channel_id)
            .ok_or("Mailbox channel not found")?;
        let ctrls = MBOX_CONTROLLERS.read();
        let ctrl = ctrls
            .get(&ch.controller_id)
            .ok_or("Mailbox controller not found")?;
        ctrl.ops.peek_data
    };
    (peek_fn)(channel_id)
}

/// Notify TX completion on a channel (called by controller driver).
pub fn tx_done(channel_id: u32, data: &[u8], result: MboxTxResult) {
    let client_id = {
        let mut channels = MBOX_CHANNELS.write();
        if let Some(ch) = channels.get_mut(&channel_id) {
            ch.tx_pending = false;
            ch.client
        } else {
            None
        }
    };

    if let Some(cid) = client_id {
        let tx_done_fn = {
            let clients = MBOX_CLIENTS.read();
            let client = clients.get(&cid);
            client.and_then(|c| c.tx_done)
        };
        if let Some(f) = tx_done_fn {
            f(channel_id, data, result);
        }
    }
}

/// List all registered controllers.
pub fn list_controllers() -> Vec<(u32, String, u32)> {
    MBOX_CONTROLLERS
        .read()
        .iter()
        .map(|(id, c)| (*id, c.name.clone(), c.num_channels))
        .collect()
}

/// Count active channels.
pub fn active_channel_count() -> usize {
    MBOX_CHANNELS.read().values().filter(|ch| ch.active).count()
}

// ── Software mailbox (loopback) ─────────────────────────────────────────

fn sw_startup(_ch_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_shutdown(_ch_id: u32) -> Result<(), &'static str> {
    Ok(())
}
fn sw_send_data(_ch_id: u32, _data: &[u8]) -> Result<(), &'static str> {
    Ok(())
}
fn sw_last_tx_done(_ch_id: u32) -> bool {
    true
}
fn sw_peek_data(_ch_id: u32) -> Result<Option<Vec<u8>>, &'static str> {
    Ok(None)
}

/// Software mailbox ops (loopback — always succeeds).
pub fn software_mbox_ops() -> MboxChanOps {
    MboxChanOps {
        startup: sw_startup,
        shutdown: sw_shutdown,
        send_data: sw_send_data,
        last_tx_done: sw_last_tx_done,
        peek_data: sw_peek_data,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("mailbox: subsystem ready");
    Ok(())
}
