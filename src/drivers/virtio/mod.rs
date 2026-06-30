//! # VirtIO Driver Framework
//!
//! Implements the VirtIO paravirtualized device specification for QEMU/KVM.
//! Supports virtio-net (0x1000), virtio-blk (0x1001), and other virtio devices
//! via the PCI transport layer.
//!
//! The driver follows the VirtIO 1.0+ specification using PCI capabilities
//! for device configuration, notification, and ISR access.

pub mod blk;
pub mod console;
pub mod net;
pub mod rng;
pub mod software;

use crate::pci::{list_devices, PciDevice};
use alloc::vec::Vec;

/// VirtIO PCI vendor ID
pub const VIRTIO_VENDOR_ID: u16 = 0x1AF4;

/// VirtIO legacy device IDs (0x1000-0x103F)
pub const VIRTIO_NET_DEVICE_ID: u16 = 0x1000;
pub const VIRTIO_BLK_DEVICE_ID: u16 = 0x1001;
pub const VIRTIO_CONSOLE_DEVICE_ID: u16 = 0x1003;
pub const VIRTIO_SCSI_DEVICE_ID: u16 = 0x1004;
pub const VIRTIO_RNG_DEVICE_ID: u16 = 0x1005;
pub const VIRTIO_9P_DEVICE_ID: u16 = 0x1009;

/// VirtIO modern device ID base (0x1040+)
pub const VIRTIO_MODERN_DEVICE_ID_BASE: u16 = 0x1040;

/// VirtIO PCI capability IDs
const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
const VIRTIO_PCI_CAP_ISR_CFG: u8 = 3;
const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;
const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;

/// VirtIO PCI capability structure (parsed from PCI config space)
#[derive(Debug, Clone, Copy)]
pub struct VirtioPciCap {
    pub cap_type: u8,
    pub bar: u8,
    pub offset: u32,
    pub length: u32,
}

/// VirtIO device types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioDeviceType {
    Network,
    Block,
    Console,
    Scsi,
    Rng,
    NineP,
    Unknown(u16),
}

impl VirtioDeviceType {
    fn from_device_id(device_id: u16) -> Self {
        match device_id {
            VIRTIO_NET_DEVICE_ID => VirtioDeviceType::Network,
            VIRTIO_BLK_DEVICE_ID => VirtioDeviceType::Block,
            VIRTIO_CONSOLE_DEVICE_ID => VirtioDeviceType::Console,
            VIRTIO_SCSI_DEVICE_ID => VirtioDeviceType::Scsi,
            VIRTIO_RNG_DEVICE_ID => VirtioDeviceType::Rng,
            VIRTIO_9P_DEVICE_ID => VirtioDeviceType::NineP,
            id if id >= VIRTIO_MODERN_DEVICE_ID_BASE => {
                let subtype = id - VIRTIO_MODERN_DEVICE_ID_BASE;
                match subtype {
                    1 => VirtioDeviceType::Network,
                    2 => VirtioDeviceType::Block,
                    3 => VirtioDeviceType::Console,
                    4 => VirtioDeviceType::Scsi,
                    5 => VirtioDeviceType::Rng,
                    9 => VirtioDeviceType::NineP,
                    _ => VirtioDeviceType::Unknown(id),
                }
            }
            id => VirtioDeviceType::Unknown(id),
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            VirtioDeviceType::Network => "virtio-net",
            VirtioDeviceType::Block => "virtio-blk",
            VirtioDeviceType::Console => "virtio-console",
            VirtioDeviceType::Scsi => "virtio-scsi",
            VirtioDeviceType::Rng => "virtio-rng",
            VirtioDeviceType::NineP => "virtio-9p",
            VirtioDeviceType::Unknown(_) => "virtio-unknown",
        }
    }
}

/// VirtIO common configuration registers (MMIO-mapped)
#[repr(C)]
pub struct VirtioCommonConfig {
    pub device_feature_select: u32,
    pub device_feature: u32,
    pub driver_feature_select: u32,
    pub driver_feature: u32,
    pub config_msix_vector: u16,
    pub num_queues: u16,
    pub device_status: u8,
    pub config_generation: u8,
    pub queue_select: u16,
    pub queue_size: u16,
    pub queue_msix_vector: u16,
    pub queue_enable: u16,
    pub queue_notify_off: u16,
    pub queue_desc_lo: u32,
    pub queue_desc_hi: u32,
    pub queue_driver_lo: u32,
    pub queue_driver_hi: u32,
    pub queue_device_lo: u32,
    pub queue_device_hi: u32,
}

/// VirtIO device status bits
pub mod status {
    pub const RESET: u8 = 0;
    pub const ACKNOWLEDGE: u8 = 1;
    pub const DRIVER: u8 = 2;
    pub const DRIVER_OK: u8 = 4;
    pub const FEATURES_OK: u8 = 8;
    pub const DEVICE_NEEDS_RESET: u8 = 64;
    pub const FAILED: u8 = 128;
}

/// Virtqueue descriptor (16 bytes)
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct VirtqDesc {
    pub addr: u64,
    pub len: u32,
    pub flags: u16,
    pub next: u16,
}

/// Virtqueue available ring
#[repr(C, align(2))]
pub struct VirtqAvail {
    pub flags: u16,
    pub idx: u16,
    pub ring: [u16; 0], // Flexible array — use raw pointer
}

/// Virtqueue used ring element
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VirtqUsedElem {
    pub id: u32,
    pub len: u32,
}

/// Virtqueue used ring
#[repr(C, align(4))]
pub struct VirtqUsed {
    pub flags: u16,
    pub idx: u16,
    pub ring: [VirtqUsedElem; 0], // Flexible array
}

/// Descriptor flags
pub mod desc_flags {
    pub const NEXT: u16 = 1;
    pub const WRITE: u16 = 2;
    pub const INDIRECT: u16 = 4;
}

/// A configured virtqueue
pub struct VirtQueue {
    pub size: u16,
    pub last_avail_idx: u16,
    pub last_used_idx: u16,
    /// Descriptor table base address (physical)
    pub desc_phys: u64,
    /// Available ring base address (physical)
    pub avail_phys: u64,
    /// Used ring base address (physical)
    pub used_phys: u64,
    /// Notify offset for this queue
    pub notify_off: u16,
    /// Virtual address of descriptor table
    pub desc_virt: *mut VirtqDesc,
    /// Virtual address of available ring
    pub avail_virt: *mut u16,
    /// Virtual address of used ring
    pub used_virt: *mut VirtqUsedElem,
}

unsafe impl Send for VirtQueue {}
unsafe impl Sync for VirtQueue {}

impl VirtQueue {
    /// Allocate and set up a virtqueue of the given size.
    pub fn new(size: u16, notify_off: u16) -> Result<Self, &'static str> {
        if size == 0 || (size & (size - 1)) != 0 {
            return Err("virtqueue size must be power of 2");
        }

        let s = size as usize;
        let desc_size = s * 16;
        let avail_size = 6 + s * 2;
        let used_size = 6 + s * 8;

        let total = desc_size + avail_size + used_size + 4096;
        let layout =
            alloc::alloc::Layout::from_size_align(total, 4096).map_err(|_| "layout error")?;

        let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            return Err("virtqueue allocation failed");
        }

        let desc_virt = ptr as *mut VirtqDesc;
        let avail_virt = unsafe { ptr.add(desc_size) } as *mut u16;
        let used_virt = unsafe { ptr.add(desc_size + avail_size) } as *mut VirtqUsedElem;

        let desc_phys = virt_to_phys(desc_virt as usize);
        let avail_phys = virt_to_phys(avail_virt as usize);
        let used_phys = virt_to_phys(used_virt as usize);

        // Initialize available ring index
        unsafe {
            *avail_virt = 0; // flags
            *avail_virt.add(1) = 0; // idx
        }

        Ok(VirtQueue {
            size,
            last_avail_idx: 0,
            last_used_idx: 0,
            desc_phys,
            avail_phys,
            used_phys,
            notify_off,
            desc_virt,
            avail_virt,
            used_virt,
        })
    }

    /// Submit a buffer to the available ring
    pub fn submit(&mut self, desc_idx: u16) {
        let avail_idx = unsafe { *self.avail_virt.add(1) };
        let ring_pos = (avail_idx % self.size) as usize;
        unsafe {
            *self.avail_virt.add(2 + ring_pos) = desc_idx;
            *self.avail_virt.add(1) = avail_idx.wrapping_add(1);
        }
    }

    /// Check if there are completed buffers in the used ring
    pub fn has_used(&self) -> bool {
        let used_idx = unsafe {
            let used_ring = self.used_virt as *const u16;
            *used_ring.add(1)
        };
        used_idx != self.last_used_idx
    }

    /// Pop a completed buffer from the used ring
    pub fn pop_used(&mut self) -> Option<(u32, u32)> {
        if !self.has_used() {
            return None;
        }
        let ring_pos = (self.last_used_idx % self.size) as usize;
        let elem = unsafe {
            let used_ring = self.used_virt as *const u8;
            let elem_ptr = used_ring.add(6) as *const VirtqUsedElem;
            *elem_ptr.add(ring_pos)
        };
        self.last_used_idx = self.last_used_idx.wrapping_add(1);
        Some((elem.id, elem.len))
    }

    /// Get a free descriptor index
    pub fn alloc_desc(&self) -> Option<u16> {
        // Simple linear scan for a free descriptor (addr == 0 means free)
        for i in 0..self.size {
            let desc = unsafe { &*self.desc_virt.add(i as usize) };
            if desc.addr == 0 && desc.len == 0 {
                return Some(i);
            }
        }
        None
    }

    /// Set up a descriptor
    pub fn set_desc(&self, idx: u16, addr: u64, len: u32, flags: u16, next: u16) {
        unsafe {
            let desc = &mut *self.desc_virt.add(idx as usize);
            desc.addr = addr;
            desc.len = len;
            desc.flags = flags;
            desc.next = next;
        }
    }

    /// Free a descriptor
    pub fn free_desc(&self, idx: u16) {
        unsafe {
            let desc = &mut *self.desc_virt.add(idx as usize);
            desc.addr = 0;
            desc.len = 0;
            desc.flags = 0;
            desc.next = 0;
        }
    }
}

/// Convert virtual address to physical address
fn virt_to_phys(virt: usize) -> u64 {
    // In RustOS, the physical memory offset is identity-mapped for MMIO regions.
    // For heap allocations, we need to convert via the physical memory offset.
    // For now, use the direct mapping (heap is in the higher half, phys offset is 0).
    // This works because QEMU's virtio uses DMA addresses that match physical memory.
    virt as u64
}

/// Parsed VirtIO PCI capabilities for a device
#[derive(Debug, Clone, Default)]
pub struct VirtioCaps {
    pub common: Option<VirtioPciCap>,
    pub isr: Option<VirtioPciCap>,
    pub device: Option<VirtioPciCap>,
    pub notify: Option<VirtioPciCap>,
    pub notify_off_multiplier: u32,
}

/// Parse VirtIO PCI capabilities from a PCI device's capability list
fn parse_virtio_caps(dev: &PciDevice) -> VirtioCaps {
    let mut caps = VirtioCaps::default();

    // The PCI scanner already found capabilities, but we need to parse
    // the virtio-specific capability structure.
    // VirtIO capabilities are in the PCI capability list at cap_ptr offsets.
    // Each virtio cap has: cap_id(1), cap_next(1), cap_len(1), cfg_type(1), bar(1), padding(3), offset(4), length(4)

    let scanner = crate::pci::get_pci_scanner();
    let scanner = scanner.lock();

    let mut cap_ptr = scanner.read_config_byte(dev.bus, dev.device, dev.function, 0x34) & 0xFC;

    while cap_ptr != 0 && cap_ptr != 0xFF {
        let cap_id = scanner.read_config_byte(dev.bus, dev.device, dev.function, cap_ptr);
        let cap_len = scanner.read_config_byte(dev.bus, dev.device, dev.function, cap_ptr + 2);

        if cap_id == 0x09 {
            // Vendor-specific capability — this is where VirtIO caps live
            let cfg_type = scanner.read_config_byte(dev.bus, dev.device, dev.function, cap_ptr + 3);
            let bar = scanner.read_config_byte(dev.bus, dev.device, dev.function, cap_ptr + 4);
            let offset = scanner.read_config_dword(dev.bus, dev.device, dev.function, cap_ptr + 8);
            let length = scanner.read_config_dword(dev.bus, dev.device, dev.function, cap_ptr + 12);

            let cap = VirtioPciCap {
                cap_type: cfg_type,
                bar,
                offset,
                length,
            };

            match cfg_type {
                VIRTIO_PCI_CAP_COMMON_CFG => caps.common = Some(cap),
                VIRTIO_PCI_CAP_ISR_CFG => caps.isr = Some(cap),
                VIRTIO_PCI_CAP_DEVICE_CFG => caps.device = Some(cap),
                VIRTIO_PCI_CAP_NOTIFY_CFG => {
                    caps.notify = Some(cap);
                    // Notify offset multiplier is at cap_ptr + 16
                    if cap_len >= 20 {
                        caps.notify_off_multiplier = scanner.read_config_dword(
                            dev.bus,
                            dev.device,
                            dev.function,
                            cap_ptr + 16,
                        );
                    }
                }
                _ => {}
            }
        }

        cap_ptr = scanner.read_config_byte(dev.bus, dev.device, dev.function, cap_ptr + 1) & 0xFC;
    }

    caps
}

/// Get the MMIO base address for a BAR
fn get_bar_address(dev: &PciDevice, bar_idx: u8) -> Option<u64> {
    let bar = dev.bars.get(bar_idx as usize)?;
    if *bar == 0 {
        return None;
    }
    // Check if it's MMIO (bit 0 = 0) or I/O (bit 0 = 1)
    if *bar & 0x1 == 0 {
        Some((*bar & 0xFFFF_FFF0) as u64)
    } else {
        // I/O port — return as u64 for port I/O
        Some((*bar & 0xFFFF_FFFC) as u64)
    }
}

/// Read from a VirtIO capability's BAR-mapped region
fn read_cap_mmio(base: u64, offset: u32) -> u32 {
    unsafe { core::ptr::read_volatile((base + offset as u64) as *const u32) }
}

/// Write to a VirtIO capability's BAR-mapped region
fn write_cap_mmio(base: u64, offset: u32, value: u32) {
    unsafe {
        core::ptr::write_volatile((base + offset as u64) as *mut u32, value);
    }
}

/// Read 8-bit from cap MMIO
fn read_cap_mmio8(base: u64, offset: u32) -> u8 {
    unsafe { core::ptr::read_volatile((base + offset as u64) as *const u8) }
}

/// Write 8-bit to cap MMIO
fn write_cap_mmio8(base: u64, offset: u32, value: u8) {
    unsafe {
        core::ptr::write_volatile((base + offset as u64) as *mut u8, value);
    }
}

/// Read 16-bit from cap MMIO
fn read_cap_mmio16(base: u64, offset: u32) -> u16 {
    unsafe { core::ptr::read_volatile((base + offset as u64) as *const u16) }
}

/// Write 16-bit to cap MMIO
fn write_cap_mmio16(base: u64, offset: u32, value: u16) {
    unsafe {
        core::ptr::write_volatile((base + offset as u64) as *mut u16, value);
    }
}

/// A handle to a VirtIO device's transport (PCI BARs, capabilities)
pub struct VirtioTransport {
    pub device_type: VirtioDeviceType,
    pub pci_device: PciDevice,
    pub caps: VirtioCaps,
    pub common_base: u64,
    pub isr_base: u64,
    pub device_base: u64,
    pub notify_base: u64,
}

unsafe impl Send for VirtioTransport {}
unsafe impl Sync for VirtioTransport {}

impl VirtioTransport {
    /// Create a transport handle from a PCI device
    pub fn from_pci(dev: &PciDevice) -> Result<Self, &'static str> {
        let device_type = VirtioDeviceType::from_device_id(dev.device_id);
        let caps = parse_virtio_caps(dev);

        // We need at least the common config capability
        if caps.common.is_none() {
            return Err("virtio: missing common config capability");
        }

        let common_cap = caps.common.unwrap();
        let common_base = get_bar_address(dev, common_cap.bar)
            .ok_or("virtio: invalid common BAR")?
            + common_cap.offset as u64;

        let isr_base = caps
            .isr
            .and_then(|c| get_bar_address(dev, c.bar).map(|b| b + c.offset as u64))
            .unwrap_or(0);

        let device_base = caps
            .device
            .and_then(|c| get_bar_address(dev, c.bar).map(|b| b + c.offset as u64))
            .unwrap_or(0);

        let notify_base = caps
            .notify
            .and_then(|c| get_bar_address(dev, c.bar).map(|b| b + c.offset as u64))
            .unwrap_or(0);

        Ok(VirtioTransport {
            device_type,
            pci_device: dev.clone(),
            caps,
            common_base,
            isr_base,
            device_base,
            notify_base,
        })
    }

    /// Read device features
    pub fn read_device_features(&self) -> u64 {
        // Select feature word 0
        write_cap_mmio(self.common_base, 0, 0);
        let lo = read_cap_mmio(self.common_base, 4);
        // Select feature word 1
        write_cap_mmio(self.common_base, 0, 1);
        let hi = read_cap_mmio(self.common_base, 4);
        ((hi as u64) << 32) | (lo as u64)
    }

    /// Write driver features (negotiated)
    pub fn write_driver_features(&self, features: u64) {
        write_cap_mmio(self.common_base, 8, 0); // select word 0
        write_cap_mmio(self.common_base, 12, features as u32);
        write_cap_mmio(self.common_base, 8, 1); // select word 1
        write_cap_mmio(self.common_base, 12, (features >> 32) as u32);
    }

    /// Read device status
    pub fn read_status(&self) -> u8 {
        read_cap_mmio8(self.common_base, 20)
    }

    /// Write device status
    pub fn write_status(&self, status: u8) {
        write_cap_mmio8(self.common_base, 20, status);
    }

    /// Select a virtqueue and get its size
    pub fn select_queue(&self, queue_idx: u16) -> u16 {
        write_cap_mmio16(self.common_base, 16, queue_idx);
        read_cap_mmio16(self.common_base, 18)
    }

    /// Configure a virtqueue's memory addresses in the device
    pub fn setup_queue(&self, queue: &VirtQueue) {
        // Write descriptor address
        write_cap_mmio(self.common_base, 32, queue.desc_phys as u32);
        write_cap_mmio(self.common_base, 36, (queue.desc_phys >> 32) as u32);
        // Write available ring address
        write_cap_mmio(self.common_base, 40, queue.avail_phys as u32);
        write_cap_mmio(self.common_base, 44, (queue.avail_phys >> 32) as u32);
        // Write used ring address
        write_cap_mmio(self.common_base, 48, queue.used_phys as u32);
        write_cap_mmio(self.common_base, 52, (queue.used_phys >> 32) as u32);

        // Enable the queue
        write_cap_mmio16(self.common_base, 28, 1);
    }

    /// Notify the device that a queue has pending buffers
    pub fn notify(&self, queue: &VirtQueue) {
        let notify_off = queue.notify_off as u64 * self.caps.notify_off_multiplier as u64;
        unsafe {
            core::ptr::write_volatile((self.notify_base + notify_off) as *mut u16, 0);
        }
    }

    /// Read ISR status (reading acknowledges the interrupt)
    pub fn read_isr(&self) -> u8 {
        if self.isr_base != 0 {
            unsafe { core::ptr::read_volatile(self.isr_base as *const u8) }
        } else {
            0
        }
    }

    /// Read device-specific config at the given offset
    pub fn read_device_config8(&self, offset: u32) -> u8 {
        if self.device_base != 0 {
            unsafe { core::ptr::read_volatile((self.device_base + offset as u64) as *const u8) }
        } else {
            0
        }
    }

    /// Read device-specific config (32-bit) at the given offset
    pub fn read_device_config32(&self, offset: u32) -> u32 {
        if self.device_base != 0 {
            unsafe { core::ptr::read_volatile((self.device_base + offset as u64) as *const u32) }
        } else {
            0
        }
    }

    /// Full device initialization sequence:
    /// 1. Reset
    /// 2. Acknowledge + set DRIVER
    /// 3. Negotiate features
    /// 4. Set FEATURES_OK
    /// 5. Set DRIVER_OK
    pub fn init_device(&self, driver_features: u64) -> Result<(), &'static str> {
        // 1. Reset device
        self.write_status(status::RESET);

        // 2. Acknowledge and set DRIVER bit
        self.write_status(status::ACKNOWLEDGE | status::DRIVER);

        // 3. Negotiate features
        let device_features = self.read_device_features();
        let negotiated = device_features & driver_features;
        self.write_driver_features(negotiated);

        // 4. Set FEATURES_OK and verify
        self.write_status(status::ACKNOWLEDGE | status::DRIVER | status::FEATURES_OK);
        let status = self.read_status();
        if (status & status::FEATURES_OK) == 0 {
            self.write_status(status::FAILED);
            return Err("virtio: FEATURES_OK not accepted");
        }

        Ok(())
    }

    /// Set DRIVER_OK to complete initialization
    pub fn set_driver_ok(&self) {
        self.write_status(
            status::ACKNOWLEDGE | status::DRIVER | status::FEATURES_OK | status::DRIVER_OK,
        );
    }
}

/// Scan PCI bus for VirtIO devices and return their transports
pub fn scan_virtio_devices() -> Vec<VirtioTransport> {
    let mut transports = Vec::new();

    let devices = list_devices();
    for dev in devices.iter() {
        if dev.vendor_id != VIRTIO_VENDOR_ID {
            continue;
        }

        let device_type = VirtioDeviceType::from_device_id(dev.device_id);
        crate::serial_println!(
            "virtio: found {} at {:02x}:{:02x}.{} device_id=0x{:04X}",
            device_type.name(),
            dev.bus,
            dev.device,
            dev.function,
            dev.device_id
        );

        match VirtioTransport::from_pci(dev) {
            Ok(transport) => {
                crate::serial_println!(
                    "virtio: {} transport ready (common=0x{:X})",
                    device_type.name(),
                    transport.common_base
                );
                transports.push(transport);
            }
            Err(e) => {
                crate::serial_println!("virtio: {} transport failed: {}", device_type.name(), e);
            }
        }
    }

    transports
}

/// Run the software/loopback virtio datapath self-tests and log a summary.
/// These exercise the split-virtqueue `add_buf`/`get_buf` path, the feature
/// negotiation handshake, and the virtio-net/virtio-blk device logic without
/// any real hardware.
fn init_software_samples() {
    let blk = software::selftest_blk();
    let net = software::selftest_net();
    match (blk, net) {
        (Ok(bf), Ok(nf)) => crate::serial_println!(
            "virtio: software loopback ready (blk feat=0x{:X} ok, net feat=0x{:X} ok)",
            bf,
            nf
        ),
        (b, n) => crate::serial_println!("virtio: software loopback blk={:?} net={:?}", b, n),
    }
}

/// Initialize all VirtIO devices found on the PCI bus
pub fn init() -> Result<(), &'static str> {
    // Always bring up the transport-agnostic software/loopback datapath so the
    // virtqueue stack is exercised even on machines with no virtio hardware.
    init_software_samples();

    let transports = scan_virtio_devices();

    if transports.is_empty() {
        crate::serial_println!("virtio: no PCI devices found (software datapath active)");
        return Ok(());
    }

    for transport in transports {
        match transport.device_type {
            VirtioDeviceType::Network => {
                if let Err(e) = net::init_virtio_net(transport) {
                    crate::serial_println!("virtio-net: init failed: {}", e);
                }
            }
            VirtioDeviceType::Block => {
                if let Err(e) = blk::init_virtio_blk(transport) {
                    crate::serial_println!("virtio-blk: init failed: {}", e);
                }
            }
            VirtioDeviceType::Rng => {
                if let Err(e) = rng::init_virtio_rng(transport) {
                    crate::serial_println!("virtio-rng: init failed: {}", e);
                }
            }
            VirtioDeviceType::Console => {
                if let Err(e) = console::init_virtio_console(transport) {
                    crate::serial_println!("virtio-console: init failed: {}", e);
                }
            }
            _ => {
                crate::serial_println!(
                    "virtio: {} not yet supported",
                    transport.device_type.name()
                );
            }
        }
    }

    Ok(())
}
