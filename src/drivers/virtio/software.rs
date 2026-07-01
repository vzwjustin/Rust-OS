//! # Software (loopback) VirtIO core
//!
//! A transport-agnostic VirtIO 1.x implementation that runs entirely in
//! kernel memory, with no real hardware, DMA, or MMIO. It is used both as a
//! reference/testbed for the split-virtqueue datapath and as the backing
//! datapath for the `vdpa` and `vhost` subsystems.
//!
//! The split virtqueue here mirrors the on-wire layout (descriptor table,
//! available ring, used ring) but stores everything in plain `Vec`s and models
//! "guest memory" with a bump-allocated arena, so the whole thing is
//! deterministic and testable. It provides the canonical `add_buf` / `get_buf`
//! driver API (chained descriptors, free-list management) and a matching
//! device-side API (`device_pop_avail` / `readable` / `write_in` /
//! `device_push_used`) so a software backend can service the queue exactly the
//! way real hardware would.

use super::{desc_flags, status};
use alloc::boxed::Box;
use alloc::collections::{BTreeMap, VecDeque};
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// A single split-virtqueue descriptor (mirrors the 16-byte on-wire layout).
#[derive(Debug, Clone, Copy, Default)]
pub struct Descriptor {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

/// One segment requested by the driver: either readable by the device
/// (driver -> device) or writable by the device (device -> driver).
pub enum Segment<'a> {
    /// Readable-by-device data (copied into the queue arena on `add_buf`).
    Out(&'a [u8]),
    /// Writable-by-device capacity of the given length (filled by the device).
    In(usize),
}

/// A software split virtqueue with a real free-list and avail/used rings.
pub struct SplitVirtqueue {
    pub size: u16,
    desc: Vec<Descriptor>,
    /// Available ring: driver -> device.
    avail_ring: Vec<u16>,
    avail_idx: u16,
    /// Used ring: device -> driver.
    used_ring: Vec<(u16, u32)>,
    used_idx: u16,
    /// Free descriptor list head and count.
    free_head: u16,
    num_free: u16,
    /// Bump-allocated "guest memory" backing descriptor buffers.
    arena: Vec<u8>,
    /// Per-head chain layout: (arena_addr, len, writable).
    chains: BTreeMap<u16, Vec<(u64, u32, bool)>>,
    /// Last avail index the device has observed.
    device_last_avail: u16,
    /// Last used index the driver has observed.
    driver_last_used: u16,
}

impl SplitVirtqueue {
    /// Build a queue of `size` descriptors (must be a power of two).
    pub fn new(size: u16) -> Result<Self, &'static str> {
        if size == 0 || (size & (size - 1)) != 0 {
            return Err("virtqueue size must be a power of two");
        }
        let mut desc = vec![Descriptor::default(); size as usize];
        // Thread the free list through `next`.
        for i in 0..size {
            desc[i as usize].next = i.wrapping_add(1);
        }
        Ok(SplitVirtqueue {
            size,
            desc,
            avail_ring: vec![0u16; size as usize],
            avail_idx: 0,
            used_ring: vec![(0u16, 0u32); size as usize],
            used_idx: 0,
            free_head: 0,
            num_free: size,
            arena: Vec::new(),
            chains: BTreeMap::new(),
            device_last_avail: 0,
            driver_last_used: 0,
        })
    }

    fn arena_alloc(&mut self, len: usize) -> u64 {
        // Align each allocation to 16 bytes, like a real descriptor buffer.
        let aligned = (self.arena.len() + 15) & !15;
        self.arena.resize(aligned + len, 0);
        aligned as u64
    }

    fn arena_slice(&self, addr: u64, len: u32) -> &[u8] {
        let start = addr as usize;
        &self.arena[start..start + len as usize]
    }

    /// Driver API: enqueue a chain of segments. Returns the head descriptor
    /// index, used as the completion token. Readable data is copied into the
    /// arena now; writable capacity is reserved for the device to fill.
    pub fn add_buf(&mut self, segments: &[Segment]) -> Result<u16, &'static str> {
        if segments.is_empty() {
            return Err("add_buf: empty segment list");
        }
        let n = segments.len() as u16;
        if n > self.num_free {
            return Err("add_buf: not enough free descriptors");
        }

        let head = self.free_head;
        let mut prev: Option<u16> = None;
        let mut chain = Vec::with_capacity(segments.len());

        for (i, seg) in segments.iter().enumerate() {
            let idx = self.free_head;
            self.free_head = self.desc[idx as usize].next;
            self.num_free -= 1;

            let (addr, len, writable) = match seg {
                Segment::Out(data) => {
                    let addr = self.arena_alloc(data.len());
                    let start = addr as usize;
                    self.arena[start..start + data.len()].copy_from_slice(data);
                    (addr, data.len() as u32, false)
                }
                Segment::In(len) => {
                    let addr = self.arena_alloc(*len);
                    (addr, *len as u32, true)
                }
            };

            let mut flags = 0u16;
            if writable {
                flags |= desc_flags::WRITE;
            }
            if i + 1 < segments.len() {
                flags |= desc_flags::NEXT;
            }
            self.desc[idx as usize] = Descriptor {
                addr,
                len,
                flags,
                next: 0,
            };
            if let Some(p) = prev {
                self.desc[p as usize].next = idx;
            }
            prev = Some(idx);
            chain.push((addr, len, writable));
        }

        self.chains.insert(head, chain);

        // Publish on the available ring.
        let ring_pos = (self.avail_idx % self.size) as usize;
        self.avail_ring[ring_pos] = head;
        self.avail_idx = self.avail_idx.wrapping_add(1);
        Ok(head)
    }

    /// Driver API: reap one completed chain. Returns `(token, written_len)`.
    /// The chain's descriptors are returned to the free list.
    pub fn get_buf(&mut self) -> Option<(u16, u32)> {
        if self.driver_last_used == self.used_idx {
            return None;
        }
        let ring_pos = (self.driver_last_used % self.size) as usize;
        let (head, len) = self.used_ring[ring_pos];
        self.driver_last_used = self.driver_last_used.wrapping_add(1);
        self.free_chain(head);
        Some((head, len))
    }

    /// Driver API: copy the device-written (writable) bytes for `head` into
    /// `out`, returning the number of bytes copied. Must be called before the
    /// matching `get_buf` frees the chain (or use the token right after).
    pub fn read_in(&self, head: u16, out: &mut [u8]) -> usize {
        let mut copied = 0;
        if let Some(chain) = self.chains.get(&head) {
            for &(addr, len, writable) in chain {
                if !writable {
                    continue;
                }
                let src = self.arena_slice(addr, len);
                let n = src.len().min(out.len() - copied);
                out[copied..copied + n].copy_from_slice(&src[..n]);
                copied += n;
                if copied == out.len() {
                    break;
                }
            }
        }
        copied
    }

    fn free_chain(&mut self, head: u16) {
        let chain = match self.chains.remove(&head) {
            Some(c) => c,
            None => return,
        };
        // Walk the descriptor chain and return each link to the free list.
        let mut idx = head;
        for _ in 0..chain.len() {
            let next = self.desc[idx as usize].next;
            let has_next = self.desc[idx as usize].flags & desc_flags::NEXT != 0;
            self.desc[idx as usize] = Descriptor {
                next: self.free_head,
                ..Default::default()
            };
            self.free_head = idx;
            self.num_free += 1;
            if !has_next {
                break;
            }
            idx = next;
        }
    }

    /// Number of free descriptors.
    pub fn free_count(&self) -> u16 {
        self.num_free
    }

    /// Number of completed-but-unreaped entries on the used ring.
    pub fn get_used_count(&self) -> u16 {
        self.used_idx.wrapping_sub(self.driver_last_used)
    }

    // ---- device-side API ------------------------------------------------

    /// Device API: pop the next head the driver made available, if any.
    pub fn device_pop_avail(&mut self) -> Option<u16> {
        if self.device_last_avail == self.avail_idx {
            return None;
        }
        let ring_pos = (self.device_last_avail % self.size) as usize;
        let head = self.avail_ring[ring_pos];
        self.device_last_avail = self.device_last_avail.wrapping_add(1);
        Some(head)
    }

    /// Device API: gather all readable (driver -> device) bytes of a chain.
    pub fn readable(&self, head: u16) -> Vec<u8> {
        let mut out = Vec::new();
        if let Some(chain) = self.chains.get(&head) {
            for &(addr, len, writable) in chain {
                if writable {
                    continue;
                }
                out.extend_from_slice(self.arena_slice(addr, len));
            }
        }
        out
    }

    /// Device API: total writable capacity of a chain.
    pub fn writable_capacity(&self, head: u16) -> usize {
        self.chains
            .get(&head)
            .map(|c| c.iter().filter(|s| s.2).map(|s| s.1 as usize).sum())
            .unwrap_or(0)
    }

    /// Device API: write `data` into the writable descriptors of a chain,
    /// returning the number of bytes written.
    pub fn write_in(&mut self, head: u16, data: &[u8]) -> u32 {
        let regions: Vec<(u64, u32)> = match self.chains.get(&head) {
            Some(chain) => chain
                .iter()
                .filter(|s| s.2)
                .map(|&(a, l, _)| (a, l))
                .collect(),
            None => return 0,
        };
        let mut written = 0usize;
        for (addr, len) in regions {
            if written >= data.len() {
                break;
            }
            let n = (len as usize).min(data.len() - written);
            let start = addr as usize;
            self.arena[start..start + n].copy_from_slice(&data[written..written + n]);
            written += n;
        }
        written as u32
    }

    /// Device API: mark a chain complete on the used ring.
    pub fn device_push_used(&mut self, head: u16, written_len: u32) {
        let ring_pos = (self.used_idx % self.size) as usize;
        self.used_ring[ring_pos] = (head, written_len);
        self.used_idx = self.used_idx.wrapping_add(1);
    }
}

/// A device-side backend that services a virtqueue when kicked.
pub trait VirtioBackend: Send + Sync {
    /// Service all currently-available chains on `vq`.
    fn service(&mut self, vq: &mut SplitVirtqueue);
}

/// Transport-agnostic VirtIO device: feature negotiation + queue ownership +
/// a backend that plays the device role. Mirrors the real status handshake.
pub struct VirtioDevice {
    pub name: String,
    pub device_features: u64,
    pub driver_features: u64,
    pub status: u8,
    pub queues: Vec<SplitVirtqueue>,
    backend: Box<dyn VirtioBackend>,
}

impl VirtioDevice {
    pub fn new(name: &str, device_features: u64, backend: Box<dyn VirtioBackend>) -> Self {
        VirtioDevice {
            name: String::from(name),
            device_features,
            driver_features: 0,
            status: status::RESET,
            queues: Vec::new(),
            backend,
        }
    }

    pub fn add_queue(&mut self, size: u16) -> Result<usize, &'static str> {
        self.queues.push(SplitVirtqueue::new(size)?);
        Ok(self.queues.len() - 1)
    }

    /// Run the standard status/feature handshake against `requested` features.
    /// Returns the negotiated feature bitmap on success.
    pub fn negotiate(&mut self, requested: u64) -> Result<u64, &'static str> {
        // 1. Reset.
        self.status = status::RESET;
        // 2. ACKNOWLEDGE that we found the device.
        self.status |= status::ACKNOWLEDGE;
        // 3. We know how to drive it.
        self.status |= status::DRIVER;
        // 4. Negotiate the 64-bit feature bitmap.
        let negotiated = self.device_features & requested;
        self.driver_features = negotiated;
        // 5. FEATURES_OK — device would clear this if it disagreed.
        self.status |= status::FEATURES_OK;
        if self.status & status::FEATURES_OK == 0 {
            self.status |= status::FAILED;
            return Err("virtio: FEATURES_OK rejected");
        }
        // 6. DRIVER_OK — device is live.
        self.status |= status::DRIVER_OK;
        Ok(negotiated)
    }

    pub fn is_live(&self) -> bool {
        self.status & status::DRIVER_OK != 0
    }

    /// Kick the device: let the backend service the given queue.
    pub fn kick(&mut self, queue: usize) -> Result<(), &'static str> {
        let vq = self
            .queues
            .get_mut(queue)
            .ok_or("virtio: bad queue index")?;
        self.backend.service(vq);
        Ok(())
    }
}

// ── virtio-net loopback ──────────────────────────────────────────────────

/// virtio-net header prepended to every frame.
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct NetHdr {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
    pub num_buffers: u16,
}

pub const NET_HDR_LEN: usize = 12;
pub const VIRTIO_NET_F_MAC: u64 = 1 << 5;
pub const VIRTIO_NET_F_STATUS: u64 = 1 << 16;

/// A loopback net backend: frames written to the TX queue are queued, and the
/// RX queue is filled from that FIFO. A single backend instance is shared by
/// wiring it through the device's `kick` on each queue.
pub struct NetLoopback {
    fifo: VecDeque<Vec<u8>>,
}

impl NetLoopback {
    pub fn new() -> Self {
        NetLoopback {
            fifo: VecDeque::new(),
        }
    }
}

impl VirtioBackend for NetLoopback {
    fn service(&mut self, vq: &mut SplitVirtqueue) {
        // This backend services whichever queue it is kicked on. A chain with
        // only readable data is a TX submission; a chain with writable space
        // is an RX buffer to be filled.
        while let Some(head) = vq.device_pop_avail() {
            if vq.writable_capacity(head) > 0 {
                // RX path: fill from the FIFO if a frame is waiting.
                if let Some(frame) = self.fifo.pop_front() {
                    let mut payload = Vec::with_capacity(NET_HDR_LEN + frame.len());
                    payload.extend_from_slice(&[0u8; NET_HDR_LEN]);
                    payload.extend_from_slice(&frame);
                    let written = vq.write_in(head, &payload);
                    vq.device_push_used(head, written);
                } else {
                    // No frame yet: leave the buffer pending by completing it
                    // with zero length (driver will re-submit).
                    vq.device_push_used(head, 0);
                }
            } else {
                // TX path: strip the net header and queue the frame.
                let buf = vq.readable(head);
                if buf.len() > NET_HDR_LEN {
                    self.fifo.push_back(buf[NET_HDR_LEN..].to_vec());
                }
                vq.device_push_used(head, 0);
            }
        }
    }
}

// ── virtio-blk software disk ──────────────────────────────────────────────

pub const SECTOR_SIZE: usize = 512;
pub const VIRTIO_BLK_T_IN: u32 = 0;
pub const VIRTIO_BLK_T_OUT: u32 = 1;
pub const VIRTIO_BLK_T_FLUSH: u32 = 4;
pub const VIRTIO_BLK_S_OK: u8 = 0;
pub const VIRTIO_BLK_S_IOERR: u8 = 1;
pub const VIRTIO_BLK_S_UNSUPP: u8 = 2;
pub const VIRTIO_BLK_F_FLUSH: u64 = 1 << 9;

/// virtio-blk request header (16 bytes on the wire).
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct BlkReqHdr {
    pub req_type: u32,
    pub reserved: u32,
    pub sector: u64,
}

/// A software virtio-blk backend backed by an in-memory disk.
pub struct BlkLoopback {
    disk: Vec<u8>,
}

impl BlkLoopback {
    pub fn new(sectors: usize) -> Self {
        BlkLoopback {
            disk: vec![0u8; sectors * SECTOR_SIZE],
        }
    }

    pub fn capacity_sectors(&self) -> u64 {
        (self.disk.len() / SECTOR_SIZE) as u64
    }
}

impl VirtioBackend for BlkLoopback {
    fn service(&mut self, vq: &mut SplitVirtqueue) {
        while let Some(head) = vq.device_pop_avail() {
            let readable = vq.readable(head);
            if readable.len() < 16 {
                vq.device_push_used(head, 0);
                continue;
            }
            let req_type = u32::from_le_bytes([readable[0], readable[1], readable[2], readable[3]]);
            let sector = u64::from_le_bytes([
                readable[8],
                readable[9],
                readable[10],
                readable[11],
                readable[12],
                readable[13],
                readable[14],
                readable[15],
            ]);
            // Data that follows the header (for OUT requests).
            let in_data = &readable[16..];

            // Writable region = data buffer (for IN) + 1 status byte.
            let writable_cap = vq.writable_capacity(head);
            let mut response: Vec<u8> = Vec::new();

            let mut status_byte = VIRTIO_BLK_S_OK;
            match req_type {
                VIRTIO_BLK_T_IN => {
                    let data_len = writable_cap.saturating_sub(1);
                    let off = sector as usize * SECTOR_SIZE;
                    if off + data_len > self.disk.len() {
                        status_byte = VIRTIO_BLK_S_IOERR;
                        response.resize(data_len, 0);
                    } else {
                        response.extend_from_slice(&self.disk[off..off + data_len]);
                    }
                }
                VIRTIO_BLK_T_OUT => {
                    let off = sector as usize * SECTOR_SIZE;
                    if off + in_data.len() > self.disk.len() {
                        status_byte = VIRTIO_BLK_S_IOERR;
                    } else {
                        self.disk[off..off + in_data.len()].copy_from_slice(in_data);
                    }
                }
                VIRTIO_BLK_T_FLUSH => {}
                _ => status_byte = VIRTIO_BLK_S_UNSUPP,
            }
            response.push(status_byte);
            let written = vq.write_in(head, &response);
            vq.device_push_used(head, written);
        }
    }
}

// ── self-test exercising the full datapath ────────────────────────────────

/// Build a software virtio-blk device, write a sector, read it back, and
/// verify the round-trip. Returns the negotiated feature bitmap on success.
pub fn selftest_blk() -> Result<u64, &'static str> {
    let mut dev = VirtioDevice::new(
        "sw-virtio-blk",
        VIRTIO_BLK_F_FLUSH,
        Box::new(BlkLoopback::new(64)),
    );
    let negotiated = dev.negotiate(VIRTIO_BLK_F_FLUSH)?;
    if !dev.is_live() {
        return Err("blk selftest: device not live");
    }
    let q = dev.add_queue(16)?;

    // Write one sector full of 0xAB at sector 3.
    let hdr = BlkReqHdr {
        req_type: VIRTIO_BLK_T_OUT,
        reserved: 0,
        sector: 3,
    };
    let hdr_bytes = unsafe {
        core::slice::from_raw_parts(
            &hdr as *const BlkReqHdr as *const u8,
            core::mem::size_of::<BlkReqHdr>(),
        )
    };
    let data = [0xABu8; SECTOR_SIZE];
    let whead =
        dev.queues[q].add_buf(&[Segment::Out(hdr_bytes), Segment::Out(&data), Segment::In(1)])?;
    dev.kick(q)?;
    let mut wstatus = [0xFFu8; 1];
    dev.queues[q].read_in(whead, &mut wstatus);
    dev.queues[q].get_buf().ok_or("blk write not completed")?;
    if wstatus[0] != VIRTIO_BLK_S_OK {
        return Err("blk selftest: write status not OK");
    }

    // Read the sector back.
    let rhdr = BlkReqHdr {
        req_type: VIRTIO_BLK_T_IN,
        reserved: 0,
        sector: 3,
    };
    let rhdr_bytes = unsafe {
        core::slice::from_raw_parts(
            &rhdr as *const BlkReqHdr as *const u8,
            core::mem::size_of::<BlkReqHdr>(),
        )
    };
    let head = dev.queues[q].add_buf(&[
        Segment::Out(rhdr_bytes),
        Segment::In(SECTOR_SIZE),
        Segment::In(1),
    ])?;
    dev.kick(q)?;
    let mut readback = [0u8; SECTOR_SIZE + 1];
    let copied = dev.queues[q].read_in(head, &mut readback);
    dev.queues[q].get_buf().ok_or("blk read not completed")?;
    if copied < SECTOR_SIZE + 1 {
        return Err("blk selftest: short read");
    }
    if readback[SECTOR_SIZE] != VIRTIO_BLK_S_OK {
        return Err("blk selftest: bad status byte");
    }
    if readback[..SECTOR_SIZE].iter().any(|&b| b != 0xAB) {
        return Err("blk selftest: data mismatch");
    }
    Ok(negotiated)
}

/// Build a software virtio-net loopback, transmit a frame, and receive it.
pub fn selftest_net() -> Result<u64, &'static str> {
    let mut dev = VirtioDevice::new(
        "sw-virtio-net",
        VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS,
        Box::new(NetLoopback::new()),
    );
    let negotiated = dev.negotiate(VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS)?;
    let rxq = dev.add_queue(16)?;
    let txq = dev.add_queue(16)?;

    let frame: [u8; 8] = [0xDE, 0xAD, 0xBE, 0xEF, 0x01, 0x02, 0x03, 0x04];

    // TX: net header + frame, readable only.
    let mut tx = Vec::new();
    tx.extend_from_slice(&[0u8; NET_HDR_LEN]);
    tx.extend_from_slice(&frame);
    dev.queues[txq].add_buf(&[Segment::Out(&tx)])?;
    dev.kick(txq)?;
    dev.queues[txq].get_buf().ok_or("net tx not completed")?;

    // RX: provide a writable buffer, then kick to fill it from the loopback.
    let rx_head = dev.queues[rxq].add_buf(&[Segment::In(NET_HDR_LEN + 64)])?;
    dev.kick(rxq)?;
    let mut rx = [0u8; NET_HDR_LEN + 64];
    let copied = dev.queues[rxq].read_in(rx_head, &mut rx);
    let (_t, len) = dev.queues[rxq].get_buf().ok_or("net rx not completed")?;
    if len == 0 || copied < NET_HDR_LEN + frame.len() {
        return Err("net selftest: short receive");
    }
    if rx[NET_HDR_LEN..NET_HDR_LEN + frame.len()] != frame {
        return Err("net selftest: frame mismatch");
    }
    Ok(negotiated)
}
