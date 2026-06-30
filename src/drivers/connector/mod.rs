//! Linux connector-style message bus.
//!
//! This is a Rust-owned mirror of Linux `drivers/connector/`: kernel
//! components register typed connector IDs and receive in-kernel messages
//! without requiring a C netlink shim.

use alloc::{collections::BTreeMap, string::String, vec::Vec};
use spin::RwLock;

/// Connector address, equivalent to Linux `cb_id`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConnectorId {
    pub idx: u32,
    pub val: u32,
}

impl ConnectorId {
    pub const fn new(idx: u32, val: u32) -> Self {
        Self { idx, val }
    }

    const fn key(self) -> u64 {
        ((self.idx as u64) << 32) | self.val as u64
    }
}

/// Connector message delivered to a registered endpoint.
#[derive(Debug, Clone)]
pub struct ConnectorMessage {
    pub id: ConnectorId,
    pub seq: u32,
    pub ack: u32,
    pub payload: Vec<u8>,
}

impl ConnectorMessage {
    pub fn new(id: ConnectorId, seq: u32, ack: u32, payload: &[u8]) -> Self {
        Self {
            id,
            seq,
            ack,
            payload: payload.to_vec(),
        }
    }
}

pub type ConnectorCallback = fn(&ConnectorMessage) -> Result<(), &'static str>;

/// Registered connector endpoint.
#[derive(Clone)]
pub struct ConnectorDevice {
    pub id: ConnectorId,
    pub name: String,
    callback: Option<ConnectorCallback>,
    pub delivered: u64,
    pub dropped: u64,
}

static CONNECTORS: RwLock<BTreeMap<u64, ConnectorDevice>> = RwLock::new(BTreeMap::new());

/// Register a connector endpoint.
pub fn register_connector(
    id: ConnectorId,
    name: &str,
    callback: Option<ConnectorCallback>,
) -> Result<(), &'static str> {
    let mut connectors = CONNECTORS.write();
    let key = id.key();
    if connectors.contains_key(&key) {
        return Err("connector id already registered");
    }

    connectors.insert(
        key,
        ConnectorDevice {
            id,
            name: String::from(name),
            callback,
            delivered: 0,
            dropped: 0,
        },
    );
    Ok(())
}

/// Remove a connector endpoint.
pub fn unregister_connector(id: ConnectorId) -> Result<(), &'static str> {
    CONNECTORS
        .write()
        .remove(&id.key())
        .map(|_| ())
        .ok_or("connector id not registered")
}

/// Deliver a connector message to its registered endpoint.
pub fn send_message(message: ConnectorMessage) -> Result<(), &'static str> {
    let callback = {
        let connectors = CONNECTORS.read();
        connectors
            .get(&message.id.key())
            .ok_or("connector id not registered")?
            .callback
    };

    let result = if let Some(callback) = callback {
        callback(&message)
    } else {
        Ok(())
    };

    let mut connectors = CONNECTORS.write();
    let connector = connectors
        .get_mut(&message.id.key())
        .ok_or("connector id not registered")?;
    if result.is_ok() {
        connector.delivered = connector.delivered.saturating_add(1);
    } else {
        connector.dropped = connector.dropped.saturating_add(1);
    }

    result
}

/// Snapshot registered connector endpoints.
pub fn list_connectors() -> Vec<ConnectorDevice> {
    CONNECTORS.read().values().cloned().collect()
}

pub fn connector_count() -> usize {
    CONNECTORS.read().len()
}

pub fn init() -> Result<(), &'static str> {
    if !CONNECTORS.read().is_empty() {
        return Ok(());
    }

    register_connector(ConnectorId::new(0, 1), "kernel-events", None)?;
    crate::serial_println!("connector: kernel-events endpoint registered");
    Ok(())
}
