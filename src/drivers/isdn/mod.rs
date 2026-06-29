//! ISDN subsystem
//!
//! Provides ISDN framework for Integrated Services Digital Network.
//! Mirrors Linux's `drivers/isdn/`.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Types ───────────────────────────────────────────────────────────────

/// ISDN device (Linux `struct isdn_dev`).
pub struct IsdnDev {
    pub id: u32,
    pub name: String,
    pub channels: u8,
    pub features: IsdnFeatures,
    pub state: IsdnState,
    pub ops: IsdnOps,
    pub channel_ids: Vec<u32>,
}

/// ISDN features (Linux `struct isdn_dev.features`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IsdnFeatures {
    pub audio: bool,
    pub net: bool,
    pub data: bool,
    pub fax: bool,
    pub modem: bool,
    pub voice: bool,
}

impl Default for IsdnFeatures {
    fn default() -> Self {
        Self {
            audio: true,
            net: true,
            data: true,
            fax: false,
            modem: false,
            voice: false,
        }
    }
}

/// ISDN state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsdnState {
    Unregistered,
    Registered,
    Active,
    Suspended,
}

/// ISDN operations (Linux `struct isdn_ops`).
pub struct IsdnOps {
    pub open: fn(dev_id: u32, channel: u8) -> Result<(), &'static str>,
    pub close: fn(dev_id: u32, channel: u8) -> Result<(), &'static str>,
    pub dial: fn(dev_id: u32, channel: u8, number: &str) -> Result<(), &'static str>,
    pub hangup: fn(dev_id: u32, channel: u8) -> Result<(), &'static str>,
    pub send: fn(dev_id: u32, channel: u8, data: &[u8]) -> Result<usize, &'static str>,
    pub recv: fn(dev_id: u32, channel: u8, buf: &mut [u8]) -> Result<usize, &'static str>,
    pub command: fn(dev_id: u32, cmd: IsdnCmd) -> Result<(), &'static str>,
}

/// ISDN command (Linux `struct isdn_cmd`).
#[derive(Debug, Clone)]
pub struct IsdnCmd {
    pub cmd: IsdnCmdType,
    pub channel: u8,
    pub arg: u32,
    pub num: String,
}

/// ISDN command type (Linux `enum isdn_cmd_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsdnCmdType {
    Dial,
    Accept,
    Hangup,
    SetProto,
    SetL2,
    SetL3,
    Audio,
    Unlock,
}

/// ISDN channel (Linux `struct isdn_chan`).
pub struct IsdnChannel {
    pub id: u32,
    pub dev_id: u32,
    pub channel: u8,
    pub state: IsdnChanState,
    pub l2_proto: IsdnL2Proto,
    pub l3_proto: IsdnL3Proto,
    pub phone: String,
    pub usage: IsdnUsage,
    pub bytes_in: u64,
    pub bytes_out: u64,
}

/// ISDN channel state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsdnChanState {
    Idle,
    Dialing,
    Ringing,
    Connected,
    Hanging,
}

/// ISDN Layer 2 protocol (Linux `enum isdn_l2_proto`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsdnL2Proto {
    X75i,
    Hdlc,
    Transparent,
    Modem,
    Fax,
    X75Btx,
}

/// ISDN Layer 3 protocol (Linux `enum isdn_l3_proto`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsdnL3Proto {
    None,
    Trans,
    Iso,
    Fax,
    Modem,
}

/// ISDN usage (Linux `enum isdn_usage`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IsdnUsage {
    Unused,
    Net,
    Outgoing,
    Incoming,
    Fax,
    Modem,
    Voice,
}

// ── Registry ────────────────────────────────────────────────────────────

static DEV_ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static CHAN_ID_COUNTER: AtomicU32 = AtomicU32::new(0);

static ISDN_DEVS: RwLock<BTreeMap<u32, IsdnDev>> = RwLock::new(BTreeMap::new());
static ISDN_CHANS: RwLock<BTreeMap<u32, IsdnChannel>> = RwLock::new(BTreeMap::new());

// ── Public API ──────────────────────────────────────────────────────────

/// Register an ISDN device (Linux `register_isdn_dev`).
pub fn register_device(
    name: &str,
    channels: u8,
    features: IsdnFeatures,
    ops: IsdnOps,
) -> Result<u32, &'static str> {
    let id = DEV_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut channel_ids = Vec::new();
    for ch in 0..channels {
        let chan_id = CHAN_ID_COUNTER.fetch_add(1, Ordering::SeqCst);
        let chan = IsdnChannel {
            id: chan_id,
            dev_id: id,
            channel: ch,
            state: IsdnChanState::Idle,
            l2_proto: IsdnL2Proto::Hdlc,
            l3_proto: IsdnL3Proto::Trans,
            phone: String::new(),
            usage: IsdnUsage::Unused,
            bytes_in: 0,
            bytes_out: 0,
        };
        ISDN_CHANS.write().insert(chan_id, chan);
        channel_ids.push(chan_id);
    }

    let dev = IsdnDev {
        id,
        name: String::from(name),
        channels,
        features,
        state: IsdnState::Registered,
        ops,
        channel_ids,
    };
    ISDN_DEVS.write().insert(id, dev);
    Ok(id)
}

/// Open a channel (Linux `isdn_open`).
pub fn open_channel(dev_id: u32, channel: u8) -> Result<(), &'static str> {
    let open_fn = {
        let devs = ISDN_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("ISDN device not found")?;
        dev.ops.open
    };
    (open_fn)(dev_id, channel)?;

    let mut devs = ISDN_DEVS.write();
    if let Some(dev) = devs.get_mut(&dev_id) {
        dev.state = IsdnState::Active;
    }
    Ok(())
}

/// Close a channel (Linux `isdn_close`).
pub fn close_channel(dev_id: u32, channel: u8) -> Result<(), &'static str> {
    let close_fn = {
        let devs = ISDN_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("ISDN device not found")?;
        dev.ops.close
    };
    (close_fn)(dev_id, channel)
}

/// Dial a number (Linux `isdn_dial`).
pub fn dial(dev_id: u32, channel: u8, number: &str) -> Result<(), &'static str> {
    let dial_fn = {
        let devs = ISDN_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("ISDN device not found")?;
        dev.ops.dial
    };
    (dial_fn)(dev_id, channel, number)?;

    // Update channel state
    let mut chans = ISDN_CHANS.write();
    for (_, chan) in chans.iter_mut() {
        if chan.dev_id == dev_id && chan.channel == channel {
            chan.state = IsdnChanState::Dialing;
            chan.phone = String::from(number);
            chan.usage = IsdnUsage::Outgoing;
            break;
        }
    }
    Ok(())
}

/// Hangup a channel (Linux `isdn_hangup`).
pub fn hangup(dev_id: u32, channel: u8) -> Result<(), &'static str> {
    let hangup_fn = {
        let devs = ISDN_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("ISDN device not found")?;
        dev.ops.hangup
    };
    (hangup_fn)(dev_id, channel)?;

    let mut chans = ISDN_CHANS.write();
    for (_, chan) in chans.iter_mut() {
        if chan.dev_id == dev_id && chan.channel == channel {
            chan.state = IsdnChanState::Idle;
            chan.usage = IsdnUsage::Unused;
            chan.phone.clear();
            break;
        }
    }
    Ok(())
}

/// Send data on a channel (Linux `isdn_write`).
pub fn send_data(dev_id: u32, channel: u8, data: &[u8]) -> Result<usize, &'static str> {
    let send_fn = {
        let devs = ISDN_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("ISDN device not found")?;
        dev.ops.send
    };
    let n = (send_fn)(dev_id, channel, data)?;

    let mut chans = ISDN_CHANS.write();
    for (_, chan) in chans.iter_mut() {
        if chan.dev_id == dev_id && chan.channel == channel {
            chan.bytes_out += n as u64;
            break;
        }
    }
    Ok(n)
}

/// Receive data on a channel (Linux `isdn_read`).
pub fn recv_data(dev_id: u32, channel: u8, buf: &mut [u8]) -> Result<usize, &'static str> {
    let recv_fn = {
        let devs = ISDN_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("ISDN device not found")?;
        dev.ops.recv
    };
    let n = (recv_fn)(dev_id, channel, buf)?;

    let mut chans = ISDN_CHANS.write();
    for (_, chan) in chans.iter_mut() {
        if chan.dev_id == dev_id && chan.channel == channel {
            chan.bytes_in += n as u64;
            break;
        }
    }
    Ok(n)
}

/// Send a command (Linux `isdn_command`).
pub fn send_command(dev_id: u32, cmd: IsdnCmd) -> Result<(), &'static str> {
    let cmd_fn = {
        let devs = ISDN_DEVS.read();
        let dev = devs.get(&dev_id).ok_or("ISDN device not found")?;
        dev.ops.command
    };
    (cmd_fn)(dev_id, cmd)
}

/// List all ISDN devices.
pub fn list_devices() -> Vec<(u32, String, u8, IsdnState, IsdnFeatures)> {
    ISDN_DEVS
        .read()
        .iter()
        .map(|(id, d)| (*id, d.name.clone(), d.channels, d.state, d.features))
        .collect()
}

/// Count registered devices.
pub fn device_count() -> usize {
    ISDN_DEVS.read().len()
}

// ── Software ISDN ───────────────────────────────────────────────────────

fn sw_open(_dev_id: u32, _channel: u8) -> Result<(), &'static str> {
    Ok(())
}
fn sw_close(_dev_id: u32, _channel: u8) -> Result<(), &'static str> {
    Ok(())
}
fn sw_dial(_dev_id: u32, _channel: u8, _number: &str) -> Result<(), &'static str> {
    Ok(())
}
fn sw_hangup(_dev_id: u32, _channel: u8) -> Result<(), &'static str> {
    Ok(())
}
fn sw_send(_dev_id: u32, _channel: u8, data: &[u8]) -> Result<usize, &'static str> {
    Ok(data.len())
}
fn sw_recv(_dev_id: u32, _channel: u8, buf: &mut [u8]) -> Result<usize, &'static str> {
    for b in buf.iter_mut() {
        *b = 0;
    }
    Ok(buf.len())
}
fn sw_command(_dev_id: u32, _cmd: IsdnCmd) -> Result<(), &'static str> {
    Ok(())
}

/// Software ISDN ops.
pub fn software_isdn_ops() -> IsdnOps {
    IsdnOps {
        open: sw_open,
        close: sw_close,
        dial: sw_dial,
        hangup: sw_hangup,
        send: sw_send,
        recv: sw_recv,
        command: sw_command,
    }
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!("isdn: subsystem ready");
    Ok(())
}
