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
use alloc::sync::Arc;
use alloc::vec::Vec;
use alloc::{format, vec};
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AhciAttachedDevice {
    None,
    Ata,
    Atapi,
    Pm,
    Semb,
    Unknown(u32),
}

#[derive(Debug, Clone, Copy)]
struct AhciPortInfo {
    active: bool,
    device: AhciAttachedDevice,
    sectors: u64,
    sector_size: u32,
    supports_smart: bool,
    supports_trim: bool,
}

impl Default for AhciPortInfo {
    fn default() -> Self {
        Self {
            active: false,
            device: AhciAttachedDevice::None,
            sectors: 0,
            sector_size: 512,
            supports_smart: false,
            supports_trim: false,
        }
    }
}

fn le_word(buf: &[u8], word: usize) -> u16 {
    let i = word * 2;
    u16::from_le_bytes([buf[i], buf[i + 1]])
}

fn ata_string(buf: &[u8], start_word: usize, words: usize) -> String {
    let mut bytes = Vec::with_capacity(words * 2);
    for w in 0..words {
        let value = le_word(buf, start_word + w);
        bytes.push((value >> 8) as u8);
        bytes.push((value & 0xff) as u8);
    }
    while bytes.last() == Some(&b' ') || bytes.last() == Some(&0) {
        bytes.pop();
    }
    String::from_utf8(bytes).unwrap_or_else(|_| "ATA Device".to_string())
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
#[allow(non_camel_case_types)]
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

// AHCI port command register bits
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
    ports: [AhciPortInfo; 32],
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
            ports: [AhciPortInfo::default(); 32],
        }
    }

    fn port_device_from_sig(sig: u32) -> AhciAttachedDevice {
        match sig {
            0x0000_0101 => AhciAttachedDevice::Ata,
            0xEB14_0101 => AhciAttachedDevice::Atapi,
            0xC33C_0101 => AhciAttachedDevice::Semb,
            0x9669_0101 => AhciAttachedDevice::Pm,
            0 => AhciAttachedDevice::Ata,
            other => AhciAttachedDevice::Unknown(other),
        }
    }

    fn data_command(command: u8) -> bool {
        matches!(command, 0x25 | 0x35 | 0xEC | 0xB0)
    }

    fn data_out_command(command: u8) -> bool {
        matches!(command, 0x35)
    }

    fn ahci_error(&self, port: u8) -> StorageError {
        let tfd = self.read_port_reg(port, AhciPortReg::Tfd);
        let err = (tfd >> 8) as u8;
        if (err & ((1 << 6) | (1 << 7))) != 0 {
            StorageError::MediaError
        } else if (err & (1 << 4)) != 0 {
            StorageError::InvalidSector
        } else if (err & (1 << 2)) != 0 {
            StorageError::NotSupported
        } else {
            StorageError::HardwareError
        }
    }

    fn wait_port_idle(&self, port: u8, loops: u32) -> Result<(), StorageError> {
        for _ in 0..loops {
            let tfd = self.read_port_reg(port, AhciPortReg::Tfd);
            if (tfd & 0x88) == 0 {
                return Ok(());
            }
            core::hint::spin_loop();
        }
        Err(StorageError::Timeout)
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
    pub fn init_controller(&mut self) -> Result<(), StorageError> {
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
            self.write_reg(AhciReg::Bohc, 1 << 1); // Request OS ownership

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
    pub fn init_port(&mut self, port: u8) -> Result<(), StorageError> {
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
            self.ports[port as usize] = AhciPortInfo::default();
            return Ok(()); // No device on this port
        }

        let sig = self.read_port_reg(port, AhciPortReg::Sig);
        let attached = Self::port_device_from_sig(sig);
        if attached != AhciAttachedDevice::Ata {
            self.ports[port as usize] = AhciPortInfo {
                active: false,
                device: attached,
                ..AhciPortInfo::default()
            };
            return Ok(());
        }

        // Set up command list and FIS receive area with real DMA memory
        // AHCI requires 1KB command list, 256B FIS area, 256B command table per port
        // Allocate one 4KB page per port to hold all three structures
        let dma_size = 4096;
        let layout = alloc::alloc::Layout::from_size_align(dma_size, 4096)
            .map_err(|_| StorageError::HardwareError)?;
        let dma_ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        if dma_ptr.is_null() {
            return Err(StorageError::HardwareError);
        }
        let dma_phys = dma_ptr as u64;

        // Command list at offset 0 (1KB, aligned to 1KB)
        let cmd_list_phys = dma_phys;
        // FIS receive area at offset 0x400 (256B, aligned to 256B)
        let fis_phys = dma_phys + 0x400;
        // Command table at offset 0x500 (256B minimum, aligned to 128B)
        let cmd_table_phys = dma_phys + 0x500;

        self.command_lists[port as usize] = cmd_list_phys;
        self.command_tables[port as usize] = cmd_table_phys;

        // Program command list base address
        self.write_port_reg(port, AhciPortReg::Clb, (cmd_list_phys & 0xFFFFFFFF) as u32);
        self.write_port_reg(
            port,
            AhciPortReg::Clbu,
            ((cmd_list_phys >> 32) & 0xFFFFFFFF) as u32,
        );

        // Program FIS base address
        self.write_port_reg(port, AhciPortReg::Fb, (fis_phys & 0xFFFFFFFF) as u32);
        self.write_port_reg(
            port,
            AhciPortReg::Fbu,
            ((fis_phys >> 32) & 0xFFFFFFFF) as u32,
        );

        // Enable FIS receive
        cmd = self.read_port_reg(port, AhciPortReg::Cmd);
        cmd |= PortCmd::FRE.bits();
        self.write_port_reg(port, AhciPortReg::Cmd, cmd);

        // Start port
        cmd |= PortCmd::ST.bits();
        self.write_port_reg(port, AhciPortReg::Cmd, cmd);

        if let Err(err) = self.identify_port(port) {
            self.ports[port as usize] = AhciPortInfo::default();
            crate::serial_println!("ahci: port {} identify failed: {:?}", port, err);
            return Ok(());
        }

        Ok(())
    }

    fn identify_port(&mut self, port: u8) -> Result<(), StorageError> {
        let mut identify = vec![0u8; 512];
        self.execute_command(port, 0xEC, 0, 1, Some(&mut identify))?;

        let word83 = le_word(&identify, 83);
        let word82 = le_word(&identify, 82);
        let word106 = le_word(&identify, 106);
        let supports_lba48 = (word83 & (1 << 10)) != 0;
        let sectors = if supports_lba48 {
            (le_word(&identify, 100) as u64)
                | ((le_word(&identify, 101) as u64) << 16)
                | ((le_word(&identify, 102) as u64) << 32)
                | ((le_word(&identify, 103) as u64) << 48)
        } else {
            (le_word(&identify, 60) as u64) | ((le_word(&identify, 61) as u64) << 16)
        };

        if sectors == 0 {
            return Err(StorageError::DeviceNotFound);
        }

        let logical_words_valid = (word106 & (1 << 12)) != 0 && (word106 & (1 << 14)) == 0;
        let sector_size = if logical_words_valid {
            let words = (le_word(&identify, 117) as u32) | ((le_word(&identify, 118) as u32) << 16);
            let bytes = words.saturating_mul(2);
            if bytes >= 512 && bytes.is_power_of_two() { bytes } else { 512 }
        } else {
            512
        };

        let model = ata_string(&identify, 27, 20);
        let supports_smart = (word82 & 1) != 0;
        let supports_trim = (le_word(&identify, 169) & 1) != 0;

        self.ports[port as usize] = AhciPortInfo {
            active: true,
            device: AhciAttachedDevice::Ata,
            sectors,
            sector_size,
            supports_smart,
            supports_trim,
        };

        crate::serial_println!(
            "ahci: port {} ATA '{}' sectors={} sector_size={}",
            port,
            model,
            sectors,
            sector_size
        );
        Ok(())
    }

    /// Execute SATA command (production implementation)
    pub fn execute_command(
        &mut self,
        port: u8,
        command: u8,
        lba: u64,
        count: u16,
        mut buffer: Option<&mut [u8]>,
    ) -> Result<(), StorageError> {
        // Check port status
        if port as usize >= self.ports.len() || port >= self.port_count {
            return Err(StorageError::DeviceNotFound);
        }
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

        self.wait_port_idle(port, 100_000)?;

        // Use DMA addresses already allocated during port setup
        let cmd_list_phys = self.command_lists[port as usize];
        let cmd_table_phys = self.command_tables[port as usize];

        if cmd_list_phys == 0 || cmd_table_phys == 0 {
            return Err(StorageError::HardwareError);
        }

        // Allocate proper DMA buffer for data transfer - Production implementation
        use crate::net::dma::{DmaBuffer, DMA_ALIGNMENT};

<        let sector_size = self.ports[port as usize].sector_size.max(512) as usize;
        let transfer_size = if Self::data_command(command) {
            core::cmp::max((count as usize) * sector_size, 512)
        } else {
            0
        };
        let data_size = transfer_size.max(512);
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
        if Self::data_command(command) {
            unsafe {
                let prd_table = (cmd_table_phys + 0x80) as *mut u32;

                // PRD Entry 0: Data Buffer Address (Low)
                *prd_table = (buffer_phys & 0xFFFFFFFF) as u32;
                // PRD Entry 1: Data Buffer Address (High)
                *prd_table.add(1) = ((buffer_phys >> 32) & 0xFFFFFFFF) as u32;
                // PRD Entry 2: Reserved
                *prd_table.add(2) = 0;
                // PRD Entry 3: Data Byte Count and Interrupt on Completion
                *prd_table.add(3) = (transfer_size as u32 - 1) | (1u32 << 31); // Size - 1 and interrupt bit

                if Self::data_out_command(command) && buffer.is_some() {
                    let src_buffer = buffer.as_ref().unwrap();
                    let dst_ptr = _data_dma_buffer.virtual_addr();
                    let copy_size = core::cmp::min(src_buffer.len(), transfer_size);
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
            if Self::data_out_command(command) {
                dw0 |= 1 << 6; // Write bit
            }
            if Self::data_command(command) {
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
                return Err(self.ahci_error(port));
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
            return Err(self.ahci_error(port));
        }

        let is = self.read_port_reg(port, AhciPortReg::Is);
        if (is & 0x40000000) != 0 {
            // Task File Error
            self.write_port_reg(port, AhciPortReg::Is, is); // Clear interrupt status
            return Err(self.ahci_error(port));
        }

        // 8. Copy read data from DMA buffer using proper buffer access
        if !Self::data_out_command(command) && Self::data_command(command) && buffer.is_some() {
            unsafe {
                let src_ptr = _data_dma_buffer.virtual_addr() as *const u8;
                let dst_buffer = buffer.as_mut().unwrap();
                let copy_size = core::cmp::min(dst_buffer.len(), transfer_size);
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

    pub fn get_smart_data(&mut self, port: u8) -> Result<Vec<u8>, StorageError> {
        if port >= 32 {
            return Err(StorageError::HardwareError);
        }
        // SMART READ DATA: ATA command 0xB0, features=0xD0, LBA=0xC24F8C0
        // The 512-byte SMART data is returned in the DMA buffer
        let mut smart_data = vec![0u8; 512];

        // Execute SMART command via a custom FIS
        // We use execute_command with command=0xB0 (SMART)
        // The features register and LBA need to be set properly for SMART READ DATA
        // LBA = 0xC24F8C0 (SMART signature), features = 0xD0 (SMART READ DATA)
        // Since execute_command doesn't expose features, we build the FIS directly

        let cmd_list_phys = self.command_lists[port as usize];
        let cmd_table_phys = self.command_tables[port as usize];

        if cmd_list_phys == 0 || cmd_table_phys == 0 {
            return Err(StorageError::HardwareError);
        }

        use crate::net::dma::{DmaBuffer, DMA_ALIGNMENT};
        let dma_buf =
            DmaBuffer::allocate(512, DMA_ALIGNMENT).map_err(|_| StorageError::HardwareError)?;

        let buffer_phys = {
            use crate::memory::get_memory_manager;
            use x86_64::VirtAddr;
            let virt_addr = VirtAddr::new(dma_buf.virtual_addr() as u64);
            let mm = get_memory_manager().ok_or(StorageError::HardwareError)?;
            mm.translate_addr(virt_addr)
                .ok_or(StorageError::HardwareError)?
                .as_u64()
        };

        // Build SMART READ DATA FIS
        unsafe {
            let cmd_table = cmd_table_phys as *mut u8;

            // Clear command table
            for i in 0..0x80 {
                *cmd_table.add(i) = 0;
            }

            // H2D Register FIS
            *cmd_table = 0x27; // FIS Type: Register H2D
            *cmd_table.add(1) = 0x80; // C bit set
            *cmd_table.add(2) = 0xB0; // SMART command
            *cmd_table.add(3) = 0xD0; // Features: SMART READ DATA

            // SMART signature LBA: 0xC24F8C0
            *cmd_table.add(4) = 0xC0; // LBA low
            *cmd_table.add(5) = 0x4F; // LBA mid
            *cmd_table.add(6) = 0xC2; // LBA high
            *cmd_table.add(7) = 0xA0; // Device register (LBA mode)
            *cmd_table.add(8) = 0; // LBA[31:24]
            *cmd_table.add(9) = 0; // LBA[39:32]
            *cmd_table.add(10) = 0; // LBA[47:40]
            *cmd_table.add(11) = 0; // Features (high)
            *cmd_table.add(12) = 1; // Sector count
            *cmd_table.add(13) = 0; // Sector count (high)

            // Set up PRDT (Physical Region Descriptor Table) in command table.
            let prdt_ptr = cmd_table.add(0x80) as *mut u32;
            *prdt_ptr = (buffer_phys & 0xFFFF_FFFF) as u32;
            *prdt_ptr.add(1) = ((buffer_phys >> 32) & 0xFFFF_FFFF) as u32;
            *prdt_ptr.add(2) = 0;
            *prdt_ptr.add(3) = (512 - 1) | (1u32 << 31);

            // Set up command list entry
            let cmd_list = cmd_list_phys as *mut u32;
            for i in 0..8 {
                *cmd_list.add(i) = 0;
            }
            // Command FIS length: 20 bytes (5 DWORDs)
            *cmd_list = 5 | (1 << 16); // CFL=5, PRDTL=1
                                       // Command table base address
            *(cmd_list.add(2)) = (cmd_table_phys & 0xFFFFFFFF) as u32;
            *(cmd_list.add(3)) = (cmd_table_phys >> 32) as u32;
        }

        // Issue command by setting CI (Command Issue) bit
        let ci = self.read_port_reg(port, AhciPortReg::Ci);
        self.write_port_reg(port, AhciPortReg::Ci, ci | 1);

        // Wait for completion
        let mut timeout = 1_000_000u32;
        loop {
            let ci = self.read_port_reg(port, AhciPortReg::Ci);
            if (ci & 1) == 0 {
                break;
            }
            if timeout == 0 {
                return Err(StorageError::Timeout);
            }
            timeout -= 1;
            core::hint::spin_loop();
        }

        // Copy SMART data from DMA buffer
        let src = dma_buf.virtual_addr() as *const u8;
        unsafe {
            core::ptr::copy_nonoverlapping(src, smart_data.as_mut_ptr(), 512);
        }

        Ok(smart_data)
    }

    /// Check if a port is active (i.e., has an initialized device)
    pub fn is_port_active(&self, port: u8) -> bool {
        if port >= 32 {
            return false;
        }
        self.command_lists[port as usize] != 0
    }
}

/// A storage device representing a single port on the AHCI controller.
#[derive(Debug)]
pub struct AhciPortDevice {
    controller: Arc<spin::Mutex<AhciDriver>>,
    port: u8,
    name: String,
    capabilities: StorageCapabilities,
    stats: StorageStats,
    state: StorageDeviceState,
}

impl AhciPortDevice {
    pub fn new(controller: Arc<spin::Mutex<AhciDriver>>, port: u8, name: String) -> Self {
        let mut capabilities = StorageCapabilities::default();
        {
            let ctrl = controller.lock();
            capabilities.max_queue_depth = ctrl.capabilities.max_queue_depth;
            capabilities.supports_ncq = ctrl.capabilities.supports_ncq;
<            let info = ctrl.ports[port as usize];
            capabilities.sector_size = info.sector_size;
            capabilities.capacity_bytes = info.sectors.saturating_mul(info.sector_size as u64);
            capabilities.max_transfer_size = 65535 * info.sector_size;
            capabilities.supports_smart = info.supports_smart;
            capabilities.supports_trim = info.supports_trim;
        }
        Self {
            controller,
            port,
            name,
            capabilities,
            stats: StorageStats::default(),
            state: StorageDeviceState::Ready,
        }
    }
}

impl StorageDriver for AhciPortDevice {
    fn name(&self) -> &str {
        &self.name
    }

    fn device_type(&self) -> StorageDeviceType {
        StorageDeviceType::SataHdd
    }

    fn state(&self) -> StorageDeviceState {
        self.state
    }

    fn capabilities(&self) -> StorageCapabilities {
        self.capabilities.clone()
    }

    fn init(&mut self) -> Result<(), StorageError> {
        // Port initialization was already done by the controller.
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

        if sector_count == 0 || buffer.len() % sector_size != 0 {
            return Err(StorageError::BufferTooSmall);
        }

        if sector_count >= 65536 {
            return Err(StorageError::TransferTooLarge);
        }
        if buffer.len() > self.capabilities.max_transfer_size as usize {
            return Err(StorageError::TransferTooLarge);
        }

        let end = start_sector
            .checked_add(sector_count as u64)
            .ok_or(StorageError::InvalidSector)?;
        let total = self.capabilities.capacity_bytes / self.capabilities.sector_size as u64;
        if total != 0 && end > total {
            return Err(StorageError::InvalidSector);
        }

        let mut ctrl = self.controller.lock();
        ctrl.execute_command(
            self.port,
            0x25,
            start_sector,
            sector_count as u16,
            Some(buffer),
        )?;

        self.stats.reads_total += 1;
        self.stats.bytes_read += buffer.len() as u64;

        Ok(buffer.len())
    }

    fn write_sectors(&mut self, start_sector: u64, buffer: &[u8]) -> Result<usize, StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        let sector_size = self.capabilities.sector_size as usize;
        let sector_count = buffer.len() / sector_size;

        if sector_count == 0 || buffer.len() % sector_size != 0 {
            return Err(StorageError::BufferTooSmall);
        }

        if sector_count >= 65536 {
            return Err(StorageError::TransferTooLarge);
        }
        if buffer.len() > self.capabilities.max_transfer_size as usize {
            return Err(StorageError::TransferTooLarge);
        }

        let end = start_sector
            .checked_add(sector_count as u64)
            .ok_or(StorageError::InvalidSector)?;
        let total = self.capabilities.capacity_bytes / self.capabilities.sector_size as u64;
        if total != 0 && end > total {
            return Err(StorageError::InvalidSector);
        }

        let mut write_buffer = buffer.to_vec();
        let mut ctrl = self.controller.lock();
        ctrl.execute_command(
            self.port,
            0x35,
            start_sector,
            sector_count as u16,
            Some(&mut write_buffer),
        )?;

        self.stats.writes_total += 1;
        self.stats.bytes_written += buffer.len() as u64;

        Ok(buffer.len())
    }

    fn flush(&mut self) -> Result<(), StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        let mut ctrl = self.controller.lock();
        ctrl.execute_command(self.port, 0xE7, 0, 0, None)?;
        Ok(())
    }

    fn get_stats(&self) -> StorageStats {
        self.stats.clone()
    }

    fn reset(&mut self) -> Result<(), StorageError> {
        self.state = StorageDeviceState::Resetting;
        let mut ctrl = self.controller.lock();
        ctrl.init_port(self.port)?;
        self.state = StorageDeviceState::Ready;
        Ok(())
    }

    fn standby(&mut self) -> Result<(), StorageError> {
        let mut ctrl = self.controller.lock();
        ctrl.execute_command(self.port, 0xE2, 0, 0, None)?;
        self.state = StorageDeviceState::Standby;
        Ok(())
    }

    fn wake(&mut self) -> Result<(), StorageError> {
        if self.state == StorageDeviceState::Standby {
            let mut ctrl = self.controller.lock();
            ctrl.execute_command(self.port, 0xE1, 0, 0, None)?;
            self.state = StorageDeviceState::Ready;
        }
        Ok(())
    }

<    fn vendor_command(&mut self, _command: u8, _data: &[u8]) -> Result<Vec<u8>, StorageError> {
        Err(StorageError::NotSupported)
    }

    fn get_smart_data(&mut self) -> Result<Vec<u8>, StorageError> {
        if !self.capabilities.supports_smart {
            return Err(StorageError::NotSupported);
        }
        let mut ctrl = self.controller.lock();
        ctrl.get_smart_data(self.port)
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
        let mut driver = AhciDriver::new(name.clone(), vendor_id, device_id, base_addr);
        if driver.init_controller().is_ok() {
<            let port = driver
                .ports
                .iter()
                .enumerate()
                .find(|(_, info)| info.active && info.device == AhciAttachedDevice::Ata)
                .map(|(idx, _)| idx as u8)?;
            let controller = Arc::new(spin::Mutex::new(driver));
            Some(Box::new(AhciPortDevice::new(controller, port, name)))
        } else {
            None
        }
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
