//! In-memory xHCI controller model.
//!
//! Models the data structures of a real xHCI host controller — TRBs, the
//! command ring, the event ring, per-endpoint transfer rings, the device
//! context base array and the doorbell array — backed entirely by heap
//! memory so the transfer paths run and can be inspected without hardware.
//!
//! The producer rings carry a Link TRB in their final slot that toggles the
//! producer cycle state (PCS) on wrap; the event ring tracks its own consumer
//! cycle state (CCS) so software can detect freshly posted events. Ringing a
//! doorbell drives the software completion path: pending Transfer TRBs are
//! dispatched to the attached `VirtualDevice` and a Transfer Event TRB is
//! posted to the event ring.

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use super::device::VirtualDevice;
use super::hcd::{
    CompletionCode, HostController, PortStatus, SetupPacket, TransferDirection, TransferResult,
    UsbSpeed,
};

// ── TRB model ───────────────────────────────────────────────────────────

/// TRB type field values (xHCI §6.4.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrbType {
    Reserved = 0,
    Normal = 1,
    SetupStage = 2,
    DataStage = 3,
    StatusStage = 4,
    Link = 6,
    EnableSlotCmd = 9,
    AddressDeviceCmd = 11,
    TransferEvent = 32,
    CommandCompletion = 33,
    PortStatusChange = 34,
}

impl TrbType {
    pub fn from_u8(v: u8) -> TrbType {
        match v {
            1 => TrbType::Normal,
            2 => TrbType::SetupStage,
            3 => TrbType::DataStage,
            4 => TrbType::StatusStage,
            6 => TrbType::Link,
            9 => TrbType::EnableSlotCmd,
            11 => TrbType::AddressDeviceCmd,
            32 => TrbType::TransferEvent,
            33 => TrbType::CommandCompletion,
            34 => TrbType::PortStatusChange,
            _ => TrbType::Reserved,
        }
    }
}

/// A 16-byte Transfer Request Block.
#[derive(Debug, Clone, Copy, Default)]
pub struct Trb {
    /// Parameter / pointer field (TRB bytes 0-7).
    pub parameter: u64,
    /// Status field (bytes 8-11): transfer length / completion info.
    pub status: u32,
    /// Control field (bytes 12-15): cycle bit (0), TRB type (10-15), flags.
    pub control: u32,
}

const CTRL_CYCLE: u32 = 1 << 0;
const CTRL_TOGGLE_CYCLE: u32 = 1 << 1;
const CTRL_TYPE_SHIFT: u32 = 10;

impl Trb {
    pub fn new(trb_type: TrbType) -> Self {
        Trb {
            parameter: 0,
            status: 0,
            control: (trb_type as u32) << CTRL_TYPE_SHIFT,
        }
    }

    pub fn trb_type(&self) -> TrbType {
        TrbType::from_u8(((self.control >> CTRL_TYPE_SHIFT) & 0x3F) as u8)
    }

    pub fn cycle(&self) -> bool {
        self.control & CTRL_CYCLE != 0
    }

    pub fn set_cycle(&mut self, cycle: bool) {
        if cycle {
            self.control |= CTRL_CYCLE;
        } else {
            self.control &= !CTRL_CYCLE;
        }
    }

    /// Completion code stored in the status field of an event TRB.
    pub fn completion_code(&self) -> u8 {
        ((self.status >> 24) & 0xFF) as u8
    }

    /// Transfer length stored in the status field.
    pub fn transfer_length(&self) -> u32 {
        self.status & 0x00FF_FFFF
    }
}

// ── Producer ring (command / transfer) ──────────────────────────────────

/// A producer ring whose last slot is a Link TRB that toggles the producer
/// cycle state on wrap.
#[derive(Debug)]
pub struct ProducerRing {
    pub trbs: Vec<Trb>,
    pub enqueue: usize,
    pub cycle: bool,
    pub wraps: u64,
}

impl ProducerRing {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(2);
        let mut trbs = alloc::vec![Trb::default(); cap];
        // Final slot is a Link TRB pointing back to index 0 with Toggle Cycle.
        let mut link = Trb::new(TrbType::Link);
        link.control |= CTRL_TOGGLE_CYCLE;
        trbs[cap - 1] = link;
        ProducerRing {
            trbs,
            enqueue: 0,
            cycle: true,
            wraps: 0,
        }
    }

    /// Number of usable (non-Link) entries.
    fn usable(&self) -> usize {
        self.trbs.len() - 1
    }

    /// Enqueue `trb`, stamping the current producer cycle state. Wraps via the
    /// Link TRB (toggling PCS) when the usable region is exhausted.
    pub fn enqueue(&mut self, mut trb: Trb) -> usize {
        trb.set_cycle(self.cycle);
        let slot = self.enqueue;
        self.trbs[slot] = trb;
        self.enqueue += 1;
        if self.enqueue >= self.usable() {
            // Reached the Link TRB: stamp it, follow it, toggle PCS.
            let link_idx = self.trbs.len() - 1;
            let cycle = self.cycle;
            self.trbs[link_idx].set_cycle(cycle);
            self.enqueue = 0;
            self.cycle = !self.cycle;
            self.wraps += 1;
        }
        slot
    }
}

// ── Event ring (consumer + producer sides) ──────────────────────────────

/// A single-segment event ring. The controller posts events on the producer
/// side; software consumes them tracking its own consumer cycle state.
#[derive(Debug)]
pub struct EventRing {
    pub trbs: Vec<Trb>,
    /// Producer enqueue index and producer cycle state.
    pub enqueue: usize,
    pub pcs: bool,
    /// Consumer dequeue index and consumer cycle state.
    pub dequeue: usize,
    pub ccs: bool,
}

impl EventRing {
    pub fn new(capacity: usize) -> Self {
        let cap = capacity.max(2);
        EventRing {
            trbs: alloc::vec![Trb::default(); cap],
            enqueue: 0,
            pcs: true,
            dequeue: 0,
            ccs: true,
        }
    }

    /// Post an event TRB, stamping the producer cycle state and wrapping.
    pub fn post(&mut self, mut trb: Trb) {
        trb.set_cycle(self.pcs);
        self.trbs[self.enqueue] = trb;
        self.enqueue += 1;
        if self.enqueue >= self.trbs.len() {
            self.enqueue = 0;
            self.pcs = !self.pcs;
        }
    }

    /// Consume the next event if its cycle bit matches the consumer cycle
    /// state, advancing the dequeue pointer.
    pub fn dequeue(&mut self) -> Option<Trb> {
        let trb = self.trbs[self.dequeue];
        if trb.cycle() != self.ccs {
            return None; // entry not yet produced
        }
        self.dequeue += 1;
        if self.dequeue >= self.trbs.len() {
            self.dequeue = 0;
            self.ccs = !self.ccs;
        }
        Some(trb)
    }
}

// ── Root-hub port + attached virtual device ─────────────────────────────

struct Port {
    status: PortStatus,
    device: Option<Box<dyn VirtualDevice>>,
    slot: Option<u8>,
}

// ── Controller ──────────────────────────────────────────────────────────

const TRANSFER_RING_CAP: usize = 16;
const COMMAND_RING_CAP: usize = 16;
const EVENT_RING_CAP: usize = 64;

/// Software xHCI controller.
pub struct XhciController {
    name: String,
    /// Modelled MMIO operational registers.
    pub usbcmd: u32,
    pub usbsts: u32,
    pub crcr: u64,
    pub dcbaap: u64,
    pub config: u32,
    /// Command ring.
    pub command_ring: ProducerRing,
    /// Event ring.
    pub event_ring: EventRing,
    /// Per-(slot,endpoint) transfer rings, keyed by `(slot << 8) | ep_addr`.
    transfer_rings: alloc::collections::BTreeMap<u32, ProducerRing>,
    /// Device Context Base Address Array (index = slot id).
    pub dcbaa: Vec<u64>,
    /// Doorbell array (index 0 = command ring, 1.. = device slots).
    pub doorbells: Vec<u32>,
    ports: Vec<Port>,
    next_slot: u8,
}

impl XhciController {
    pub fn new(name: &str, port_count: u8) -> Self {
        let max_slots = 16usize;
        let mut ports = Vec::with_capacity(port_count as usize);
        for _ in 0..port_count {
            ports.push(Port {
                status: PortStatus {
                    connected: false,
                    enabled: false,
                    powered: true,
                    reset: false,
                    speed: UsbSpeed::High,
                },
                device: None,
                slot: None,
            });
        }
        XhciController {
            name: name.to_string(),
            usbcmd: 0,
            usbsts: 1, // HCHalted set until run
            crcr: 0,
            dcbaap: 0,
            config: max_slots as u32,
            command_ring: ProducerRing::new(COMMAND_RING_CAP),
            event_ring: EventRing::new(EVENT_RING_CAP),
            transfer_rings: alloc::collections::BTreeMap::new(),
            dcbaa: alloc::vec![0u64; max_slots + 1],
            doorbells: alloc::vec![0u32; max_slots + 1],
            ports,
            next_slot: 1,
        }
    }

    /// Bring the controller out of halt (sets Run/Stop, clears HCHalted).
    pub fn run(&mut self) {
        self.usbcmd |= 1; // Run/Stop
        self.usbsts &= !1; // clear HCHalted
    }

    /// Attach a virtual device to `port` (1-based), marking it connected.
    pub fn attach(&mut self, port: u8, device: Box<dyn VirtualDevice>) -> Result<(), &'static str> {
        let idx = (port as usize)
            .checked_sub(1)
            .filter(|i| *i < self.ports.len())
            .ok_or("xhci: port out of range")?;
        let speed = device.speed();
        self.ports[idx].device = Some(device);
        self.ports[idx].status.connected = true;
        self.ports[idx].status.speed = speed;
        Ok(())
    }

    fn ring_key(slot: u8, endpoint: u8) -> u32 {
        ((slot as u32) << 8) | endpoint as u32
    }

    fn transfer_ring(&mut self, slot: u8, endpoint: u8) -> &mut ProducerRing {
        self.transfer_rings
            .entry(Self::ring_key(slot, endpoint))
            .or_insert_with(|| ProducerRing::new(TRANSFER_RING_CAP))
    }

    fn port_for_slot(&self, slot: u8) -> Option<usize> {
        self.ports.iter().position(|p| p.slot == Some(slot))
    }

    /// Post a Transfer Event TRB and immediately consume it, returning the
    /// transfer result it encodes. This is the software completion path.
    fn post_and_consume(
        &mut self,
        trb_pointer: u64,
        result: TransferResult,
    ) -> Result<TransferResult, &'static str> {
        let mut event = Trb::new(TrbType::TransferEvent);
        event.parameter = trb_pointer;
        event.status =
            (result.transferred as u32 & 0x00FF_FFFF) | ((result.completion.as_u8() as u32) << 24);
        self.event_ring.post(event);

        let consumed = self
            .event_ring
            .dequeue()
            .ok_or("xhci: event ring empty after post")?;
        if consumed.trb_type() != TrbType::TransferEvent {
            return Err("xhci: unexpected event type");
        }
        let code = consumed.completion_code();
        let completion = match code {
            1 => CompletionCode::Success,
            13 => CompletionCode::ShortPacket,
            6 => CompletionCode::Stall,
            _ => CompletionCode::TransactionError,
        };
        Ok(TransferResult {
            completion,
            transferred: consumed.transfer_length() as usize,
        })
    }
}

impl HostController for XhciController {
    fn name(&self) -> &str {
        &self.name
    }

    fn port_count(&self) -> u8 {
        self.ports.len() as u8
    }

    fn port_status(&self, port: u8) -> Result<PortStatus, &'static str> {
        let idx = (port as usize)
            .checked_sub(1)
            .filter(|i| *i < self.ports.len())
            .ok_or("xhci: port out of range")?;
        Ok(self.ports[idx].status)
    }

    fn reset_port(&mut self, port: u8) -> Result<(), &'static str> {
        let idx = (port as usize)
            .checked_sub(1)
            .filter(|i| *i < self.ports.len())
            .ok_or("xhci: port out of range")?;
        if !self.ports[idx].status.connected {
            return Err("xhci: reset on unconnected port");
        }
        // Model the reset pulse, then leave the port enabled.
        self.ports[idx].status.reset = true;
        self.ports[idx].status.reset = false;
        self.ports[idx].status.enabled = true;
        // Post a Port Status Change event for completeness.
        let mut event = Trb::new(TrbType::PortStatusChange);
        event.parameter = (port as u64) << 24;
        self.event_ring.post(event);
        let _ = self.event_ring.dequeue();
        Ok(())
    }

    fn enable_slot(&mut self, port: u8) -> Result<u8, &'static str> {
        let idx = (port as usize)
            .checked_sub(1)
            .filter(|i| *i < self.ports.len())
            .ok_or("xhci: port out of range")?;
        if !self.ports[idx].status.enabled {
            return Err("xhci: enable_slot before port reset");
        }
        if let Some(existing) = self.ports[idx].slot {
            return Ok(existing);
        }
        let slot = self.next_slot;
        self.next_slot += 1;
        self.ports[idx].slot = Some(slot);
        // Record a device context pointer in the DCBAA.
        if (slot as usize) < self.dcbaa.len() {
            self.dcbaa[slot as usize] = 0xD0_0000 + slot as u64 * 0x1000;
        }
        // Issue an Enable Slot command on the command ring + completion event.
        let cmd = Trb::new(TrbType::EnableSlotCmd);
        let slot_addr = self.command_ring.enqueue(cmd) as u64;
        let mut event = Trb::new(TrbType::CommandCompletion);
        event.parameter = slot_addr;
        event.status = (CompletionCode::Success.as_u8() as u32) << 24;
        event.control |= (slot as u32) << 24; // slot id in event
        self.event_ring.post(event);
        let _ = self.event_ring.dequeue();
        self.doorbells[0] = self.doorbells[0].wrapping_add(1);
        Ok(slot)
    }

    fn control_transfer(
        &mut self,
        slot: u8,
        setup: SetupPacket,
        data: Option<&mut [u8]>,
    ) -> Result<TransferResult, &'static str> {
        // Enqueue Setup / Data / Status stage TRBs on EP0's transfer ring so
        // the cycle bit advances exactly as on hardware.
        let setup_bytes = setup.to_bytes();
        let mut setup_trb = Trb::new(TrbType::SetupStage);
        setup_trb.parameter = u64::from_le_bytes(setup_bytes);
        setup_trb.status = 8;
        let pointer;
        {
            let ring = self.transfer_ring(slot, 0);
            pointer = ring.enqueue(setup_trb) as u64;
            if setup.length != 0 {
                let mut data_trb = Trb::new(TrbType::DataStage);
                data_trb.status = setup.length as u32;
                if setup.is_device_to_host() {
                    data_trb.control |= 1 << 16; // DIR = IN
                }
                ring.enqueue(data_trb);
            }
            ring.enqueue(Trb::new(TrbType::StatusStage));
        }

        // Ring the EP0 doorbell and run the software completion path.
        self.doorbells[slot as usize] = 1;
        let port_idx = self
            .port_for_slot(slot)
            .ok_or("xhci: control transfer to unknown slot")?;
        let result = match self.ports[port_idx].device.as_mut() {
            Some(dev) => dev.control(setup, data),
            None => TransferResult::stall(),
        };
        self.post_and_consume(pointer, result)
    }

    fn bulk_transfer(
        &mut self,
        slot: u8,
        endpoint: u8,
        dir: TransferDirection,
        buffer: &mut [u8],
    ) -> Result<TransferResult, &'static str> {
        let mut trb = Trb::new(TrbType::Normal);
        trb.status = buffer.len() as u32;
        let pointer = {
            let ring = self.transfer_ring(slot, endpoint);
            ring.enqueue(trb) as u64
        };
        self.doorbells[slot as usize] = endpoint as u32;
        let port_idx = self
            .port_for_slot(slot)
            .ok_or("xhci: bulk transfer to unknown slot")?;
        let result = match self.ports[port_idx].device.as_mut() {
            Some(dev) => dev.bulk(endpoint, dir, buffer),
            None => TransferResult::stall(),
        };
        self.post_and_consume(pointer, result)
    }

    fn interrupt_transfer(
        &mut self,
        slot: u8,
        endpoint: u8,
        buffer: &mut [u8],
    ) -> Result<TransferResult, &'static str> {
        let mut trb = Trb::new(TrbType::Normal);
        trb.status = buffer.len() as u32;
        let pointer = {
            let ring = self.transfer_ring(slot, endpoint);
            ring.enqueue(trb) as u64
        };
        self.doorbells[slot as usize] = endpoint as u32;
        let port_idx = self
            .port_for_slot(slot)
            .ok_or("xhci: interrupt transfer to unknown slot")?;
        let result = match self.ports[port_idx].device.as_mut() {
            Some(dev) => dev.interrupt(endpoint, buffer),
            None => TransferResult::stall(),
        };
        self.post_and_consume(pointer, result)
    }
}
