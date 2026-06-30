//! # RustOS Hardware Drivers Module
//!
//! This module provides a unified interface for all hardware drivers in RustOS,
//! including graphics, input, network, and storage drivers with hot-plug support.

pub mod acpi;
// Linux-mirror subsystems (driver core + additional buses)
pub mod accel;
pub mod agp;
pub mod amba;
pub mod android;
pub mod apm;
pub mod ata;
pub mod atm;
pub mod auxdisplay;
pub mod auxiliary;
pub mod backlight;
pub mod base;
pub mod bcma;
pub mod block;
pub mod bt;
pub mod bus;
pub mod cache;
pub mod cdrom;
pub mod cdx;
pub mod cec;
pub mod char;
pub mod clk;
pub mod clocksource;
pub mod comedi;
pub mod connector;
pub mod coresight;
pub mod counter;
pub mod crypto;
pub mod cxl;
pub mod dax;
pub mod dca;
pub mod devfreq;
pub mod display;
pub mod dma;
pub mod dma_buf;
pub mod dpll;
pub mod dvb;
pub mod edac;
pub mod eisa;
pub mod extcon;
pub mod ffa;
pub mod firewire;
pub mod firmware;
pub mod fpga;
pub mod fsi;
pub mod fwctl;
pub mod gnss;
pub mod gpib;
pub mod gpio;
pub mod gpu;
pub mod greybus;
pub mod hid;
pub mod hidraw;
pub mod hotplug;
pub mod hsi;
pub mod hte;
pub mod hv;
pub mod hwmon;
pub mod hwrng;
pub mod hwspinlock;
pub mod hwtracing;
pub mod i2c;
pub mod i3c;
pub mod idle;
pub mod iio;
pub mod infiniband;
pub mod input;
pub mod input_manager;
pub mod interconnect;
pub mod iommu;
pub mod iommufd;
pub mod ipack;
pub mod irqchip;
pub mod isapnp;
pub mod isdbt;
pub mod isdn;
pub mod ishtp;
pub mod leds;
pub mod linux_mirror;
pub mod mailbox;
pub mod mcb;
pub mod media;
pub mod mei;
pub mod memory;
pub mod memstick;
pub mod message;
pub mod mfd;
pub mod misc;
pub mod mmc;
pub mod most;
pub mod moxtet;
pub mod mtd;
pub mod mux;
pub mod network;
pub mod nfc;
pub mod ntb;
pub mod ntsync;
pub mod nvdimm;
pub mod nvme;
pub mod nvmem;
pub mod nvmem_layouts;
pub mod of;
pub mod opp;
pub mod parport;
pub mod pci;
pub mod pcmcia;
pub mod peci;
pub mod perf;
pub mod phy;
pub mod pinctrl;
pub mod platform;
pub mod pmdomain;
pub mod pnp;
pub mod power;
pub mod power_supply;
pub mod powercap;
pub mod pps;
pub mod ps2_controller;
pub mod ps2_mouse;
pub mod ptp;
pub mod pwm;
pub mod rapidio;
pub mod ras;
pub mod regmap;
pub mod regulator;
pub mod remoteproc;
pub mod resctrl;
pub mod reset;
pub mod rpmsg;
pub mod rtc;
pub mod scsi;
pub mod serio;
pub mod siox;
pub mod slimbus;
pub mod slimproc;
pub mod soc;
pub mod sound;
pub mod soundwire;
pub mod spi;
pub mod spmi;
pub mod ssb;
pub mod storage;
pub mod target;
pub mod tee;
pub mod thermal;
pub mod thunderbolt;
pub mod tty;
pub mod udmabuf;
pub mod ufs;
pub mod uio;
pub mod usb;
pub mod v4l2;
pub mod vbe;
pub mod vbe_io;
pub mod vdpa;
pub mod vfio;
pub mod vhost;
pub mod video;
pub mod virt;
pub mod virtio;
pub mod virtio_pci;
pub mod w1;
pub mod watchdog;
pub mod wmi;
pub mod xen;

// Removed unused imports
use alloc::string::String;
use core::fmt;

// Re-export VBE driver functionality

// Re-export display driver functionality

// Re-export PCI functionality
pub use pci::{get_pci_stats, init as init_pci};

// Re-export hot-plug functionality
pub use hotplug::{
    get_hotplug_stats, init as init_hotplug, process_events as process_hotplug_events,
    scan_devices as scan_hotplug_devices,
};

// Re-export input functionality
pub use input_manager::{
    get_cursor_position, get_event as get_input_event, set_cursor_bounds, InputEvent, MouseButton,
};

// Re-export storage functionality
pub use storage::StorageDriver;

/// Driver types supported by RustOS
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverType {
    Graphics,
    Network,
    Storage,
    Input,
    Audio,
    USB,
    PCI,
    System,
}

impl fmt::Display for DriverType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverType::Graphics => write!(f, "Graphics"),
            DriverType::Network => write!(f, "Network"),
            DriverType::Storage => write!(f, "Storage"),
            DriverType::Input => write!(f, "Input"),
            DriverType::Audio => write!(f, "Audio"),
            DriverType::USB => write!(f, "USB"),
            DriverType::PCI => write!(f, "PCI"),
            DriverType::System => write!(f, "System"),
        }
    }
}

/// Driver status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverStatus {
    Uninitialized,
    Initializing,
    Ready,
    Error,
    Disabled,
}

impl fmt::Display for DriverStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverStatus::Uninitialized => write!(f, "Uninitialized"),
            DriverStatus::Initializing => write!(f, "Initializing"),
            DriverStatus::Ready => write!(f, "Ready"),
            DriverStatus::Error => write!(f, "Error"),
            DriverStatus::Disabled => write!(f, "Disabled"),
        }
    }
}

/// Generic driver information
#[derive(Debug, Clone)]
pub struct DriverInfo {
    pub name: String,
    pub version: String,
    pub driver_type: DriverType,
    pub status: DriverStatus,
    pub vendor: String,
    pub device_id: Option<u32>,
    pub description: String,
}

impl DriverInfo {
    /// Create new driver info
    pub fn new(
        name: String,
        version: String,
        driver_type: DriverType,
        vendor: String,
        description: String,
    ) -> Self {
        Self {
            name,
            version,
            driver_type,
            status: DriverStatus::Uninitialized,
            vendor,
            device_id: None,
            description,
        }
    }

    /// Set device ID
    pub fn with_device_id(mut self, device_id: u32) -> Self {
        self.device_id = Some(device_id);
        self
    }

    /// Set status
    pub fn with_status(mut self, status: DriverStatus) -> Self {
        self.status = status;
        self
    }
}

/// Hardware device information
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub vendor_id: u16,
    pub device_id: u16,
    pub class_code: u8,
    pub subclass: u8,
    pub prog_if: u8,
    pub revision: u8,
    pub bus: u8,
    pub device: u8,
    pub function: u8,
    pub name: String,
    pub driver_loaded: bool,
}

impl DeviceInfo {
    /// Create new device info
    pub fn new(
        vendor_id: u16,
        device_id: u16,
        class_code: u8,
        subclass: u8,
        prog_if: u8,
        revision: u8,
        bus: u8,
        device: u8,
        function: u8,
        name: String,
    ) -> Self {
        Self {
            vendor_id,
            device_id,
            class_code,
            subclass,
            prog_if,
            revision,
            bus,
            device,
            function,
            name,
            driver_loaded: false,
        }
    }

    /// Get device type based on class code
    pub fn get_device_type(&self) -> DriverType {
        match self.class_code {
            0x00 => DriverType::System,   // Unclassified
            0x01 => DriverType::Storage,  // Mass Storage Controller
            0x02 => DriverType::Network,  // Network Controller
            0x03 => DriverType::Graphics, // Display Controller
            0x04 => DriverType::Audio,    // Multimedia Controller
            0x06 => DriverType::System,   // Bridge Device
            0x0C => DriverType::USB,      // Serial Bus Controller
            _ => DriverType::System,      // Other/Unknown
        }
    }

    /// Check if this is a graphics device
    pub fn is_graphics_device(&self) -> bool {
        self.class_code == 0x03 || (self.class_code == 0x00 && self.subclass == 0x01)
        // VGA-compatible
    }

    /// Get vendor name
    pub fn get_vendor_name(&self) -> &'static str {
        match self.vendor_id {
            0x8086 => "Intel",
            0x10DE => "NVIDIA",
            0x1002 => "AMD",
            0x1234 => "QEMU",
            0x80EE => "VirtualBox",
            0x15AD => "VMware",
            0x1AF4 => "Virtio",
            _ => "Unknown",
        }
    }
}

/// Driver manager for handling all system drivers
pub struct DriverManager {
    driver_count: usize,
    device_count: usize,
    graphics_initialized: bool,
    input_initialized: bool,
}

impl DriverManager {
    /// Create a new driver manager
    pub const fn new() -> Self {
        Self {
            driver_count: 0,
            device_count: 0,
            graphics_initialized: false,
            input_initialized: false,
        }
    }

    /// Initialize all drivers by scanning PCI for real hardware
    pub fn init(&mut self) -> Result<(), &'static str> {
        // Detect hardware devices via PCI
        self.detect_devices()?;

        // Initialize graphics drivers
        self.init_graphics_drivers()?;

        // Initialize input drivers
        self.init_input_drivers()?;

        // Initialize other drivers as needed
        self.init_system_drivers()?;

        Ok(())
    }

    /// Detect hardware devices by scanning PCI bus
    fn detect_devices(&mut self) -> Result<(), &'static str> {
        use crate::pci::list_devices;

        let devices = list_devices();
        self.device_count = devices.len();

        for dev in devices.iter() {
            crate::serial_println!(
                "pci: {:02x}:{:02x}.{} {:04X}:{:04X} class={:?} ({})",
                dev.bus,
                dev.device,
                dev.function,
                dev.vendor_id,
                dev.device_id,
                dev.class_code,
                dev.name
            );
        }

        Ok(())
    }

    /// Initialize graphics drivers based on PCI detection
    fn init_graphics_drivers(&mut self) -> Result<(), &'static str> {
        use crate::pci::{get_devices_by_class, PciClass};

        let gpu_count = get_devices_by_class(PciClass::Display).len();
        self.driver_count += gpu_count;
        self.graphics_initialized = gpu_count > 0 || display::is_ready();

        Ok(())
    }

    /// Initialize input drivers (PS/2 is already initialized in boot path)
    fn init_input_drivers(&mut self) -> Result<(), &'static str> {
        self.driver_count += 2;
        self.input_initialized = true;
        Ok(())
    }

    /// Initialize system drivers (PCI, hotplug already initialized)
    fn init_system_drivers(&mut self) -> Result<(), &'static str> {
        use crate::pci::{get_devices_by_class, PciClass};

        let net_count = get_devices_by_class(PciClass::Network).len();
        let storage_count = get_devices_by_class(PciClass::MassStorage).len();
        self.driver_count += net_count + storage_count;

        Ok(())
    }

    /// Get driver count by type
    pub fn get_drivers_by_type_count(&self, driver_type: DriverType) -> usize {
        use crate::pci::{get_devices_by_class, PciClass};
        match driver_type {
            DriverType::Graphics => {
                if self.graphics_initialized {
                    1
                } else {
                    0
                }
            }
            DriverType::Network => get_devices_by_class(PciClass::Network).len(),
            DriverType::Storage => get_devices_by_class(PciClass::MassStorage).len(),
            DriverType::Input => {
                if self.input_initialized {
                    1
                } else {
                    0
                }
            }
            DriverType::Audio => 0,
            DriverType::USB => 0,
            DriverType::PCI => 0,
            DriverType::System => 0,
        }
    }

    /// Get driver count
    pub fn driver_count(&self) -> usize {
        self.driver_count
    }

    /// Get device count
    pub fn device_count(&self) -> usize {
        self.device_count
    }

    /// Get ready driver count (graphics + input + PCI devices)
    pub fn ready_driver_count(&self) -> usize {
        use crate::pci::{get_devices_by_class, PciClass};
        let mut count = 0;
        if self.graphics_initialized {
            count += 1;
        }
        if self.input_initialized {
            count += 1;
        }
        count += get_devices_by_class(PciClass::Network).len();
        count += get_devices_by_class(PciClass::MassStorage).len();
        count
    }

    /// Check if graphics is initialized
    pub fn is_graphics_initialized(&self) -> bool {
        self.graphics_initialized
    }

    /// Check if input is initialized
    pub fn is_input_initialized(&self) -> bool {
        self.input_initialized
    }

    /// Get system status
    pub fn get_system_status(&self) -> DriverSystemStatus {
        DriverSystemStatus {
            total_drivers: self.driver_count(),
            ready_drivers: self.ready_driver_count(),
            total_devices: self.device_count(),
            graphics_ready: self.graphics_initialized,
            input_ready: self.input_initialized,
        }
    }
}

/// System-wide driver status
#[derive(Debug, Clone, Copy)]
pub struct DriverSystemStatus {
    pub total_drivers: usize,
    pub ready_drivers: usize,
    pub total_devices: usize,
    pub graphics_ready: bool,
    pub input_ready: bool,
}

/// Global driver manager state (simplified)
static mut DRIVER_MANAGER_INITIALIZED: bool = false;
static mut GRAPHICS_INITIALIZED: bool = false;

/// Initialize the global driver manager (simplified)
pub fn init_drivers() -> Result<(), &'static str> {
    unsafe {
        if DRIVER_MANAGER_INITIALIZED {
            return Ok(());
        }
    }

    // Initialize PCI subsystem
    if let Err(_e) = init_pci() {
        return Err("PCI initialization failed");
    }

    // Initialize hot-plug subsystem
    if let Err(_e) = init_hotplug() {
        return Err("Hot-plug initialization failed");
    }

    // Seed hot-plug state from devices discovered during the initial PCI scan.
    let _ = scan_hotplug_devices();

    // Process any initial hot-plug events
    let _ = process_hotplug_events();

    // Initialize VirtIO devices (virtio-net, virtio-blk)
    if let Err(e) = virtio::init() {
        crate::serial_println!("virtio: init failed: {}", e);
    }

    if let Err(e) = dma::init() {
        crate::serial_println!("dma: init failed: {}", e);
    }

    if let Err(e) = i2c::init() {
        crate::serial_println!("i2c: init failed: {}", e);
    }

    if let Err(e) = thermal::init() {
        crate::serial_println!("thermal: init failed: {}", e);
    }

    if let Err(e) = regulator::init() {
        crate::serial_println!("regulator: init failed: {}", e);
    }

    if let Err(e) = hid::init() {
        crate::serial_println!("hid: init failed: {}", e);
    }

    if let Err(e) = gpio::init() {
        crate::serial_println!("gpio: init failed: {}", e);
    }

    if let Err(e) = leds::init() {
        crate::serial_println!("leds: init failed: {}", e);
    }

    if let Err(e) = hwmon::init() {
        crate::serial_println!("hwmon: init failed: {}", e);
    }

    if let Err(e) = platform::init() {
        crate::serial_println!("platform: init failed: {}", e);
    }

    if let Err(e) = regmap::init() {
        crate::serial_println!("regmap: init failed: {}", e);
    }

    if let Err(e) = firmware::init() {
        crate::serial_println!("firmware: init failed: {}", e);
    }

    if let Err(e) = dma_buf::init() {
        crate::serial_println!("dma_buf: init failed: {}", e);
    }

    if let Err(e) = dvb::init() {
        crate::serial_println!("dvb: init failed: {}", e);
    }

    if let Err(e) = pwm::init() {
        crate::serial_println!("pwm: init failed: {}", e);
    }

    if let Err(e) = nvmem::init() {
        crate::serial_println!("nvmem: init failed: {}", e);
    }

    if let Err(e) = clk::init() {
        crate::serial_println!("clk: init failed: {}", e);
    }

    if let Err(e) = pinctrl::init() {
        crate::serial_println!("pinctrl: init failed: {}", e);
    }

    if let Err(e) = power_supply::init() {
        crate::serial_println!("power_supply: init failed: {}", e);
    }

    if let Err(e) = backlight::init() {
        crate::serial_println!("backlight: init failed: {}", e);
    }

    let input_hardware_ready = ps2_controller::get_device_info()
        .map(|(port1_available, port1_device, port2_available, port2_device)| {
            (port1_available && port1_device == ps2_controller::Ps2DeviceType::Keyboard)
                || (port2_available
                    && matches!(
                        port2_device,
                        ps2_controller::Ps2DeviceType::StandardMouse
                            | ps2_controller::Ps2DeviceType::MouseWithScrollWheel
                            | ps2_controller::Ps2DeviceType::Mouse5Button
                    ))
        })
        .unwrap_or(false);

    if input_hardware_ready {
        if let Err(e) = input::init() {
            crate::serial_println!("input: init failed: {}", e);
        }
        input::evdev::init_evdev_devices();
    } else {
        crate::serial_println!("input: no initialized hardware; skipping generic device registration");
    }

    if let Err(e) = char::init() {
        crate::serial_println!("char: init failed: {}", e);
    }

    if let Err(e) = misc::init() {
        crate::serial_println!("misc: init failed: {}", e);
    }

    if let Err(e) = reset::init() {
        crate::serial_println!("reset: init failed: {}", e);
    }

    if let Err(e) = mtd::init() {
        crate::serial_println!("mtd: init failed: {}", e);
    }

    if let Err(e) = mmc::init() {
        crate::serial_println!("mmc: init failed: {}", e);
    }

    if let Err(e) = opp::init() {
        crate::serial_println!("opp: init failed: {}", e);
    }

    if let Err(e) = iio::init() {
        crate::serial_println!("iio: init failed: {}", e);
    }

    if let Err(e) = extcon::init() {
        crate::serial_println!("extcon: init failed: {}", e);
    }

    if let Err(e) = phy::init() {
        crate::serial_println!("phy: init failed: {}", e);
    }

    if let Err(e) = devfreq::init() {
        crate::serial_println!("devfreq: init failed: {}", e);
    }

    if let Err(e) = pmdomain::init() {
        crate::serial_println!("pmdomain: init failed: {}", e);
    }

    if let Err(e) = ptp::init() {
        crate::serial_println!("ptp: init failed: {}", e);
    }

    if let Err(e) = counter::init() {
        crate::serial_println!("counter: init failed: {}", e);
    }

    if let Err(e) = mailbox::init() {
        crate::serial_println!("mailbox: init failed: {}", e);
    }

    if let Err(e) = edac::init() {
        crate::serial_println!("edac: init failed: {}", e);
    }

    if let Err(e) = nfc::init() {
        crate::serial_println!("nfc: init failed: {}", e);
    }

    // sound::init() is called from boot_ui::driver_loading_progress() which
    // also marks the subsystem.  Calling it here would create a duplicate
    // sound card (NEXT_CARD is monotonic, not idempotent).

    if let Err(e) = nvdimm::init() {
        crate::serial_println!("nvdimm: init failed: {}", e);
    }

    if let Err(e) = vfio::init() {
        crate::serial_println!("vfio: init failed: {}", e);
    }

    if let Err(e) = spmi::init() {
        crate::serial_println!("spmi: init failed: {}", e);
    }

    if let Err(e) = cec::init() {
        crate::serial_println!("cec: init failed: {}", e);
    }

    if let Err(e) = cxl::init() {
        crate::serial_println!("cxl: init failed: {}", e);
    }

    if let Err(e) = dax::init() {
        crate::serial_println!("dax: init failed: {}", e);
    }

    if let Err(e) = dpll::init() {
        crate::serial_println!("dpll: init failed: {}", e);
    }

    if let Err(e) = iommufd::init() {
        crate::serial_println!("iommufd: init failed: {}", e);
    }

    if let Err(e) = isdbt::init() {
        crate::serial_println!("isdbt: init failed: {}", e);
    }

    if let Err(e) = ntb::init() {
        crate::serial_println!("ntb: init failed: {}", e);
    }

    if let Err(e) = ntsync::init() {
        crate::serial_println!("ntsync: init failed: {}", e);
    }

    scsi::init();

    if let Err(e) = v4l2::init() {
        crate::serial_println!("v4l2: init failed: {}", e);
    }

    if let Err(e) = amba::init() {
        crate::serial_println!("amba: init failed: {}", e);
    }

    if let Err(e) = hidraw::init() {
        crate::serial_println!("hidraw: init failed: {}", e);
    }

    if let Err(e) = hsi::init() {
        crate::serial_println!("hsi: init failed: {}", e);
    }

    if let Err(e) = ipack::init() {
        crate::serial_println!("ipack: init failed: {}", e);
    }

    if let Err(e) = ishtp::init() {
        crate::serial_println!("ishtp: init failed: {}", e);
    }

    if let Err(e) = mei::init() {
        crate::serial_println!("mei: init failed: {}", e);
    }

    if let Err(e) = nvme::init() {
        crate::serial_println!("nvme: init failed: {}", e);
    }

    if let Err(e) = peci::init() {
        crate::serial_println!("peci: init failed: {}", e);
    }

    if let Err(e) = rpmsg::init() {
        crate::serial_println!("rpmsg: init failed: {}", e);
    }

    if let Err(e) = slimbus::init() {
        crate::serial_println!("slimbus: init failed: {}", e);
    }

    if let Err(e) = slimproc::init() {
        crate::serial_println!("slimproc: init failed: {}", e);
    }

    if let Err(e) = soc::init() {
        crate::serial_println!("soc: init failed: {}", e);
    }

    if let Err(e) = tee::init() {
        crate::serial_println!("tee: init failed: {}", e);
    }

    if let Err(e) = udmabuf::init() {
        crate::serial_println!("udmabuf: init failed: {}", e);
    }

    if let Err(e) = virtio_pci::init() {
        crate::serial_println!("virtio_pci: init failed: {}", e);
    }

    if let Err(e) = wmi::init() {
        crate::serial_println!("wmi: init failed: {}", e);
    }

    if let Err(e) = xen::init() {
        crate::serial_println!("xen: init failed: {}", e);
    }

    if let Err(e) = block::init() {
        crate::serial_println!("block: init failed: {}", e);
    }

    if let Err(e) = bt::init() {
        crate::serial_println!("bt: init failed: {}", e);
    }

    if let Err(e) = cdrom::init() {
        crate::serial_println!("cdrom: init failed: {}", e);
    }

    if let Err(e) = crypto::init() {
        crate::serial_println!("crypto: init failed: {}", e);
    }

    if let Err(e) = firewire::init() {
        crate::serial_println!("firewire: init failed: {}", e);
    }

    if let Err(e) = hwrng::init() {
        crate::serial_println!("hwrng: init failed: {}", e);
    }

    if let Err(e) = infiniband::init() {
        crate::serial_println!("infiniband: init failed: {}", e);
    }

    if let Err(e) = mfd::init() {
        crate::serial_println!("mfd: init failed: {}", e);
    }

    if let Err(e) = serio::init() {
        crate::serial_println!("serio: init failed: {}", e);
    }

    if let Err(e) = thunderbolt::init() {
        crate::serial_println!("thunderbolt: init failed: {}", e);
    }

    if let Err(e) = apm::init() {
        crate::serial_println!("apm: init failed: {}", e);
    }

    if let Err(e) = parport::init() {
        crate::serial_println!("parport: init failed: {}", e);
    }

    if let Err(e) = agp::init() {
        crate::serial_println!("agp: init failed: {}", e);
    }

    if let Err(e) = auxdisplay::init() {
        crate::serial_println!("auxdisplay: init failed: {}", e);
    }

    if let Err(e) = isdn::init() {
        crate::serial_println!("isdn: init failed: {}", e);
    }

    if let Err(e) = pcmcia::init() {
        crate::serial_println!("pcmcia: init failed: {}", e);
    }

    if let Err(e) = powercap::init() {
        crate::serial_println!("powercap: init failed: {}", e);
    }

    if let Err(e) = ras::init() {
        crate::serial_println!("ras: init failed: {}", e);
    }

    if let Err(e) = coresight::init() {
        crate::serial_println!("coresight: init failed: {}", e);
    }

    if let Err(e) = hte::init() {
        crate::serial_println!("hte: init failed: {}", e);
    }

    if let Err(e) = acpi::init() {
        crate::serial_println!("acpi: init failed: {}", e);
    }

    if let Err(e) = auxiliary::init() {
        crate::serial_println!("auxiliary: init failed: {}", e);
    }

    if let Err(e) = cdx::init() {
        crate::serial_println!("cdx: init failed: {}", e);
    }

    if let Err(e) = eisa::init() {
        crate::serial_println!("eisa: init failed: {}", e);
    }

    if let Err(e) = ffa::init() {
        crate::serial_println!("ffa: init failed: {}", e);
    }

    if let Err(e) = fpga::init() {
        crate::serial_println!("fpga: init failed: {}", e);
    }

    if let Err(e) = gnss::init() {
        crate::serial_println!("gnss: init failed: {}", e);
    }

    if let Err(e) = gpu::init() {
        crate::serial_println!("gpu: init failed: {}", e);
    }

    if let Err(e) = hwspinlock::init() {
        crate::serial_println!("hwspinlock: init failed: {}", e);
    }

    if let Err(e) = i3c::init() {
        crate::serial_println!("i3c: init failed: {}", e);
    }

    if let Err(e) = interconnect::init() {
        crate::serial_println!("interconnect: init failed: {}", e);
    }

    if let Err(e) = iommu::init() {
        crate::serial_println!("iommu: init failed: {}", e);
    }

    if let Err(e) = isapnp::init() {
        crate::serial_println!("isapnp: init failed: {}", e);
    }

    if let Err(e) = moxtet::init() {
        crate::serial_println!("moxtet: init failed: {}", e);
    }

    if let Err(e) = mux::init() {
        crate::serial_println!("mux: init failed: {}", e);
    }

    if let Err(e) = nvmem_layouts::init() {
        crate::serial_println!("nvmem_layouts: init failed: {}", e);
    }

    if let Err(e) = of::init() {
        crate::serial_println!("of: init failed: {}", e);
    }

    if let Err(e) = soundwire::init() {
        crate::serial_println!("soundwire: init failed: {}", e);
    }

    if let Err(e) = spi::init() {
        crate::serial_println!("spi: init failed: {}", e);
    }

    if let Err(e) = ufs::init() {
        crate::serial_println!("ufs: init failed: {}", e);
    }

    if let Err(e) = usb::init() {
        crate::serial_println!("usb: init failed: {}", e);
    }

    if let Err(e) = vhost::init() {
        crate::serial_println!("vhost: init failed: {}", e);
    }

    if let Err(e) = w1::init() {
        crate::serial_println!("w1: init failed: {}", e);
    }

    if let Err(e) = vdpa::init() {
        crate::serial_println!("vdpa: init failed: {}", e);
    }

    if let Err(e) = uio::init() {
        crate::serial_println!("uio: init failed: {}", e);
    }

    if let Err(e) = remoteproc::init() {
        crate::serial_println!("remoteproc: init failed: {}", e);
    }

    if let Err(e) = perf::init() {
        crate::serial_println!("perf: init failed: {}", e);
    }

    if let Err(e) = pnp::init() {
        crate::serial_println!("pnp: init failed: {}", e);
    }

    if let Err(e) = ata::init() {
        crate::serial_println!("ata: init failed: {}", e);
    }

    if let Err(e) = bus::init() {
        crate::serial_println!("bus: init failed: {}", e);
    }

    if let Err(e) = cache::init() {
        crate::serial_println!("cache: init failed: {}", e);
    }

    if let Err(e) = clocksource::init() {
        crate::serial_println!("clocksource: init failed: {}", e);
    }

    if let Err(e) = irqchip::init() {
        crate::serial_println!("irqchip: init failed: {}", e);
    }

    if let Err(e) = media::init() {
        crate::serial_println!("media: init failed: {}", e);
    }

    if let Err(e) = memory::init() {
        crate::serial_println!("memory: init failed: {}", e);
    }

    if let Err(e) = power::init() {
        crate::serial_println!("power: init failed: {}", e);
    }

    if let Err(e) = pps::init() {
        crate::serial_println!("pps: init failed: {}", e);
    }

    if let Err(e) = hwtracing::init() {
        crate::serial_println!("hwtracing: init failed: {}", e);
    }

    if let Err(e) = resctrl::init() {
        crate::serial_println!("resctrl: init failed: {}", e);
    }

    if let Err(e) = target::init() {
        crate::serial_println!("target: init failed: {}", e);
    }

    if let Err(e) = video::init() {
        crate::serial_println!("video: init failed: {}", e);
    }

    if let Err(e) = virt::init() {
        crate::serial_println!("virt: init failed: {}", e);
    }

    if let Err(e) = connector::init() {
        crate::serial_println!("connector: init failed: {}", e);
    }

    if let Err(e) = dca::init() {
        crate::serial_println!("dca: init failed: {}", e);
    }

    if let Err(e) = memstick::init() {
        crate::serial_println!("memstick: init failed: {}", e);
    }

    if let Err(e) = rapidio::init() {
        crate::serial_println!("rapidio: init failed: {}", e);
    }

    if let Err(e) = linux_mirror::init() {
        crate::serial_println!("linux_mirror: init failed: {}", e);
    }

    if let Err(e) = bcma::init() {
        crate::serial_println!("bcma: init failed: {}", e);
    }

    if let Err(e) = ssb::init() {
        crate::serial_println!("ssb: init failed: {}", e);
    }

    if let Err(e) = fwctl::init() {
        crate::serial_println!("fwctl: init failed: {}", e);
    }

    if let Err(e) = hv::init() {
        crate::serial_println!("hv: init failed: {}", e);
    }

    if let Err(e) = idle::init() {
        crate::serial_println!("idle: init failed: {}", e);
    }

    // Driver-core device model (must come before subsystems that may register
    // into it) plus the remaining Linux-mirror subsystems.
    if let Err(e) = base::init() {
        crate::serial_println!("base: init failed: {}", e);
    }
    if let Err(e) = accel::init() {
        crate::serial_println!("accel: init failed: {}", e);
    }
    if let Err(e) = android::init() {
        crate::serial_println!("android: init failed: {}", e);
    }
    if let Err(e) = atm::init() {
        crate::serial_println!("atm: init failed: {}", e);
    }
    if let Err(e) = comedi::init() {
        crate::serial_println!("comedi: init failed: {}", e);
    }
    if let Err(e) = fsi::init() {
        crate::serial_println!("fsi: init failed: {}", e);
    }
    if let Err(e) = gpib::init() {
        crate::serial_println!("gpib: init failed: {}", e);
    }
    if let Err(e) = greybus::init() {
        crate::serial_println!("greybus: init failed: {}", e);
    }
    if let Err(e) = mcb::init() {
        crate::serial_println!("mcb: init failed: {}", e);
    }
    if let Err(e) = message::init() {
        crate::serial_println!("message: init failed: {}", e);
    }
    if let Err(e) = most::init() {
        crate::serial_println!("most: init failed: {}", e);
    }
    if let Err(e) = siox::init() {
        crate::serial_println!("siox: init failed: {}", e);
    }

    unsafe {
        DRIVER_MANAGER_INITIALIZED = true;
        GRAPHICS_INITIALIZED = true;
    }

    // Display driver statistics
    let _pci_stats = get_pci_stats();
    let _hotplug_stats = get_hotplug_stats();

    // Production: drivers initialized silently

    Ok(())
}

/// Check if driver manager is initialized
pub fn is_driver_manager_initialized() -> bool {
    unsafe { DRIVER_MANAGER_INITIALIZED }
}

/// Get driver system status (simplified)
pub fn get_driver_system_status() -> Option<DriverSystemStatus> {
    use crate::pci::{get_devices_by_class, list_devices, PciClass};

    unsafe {
        if DRIVER_MANAGER_INITIALIZED {
            let total_devices = list_devices().len();
            let gpu_count = get_devices_by_class(PciClass::Display).len();
            let net_count = get_devices_by_class(PciClass::Network).len();
            let storage_count = get_devices_by_class(PciClass::MassStorage).len();
            let total_drivers = gpu_count + net_count + storage_count + 2; // +2 for input
            Some(DriverSystemStatus {
                total_drivers,
                ready_drivers: total_drivers,
                total_devices,
                graphics_ready: GRAPHICS_INITIALIZED,
                input_ready: true,
            })
        } else {
            None
        }
    }
}

/// Check if graphics drivers are ready
pub fn is_graphics_ready() -> bool {
    unsafe { GRAPHICS_INITIALIZED }
}

/// Check if input drivers are ready
pub fn is_input_ready() -> bool {
    unsafe { DRIVER_MANAGER_INITIALIZED }
}

/// Print driver information (simplified)
pub fn print_driver_info() {
    unsafe {
        if DRIVER_MANAGER_INITIALIZED {
            // Driver system initialized
            // Total Drivers: 4
            // Ready Drivers: 4
            // Total Devices: 4
            // Graphics Ready
        }
    }
}

#[cfg(all(test, feature = "disabled-tests"))]
mod tests {
    use super::*;
    use crate::{serial_print, serial_println, ToString};
    use alloc::format;

    #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_driver_info_creation() {
        serial_print!("test_driver_info_creation... ");
        let driver = DriverInfo::new(
            "Test Driver".to_string(),
            "1.0.0".to_string(),
            DriverType::Graphics,
            "Test Vendor".to_string(),
            "Test Description".to_string(),
        );

        assert_eq!(driver.name, "Test Driver");
        assert_eq!(driver.version, "1.0.0");
        assert_eq!(driver.driver_type, DriverType::Graphics);
        assert_eq!(driver.status, DriverStatus::Uninitialized);
        assert_eq!(driver.vendor, "Test Vendor");
        assert!(driver.device_id.is_none());
        serial_println!("[ok]");
    }

    #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_device_info_creation() {
        serial_print!("test_device_info_creation... ");
        let device = DeviceInfo::new(
            0x8086,
            0x1234,
            0x03,
            0x00,
            0x00,
            0x01,
            0x00,
            0x02,
            0x00,
            "Test Graphics Card".to_string(),
        );

        assert_eq!(device.vendor_id, 0x8086);
        assert_eq!(device.device_id, 0x1234);
        assert_eq!(device.get_vendor_name(), "Intel");
        assert_eq!(device.get_device_type(), DriverType::Graphics);
        assert!(device.is_graphics_device());
        assert!(!device.driver_loaded);
        serial_println!("[ok]");
    }

    #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_driver_manager_creation() {
        serial_print!("test_driver_manager_creation... ");
        let manager = DriverManager::new();
        assert_eq!(manager.driver_count(), 0);
        assert_eq!(manager.device_count(), 0);
        assert!(!manager.is_graphics_initialized());
        assert!(!manager.is_input_initialized());
        serial_println!("[ok]");
    }

    #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_driver_types_display() {
        serial_print!("test_driver_types_display... ");
        assert_eq!(format!("{}", DriverType::Graphics), "Graphics");
        assert_eq!(format!("{}", DriverType::Network), "Network");
        assert_eq!(format!("{}", DriverType::Storage), "Storage");
        serial_println!("[ok]");
    }

    #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case]
    fn test_driver_status_display() {
        serial_print!("test_driver_status_display... ");
        assert_eq!(format!("{}", DriverStatus::Ready), "Ready");
        assert_eq!(format!("{}", DriverStatus::Error), "Error");
        assert_eq!(format!("{}", DriverStatus::Uninitialized), "Uninitialized");
        serial_println!("[ok]");
    }
}
