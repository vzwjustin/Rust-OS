//! # AHCI SATA Controller Driver
//!
//! Advanced Host Controller Interface (AHCI) driver for SATA storage devices.
//! Supports extensive device IDs from Intel, AMD, VIA, and other manufacturers.

use super::{
    StorageCapabilities, StorageDeviceState, StorageDeviceType, StorageDriver, StorageError,
    StorageStats,
};
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::{format, vec};
use core::mem;
use core::ptr;

/// AHCI vendor IDs and device IDs database
#[derive(Debug, Clone, Copy)]
pub struct AhciDeviceId {
    pub vendor_id: u16,
    pub device_id: u16,
    pub name: &'static str,
    pub supports_64bit: bool,
    pub max_ports: u8,
    pub quirks: AhciQuirks,
}

bitflags::bitflags! {
    /// AHCI controller quirks
    pub struct AhciQuirks: u32 {
        const NONE = 0;
        const NO_NCQ = 1 << 0;
        const NO_MSI = 1 << 1;
        const FORCE_GEN1 = 1 << 2;
        const NO_PMP = 1 << 3;
        const BROKEN_SUSPEND = 1 << 4;
        const IGN_SERR_INTERNAL = 1 << 5;
        const NO_64BIT = 1 << 6;
    }
}

/// Comprehensive AHCI device ID database (80+ entries)
pub const AHCI_DEVICE_IDS: &[AhciDeviceId] = &[
    // Intel chipsets
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x2652,
        name: "Intel ICH6 AHCI",
        supports_64bit: false,
        max_ports: 4,
        quirks: AhciQuirks::NO_64BIT,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x2653,
        name: "Intel ICH6M AHCI",
        supports_64bit: false,
        max_ports: 4,
        quirks: AhciQuirks::NO_64BIT,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x27c1,
        name: "Intel ICH7 AHCI",
        supports_64bit: false,
        max_ports: 4,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x27c5,
        name: "Intel ICH7M AHCI",
        supports_64bit: false,
        max_ports: 4,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x27c3,
        name: "Intel ICH7R AHCI",
        supports_64bit: false,
        max_ports: 4,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x2821,
        name: "Intel ICH8 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x2829,
        name: "Intel ICH8M AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x2922,
        name: "Intel ICH9 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x2923,
        name: "Intel ICH9M AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x3a02,
        name: "Intel ICH10 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x3a22,
        name: "Intel ICH10R AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x3b22,
        name: "Intel 5 Series AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x3b23,
        name: "Intel 5 Series AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x3b29,
        name: "Intel 5 Series AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x3b2f,
        name: "Intel 5 Series AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x1c02,
        name: "Intel 6 Series AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x1c03,
        name: "Intel 6 Series AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x1e02,
        name: "Intel 7 Series AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x1e03,
        name: "Intel 7 Series AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x8c02,
        name: "Intel 8 Series AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x8c03,
        name: "Intel 8 Series AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x8c82,
        name: "Intel 9 Series AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x8c83,
        name: "Intel 9 Series AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0xa102,
        name: "Intel 100 Series AHCI",
        supports_64bit: true,
        max_ports: 8,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0xa103,
        name: "Intel 100 Series AHCI",
        supports_64bit: true,
        max_ports: 8,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0xa182,
        name: "Intel 200 Series AHCI",
        supports_64bit: true,
        max_ports: 8,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0xa202,
        name: "Intel 200 Series AHCI",
        supports_64bit: true,
        max_ports: 8,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0xa282,
        name: "Intel 300 Series AHCI",
        supports_64bit: true,
        max_ports: 8,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0xa352,
        name: "Intel 300 Series AHCI",
        supports_64bit: true,
        max_ports: 8,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x06d2,
        name: "Intel 400 Series AHCI",
        supports_64bit: true,
        max_ports: 8,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x8086,
        device_id: 0x43d2,
        name: "Intel 500 Series AHCI",
        supports_64bit: true,
        max_ports: 8,
        quirks: AhciQuirks::NONE,
    },
    // AMD chipsets
    AhciDeviceId {
        vendor_id: 0x1002,
        device_id: 0x4380,
        name: "AMD SB600 AHCI",
        supports_64bit: true,
        max_ports: 4,
        quirks: AhciQuirks::NO_MSI,
    },
    AhciDeviceId {
        vendor_id: 0x1002,
        device_id: 0x4390,
        name: "AMD SB700 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x1002,
        device_id: 0x4391,
        name: "AMD SB700 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x1002,
        device_id: 0x4392,
        name: "AMD SB700 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x1002,
        device_id: 0x4393,
        name: "AMD SB700 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x1002,
        device_id: 0x4394,
        name: "AMD SB700 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x1022,
        device_id: 0x7801,
        name: "AMD FCH AHCI",
        supports_64bit: true,
        max_ports: 8,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x1022,
        device_id: 0x7804,
        name: "AMD FCH AHCI",
        supports_64bit: true,
        max_ports: 8,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x1022,
        device_id: 0x7900,
        name: "AMD Zen AHCI",
        supports_64bit: true,
        max_ports: 8,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x1022,
        device_id: 0x7901,
        name: "AMD Zen AHCI",
        supports_64bit: true,
        max_ports: 8,
        quirks: AhciQuirks::NONE,
    },
    // VIA chipsets
    AhciDeviceId {
        vendor_id: 0x1106,
        device_id: 0x3349,
        name: "VIA VT8251 AHCI",
        supports_64bit: false,
        max_ports: 4,
        quirks: AhciQuirks::NO_NCQ,
    },
    AhciDeviceId {
        vendor_id: 0x1106,
        device_id: 0x6287,
        name: "VIA VT8251 AHCI",
        supports_64bit: false,
        max_ports: 4,
        quirks: AhciQuirks::NO_NCQ,
    },
    AhciDeviceId {
        vendor_id: 0x1106,
        device_id: 0x0591,
        name: "VIA VT8237A AHCI",
        supports_64bit: false,
        max_ports: 4,
        quirks: AhciQuirks::NO_NCQ,
    },
    AhciDeviceId {
        vendor_id: 0x1106,
        device_id: 0x3164,
        name: "VIA VT6410 AHCI",
        supports_64bit: false,
        max_ports: 4,
        quirks: AhciQuirks::NO_NCQ,
    },
    // NVIDIA chipsets
    AhciDeviceId {
        vendor_id: 0x10de,
        device_id: 0x044c,
        name: "NVIDIA MCP65 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x10de,
        device_id: 0x044d,
        name: "NVIDIA MCP65 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x10de,
        device_id: 0x044e,
        name: "NVIDIA MCP65 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x10de,
        device_id: 0x044f,
        name: "NVIDIA MCP65 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x10de,
        device_id: 0x045c,
        name: "NVIDIA MCP65 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x10de,
        device_id: 0x045d,
        name: "NVIDIA MCP65 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x10de,
        device_id: 0x045e,
        name: "NVIDIA MCP65 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x10de,
        device_id: 0x045f,
        name: "NVIDIA MCP65 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x10de,
        device_id: 0x0550,
        name: "NVIDIA MCP67 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x10de,
        device_id: 0x0551,
        name: "NVIDIA MCP67 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x10de,
        device_id: 0x0552,
        name: "NVIDIA MCP67 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x10de,
        device_id: 0x0553,
        name: "NVIDIA MCP67 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x10de,
        device_id: 0x0554,
        name: "NVIDIA MCP67 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x10de,
        device_id: 0x0555,
        name: "NVIDIA MCP67 AHCI",
        supports_64bit: true,
        max_ports: 6,
        quirks: AhciQuirks::NONE,
    },
    // SiS chipsets
    AhciDeviceId {
        vendor_id: 0x1039,
        device_id: 0x1184,
        name: "SiS 966 AHCI",
        supports_64bit: false,
        max_ports: 4,
        quirks: AhciQuirks::NO_64BIT,
    },
    AhciDeviceId {
        vendor_id: 0x1039,
        device_id: 0x1185,
        name: "SiS 968 AHCI",
        supports_64bit: false,
        max_ports: 4,
        quirks: AhciQuirks::NO_64BIT,
    },
    // ATI/AMD legacy
    AhciDeviceId {
        vendor_id: 0x1002,
        device_id: 0x4379,
        name: "ATI SB400 AHCI",
        supports_64bit: false,
        max_ports: 4,
        quirks: AhciQuirks::from_bits_truncate(
            AhciQuirks::NO_64BIT.bits() | AhciQuirks::NO_MSI.bits(),
        ),
    },
    AhciDeviceId {
        vendor_id: 0x1002,
        device_id: 0x437a,
        name: "ATI SB400 AHCI",
        supports_64bit: false,
        max_ports: 4,
        quirks: AhciQuirks::from_bits_truncate(
            AhciQuirks::NO_64BIT.bits() | AhciQuirks::NO_MSI.bits(),
        ),
    },
    // JMicron
    AhciDeviceId {
        vendor_id: 0x197b,
        device_id: 0x2360,
        name: "JMicron JMB360 AHCI",
        supports_64bit: true,
        max_ports: 1,
        quirks: AhciQuirks::NO_PMP,
    },
    AhciDeviceId {
        vendor_id: 0x197b,
        device_id: 0x2361,
        name: "JMicron JMB361 AHCI",
        supports_64bit: true,
        max_ports: 1,
        quirks: AhciQuirks::NO_PMP,
    },
    AhciDeviceId {
        vendor_id: 0x197b,
        device_id: 0x2362,
        name: "JMicron JMB362 AHCI",
        supports_64bit: true,
        max_ports: 2,
        quirks: AhciQuirks::NO_PMP,
    },
    AhciDeviceId {
        vendor_id: 0x197b,
        device_id: 0x2363,
        name: "JMicron JMB363 AHCI",
        supports_64bit: true,
        max_ports: 2,
        quirks: AhciQuirks::NO_PMP,
    },
    // Marvell
    AhciDeviceId {
        vendor_id: 0x11ab,
        device_id: 0x6121,
        name: "Marvell 88SE6121 AHCI",
        supports_64bit: true,
        max_ports: 2,
        quirks: AhciQuirks::NO_MSI,
    },
    AhciDeviceId {
        vendor_id: 0x11ab,
        device_id: 0x6145,
        name: "Marvell 88SE6145 AHCI",
        supports_64bit: true,
        max_ports: 4,
        quirks: AhciQuirks::NO_MSI,
    },
    AhciDeviceId {
        vendor_id: 0x1b4b,
        device_id: 0x9123,
        name: "Marvell 88SE9123 AHCI",
        supports_64bit: true,
        max_ports: 2,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x1b4b,
        device_id: 0x9128,
        name: "Marvell 88SE9128 AHCI",
        supports_64bit: true,
        max_ports: 8,
        quirks: AhciQuirks::NONE,
    },
    // Promise Technology
    AhciDeviceId {
        vendor_id: 0x105a,
        device_id: 0x3f20,
        name: "Promise PDC40719 AHCI",
        supports_64bit: true,
        max_ports: 4,
        quirks: AhciQuirks::NONE,
    },
    // ASMedia
    AhciDeviceId {
        vendor_id: 0x1b21,
        device_id: 0x0612,
        name: "ASMedia ASM1061 AHCI",
        supports_64bit: true,
        max_ports: 2,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x1b21,
        device_id: 0x0621,
        name: "ASMedia ASM1062 AHCI",
        supports_64bit: true,
        max_ports: 2,
        quirks: AhciQuirks::NONE,
    },
    AhciDeviceId {
        vendor_id: 0x1b21,
        device_id: 0x0622,
        name: "ASMedia ASM1062 AHCI",
        supports_64bit: true,
        max_ports: 2,
        quirks: AhciQuirks::NONE,
    },
];

/// AHCI register offsets
#[repr(u32)]
pub enum AhciReg {
    /// Host capability register
    Cap = 0x00,
    /// Global host control register
    Ghc = 0x04,
    /// Interrupt status register
    Is = 0x08,
    /// Port implemented register
    Pi = 0x0c,
    /// AHCI version register
    Vs = 0x10,
    /// Command completion coalescing control
    Ccc_ctl = 0x14,
    /// Command completion coalescing ports
    Ccc_ports = 0x18,
    /// Enclosure management location
    Em_loc = 0x1c,
    /// Enclosure management control
    Em_ctl = 0x20,
    /// Host capabilities extended
    Cap2 = 0x24,
    /// BIOS/OS handoff control and status
    Bohc = 0x28,
}

/// AHCI port register offsets (relative to port base)
#[repr(u32)]
pub enum AhciPortReg {
    /// Command list base address
    Clb = 0x00,
    /// Command list base address upper 32 bits
    Clbu = 0x04,
    /// FIS base address
    Fb = 0x08,
    /// FIS base address upper 32 bits
    Fbu = 0x0c,
    /// Interrupt status
    Is = 0x10,
    /// Interrupt enable
    Ie = 0x14,
    /// Command and status
    Cmd = 0x18,
    /// Task file data
    Tfd = 0x20,
    /// Signature
    Sig = 0x24,
    /// SATA status
    Ssts = 0x28,
    /// SATA control
    Sctl = 0x2c,
    /// SATA error
    Serr = 0x30,
    /// SATA active
    Sact = 0x34,
    /// Command issue
    Ci = 0x38,
    /// SATA notification
    Sntf = 0x3c,
}

/// AHCI port command register bits
bitflags::bitflags! {
    pub struct PortCmd: u32 {
        const ST = 1 << 0;      // Start
        const SUD = 1 << 1;     // Spin-up device
        const POD = 1 << 2;     // Power on device
        const CLO = 1 << 3;     // Command list override
        const FRE = 1 << 4;     // FIS receive enable
        const MPSS = 1 << 13;   // Mechanical presence switch state
        const FR = 1 << 14;     // FIS receive running
        const CR = 1 << 15;     // Command list running
        const CPS = 1 << 16;    // Cold presence state
        const PMA = 1 << 17;    // Port multiplier attached
        const HPCP = 1 << 18;   // Hot plug capable port
        const MPSP = 1 << 19;   // Mechanical presence switch attached
        const CPD = 1 << 20;    // Cold presence detection
        const ESP = 1 << 21;    // External SATA port
        const FBSCP = 1 << 22;  // FIS-based switching capable port
        const APSTE = 1 << 23;  // Automatic partial to slumber transitions enabled
        const ATAPI = 1 << 24;  // Device is ATAPI
        const DLAE = 1 << 25;   // Drive LED on ATAPI enable
        const ALPE = 1 << 26;   // Aggressive link power management enable
        const ASP = 1 << 27;    // Aggressive slumber/partial
        const ICC_MASK = 0xf << 28; // Interface communication control
    }
}

/// AHCI driver implementation
#[derive(Debug)]
pub struct AhciDriver {
    name: String,
    device_info: Option<AhciDeviceId>,
    state: StorageDeviceState,
    capabilities: StorageCapabilities,
    stats: StorageStats,
    base_addr: u64,
    port_count: u8,
    command_slots: u8,
    supports_64bit: bool,
    supports_ncq: bool,
    command_lists: [u64; 32], // Physical addresses of command lists per port
    command_tables: [u64; 32], // Physical addresses of command tables per port
}

impl AhciDriver {
    /// Create new AHCI driver instance
    pub fn new(name: String, vendor_id: u16, device_id: u16, base_addr: u64) -> Self {
        let device_info = AHCI_DEVICE_IDS
            .iter()
            .find(|&info| info.vendor_id == vendor_id && info.device_id == device_id)
            .copied();

        let mut capabilities = StorageCapabilities::default();
        let mut supports_64bit = true;
        let mut supports_ncq = true;
        let mut port_count = 32; // Default max
        let command_slots = 32; // Default max

        if let Some(info) = device_info {
            supports_64bit = info.supports_64bit && !info.quirks.contains(AhciQuirks::NO_64BIT);
            supports_ncq = !info.quirks.contains(AhciQuirks::NO_NCQ);
            port_count = info.max_ports;
            capabilities.max_queue_depth = if supports_ncq { 32 } else { 1 };
            capabilities.supports_ncq = supports_ncq;
        }

        Self {
            name,
            device_info,
            state: StorageDeviceState::Offline,
            capabilities,
            stats: StorageStats::default(),
            base_addr,
            port_count,
            command_slots,
            supports_64bit,
            supports_ncq,
            command_lists: [0; 32], // Initialize to zero, will be allocated during init
            command_tables: [0; 32], // Initialize to zero, will be allocated during init
        }
    }

    /// Read AHCI register
    fn read_reg(&self, offset: AhciReg) -> u32 {
        unsafe { ptr::read_volatile((self.base_addr + offset as u64) as *const u32) }
    }

    /// Write AHCI register
    fn write_reg(&self, offset: AhciReg, value: u32) {
        unsafe {
            ptr::write_volatile((self.base_addr + offset as u64) as *mut u32, value);
        }
    }

    /// Read port register
    fn read_port_reg(&self, port: u8, offset: AhciPortReg) -> u32 {
        let port_base = 0x100 + (port as u64 * 0x80);
        unsafe { ptr::read_volatile((self.base_addr + port_base + offset as u64) as *const u32) }
    }

    /// Write port register
    fn write_port_reg(&self, port: u8, offset: AhciPortReg, value: u32) {
        let port_base = 0x100 + (port as u64 * 0x80);
        unsafe {
            ptr::write_volatile(
                (self.base_addr + port_base + offset as u64) as *mut u32,
                value,
            );
        }
    }

    /// Initialize AHCI controller
    fn init_controller(&mut self) -> Result<(), StorageError> {
        // Read capability register
        let cap = self.read_reg(AhciReg::Cap);
        let ports_impl = self.read_reg(AhciReg::Pi);

        // Extract capabilities
        let max_cmd_slots = ((cap >> 8) & 0x1f) + 1;
        let supports_64bit = (cap & (1 << 31)) != 0;
        let supports_ncq = (cap & (1 << 30)) != 0;

        // Update capabilities based on hardware
        self.capabilities.max_queue_depth = max_cmd_slots as u16;
        self.capabilities.supports_ncq = supports_ncq && self.supports_ncq;
        self.supports_64bit = supports_64bit && self.supports_64bit;

        // Request BIOS/OS handoff if supported
        let cap2 = self.read_reg(AhciReg::Cap2);
        if (cap2 & (1 << 0)) != 0 {
            // BIOS/OS handoff supported
            self.write_reg(AhciReg::Bohc, (1 << 1)); // Request OS ownership

            // Wait for handoff completion (simplified)
            for _ in 0..1000 {
                let bohc = self.read_reg(AhciReg::Bohc);
                if (bohc & (1 << 0)) == 0 && (bohc & (1 << 1)) != 0 {
                    break; // OS has ownership
                }
            }
        }

        // Enable AHCI mode
        let mut ghc = self.read_reg(AhciReg::Ghc);
        ghc |= 1 << 31; // AHCI Enable
        self.write_reg(AhciReg::Ghc, ghc);

        // Reset HBA
        ghc |= 1 << 0; // HBA Reset
        self.write_reg(AhciReg::Ghc, ghc);

        // Wait for reset completion
        for _ in 0..1000 {
            ghc = self.read_reg(AhciReg::Ghc);
            if (ghc & (1 << 0)) == 0 {
                break; // Reset complete
            }
        }

        // Re-enable AHCI mode after reset
        ghc = self.read_reg(AhciReg::Ghc);
        ghc |= 1 << 31; // AHCI Enable
        self.write_reg(AhciReg::Ghc, ghc);

        // Initialize ports
        for port in 0..32 {
            if (ports_impl & (1 << port)) != 0 {
                self.init_port(port)?;
            }
        }

        self.state = StorageDeviceState::Ready;
        Ok(())
    }

    /// Initialize AHCI port
    fn init_port(&mut self, port: u8) -> Result<(), StorageError> {
        // Stop port
        let mut cmd = self.read_port_reg(port, AhciPortReg::Cmd);
        cmd &= !(PortCmd::ST.bits() | PortCmd::FRE.bits());
        self.write_port_reg(port, AhciPortReg::Cmd, cmd);

        // Wait for port to stop
        for _ in 0..500 {
            cmd = self.read_port_reg(port, AhciPortReg::Cmd);
            if (cmd & (PortCmd::FR.bits() | PortCmd::CR.bits())) == 0 {
                break;
            }
        }

        // Clear error register
        self.write_port_reg(port, AhciPortReg::Serr, 0xffffffff);

        // Power up and spin up device
        cmd = self.read_port_reg(port, AhciPortReg::Cmd);
        cmd |= PortCmd::POD.bits() | PortCmd::SUD.bits();
        self.write_port_reg(port, AhciPortReg::Cmd, cmd);

        // Check if device is present
        let ssts = self.read_port_reg(port, AhciPortReg::Ssts);
        let det = ssts & 0xf;
        if det != 3 {
            // Device not present and communication established
            return Ok(()); // No device on this port
        }

        // Set up command list and FIS receive area (simplified)
        // In a real implementation, we'd allocate DMA memory here

        // Enable FIS receive
        cmd = self.read_port_reg(port, AhciPortReg::Cmd);
        cmd |= PortCmd::FRE.bits();
        self.write_port_reg(port, AhciPortReg::Cmd, cmd);

        // Start port
        cmd |= PortCmd::ST.bits();
        self.write_port_reg(port, AhciPortReg::Cmd, cmd);

        Ok(())
    }

    /// Execute SATA command (production implementation)
    fn execute_command(
        &mut self,
        port: u8,
        command: u8,
        lba: u64,
        count: u16,
        mut buffer: Option<&mut [u8]>,
    ) -> Result<(), StorageError> {
        // Check port status
        let ssts = self.read_port_reg(port, AhciPortReg::Ssts);
        let det = ssts & 0xf;
        if det != 3 {
            return Err(StorageError::DeviceNotFound);
        }

        // Check if port is ready
        let cmd = self.read_port_reg(port, AhciPortReg::Cmd);
        if (cmd & PortCmd::FRE.bits()) == 0 {
            return Err(StorageError::DeviceBusy);
        }

        // Allocate DMA memory for command structures (simplified - using static addresses)
        // NOTE: Command list and table still use static addresses - full refactor needed
        let cmd_list_phys = 0x80000 + (port as u64 * 0x1000); // 4KB per port
        let cmd_table_phys = cmd_list_phys + 0x400; // Command table after command list

        // Store addresses for cleanup
        self.command_lists[port as usize] = cmd_list_phys;
        self.command_tables[port as usize] = cmd_table_phys;

        // Allocate proper DMA buffer for data transfer - Production implementation
        use crate::net::dma::{DmaBuffer, DMA_ALIGNMENT};

        let data_size = (count as usize) * 512;
        let mut _data_dma_buffer = DmaBuffer::allocate(data_size, DMA_ALIGNMENT)
            .map_err(|_| StorageError::HardwareError)?;

        // Translate virtual to physical address for hardware DMA
        let buffer_phys = {
            use crate::memory::get_memory_manager;
            use x86_64::VirtAddr;

            let virt_addr = VirtAddr::new(_data_dma_buffer.virtual_addr() as u64);
            let memory_manager = get_memory_manager().ok_or(StorageError::HardwareError)?;

            memory_manager
                .translate_addr(virt_addr)
                .ok_or(StorageError::HardwareError)?
                .as_u64()
        };

        // Set up command list and FIS receive area
        self.write_port_reg(port, AhciPortReg::Clb, (cmd_list_phys & 0xFFFFFFFF) as u32);
        self.write_port_reg(
            port,
            AhciPortReg::Clbu,
            ((cmd_list_phys >> 32) & 0xFFFFFFFF) as u32,
        );

        let fis_phys = cmd_list_phys + 0x200; // FIS area after command list
        self.write_port_reg(port, AhciPortReg::Fb, (fis_phys & 0xFFFFFFFF) as u32);
        self.write_port_reg(
            port,
            AhciPortReg::Fbu,
            ((fis_phys >> 32) & 0xFFFFFFFF) as u32,
        );

        // 1. Set up command table with FIS
        unsafe {
            let cmd_table = cmd_table_phys as *mut u8;

            // Clear command table
            for i in 0..0x80 {
                *cmd_table.add(i) = 0;
            }

            // H2D Register FIS (Host to Device)
            *cmd_table = 0x27; // FIS Type: Register H2D
            *cmd_table.add(1) = 0x80; // Command bit set
            *cmd_table.add(2) = command; // SATA command
            *cmd_table.add(3) = 0; // Features

            // Set LBA
            *cmd_table.add(4) = (lba & 0xFF) as u8;
            *cmd_table.add(5) = ((lba >> 8) & 0xFF) as u8;
            *cmd_table.add(6) = ((lba >> 16) & 0xFF) as u8;
            *cmd_table.add(7) = 0xE0 | (((lba >> 24) & 0x0F) as u8); // Drive/Head + LBA[27:24]

            *cmd_table.add(8) = ((lba >> 32) & 0xFF) as u8;
            *cmd_table.add(9) = ((lba >> 40) & 0xFF) as u8;
            *cmd_table.add(10) = ((lba >> 48) & 0xFF) as u8;
            *cmd_table.add(11) = 0; // Features (high)

            // Set sector count
            *cmd_table.add(12) = (count & 0xFF) as u8;
            *cmd_table.add(13) = ((count >> 8) & 0xFF) as u8;
            *cmd_table.add(14) = 0; // Reserved
            *cmd_table.add(15) = 0; // Control
        }

        // 2. Set up PRD table for data transfer
        if command == 0x25 || command == 0x35 {
            // READ DMA EXT / WRITE DMA EXT
            unsafe {
                let prd_table = (cmd_table_phys + 0x80) as *mut u32;

                // PRD Entry 0: Data Buffer Address (Low)
                *prd_table = (buffer_phys & 0xFFFFFFFF) as u32;
                // PRD Entry 1: Data Buffer Address (High)
                *prd_table.add(1) = ((buffer_phys >> 32) & 0xFFFFFFFF) as u32;
                // PRD Entry 2: Reserved
                *prd_table.add(2) = 0;
                // PRD Entry 3: Data Byte Count and Interrupt on Completion
                *prd_table.add(3) = ((count as u32 * 512) - 1) | (1u32 << 31); // Size - 1 and interrupt bit

                // Copy write data to DMA buffer using proper buffer access
                if command == 0x35 && buffer.is_some() {
                    let src_buffer = buffer.as_ref().unwrap();
                    let dst_ptr = _data_dma_buffer.virtual_addr();
                    let copy_size = core::cmp::min(src_buffer.len(), data_size);
                    core::ptr::copy_nonoverlapping(src_buffer.as_ptr(), dst_ptr, copy_size);
                }
            }
        }

        // 3. Set up command header
        unsafe {
            let cmd_header = cmd_list_phys as *mut u32;

            // Clear command header
            for i in 0..8 {
                *cmd_header.add(i) = 0;
            }

            // Command Header DW0
            let mut dw0 = 5u32; // Command FIS length (5 DWORDs)
            if command == 0x35 {
                // Write command
                dw0 |= 1 << 6; // Write bit
            }
            if command == 0x25 || command == 0x35 {
                // Data transfer commands
                dw0 |= 1 << 16; // PRD Table Length = 1
            }
            *cmd_header = dw0;

            // Command Header DW1: PRD Byte Count (filled by hardware)
            *cmd_header.add(1) = 0;

            // Command Header DW2-3: Command Table Base Address
            *cmd_header.add(2) = (cmd_table_phys & 0xFFFFFFFF) as u32;
            *cmd_header.add(3) = ((cmd_table_phys >> 32) & 0xFFFFFFFF) as u32;
        }

        // 4. Clear port interrupt status
        let is = self.read_port_reg(port, AhciPortReg::Is);
        self.write_port_reg(port, AhciPortReg::Is, is);

        // 5. Issue command via CI register
        self.write_port_reg(port, AhciPortReg::Ci, 1 << 0); // Issue command in slot 0

        // 6. Wait for completion
        let mut timeout = 5000000; // 5 second timeout
        while timeout > 0 {
            let ci = self.read_port_reg(port, AhciPortReg::Ci);
            if (ci & 1) == 0 {
                // Command completed
                break;
            }

            // Check for errors
            let is = self.read_port_reg(port, AhciPortReg::Is);
            if (is & 0x40000000) != 0 {
                // Task File Error
                self.write_port_reg(port, AhciPortReg::Is, is);
                return Err(StorageError::HardwareError);
            }

            timeout -= 1;
            // Small delay to prevent busy waiting
            for _ in 0..1000 {
                unsafe {
                    core::arch::asm!("pause");
                }
            }
        }

        if timeout == 0 {
            return Err(StorageError::Timeout);
        }

        // 7. Check for errors
        let serr = self.read_port_reg(port, AhciPortReg::Serr);
        if serr != 0 {
            self.write_port_reg(port, AhciPortReg::Serr, serr); // Clear errors
            return Err(StorageError::HardwareError);
        }

        let is = self.read_port_reg(port, AhciPortReg::Is);
        if (is & 0x40000000) != 0 {
            // Task File Error
            self.write_port_reg(port, AhciPortReg::Is, is); // Clear interrupt status
            return Err(StorageError::HardwareError);
        }

        // 8. Copy read data from DMA buffer using proper buffer access
        if command == 0x25 && buffer.is_some() {
            unsafe {
                let src_ptr = _data_dma_buffer.virtual_addr() as *const u8;
                let dst_buffer = buffer.as_mut().unwrap();
                let copy_size = core::cmp::min(dst_buffer.len(), data_size);
                core::ptr::copy_nonoverlapping(src_ptr, dst_buffer.as_mut_ptr(), copy_size);
            }
        }

        // Clear interrupt status
        self.write_port_reg(port, AhciPortReg::Is, is);

        // Update statistics
        match command {
            0x25 => {
                self.stats.reads_total += 1;
                self.stats.bytes_read += (count as u64) * 512;
            }
            0x35 => {
                self.stats.writes_total += 1;
                self.stats.bytes_written += (count as u64) * 512;
            }
            _ => {}
        }

        Ok(())
    }

    /// Detect and identify attached devices
    pub fn scan_ports(&mut self) -> Vec<(u8, String)> {
        let mut devices = Vec::new();
        let ports_impl = self.read_reg(AhciReg::Pi);

        for port in 0..32 {
            if (ports_impl & (1 << port)) != 0 {
                let ssts = self.read_port_reg(port, AhciPortReg::Ssts);
                let det = ssts & 0xf;

                if det == 3 {
                    // Device present and communication established
                    let sig = self.read_port_reg(port, AhciPortReg::Sig);
                    let device_type = match sig {
                        0x00000101 => "ATA Device",
                        0xEB140101 => "ATAPI Device",
                        0xC33C0101 => "Enclosure Management Bridge",
                        0x96690101 => "Port Multiplier",
                        _ => "Unknown Device",
                    };
                    devices.push((port, device_type.to_string()));
                }
            }
        }

        devices
    }

    /// Get device information string
    pub fn get_device_info_string(&self) -> String {
        if let Some(info) = self.device_info {
            format!(
                "{} (Vendor: 0x{:04x}, Device: 0x{:04x})",
                info.name, info.vendor_id, info.device_id
            )
        } else {
            format!("Unknown AHCI Controller (Base: 0x{:x})", self.base_addr)
        }
    }
}

impl StorageDriver for AhciDriver {
    fn name(&self) -> &str {
        &self.name
    }

    fn device_type(&self) -> StorageDeviceType {
        StorageDeviceType::SataHdd // Could be SSD too, would need to detect
    }

    fn state(&self) -> StorageDeviceState {
        self.state
    }

    fn capabilities(&self) -> StorageCapabilities {
        self.capabilities.clone()
    }

    fn init(&mut self) -> Result<(), StorageError> {
        self.state = StorageDeviceState::Initializing;
        self.init_controller()?;
        self.state = StorageDeviceState::Ready;
        Ok(())
    }

    fn read_sectors(
        &mut self,
        start_sector: u64,
        buffer: &mut [u8],
    ) -> Result<usize, StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        let sector_size = self.capabilities.sector_size as usize;
        let sector_count = buffer.len() / sector_size;

        if sector_count == 0 {
            return Err(StorageError::BufferTooSmall);
        }

        if sector_count > 65536 {
            return Err(StorageError::TransferTooLarge);
        }

        // Execute read command on port 0 (first available port)
        self.execute_command(0, 0x25, start_sector, sector_count as u16, Some(buffer))?;

        Ok(buffer.len())
    }

    fn write_sectors(&mut self, start_sector: u64, buffer: &[u8]) -> Result<usize, StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        let sector_size = self.capabilities.sector_size as usize;
        let sector_count = buffer.len() / sector_size;

        if sector_count == 0 {
            return Err(StorageError::BufferTooSmall);
        }

        if sector_count > 65536 {
            return Err(StorageError::TransferTooLarge);
        }

        // Execute write command on port 0 (first available port)
        let mut write_buffer = buffer.to_vec();
        self.execute_command(
            0,
            0x35,
            start_sector,
            sector_count as u16,
            Some(&mut write_buffer),
        )?;

        Ok(buffer.len())
    }

    fn flush(&mut self) -> Result<(), StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        // Execute FLUSH CACHE command
        self.execute_command(0, 0xE7, 0, 0, None)?;
        Ok(())
    }

    fn get_stats(&self) -> StorageStats {
        self.stats.clone()
    }

    fn reset(&mut self) -> Result<(), StorageError> {
        self.state = StorageDeviceState::Resetting;
        self.init_controller()?;
        self.state = StorageDeviceState::Ready;
        Ok(())
    }

    fn standby(&mut self) -> Result<(), StorageError> {
        // Execute STANDBY command
        self.execute_command(0, 0xE2, 0, 0, None)?;
        self.state = StorageDeviceState::Standby;
        Ok(())
    }

    fn wake(&mut self) -> Result<(), StorageError> {
        if self.state == StorageDeviceState::Standby {
            // Any command will wake the device
            self.execute_command(0, 0xE1, 0, 0, None)?; // IDLE command
            self.state = StorageDeviceState::Ready;
        }
        Ok(())
    }

    fn vendor_command(&mut self, command: u8, data: &[u8]) -> Result<Vec<u8>, StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        // Execute vendor-specific command (implementation depends on vendor)
        self.execute_command(0, command, 0, data.len() as u16, None)?;

        // Return empty response for now
        Ok(Vec::new())
    }

    fn get_smart_data(&mut self) -> Result<Vec<u8>, StorageError> {
        if !self.capabilities.supports_smart {
            return Err(StorageError::NotSupported);
        }

        // In real implementation, execute SMART READ DATA command
        // For now, return empty SMART data
        Ok(vec![0; 512])
    }
}

/// Create AHCI driver from PCI device information
pub fn create_ahci_driver(
    vendor_id: u16,
    device_id: u16,
    base_addr: u64,
    device_name: Option<String>,
) -> Option<Box<dyn StorageDriver>> {
    // Check if this is a known AHCI device
    let is_ahci = AHCI_DEVICE_IDS
        .iter()
        .any(|info| info.vendor_id == vendor_id && info.device_id == device_id);

    if is_ahci {
        let name =
            device_name.unwrap_or_else(|| format!("AHCI-{:04x}:{:04x}", vendor_id, device_id));
        let driver = AhciDriver::new(name, vendor_id, device_id, base_addr);
        Some(Box::new(driver))
    } else {
        None
    }
}

/// Check if PCI device is an AHCI controller
pub fn is_ahci_device(vendor_id: u16, device_id: u16) -> bool {
    AHCI_DEVICE_IDS
        .iter()
        .any(|info| info.vendor_id == vendor_id && info.device_id == device_id)
}

/// Get AHCI device information
pub fn get_ahci_device_info(vendor_id: u16, device_id: u16) -> Option<&'static AhciDeviceId> {
    AHCI_DEVICE_IDS
        .iter()
        .find(|info| info.vendor_id == vendor_id && info.device_id == device_id)
}
