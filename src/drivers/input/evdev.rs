//! Evdev client buffer ring and ioctl handling, ported from Linux
//! `drivers/input/evdev.c`. Provides per-client event buffering with
//! power-of-two ring semantics, SYN_DROPPED overflow handling, and
//! the full evdev ioctl dispatch (EVIOCGBIT, EVIOCGABS, EVIOCSABS,
//! EVIOCGNAME, EVIOCGID, EVIOCGRAB, EVIOCGVERSION, etc.).
//!
//! Ported statically from `/home/justin/Downloads/linux-master/drivers/input/evdev.c`
//! and `/home/justin/Downloads/linux-master/include/uapi/linux/input.h`.

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use spin::Mutex;

use super::{
    InputEvent, ABS_MAX, ABS_MT_SLOT, EV_ABS, EV_FF, EV_KEY, EV_LED, EV_MSC, EV_REL, EV_REP,
    EV_SND, EV_SW, EV_SYN, FF_MAX, KEY_MAX, LED_MAX, MSC_MAX, REL_MAX, SND_MAX, SW_MAX,
    SYN_DROPPED, SYN_REPORT,
};

/// Linux `EV_VERSION` — evdev protocol version.
pub const EV_VERSION: u32 = 0x010001;

/// Last event type Linux exposes through EVIOCGBIT(0, ...).
const EV_MAX: u16 = 0x1f;

/// Minimum buffer size (Linux `EVDEV_MIN_BUFFER_SIZE`).
const EVDEV_MIN_BUFFER_SIZE: u32 = 64;

/// Buffer packets multiplier (Linux `EVDEV_BUF_PACKETS`).
const EVDEV_BUF_PACKETS: u32 = 8;

/// Maximum number of evdev clients (Linux `EVDEV_MINORS`).
pub const EVDEV_MAX_CLIENTS: usize = 32;

/// Clock type for timestamps (Linux `enum input_clock_type`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputClockType {
    Realtime,
    Monotonic,
    Boottime,
}

/// A single evdev client, mirroring Linux `struct evdev_client`.
/// Has a power-of-two ring buffer of `InputEvent`s.
pub struct EvdevClient {
    /// Ring buffer (always power-of-two size).
    pub buffer: Vec<InputEvent>,
    /// Buffer capacity (power-of-two).
    pub bufsize: u32,
    /// Write index.
    pub head: u32,
    /// Read index.
    pub tail: u32,
    /// Packet boundary (first element of next packet).
    pub packet_head: u32,
    /// Clock type for timestamps.
    pub clk_type: InputClockType,
    /// Whether the client has been revoked.
    pub revoked: bool,
    /// Whether the client has grabbed the device.
    pub grabbed: bool,
    /// Device index this client is attached to.
    pub device_idx: usize,
}

impl EvdevClient {
    /// Create a new client with the given buffer size (rounded up to power-of-two).
    /// Mirrors Linux `evdev_open` → `evdev_compute_buffer_size`.
    pub fn new(device_idx: usize, hint_events_per_packet: u32) -> Self {
        let n_events = (hint_events_per_packet * EVDEV_BUF_PACKETS).max(EVDEV_MIN_BUFFER_SIZE);
        let bufsize = next_pow2(n_events);
        let buffer = alloc::vec![InputEvent {
            event_type: 0,
            code: 0,
            value: 0,
            timestamp_ms: 0,
        }; bufsize as usize];

        Self {
            buffer,
            bufsize,
            head: 0,
            tail: 0,
            packet_head: 0,
            clk_type: InputClockType::Monotonic,
            revoked: false,
            grabbed: false,
            device_idx,
        }
    }

    /// Mask for ring buffer indexing.
    fn mask(&self) -> u32 {
        self.bufsize - 1
    }

    /// Number of events available to read (complete packets only).
    pub fn available(&self) -> bool {
        self.packet_head != self.tail
    }

    /// Pass an event into the ring buffer, mirroring Linux `__pass_event`.
    /// Caller must hold the client lock.
    pub fn pass_event(&mut self, event: &InputEvent) {
        let mask = self.mask();
        self.buffer[self.head as usize] = *event;
        self.head = (self.head + 1) & mask;

        if self.head == self.tail {
            // Buffer full: drop all unconsumed events, leave SYN_DROPPED
            // plus the newest event.
            self.tail = (self.head.wrapping_sub(2)) & mask;
            let ts = event.timestamp_ms;
            self.buffer[self.tail as usize] = InputEvent {
                event_type: EV_SYN,
                code: SYN_DROPPED,
                value: 0,
                timestamp_ms: ts,
            };
            self.packet_head = self.tail;
        }

        if event.event_type == EV_SYN && event.code == SYN_REPORT {
            self.packet_head = self.head;
        }
    }

    /// Queue a SYN_DROPPED event, mirroring Linux `__evdev_queue_syn_dropped`.
    /// Caller must hold the client lock.
    pub fn queue_syn_dropped(&mut self) {
        let mask = self.mask();
        let ts = crate::time::uptime_ms();
        self.buffer[self.head as usize] = InputEvent {
            event_type: EV_SYN,
            code: SYN_DROPPED,
            value: 0,
            timestamp_ms: ts,
        };
        self.head = (self.head + 1) & mask;

        if self.head == self.tail {
            self.tail = (self.head.wrapping_sub(1)) & mask;
            self.packet_head = self.tail;
        }
    }

    /// Flush queued events of a given type, mirroring Linux `__evdev_flush_queue`.
    /// Caller must hold the client lock.
    pub fn flush_queue(&mut self, ev_type: u16) {
        if ev_type == EV_SYN {
            return;
        }

        let mask = self.mask();
        let mut head = self.tail;
        self.packet_head = self.tail;
        let mut num: u32 = 1; // init to 1 so leading SYN_REPORT is not dropped

        let mut i = self.tail;
        while i != self.head {
            let ev = &self.buffer[i as usize];
            let is_report = ev.event_type == EV_SYN && ev.code == SYN_REPORT;

            if ev.event_type == ev_type {
                // drop matched entry
            } else if is_report && num == 0 {
                // drop empty SYN_REPORT groups
            } else {
                if head != i {
                    self.buffer[head as usize] = *ev;
                }
                num += 1;
                head = (head + 1) & mask;
                if is_report {
                    num = 0;
                    self.packet_head = head;
                }
            }
            i = (i + 1) & mask;
        }

        self.head = head;
    }

    /// Fetch the next event from the ring, mirroring Linux `evdev_fetch_next_event`.
    /// Returns `Some(event)` if available, `None` otherwise.
    pub fn fetch_next_event(&mut self) -> Option<InputEvent> {
        if self.packet_head == self.tail {
            return None;
        }
        let mask = self.mask();
        let event = self.buffer[self.tail as usize];
        self.tail = (self.tail + 1) & mask;
        Some(event)
    }

    /// Drain all available events into a vector.
    pub fn drain_events(&mut self) -> Vec<InputEvent> {
        let mut events = Vec::new();
        while let Some(ev) = self.fetch_next_event() {
            events.push(ev);
        }
        events
    }

    /// Set the clock type, mirroring Linux `evdev_set_clk_type`.
    /// Flushes pending events and queues SYN_DROPPED if queue is non-empty.
    pub fn set_clk_type(&mut self, clkid: u32) -> Result<(), &'static str> {
        let new_clk = match clkid {
            0 => InputClockType::Realtime,  // CLOCK_REALTIME
            1 => InputClockType::Monotonic, // CLOCK_MONOTONIC
            7 => InputClockType::Boottime,  // CLOCK_BOOTTIME
            _ => return Err("invalid clockid"),
        };

        if self.clk_type != new_clk {
            self.clk_type = new_clk;
            if self.head != self.tail {
                self.packet_head = self.head;
                self.tail = self.head;
                self.queue_syn_dropped();
            }
        }
        Ok(())
    }
}

/// Compute next power-of-two >= n (Linux `roundup_pow_of_two`).
fn next_pow2(n: u32) -> u32 {
    if n == 0 {
        return 1;
    }
    let mut v = n;
    v -= 1;
    v |= v >> 1;
    v |= v >> 2;
    v |= v >> 4;
    v |= v >> 8;
    v |= v >> 16;
    v + 1
}

// ── Evdev device state ──────────────────────────────────────────────────

/// An evdev device, mirroring Linux `struct evdev`.
pub struct EvdevDevice {
    pub name: String,
    pub phys: String,
    pub uniq: String,
    pub bustype: u16,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
    pub open_count: u32,
    pub exist: bool,
    /// Hint for buffer size computation (Linux `hint_events_per_packet`).
    pub hint_events_per_packet: u32,
    /// Capability bitmaps, indexed by event type.
    /// Each bitmap is a Vec<u8> where bit N means code N is supported.
    pub ev_bits: Vec<u8>, // EV_SYN bit set = which event types supported
    pub key_bits: Vec<u8>,  // EV_KEY capability bitmap
    pub rel_bits: Vec<u8>,  // EV_REL capability bitmap
    pub abs_bits: Vec<u8>,  // EV_ABS capability bitmap
    pub msc_bits: Vec<u8>,  // EV_MSC capability bitmap
    pub led_bits: Vec<u8>,  // EV_LED capability bitmap
    pub sw_bits: Vec<u8>,   // EV_SW capability bitmap
    pub snd_bits: Vec<u8>,  // EV_SND capability bitmap
    pub ff_bits: Vec<u8>,   // EV_FF capability bitmap
    pub rep_bits: Vec<u8>,  // EV_REP capability bitmap
    pub prop_bits: Vec<u8>, // INPUT_PROP_* bitmap
    /// Absolute axis info, indexed by axis code (0..ABS_MAX).
    pub absinfo: Vec<InputAbsInfo>,
    /// Key state bitmap (current pressed keys).
    pub key_state: Vec<u8>,
    /// LED state bitmap.
    pub led_state: Vec<u8>,
    /// SW state bitmap.
    pub sw_state: Vec<u8>,
    /// SND state bitmap.
    pub snd_state: Vec<u8>,
    /// Repeat delay (REP_DELAY).
    pub rep_delay: u32,
    /// Repeat period (REP_PERIOD).
    pub rep_period: u32,
}

/// Linux `struct input_absinfo` (24 bytes).
#[derive(Debug, Clone, Copy, Default)]
pub struct InputAbsInfo {
    pub value: i32,
    pub minimum: i32,
    pub maximum: i32,
    pub fuzz: i32,
    pub flat: i32,
    pub resolution: i32,
}

impl InputAbsInfo {
    pub fn to_bytes(&self) -> [u8; 24] {
        let mut buf = [0u8; 24];
        buf[0..4].copy_from_slice(&self.value.to_le_bytes());
        buf[4..8].copy_from_slice(&self.minimum.to_le_bytes());
        buf[8..12].copy_from_slice(&self.maximum.to_le_bytes());
        buf[12..16].copy_from_slice(&self.fuzz.to_le_bytes());
        buf[16..20].copy_from_slice(&self.flat.to_le_bytes());
        buf[20..24].copy_from_slice(&self.resolution.to_le_bytes());
        buf
    }

    pub fn from_bytes(buf: &[u8]) -> Self {
        if buf.len() < 24 {
            return Self::default();
        }
        Self {
            value: i32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
            minimum: i32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            maximum: i32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
            fuzz: i32::from_le_bytes([buf[12], buf[13], buf[14], buf[15]]),
            flat: i32::from_le_bytes([buf[16], buf[17], buf[18], buf[19]]),
            resolution: i32::from_le_bytes([buf[20], buf[21], buf[22], buf[23]]),
        }
    }
}

/// Linux `struct input_id` (8 bytes).
#[derive(Debug, Clone, Copy, Default)]
pub struct InputId {
    pub bustype: u16,
    pub vendor: u16,
    pub product: u16,
    pub version: u16,
}

impl InputId {
    pub fn to_bytes(&self) -> [u8; 8] {
        let mut buf = [0u8; 8];
        buf[0..2].copy_from_slice(&self.bustype.to_le_bytes());
        buf[2..4].copy_from_slice(&self.vendor.to_le_bytes());
        buf[4..6].copy_from_slice(&self.product.to_le_bytes());
        buf[6..8].copy_from_slice(&self.version.to_le_bytes());
        buf
    }
}

/// Linux `struct input_mask` (16 bytes).
#[derive(Debug, Clone, Copy, Default)]
pub struct InputMask {
    pub mask_type: u32,
    pub codes_size: u32,
    pub codes_ptr: u64,
}

impl InputMask {
    pub fn from_bytes(buf: &[u8]) -> Self {
        if buf.len() < 16 {
            return Self::default();
        }
        Self {
            mask_type: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
            codes_size: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            codes_ptr: u64::from_le_bytes([
                buf[8], buf[9], buf[10], buf[11], buf[12], buf[13], buf[14], buf[15],
            ]),
        }
    }
}

impl EvdevDevice {
    /// Create a new evdev device with default capabilities.
    pub fn new(name: &str, bustype: u16, vendor: u16, product: u16, version: u16) -> Self {
        let mut rep_bits = vec![0u8; ((super::REP_MAX as usize + 1) + 7) / 8];
        Self::set_bit(&mut rep_bits, super::REP_DELAY);
        Self::set_bit(&mut rep_bits, super::REP_PERIOD);

        Self {
            name: String::from(name),
            phys: String::from("isa0060/serio0/input0"),
            uniq: String::new(),
            bustype,
            vendor,
            product,
            version,
            open_count: 0,
            exist: true,
            hint_events_per_packet: 8,
            ev_bits: vec![0u8; ((EV_MAX as usize + 1) + 7) / 8],
            key_bits: vec![0u8; ((KEY_MAX as usize + 1) + 7) / 8],
            rel_bits: vec![0u8; ((REL_MAX as usize + 1) + 7) / 8],
            abs_bits: vec![0u8; ((ABS_MAX as usize + 1) + 7) / 8],
            msc_bits: vec![0u8; ((MSC_MAX as usize + 1) + 7) / 8],
            led_bits: vec![0u8; ((LED_MAX as usize + 1) + 7) / 8],
            sw_bits: vec![0u8; ((SW_MAX as usize + 1) + 7) / 8],
            snd_bits: vec![0u8; ((SND_MAX as usize + 1) + 7) / 8],
            ff_bits: vec![0u8; ((FF_MAX as usize + 1) + 7) / 8],
            rep_bits,
            prop_bits: vec![0u8; 1],
            absinfo: alloc::vec![InputAbsInfo::default(); (ABS_MAX as usize) + 1],
            key_state: vec![0u8; ((KEY_MAX as usize + 1) + 7) / 8],
            led_state: vec![0u8; ((LED_MAX as usize + 1) + 7) / 8],
            sw_state: vec![0u8; ((SW_MAX as usize + 1) + 7) / 8],
            snd_state: vec![0u8; ((SND_MAX as usize + 1) + 7) / 8],
            rep_delay: 250,
            rep_period: 33,
        }
    }

    /// Set a bit in a capability bitmap.
    fn set_bit(bitmap: &mut [u8], code: u16) {
        let byte = (code / 8) as usize;
        let bit = (code % 8) as u8;
        if byte < bitmap.len() {
            bitmap[byte] |= 1 << bit;
        }
    }

    /// Clear a bit in a state bitmap.
    fn clear_bit(bitmap: &mut [u8], code: u16) {
        let byte = (code / 8) as usize;
        let bit = (code % 8) as u8;
        if byte < bitmap.len() {
            bitmap[byte] &= !(1 << bit);
        }
    }

    /// Test a bit in a capability bitmap.
    fn test_bit(bitmap: &[u8], code: u16) -> bool {
        let byte = (code / 8) as usize;
        let bit = (code % 8) as u8;
        if byte < bitmap.len() {
            (bitmap[byte] & (1 << bit)) != 0
        } else {
            false
        }
    }

    /// Mark an event type as supported (set bit in ev_bits).
    pub fn set_ev_type(&mut self, ev_type: u16) {
        Self::set_bit(&mut self.ev_bits, ev_type);
    }

    /// Mark a key code as supported.
    pub fn set_key(&mut self, code: u16) {
        Self::set_bit(&mut self.key_bits, code);
    }

    /// Mark a relative code as supported.
    pub fn set_rel(&mut self, code: u16) {
        Self::set_bit(&mut self.rel_bits, code);
    }

    /// Mark an absolute code as supported.
    pub fn set_abs(&mut self, code: u16) {
        Self::set_bit(&mut self.abs_bits, code);
    }

    /// Set absolute axis info for a given axis.
    pub fn set_absinfo(&mut self, code: u16, info: InputAbsInfo) {
        if (code as usize) < self.absinfo.len() {
            self.absinfo[code as usize] = info;
        }
    }

    /// Get the capability bitmap for a given event type (Linux `handle_eviocgbit`).
    pub fn get_capability_bitmap(&self, ev_type: u16) -> &[u8] {
        match ev_type {
            0 => &self.ev_bits, // EV_SYN → which event types
            EV_KEY => &self.key_bits,
            EV_REL => &self.rel_bits,
            EV_ABS => &self.abs_bits,
            EV_MSC => &self.msc_bits,
            EV_LED => &self.led_bits,
            EV_SND => &self.snd_bits,
            EV_FF => &self.ff_bits,
            EV_SW => &self.sw_bits,
            EV_REP => &self.rep_bits,
            _ => &[],
        }
    }

    /// Get the max code count for a given event type (Linux `evdev_get_mask_cnt`).
    pub fn get_mask_cnt(ev_type: u16) -> usize {
        match ev_type {
            0 => 0x20, // EV_CNT = EV_MAX + 1
            EV_KEY => KEY_MAX as usize + 1,
            EV_REL => REL_MAX as usize + 1,
            EV_ABS => ABS_MAX as usize + 1,
            EV_MSC => MSC_MAX as usize + 1,
            EV_LED => LED_MAX as usize + 1,
            EV_SW => SW_MAX as usize + 1,
            EV_SND => SND_MAX as usize + 1,
            EV_FF => FF_MAX as usize + 1,
            EV_REP => super::REP_MAX as usize + 1,
            _ => 0,
        }
    }

    /// Get the state bitmap for EVIOCGKEY/EVIOCGLED/EVIOCGSW/EVIOCGSND.
    pub fn get_state_bitmap(&self, ev_type: u16) -> &[u8] {
        match ev_type {
            EV_KEY => &self.key_state,
            EV_LED => &self.led_state,
            EV_SW => &self.sw_state,
            EV_SND => &self.snd_state,
            _ => &[],
        }
    }

    /// Update Linux-style device state for stateful event types before events
    /// are fanned out to clients. Relative and sync events have no persistent
    /// state; EV_KEY value 2 (autorepeat) leaves the key marked pressed.
    fn update_state(&mut self, event: &InputEvent) {
        match event.event_type {
            EV_KEY => {
                if event.value == 0 {
                    Self::clear_bit(&mut self.key_state, event.code);
                } else {
                    Self::set_bit(&mut self.key_state, event.code);
                }
            }
            EV_ABS => {
                if let Some(abs) = self.absinfo.get_mut(event.code as usize) {
                    abs.value = event.value;
                }
            }
            EV_LED => {
                if event.value == 0 {
                    Self::clear_bit(&mut self.led_state, event.code);
                } else {
                    Self::set_bit(&mut self.led_state, event.code);
                }
            }
            EV_SW => {
                if event.value == 0 {
                    Self::clear_bit(&mut self.sw_state, event.code);
                } else {
                    Self::set_bit(&mut self.sw_state, event.code);
                }
            }
            EV_SND => {
                if event.value == 0 {
                    Self::clear_bit(&mut self.snd_state, event.code);
                } else {
                    Self::set_bit(&mut self.snd_state, event.code);
                }
            }
            _ => {}
        }
    }
}

// ── Global evdev registry ───────────────────────────────────────────────

lazy_static! {
    /// Global list of evdev devices, indexed by minor number (0..EVDEV_MAX_CLIENTS).
    static ref EVDEV_DEVICES: Mutex<Vec<Option<EvdevDevice>>> = {
        let mut v = Vec::with_capacity(EVDEV_MAX_CLIENTS);
        for _ in 0..EVDEV_MAX_CLIENTS {
            v.push(None);
        }
        Mutex::new(v)
    };

    /// Global list of evdev clients, indexed by fd or client id.
    static ref EVDEV_CLIENTS: Mutex<Vec<EvdevClient>> = Mutex::new(Vec::new());
}

/// Register an evdev device at the next available minor.
/// Returns the minor number (0-based), or Err if no slots available.
pub fn register_evdev_device(device: EvdevDevice) -> Result<usize, &'static str> {
    let mut devices = EVDEV_DEVICES.lock();
    for i in 0..EVDEV_MAX_CLIENTS {
        if devices[i].is_none() {
            devices[i] = Some(device);
            return Ok(i);
        }
    }
    Err("no free evdev minor")
}

/// Create a new evdev client for the device at `device_idx`.
/// Returns a client ID that can be used to look up the client later.
pub fn create_client(device_idx: usize) -> Result<usize, &'static str> {
    let devices = EVDEV_DEVICES.lock();
    let device = devices
        .get(device_idx)
        .and_then(|d| d.as_ref())
        .ok_or("evdev device not found")?;
    if !device.exist {
        return Err("device does not exist");
    }
    let hint = device.hint_events_per_packet;
    drop(devices);

    let client = EvdevClient::new(device_idx, hint);
    let mut clients = EVDEV_CLIENTS.lock();
    let id = clients.len();
    clients.push(client);
    Ok(id)
}

/// Get a client by ID for reading events.
pub fn with_client_mut<F, R>(client_id: usize, f: F) -> Result<R, &'static str>
where
    F: FnOnce(&mut EvdevClient) -> R,
{
    let mut clients = EVDEV_CLIENTS.lock();
    let client = clients.get_mut(client_id).ok_or("evdev client not found")?;
    Ok(f(client))
}

/// Pass an event to all clients attached to a given device.
/// Mirrors Linux `evdev_events` → `evdev_pass_values`.
pub fn pass_event_to_device(device_idx: usize, event: &InputEvent) {
    if let Ok(()) = with_device_mut(device_idx, |dev| dev.update_state(event)) {
        // state updated
    }

    let mut clients = EVDEV_CLIENTS.lock();
    let grabbed = clients
        .iter()
        .any(|c| c.device_idx == device_idx && c.grabbed && !c.revoked);
    for client in clients.iter_mut() {
        if client.device_idx == device_idx && !client.revoked && (!grabbed || client.grabbed) {
            client.pass_event(event);
        }
    }
}

// ── Minimal Linux evdev ioctl dispatcher ────────────────────────────────

const IOC_NRBITS: u32 = 8;
const IOC_TYPEBITS: u32 = 8;
const IOC_SIZEBITS: u32 = 14;
const IOC_NRSHIFT: u32 = 0;
const IOC_TYPESHIFT: u32 = IOC_NRSHIFT + IOC_NRBITS;
const IOC_SIZESHIFT: u32 = IOC_TYPESHIFT + IOC_TYPEBITS;
const IOC_DIRSHIFT: u32 = IOC_SIZESHIFT + IOC_SIZEBITS;
const IOC_WRITE: u32 = 1;
const IOC_READ: u32 = 2;
const EVDEV_IOC_TYPE: u32 = b'E' as u32;

fn ioc(dir: u32, nr: u32, size: u32) -> u32 {
    (dir << IOC_DIRSHIFT)
        | (EVDEV_IOC_TYPE << IOC_TYPESHIFT)
        | (nr << IOC_NRSHIFT)
        | (size << IOC_SIZESHIFT)
}

fn ioc_nr(request: u32) -> u8 {
    ((request >> IOC_NRSHIFT) & 0xff) as u8
}

fn ioc_type(request: u32) -> u8 {
    ((request >> IOC_TYPESHIFT) & 0xff) as u8
}

fn ioc_size(request: u32) -> usize {
    ((request >> IOC_SIZESHIFT) & 0x3fff) as usize
}

fn put_bytes(dst: &mut [u8], src: &[u8]) -> usize {
    let n = core::cmp::min(dst.len(), src.len());
    dst[..n].copy_from_slice(&src[..n]);
    n
}

fn put_u32(dst: &mut [u8], value: u32) -> usize {
    put_bytes(dst, &value.to_le_bytes())
}

fn get_i32(src: &[u8]) -> Result<i32, &'static str> {
    if src.len() < 4 {
        return Err("evdev ioctl argument too short");
    }
    Ok(i32::from_le_bytes([src[0], src[1], src[2], src[3]]))
}

fn put_string(dst: &mut [u8], value: &str) -> usize {
    if dst.is_empty() {
        return 0;
    }
    let bytes = value.as_bytes();
    let n = core::cmp::min(dst.len().saturating_sub(1), bytes.len());
    dst[..n].copy_from_slice(&bytes[..n]);
    dst[n] = 0;
    n + 1
}

/// Dispatch a Linux-compatible subset of evdev ioctls against a client.
/// Returns the number of bytes written to `arg` for read ioctls and 0 for
/// write-only ioctls. Supported requests include EVIOCGVERSION, EVIOCGID,
/// EVIOCGNAME/PHYS/UNIQ, EVIOCGBIT, EVIOCGKEY/LED/SW/SND,
/// EVIOCGABS/EVIOCSABS, EVIOCGREP/EVIOCSREP, EVIOCGRAB, EVIOCREVOKE and
/// EVIOCSCLOCKID.
pub fn evdev_ioctl(client_id: usize, request: u32, arg: &mut [u8]) -> Result<usize, &'static str> {
    if ioc_type(request) != EVDEV_IOC_TYPE as u8 {
        return Err("evdev ioctl wrong type");
    }

    let nr = ioc_nr(request);
    let size = ioc_size(request).min(arg.len());
    let client_device = {
        let clients = EVDEV_CLIENTS.lock();
        let client = clients.get(client_id).ok_or("evdev client not found")?;
        if client.revoked {
            return Err("evdev client revoked");
        }
        client.device_idx
    };

    match nr {
        0x01 => return Ok(put_u32(arg, EV_VERSION)),
        0x02 => {
            return with_device(client_device, |dev| {
                InputId {
                    bustype: dev.bustype,
                    vendor: dev.vendor,
                    product: dev.product,
                    version: dev.version,
                }
                .to_bytes()
            })
            .map(|id| put_bytes(arg, &id));
        }
        0x03 => {
            if request == ioc(IOC_READ, 0x03, 8) {
                return with_device(client_device, |dev| {
                    let mut rep = [0u8; 8];
                    rep[0..4].copy_from_slice(&dev.rep_delay.to_le_bytes());
                    rep[4..8].copy_from_slice(&dev.rep_period.to_le_bytes());
                    rep
                })
                .map(|rep| put_bytes(arg, &rep));
            }
            if request == ioc(IOC_WRITE, 0x03, 8) {
                if arg.len() < 8 {
                    return Err("EVIOCSREP argument too short");
                }
                let delay = u32::from_le_bytes([arg[0], arg[1], arg[2], arg[3]]);
                let period = u32::from_le_bytes([arg[4], arg[5], arg[6], arg[7]]);
                return with_device_mut(client_device, |dev| {
                    dev.rep_delay = delay;
                    dev.rep_period = period;
                })
                .map(|_| 0);
            }
        }
        0x06 => return with_device(client_device, |dev| put_string(arg, &dev.name)),
        0x07 => return with_device(client_device, |dev| put_string(arg, &dev.phys)),
        0x08 => return with_device(client_device, |dev| put_string(arg, &dev.uniq)),
        0x09 => {
            return with_device(client_device, |dev| {
                put_bytes(&mut arg[..size], &dev.prop_bits)
            })
        }
        0x18 => {
            return with_device(client_device, |dev| {
                put_bytes(&mut arg[..size], &dev.key_state)
            })
        }
        0x19 => {
            return with_device(client_device, |dev| {
                put_bytes(&mut arg[..size], &dev.led_state)
            })
        }
        0x1a => {
            return with_device(client_device, |dev| {
                put_bytes(&mut arg[..size], &dev.snd_state)
            })
        }
        0x1b => {
            return with_device(client_device, |dev| {
                put_bytes(&mut arg[..size], &dev.sw_state)
            })
        }
        0x90 => {
            let grab = get_i32(arg)? != 0;
            let mut clients = EVDEV_CLIENTS.lock();
            if grab
                && clients.iter().enumerate().any(|(id, c)| {
                    id != client_id && c.device_idx == client_device && c.grabbed && !c.revoked
                })
            {
                return Err("evdev device already grabbed");
            }
            let client = clients.get_mut(client_id).ok_or("evdev client not found")?;
            client.grabbed = grab;
            return Ok(0);
        }
        0x91 => {
            let revoke = get_i32(arg)? != 0;
            if revoke {
                let mut clients = EVDEV_CLIENTS.lock();
                let client = clients.get_mut(client_id).ok_or("evdev client not found")?;
                client.revoked = true;
                client.head = client.tail;
                client.packet_head = client.tail;
            }
            return Ok(0);
        }
        0xa0 => {
            let clkid = get_i32(arg)? as u32;
            return with_client_mut(client_id, |client| client.set_clk_type(clkid))
                .and_then(|r| r.map(|_| 0));
        }
        _ => {}
    }

    if (0x20..=0x20 + EV_MAX as u8).contains(&nr) {
        let ev_type = (nr - 0x20) as u16;
        return with_device(client_device, |dev| {
            put_bytes(&mut arg[..size], dev.get_capability_bitmap(ev_type))
        });
    }

    if (0x40..=0x40 + ABS_MAX as u8).contains(&nr) {
        let code = (nr - 0x40) as usize;
        return with_device(client_device, |dev| dev.absinfo[code].to_bytes())
            .map(|abs| put_bytes(arg, &abs));
    }

    if (0xc0..=0xc0 + ABS_MAX as u8).contains(&nr) {
        if arg.len() < 24 {
            return Err("EVIOCSABS argument too short");
        }
        let code = (nr - 0xc0) as u16;
        let info = InputAbsInfo::from_bytes(arg);
        return with_device_mut(client_device, |dev| dev.set_absinfo(code, info)).map(|_| 0);
    }

    Err("unsupported evdev ioctl")
}

fn is_ps2_mouse(device: crate::drivers::ps2_controller::Ps2DeviceType) -> bool {
    matches!(
        device,
        crate::drivers::ps2_controller::Ps2DeviceType::StandardMouse
            | crate::drivers::ps2_controller::Ps2DeviceType::MouseWithScrollWheel
            | crate::drivers::ps2_controller::Ps2DeviceType::Mouse5Button
    )
}

/// Initialize evdev nodes for real input hardware already detected by lower
/// level drivers. This deliberately does not create generic placeholder
/// keyboard/mouse nodes; callers may invoke it repeatedly as PS/2 discovery
/// completes and it will only register missing hardware-backed devices.
pub fn init_evdev_devices() {
    let Some((port1_available, port1_device, port2_available, port2_device)) =
        crate::drivers::ps2_controller::get_device_info()
    else {
        return;
    };

    let has_keyboard =
        port1_available && port1_device == crate::drivers::ps2_controller::Ps2DeviceType::Keyboard;
    let has_mouse = port2_available && is_ps2_mouse(port2_device);

    let (keyboard_registered, mouse_registered) = {
        let devices = EVDEV_DEVICES.lock();
        let keyboard_registered = devices
            .iter()
            .flatten()
            .any(|dev| dev.name == "RustOS PS/2 Keyboard");
        let mouse_registered = devices
            .iter()
            .flatten()
            .any(|dev| dev.name == "RustOS PS/2 Mouse");
        (keyboard_registered, mouse_registered)
    };

    if has_keyboard && !keyboard_registered {
        let mut kbd = EvdevDevice::new("RustOS PS/2 Keyboard", 0x0011, 0x0001, 0x0001, 0x0001);
        kbd.phys = String::from("isa0060/serio0/input0");
        kbd.set_ev_type(EV_SYN);
        kbd.set_ev_type(EV_KEY);
        kbd.set_ev_type(EV_MSC);
        kbd.set_ev_type(EV_LED);
        kbd.set_ev_type(EV_REP);
        // Set common key codes
        for code in 1..=57u16 {
            kbd.set_key(code);
        }
        kbd.set_key(super::KEY_CAPSLOCK);
        kbd.set_key(super::KEY_F1);
        kbd.set_key(super::KEY_F2);
        kbd.set_key(super::KEY_UP);
        kbd.set_key(super::KEY_DOWN);
        kbd.set_key(super::KEY_LEFT);
        kbd.set_key(super::KEY_RIGHT);
        kbd.set_key(super::KEY_ENTER);
        kbd.set_key(super::KEY_SPACE);
        kbd.set_key(super::KEY_LEFTSHIFT);
        kbd.set_key(super::KEY_RIGHTSHIFT);
        kbd.set_key(super::KEY_LEFTCTRL);
        kbd.set_key(super::KEY_RIGHTCTRL);
        kbd.set_key(super::KEY_LEFTALT);
        kbd.set_key(super::KEY_RIGHTALT);
        kbd.set_key(super::KEY_LEFTMETA);
        kbd.set_key(super::KEY_RIGHTMETA);
        kbd.set_key(super::KEY_TAB);
        kbd.set_key(super::KEY_BACKSPACE);
        kbd.set_key(super::KEY_INSERT);
        kbd.set_key(super::KEY_DELETE);
        kbd.set_key(super::KEY_HOME);
        kbd.set_key(super::KEY_END);
        kbd.set_key(super::KEY_PAGEUP);
        kbd.set_key(super::KEY_PAGEDOWN);
        kbd.set_key(super::KEY_NUMLOCK);
        kbd.set_key(super::KEY_SCROLLLOCK);
        kbd.set_key(super::KEY_PAUSE);
        kbd.set_key(super::KEY_MUTE);
        kbd.set_key(super::KEY_VOLUMEDOWN);
        kbd.set_key(super::KEY_VOLUMEUP);
        kbd.set_key(super::KEY_POWER);
        // F1-F12
        for f in 59..=88u16 {
            kbd.set_key(f);
        }
        // F13-F24
        for f in 183..=194u16 {
            kbd.set_key(f);
        }

        let _ = register_evdev_device(kbd);
    }

    if has_mouse && !mouse_registered {
        let mut mouse = EvdevDevice::new("RustOS PS/2 Mouse", 0x0011, 0x0002, 0x0001, 0x0001);
        mouse.phys = String::from("isa0060/serio1/input0");
        mouse.set_ev_type(EV_SYN);
        mouse.set_ev_type(EV_KEY);
        mouse.set_ev_type(EV_REL);
        mouse.set_key(super::BTN_LEFT);
        mouse.set_key(super::BTN_RIGHT);
        mouse.set_key(super::BTN_MIDDLE);
        mouse.set_key(super::BTN_SIDE);
        mouse.set_key(super::BTN_EXTRA);
        mouse.set_key(super::BTN_FORWARD);
        mouse.set_key(super::BTN_BACK);
        mouse.set_rel(super::REL_X);
        mouse.set_rel(super::REL_Y);
        mouse.set_rel(super::REL_WHEEL);
        mouse.set_rel(super::REL_HWHEEL);
        mouse.set_rel(super::REL_WHEEL_HI_RES);
        mouse.set_rel(super::REL_HWHEEL_HI_RES);

        let _ = register_evdev_device(mouse);
    }
}

/// Get a reference to an evdev device by index.
pub fn with_device<F, R>(idx: usize, f: F) -> Result<R, &'static str>
where
    F: FnOnce(&EvdevDevice) -> R,
{
    let devices = EVDEV_DEVICES.lock();
    let device = devices
        .get(idx)
        .and_then(|d| d.as_ref())
        .ok_or("evdev device not found")?;
    Ok(f(device))
}

/// Get a mutable reference to an evdev device by index.
pub fn with_device_mut<F, R>(idx: usize, f: F) -> Result<R, &'static str>
where
    F: FnOnce(&mut EvdevDevice) -> R,
{
    let mut devices = EVDEV_DEVICES.lock();
    let device = devices
        .get_mut(idx)
        .and_then(|d| d.as_mut())
        .ok_or("evdev device not found")?;
    Ok(f(device))
}

/// Get the number of registered evdev devices.
pub fn device_count() -> usize {
    EVDEV_DEVICES.lock().iter().filter(|d| d.is_some()).count()
}
