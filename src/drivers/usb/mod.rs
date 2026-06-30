//! USB host stack.
//!
//! Layers, bottom to top:
//!   * [`hcd`]        — the hardware-independent `HostController` interface.
//!   * [`xhci`]       — an in-memory xHCI controller model (rings, TRBs,
//!                      cycle-bit toggle, doorbells, event posting).
//!   * [`descriptor`] — standard USB descriptor structures and parsers.
//!   * [`device`]     — device-side virtual peripherals (HID keyboard, BOT
//!                      flash disk) plus the shared soft SCSI engine.
//!   * [`hub`]        — root-hub enumeration (reset → address → descriptors →
//!                      configure).
//!   * [`class`]      — HID boot and Bulk-Only-Transport class drivers.
//!
//! [`init`] builds a software controller, attaches the two virtual devices,
//! enumerates them end to end and exercises both transfer paths. It also keeps
//! the legacy PCI scan and the soft mass-storage device registered with the
//! storage manager so existing callers (`msc_execute_scsi`) keep working.

pub mod class;
pub mod descriptor;
pub mod device;
pub mod hcd;
pub mod hub;
pub mod xhci;

use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use device::SoftDisk;

use super::storage::usb_mass_storage::{
    create_usb_mass_storage_driver_with_host, is_usb_mass_storage_device, CommandStatusWrapper,
    UsbMscProtocol,
};
use super::storage::StorageError;

// ── xHCI register model (PCI discovery) ─────────────────────────────────

pub const XHCI_PCI_CLASS: u8 = 0x0C;
pub const XHCI_PCI_SUBCLASS: u8 = 0x03;
pub const XHCI_PCI_PROG_IF: u8 = 0x30;

pub const XHCI_CAP_CAPLENGTH: u32 = 0x00;
pub const XHCI_CAP_HCIVERSION: u32 = 0x02;
pub const XHCI_CAP_HCSPARAMS1: u32 = 0x04;

pub const XHCI_OP_USBSTS: u32 = 0x04;
pub const XHCI_USBSTS_HCH: u32 = 1 << 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UsbHostState {
    Uninitialized,
    Mapped,
    Running,
    Error,
}

#[derive(Debug, Clone)]
pub struct XhciHost {
    pub id: u32,
    pub location: String,
    pub vendor_id: u16,
    pub device_id: u16,
    pub mmio_base: u64,
    pub cap_length: u8,
    pub hci_version: u16,
    pub max_slots: u8,
    pub max_ports: u8,
    pub state: UsbHostState,
}

#[derive(Debug)]
struct UsbDeviceEntry {
    #[allow(dead_code)]
    host_id: u32,
    #[allow(dead_code)]
    bulk_out_ep: u8,
    #[allow(dead_code)]
    bulk_in_ep: u8,
    backend: SoftDisk,
}

static NEXT_HOST_ID: AtomicU32 = AtomicU32::new(1);
static NEXT_DEVICE_ID: AtomicU32 = AtomicU32::new(1);

static XHCI_HOSTS: RwLock<alloc::vec::Vec<XhciHost>> = RwLock::new(alloc::vec::Vec::new());
static USB_DEVICES: RwLock<BTreeMap<u32, UsbDeviceEntry>> = RwLock::new(BTreeMap::new());
/// Software xHCI controllers indexed by id.
static CONTROLLERS: RwLock<BTreeMap<u32, xhci::XhciController>> = RwLock::new(BTreeMap::new());
static USB_INITIALIZED: RwLock<bool> = RwLock::new(false);
/// Cached stats from the last successful `init()` so `get_stats()` reports the
/// real enumeration counts on the idempotent path.
static LAST_STATS: RwLock<UsbInitStats> = RwLock::new(UsbInitStats {
    host_count: 0,
    device_count: 0,
    msc_enumerated: 0,
    enumerated_devices: 0,
    hid_devices: 0,
    bot_devices: 0,
});

fn mmio_read32(base: u64, offset: u32) -> u32 {
    let addr = (base + offset as u64) as *const u32;
    unsafe { core::ptr::read_volatile(addr) }
}

fn probe_xhci_controller(dev: &crate::pci::PciDevice) -> Result<XhciHost, &'static str> {
    let bar0 = dev.bars[0];
    if bar0 & 1 != 0 {
        return Err("xHCI BAR0 is not memory-mapped");
    }
    let base = (bar0 & !0xF) as u64;
    if base == 0 {
        return Err("xHCI BAR0 unset");
    }

    crate::memory::map_mmio_region(base as usize, 0x4000).map_err(|_| "xHCI MMIO map failed")?;

    let cap0 = mmio_read32(base, XHCI_CAP_CAPLENGTH);
    let cap_length = (cap0 & 0xFF) as u8;
    let hci_version = mmio_read32(base, XHCI_CAP_HCIVERSION) as u16;
    let hcsparams1 = mmio_read32(base, XHCI_CAP_HCSPARAMS1);
    let max_slots = ((hcsparams1 >> 8) & 0xFF) as u8;
    let max_ports = (hcsparams1 & 0xFF) as u8;

    let op_base = base + cap_length as u64;
    let usbsts = mmio_read32(op_base, XHCI_OP_USBSTS);
    let state = if usbsts & XHCI_USBSTS_HCH != 0 {
        UsbHostState::Mapped
    } else {
        UsbHostState::Running
    };

    Ok(XhciHost {
        id: NEXT_HOST_ID.fetch_add(1, Ordering::SeqCst),
        location: dev.location(),
        vendor_id: dev.vendor_id,
        device_id: dev.device_id,
        mmio_base: base,
        cap_length,
        hci_version,
        max_slots,
        max_ports,
        state,
    })
}

// ── Legacy soft mass-storage (storage-manager integration) ──────────────

fn register_soft_msc_device(host_id: u32, size_mb: u32) -> u32 {
    let dev_id = NEXT_DEVICE_ID.fetch_add(1, Ordering::SeqCst);
    USB_DEVICES.write().insert(
        dev_id,
        UsbDeviceEntry {
            host_id,
            bulk_out_ep: 0x01,
            bulk_in_ep: 0x81,
            backend: SoftDisk::new(size_mb),
        },
    );
    dev_id
}

/// Execute SCSI over a host-attached soft MSC device. Shares the SCSI engine
/// with the BOT virtual device via [`SoftDisk::execute_scsi`].
pub fn msc_execute_scsi(
    device_id: u32,
    command: &[u8],
    data_length: u32,
    direction_in: bool,
    buffer: Option<&mut [u8]>,
    tag: u32,
) -> Result<CommandStatusWrapper, StorageError> {
    let mut devices = USB_DEVICES.write();
    let entry = devices
        .get_mut(&device_id)
        .ok_or(StorageError::DeviceNotFound)?;
    entry
        .backend
        .execute_scsi(command, data_length, direction_in, buffer, tag)
}

fn enumerate_on_host(host: &XhciHost) -> usize {
    let dev_id = register_soft_msc_device(host.id, 64);
    let driver = create_usb_mass_storage_driver_with_host(
        dev_id,
        0x1234,
        0x5678,
        0x06,
        UsbMscProtocol::BulkOnly as u8,
        Some(format!("usb-msc-{}", host.location)),
    );

    let timestamp = crate::time::get_system_time_ms();
    let register_result: Option<Result<u32, StorageError>> =
        super::storage::with_storage_manager(|mgr| -> Result<u32, StorageError> {
            let storage_id = mgr.register_device(
                driver,
                String::from("RustOS USB Soft MSC"),
                format!("USB-{}", host.location),
                String::from("1.0"),
                timestamp,
            )?;
            if let Some(device) = mgr.get_device_mut(storage_id) {
                device.driver.init()?;
            }
            Ok(storage_id)
        });

    match register_result {
        Some(Ok(storage_id)) => {
            crate::serial_println!(
                "usb: MSC on {} host={} usb_dev={} storage={}",
                host.location,
                host.id,
                dev_id,
                storage_id
            );
            1
        }
        Some(Err(e)) => {
            crate::serial_println!("usb: MSC register failed: {:?}", e);
            0
        }
        None => 0,
    }
}

// ── Software USB stack exercise ─────────────────────────────────────────

/// Build a virtual controller, enumerate the attached devices and run both a
/// HID interrupt poll and a BOT read/write round-trip. Returns the controller
/// id together with the number of HID devices and BOT round-trips that worked.
fn build_and_exercise_virtual_stack() -> (u32, usize, usize) {
    let mut controller = xhci::XhciController::new("soft-xhci", 4);
    controller.run();

    // Port 1: boot keyboard (interrupt-IN endpoint 0x81).
    let _ = controller.attach(1, Box::new(device::VirtualHidKeyboard::new(0x81)));
    // Port 2: BOT flash disk (bulk-IN 0x82, bulk-OUT 0x02), 8 MiB.
    let _ = controller.attach(
        2,
        Box::new(device::VirtualBotDisk::new(8, 0x82, 0x02)),
    );

    let enumerated = hub::enumerate_all(&mut controller);

    let mut hid_devices = 0usize;
    let mut bot_devices = 0usize;

    for dev in &enumerated {
        if let Some(hid) = class::hid::bind(dev) {
            match hid.poll_keyboard(&mut controller) {
                Ok(Some(report)) => {
                    crate::serial_println!(
                        "usb-hid: keyboard slot={} ep={:#x} first_key={:?}",
                        hid.slot,
                        hid.interrupt_in_ep,
                        report.first_key()
                    );
                    hid_devices += 1;
                }
                Ok(None) => {
                    hid_devices += 1;
                }
                Err(e) => crate::serial_println!("usb-hid: poll failed: {}", e),
            }
        } else if let Some(mut bot) = class::storage::bind(dev) {
            match exercise_bot(&mut controller, &mut bot) {
                Ok(()) => {
                    crate::serial_println!(
                        "usb-bot: slot={} {} blocks x {} bytes verified",
                        bot.slot,
                        bot.block_count,
                        bot.block_size
                    );
                    bot_devices += 1;
                }
                Err(e) => crate::serial_println!("usb-bot: exercise failed: {}", e),
            }
        }
    }

    let id = NEXT_HOST_ID.fetch_add(1, Ordering::SeqCst);
    CONTROLLERS.write().insert(id, controller);
    (id, hid_devices, bot_devices)
}

/// Run INQUIRY → READ CAPACITY → WRITE(10) → READ(10) and verify the data.
fn exercise_bot(
    hc: &mut dyn hcd::HostController,
    bot: &mut class::storage::BotDevice,
) -> Result<(), &'static str> {
    let _inq = bot.inquiry(hc)?;
    let (blocks, block_size) = bot.read_capacity(hc)?;
    if blocks == 0 || block_size == 0 {
        return Err("usb-bot: zero capacity");
    }

    let mut write_buf = alloc::vec![0u8; block_size as usize];
    for (i, b) in write_buf.iter_mut().enumerate() {
        *b = (i as u8) ^ 0xA5;
    }
    bot.write10(hc, 0, 1, &mut write_buf)?;

    let mut read_buf = alloc::vec![0u8; block_size as usize];
    bot.read10(hc, 0, 1, &mut read_buf)?;

    if read_buf != write_buf {
        return Err("usb-bot: read-back mismatch");
    }
    Ok(())
}

// ── Public init / stats ─────────────────────────────────────────────────

/// Initialize USB host controllers and enumerate devices. Idempotent.
pub fn init() -> Result<UsbInitStats, &'static str> {
    {
        let mut init = USB_INITIALIZED.write();
        if *init {
            return Ok(get_stats());
        }
        *init = true;
    }

    let pci_devices = crate::pci::list_devices();
    let mut hosts = alloc::vec::Vec::new();
    let mut msc_enumerated = 0usize;

    for dev in pci_devices.iter() {
        if dev.class != XHCI_PCI_CLASS
            || dev.subclass != XHCI_PCI_SUBCLASS
            || dev.prog_if != XHCI_PCI_PROG_IF
        {
            continue;
        }

        match probe_xhci_controller(dev) {
            Ok(host) => {
                crate::serial_println!(
                    "usb: xHCI {} {:04x}:{:04x} slots={} ports={} ver={:x}",
                    host.location,
                    host.vendor_id,
                    host.device_id,
                    host.max_slots,
                    host.max_ports,
                    host.hci_version
                );
                msc_enumerated += enumerate_on_host(&host);
                hosts.push(host);
            }
            Err(e) => {
                crate::serial_println!("usb: xHCI probe failed for {}: {}", dev.location(), e);
            }
        }
    }

    if hosts.is_empty() {
        let host = XhciHost {
            id: NEXT_HOST_ID.fetch_add(1, Ordering::SeqCst),
            location: String::from("soft"),
            vendor_id: 0,
            device_id: 0,
            mmio_base: 0,
            cap_length: 0,
            hci_version: 0x0100,
            max_slots: 32,
            max_ports: 8,
            state: UsbHostState::Running,
        };
        msc_enumerated += enumerate_on_host(&host);
        hosts.push(host);
        crate::serial_println!("usb: soft xHCI host (no PCI controller)");
    }

    *XHCI_HOSTS.write() = hosts;

    // Bring up the software USB stack and exercise the transfer paths.
    let (_ctrl_id, hid_devices, bot_devices) = build_and_exercise_virtual_stack();
    let enumerated_devices = hid_devices + bot_devices;

    let stats = UsbInitStats {
        host_count: XHCI_HOSTS.read().len() + CONTROLLERS.read().len(),
        device_count: USB_DEVICES.read().len(),
        msc_enumerated,
        enumerated_devices,
        hid_devices,
        bot_devices,
    };
    *LAST_STATS.write() = stats;

    crate::serial_println!(
        "usb: ready hosts={} soft-ctrl={} enum={} hid={} bot={} msc={}",
        XHCI_HOSTS.read().len(),
        CONTROLLERS.read().len(),
        enumerated_devices,
        hid_devices,
        bot_devices,
        msc_enumerated
    );

    Ok(stats)
}

#[derive(Debug, Clone, Copy, Default)]
pub struct UsbInitStats {
    pub host_count: usize,
    pub device_count: usize,
    pub msc_enumerated: usize,
    pub enumerated_devices: usize,
    pub hid_devices: usize,
    pub bot_devices: usize,
}

pub fn get_stats() -> UsbInitStats {
    let mut stats = *LAST_STATS.read();
    // Reflect live registry sizes in case they changed since init.
    stats.host_count = XHCI_HOSTS.read().len() + CONTROLLERS.read().len();
    stats.device_count = USB_DEVICES.read().len();
    stats
}

pub fn host_count() -> usize {
    XHCI_HOSTS.read().len() + CONTROLLERS.read().len()
}

pub fn is_mass_storage_device(class: u8, subclass: u8, protocol: u8) -> bool {
    is_usb_mass_storage_device(class, subclass, protocol)
}
