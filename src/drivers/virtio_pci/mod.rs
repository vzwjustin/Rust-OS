//! # virtio-pci modern transport
//!
//! Implements helpers for the modern (VirtIO 1.x) virtio-over-PCI transport.
//! The in-memory `MmioRegion` model is retained for tests, but driver init no
//! longer publishes a synthetic PCI function as real boot hardware.
//!
//! Mirrors Linux's `drivers/virtio/virtio_pci_modern.c`.

use crate::drivers::virtio::software::VirtioDevice;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

// ── Capability layout ─────────────────────────────────────────────────────

/// virtio-pci capability `cfg_type` values (modern spec §4.1.4).
pub const VIRTIO_PCI_CAP_COMMON_CFG: u8 = 1;
pub const VIRTIO_PCI_CAP_NOTIFY_CFG: u8 = 2;
pub const VIRTIO_PCI_CAP_ISR_CFG: u8 = 3;
pub const VIRTIO_PCI_CAP_DEVICE_CFG: u8 = 4;

/// A parsed virtio-pci capability (one entry of the PCI capability list).
#[derive(Debug, Clone, Copy)]
pub struct VirtioPciCap {
    pub cfg_type: u8,
    pub bar: u8,
    pub offset: u32,
    pub length: u32,
}

/// Common-config register byte offsets (modern spec §4.1.4.3).
pub mod common_cfg {
    pub const DEVICE_FEATURE_SELECT: usize = 0;
    pub const DEVICE_FEATURE: usize = 4;
    pub const DRIVER_FEATURE_SELECT: usize = 8;
    pub const DRIVER_FEATURE: usize = 12;
    pub const MSIX_CONFIG: usize = 16;
    pub const NUM_QUEUES: usize = 18;
    pub const DEVICE_STATUS: usize = 20;
    pub const CONFIG_GENERATION: usize = 21;
    pub const QUEUE_SELECT: usize = 22;
    pub const QUEUE_SIZE: usize = 24;
    pub const QUEUE_MSIX_VECTOR: usize = 26;
    pub const QUEUE_ENABLE: usize = 28;
    pub const QUEUE_NOTIFY_OFF: usize = 30;
    pub const QUEUE_DESC: usize = 32;
    pub const QUEUE_DRIVER: usize = 40;
    pub const QUEUE_DEVICE: usize = 48;
    pub const COMMON_CFG_LEN: usize = 56;
}

/// VirtIO status bits (re-exported for callers of this transport).
pub mod status {
    pub const RESET: u8 = 0;
    pub const ACKNOWLEDGE: u8 = 1;
    pub const DRIVER: u8 = 2;
    pub const DRIVER_OK: u8 = 4;
    pub const FEATURES_OK: u8 = 8;
    pub const FAILED: u8 = 128;
}

// ── In-memory MMIO model ───────────────────────────────────────────────────

/// A byte-addressable region modeling a device's BAR-mapped MMIO window.
pub struct MmioRegion {
    data: Vec<u8>,
}

impl MmioRegion {
    pub fn new(len: usize) -> Self {
        MmioRegion {
            data: vec![0u8; len],
        }
    }

    pub fn read8(&self, off: usize) -> u8 {
        self.data.get(off).copied().unwrap_or(0)
    }
    pub fn write8(&mut self, off: usize, v: u8) {
        if let Some(b) = self.data.get_mut(off) {
            *b = v;
        }
    }
    pub fn read16(&self, off: usize) -> u16 {
        u16::from_le_bytes([self.read8(off), self.read8(off + 1)])
    }
    pub fn write16(&mut self, off: usize, v: u16) {
        let b = v.to_le_bytes();
        self.write8(off, b[0]);
        self.write8(off + 1, b[1]);
    }
    pub fn read32(&self, off: usize) -> u32 {
        u32::from_le_bytes([
            self.read8(off),
            self.read8(off + 1),
            self.read8(off + 2),
            self.read8(off + 3),
        ])
    }
    pub fn write32(&mut self, off: usize, v: u32) {
        let b = v.to_le_bytes();
        for (i, &x) in b.iter().enumerate() {
            self.write8(off + i, x);
        }
    }
    pub fn read64(&self, off: usize) -> u64 {
        (self.read32(off) as u64) | ((self.read32(off + 4) as u64) << 32)
    }
    pub fn write64(&mut self, off: usize, v: u64) {
        self.write32(off, v as u32);
        self.write32(off + 4, (v >> 32) as u32);
    }
}

// ── Modern transport over the modeled MMIO ─────────────────────────────────

/// A modern virtio-pci transport bound to an in-memory common-config window.
/// It holds the 64-bit device-feature bitmap that a real device would expose
/// and services the feature/status handshake the way hardware would.
pub struct ModernTransport {
    pub common: MmioRegion,
    pub device_cfg: MmioRegion,
    pub isr: MmioRegion,
    pub notify_off_multiplier: u32,
    /// Features the (modeled) device advertises.
    device_features: u64,
    /// Last 32-bit feature word the driver wrote, per select word.
    driver_features: u64,
    notify_count: u32,
}

impl ModernTransport {
    pub fn new(device_features: u64, num_queues: u16, device_cfg_len: usize) -> Self {
        let mut common = MmioRegion::new(common_cfg::COMMON_CFG_LEN);
        common.write16(common_cfg::NUM_QUEUES, num_queues);
        common.write8(common_cfg::DEVICE_STATUS, status::RESET);
        ModernTransport {
            common,
            device_cfg: MmioRegion::new(device_cfg_len),
            isr: MmioRegion::new(4),
            notify_off_multiplier: 4,
            device_features,
            driver_features: 0,
            notify_count: 0,
        }
    }

    /// Read the device feature word selected via DEVICE_FEATURE_SELECT.
    pub fn read_device_features(&self) -> u32 {
        let select = self.common.read32(common_cfg::DEVICE_FEATURE_SELECT);
        if select == 0 {
            self.device_features as u32
        } else {
            (self.device_features >> 32) as u32
        }
    }

    /// Set the device-feature select word and return the selected feature word.
    pub fn select_device_features(&mut self, word: u32) -> u32 {
        self.common.write32(common_cfg::DEVICE_FEATURE_SELECT, word);
        self.read_device_features()
    }

    /// Write a driver feature word (latched into the modeled register).
    pub fn write_driver_features(&mut self, word: u32, value: u32) {
        self.common.write32(common_cfg::DRIVER_FEATURE_SELECT, word);
        self.common.write32(common_cfg::DRIVER_FEATURE, value);
        if word == 0 {
            self.driver_features = (self.driver_features & !0xFFFF_FFFF) | value as u64;
        } else {
            self.driver_features = (self.driver_features & 0xFFFF_FFFF) | ((value as u64) << 32);
        }
    }

    pub fn driver_features(&self) -> u64 {
        self.driver_features
    }

    pub fn get_status(&self) -> u8 {
        self.common.read8(common_cfg::DEVICE_STATUS)
    }

    /// Write the device status. The modeled device clears FEATURES_OK if the
    /// driver tried to ack features the device does not offer.
    pub fn set_status(&mut self, value: u8) {
        let mut effective = value;
        if value & status::FEATURES_OK != 0 {
            if self.driver_features & !self.device_features != 0 {
                effective &= !status::FEATURES_OK;
            }
        }
        self.common.write8(common_cfg::DEVICE_STATUS, effective);
    }

    pub fn select_queue(&mut self, idx: u16) {
        self.common.write16(common_cfg::QUEUE_SELECT, idx);
    }

    pub fn set_queue_size(&mut self, size: u16) {
        self.common.write16(common_cfg::QUEUE_SIZE, size);
    }

    pub fn queue_size(&self) -> u16 {
        self.common.read16(common_cfg::QUEUE_SIZE)
    }

    pub fn set_queue_addrs(&mut self, desc: u64, driver: u64, device: u64) {
        self.common.write64(common_cfg::QUEUE_DESC, desc);
        self.common.write64(common_cfg::QUEUE_DRIVER, driver);
        self.common.write64(common_cfg::QUEUE_DEVICE, device);
    }

    pub fn enable_queue(&mut self) {
        self.common.write16(common_cfg::QUEUE_ENABLE, 1);
    }

    pub fn queue_enabled(&self) -> bool {
        self.common.read16(common_cfg::QUEUE_ENABLE) == 1
    }

    /// "Notify" the device — increments a counter and raises the ISR bit.
    pub fn notify(&mut self) {
        self.notify_count += 1;
        self.isr.write8(0, 1);
    }

    pub fn notify_count(&self) -> u32 {
        self.notify_count
    }

    /// Read (and clear) the ISR status byte.
    pub fn read_isr(&mut self) -> u8 {
        let v = self.isr.read8(0);
        self.isr.write8(0, 0);
        v
    }

    /// Drive the full modern feature/status handshake for `requested`
    /// features. Returns the negotiated 64-bit bitmap.
    pub fn negotiate(&mut self, requested: u64) -> Result<u64, &'static str> {
        self.set_status(status::RESET);
        self.set_status(status::ACKNOWLEDGE);
        self.set_status(status::ACKNOWLEDGE | status::DRIVER);

        // Read 64-bit device features in two words.
        let lo = self.select_device_features(0) as u64;
        let hi = self.select_device_features(1) as u64;
        let device_features = (hi << 32) | lo;
        let negotiated = device_features & requested;

        self.write_driver_features(0, negotiated as u32);
        self.write_driver_features(1, (negotiated >> 32) as u32);

        let s = status::ACKNOWLEDGE | status::DRIVER | status::FEATURES_OK;
        self.set_status(s);
        if self.get_status() & status::FEATURES_OK == 0 {
            self.set_status(status::FAILED);
            return Err("virtio-pci: FEATURES_OK rejected");
        }
        self.set_status(s | status::DRIVER_OK);
        Ok(negotiated)
    }

    pub fn driver_ok(&self) -> bool {
        self.get_status() & status::DRIVER_OK != 0
    }
}

// ── Registry ────────────────────────────────────────────────────────────

/// virtio-pci transport flavor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VirtioPciTransport {
    Legacy,
    Modern,
    Transitional,
}

/// A registered virtio-pci function bound to a software VirtioDevice.
pub struct VirtioPciFunction {
    pub id: u32,
    pub name: String,
    pub pci_vendor: u16,
    pub pci_device: u16,
    pub transport: VirtioPciTransport,
    pub virtio_device_id: u32,
    pub caps: Vec<VirtioPciCap>,
    pub negotiated_features: u64,
    pub modern: ModernTransport,
    pub device: VirtioDevice,
}

static ID_COUNTER: AtomicU32 = AtomicU32::new(0);
static FUNCTIONS: RwLock<BTreeMap<u32, VirtioPciFunction>> = RwLock::new(BTreeMap::new());

/// Build the standard four-capability layout a modern device exposes.
fn default_caps() -> Vec<VirtioPciCap> {
    vec![
        VirtioPciCap {
            cfg_type: VIRTIO_PCI_CAP_COMMON_CFG,
            bar: 4,
            offset: 0,
            length: common_cfg::COMMON_CFG_LEN as u32,
        },
        VirtioPciCap {
            cfg_type: VIRTIO_PCI_CAP_NOTIFY_CFG,
            bar: 4,
            offset: 0x1000,
            length: 0x1000,
        },
        VirtioPciCap {
            cfg_type: VIRTIO_PCI_CAP_ISR_CFG,
            bar: 4,
            offset: 0x2000,
            length: 4,
        },
        VirtioPciCap {
            cfg_type: VIRTIO_PCI_CAP_DEVICE_CFG,
            bar: 4,
            offset: 0x3000,
            length: 0x1000,
        },
    ]
}

/// Bind a discovered PCI function to a software VirtioDevice and run the
/// modern transport handshake. Returns the new function id.
pub fn bind_function(
    name: &str,
    pci_vendor: u16,
    pci_device: u16,
    transport: VirtioPciTransport,
    virtio_device_id: u32,
    device_features: u64,
    requested_features: u64,
    device: VirtioDevice,
) -> Result<u32, &'static str> {
    let id = ID_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut modern = ModernTransport::new(device_features, 2, 0x1000);
    let negotiated = modern.negotiate(requested_features)?;

    // Program a queue through the modeled common config.
    modern.select_queue(0);
    modern.set_queue_size(256);
    modern.set_queue_addrs(0x1000, 0x2000, 0x3000);
    modern.enable_queue();

    let func = VirtioPciFunction {
        id,
        name: String::from(name),
        pci_vendor,
        pci_device,
        transport,
        virtio_device_id,
        caps: default_caps(),
        negotiated_features: negotiated,
        modern,
        device,
    };
    FUNCTIONS.write().insert(id, func);
    Ok(id)
}

/// Read device-specific config bytes through the modeled device-config window.
pub fn read_device_config(id: u32, offset: usize, buf: &mut [u8]) -> Result<usize, &'static str> {
    let funcs = FUNCTIONS.read();
    let f = funcs.get(&id).ok_or("virtio-pci function not found")?;
    for (i, b) in buf.iter_mut().enumerate() {
        *b = f.modern.device_cfg.read8(offset + i);
    }
    Ok(buf.len())
}

/// Write device-specific config bytes through the modeled device-config window.
pub fn write_device_config(id: u32, offset: usize, data: &[u8]) -> Result<usize, &'static str> {
    let mut funcs = FUNCTIONS.write();
    let f = funcs.get_mut(&id).ok_or("virtio-pci function not found")?;
    for (i, &b) in data.iter().enumerate() {
        f.modern.device_cfg.write8(offset + i, b);
    }
    Ok(data.len())
}

/// Get the negotiated feature bitmap for a function.
pub fn negotiated_features(id: u32) -> Result<u64, &'static str> {
    let funcs = FUNCTIONS.read();
    funcs
        .get(&id)
        .map(|f| f.negotiated_features)
        .ok_or("virtio-pci function not found")
}

/// Get the device status of a function's modern transport.
pub fn get_status(id: u32) -> Result<u8, &'static str> {
    let funcs = FUNCTIONS.read();
    funcs
        .get(&id)
        .map(|f| f.modern.get_status())
        .ok_or("virtio-pci function not found")
}

/// List all bound functions: `(id, name, transport, virtio_device_id)`.
pub fn list_devices() -> Vec<(u32, String, VirtioPciTransport, u32)> {
    FUNCTIONS
        .read()
        .iter()
        .map(|(id, f)| (*id, f.name.clone(), f.transport, f.virtio_device_id))
        .collect()
}

pub fn device_count() -> usize {
    FUNCTIONS.read().len()
}

// ── Init ────────────────────────────────────────────────────────────────

pub fn init() -> Result<(), &'static str> {
    crate::serial_println!(
        "virtio_pci: transport helpers ready ({} hardware function(s) bound)",
        device_count()
    );
    Ok(())
}
