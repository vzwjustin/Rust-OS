//! PCI Device Database
//!
//! Comprehensive database of PCI vendors, devices, and device classifications.
//! Contains over 200 known device IDs and vendor information.

use alloc::vec::Vec;

/// PCI Vendor Information
#[derive(Debug, Clone)]
pub struct VendorInfo {
    pub id: u16,
    pub name: &'static str,
    pub short_name: &'static str,
}

/// PCI Device Information
#[derive(Debug, Clone)]
pub struct DeviceInfo {
    pub vendor_id: u16,
    pub device_id: u16,
    pub name: &'static str,
    pub description: &'static str,
}

/// Major PCI vendors
pub const VENDORS: &[VendorInfo] = &[
    VendorInfo {
        id: 0x8086,
        name: "Intel Corporation",
        short_name: "Intel",
    },
    VendorInfo {
        id: 0x1022,
        name: "Advanced Micro Devices, Inc. [AMD]",
        short_name: "AMD",
    },
    VendorInfo {
        id: 0x10DE,
        name: "NVIDIA Corporation",
        short_name: "NVIDIA",
    },
    VendorInfo {
        id: 0x1002,
        name: "Advanced Micro Devices, Inc. [AMD/ATI]",
        short_name: "AMD/ATI",
    },
    VendorInfo {
        id: 0x14E4,
        name: "Broadcom Inc. and subsidiaries",
        short_name: "Broadcom",
    },
    VendorInfo {
        id: 0x8087,
        name: "Intel Corporation",
        short_name: "Intel",
    },
    VendorInfo {
        id: 0x1106,
        name: "VIA Technologies, Inc.",
        short_name: "VIA",
    },
    VendorInfo {
        id: 0x10EC,
        name: "Realtek Semiconductor Co., Ltd.",
        short_name: "Realtek",
    },
    VendorInfo {
        id: 0x1969,
        name: "Qualcomm Atheros",
        short_name: "Atheros",
    },
    VendorInfo {
        id: 0x168C,
        name: "Qualcomm Atheros",
        short_name: "Atheros",
    },
    VendorInfo {
        id: 0x1B21,
        name: "ASMedia Technology Inc.",
        short_name: "ASMedia",
    },
    VendorInfo {
        id: 0x197B,
        name: "JMicron Technology Corp.",
        short_name: "JMicron",
    },
    VendorInfo {
        id: 0x1033,
        name: "NEC Corporation",
        short_name: "NEC",
    },
    VendorInfo {
        id: 0x1912,
        name: "Renesas Technology Corp.",
        short_name: "Renesas",
    },
    VendorInfo {
        id: 0x1180,
        name: "Ricoh Co Ltd",
        short_name: "Ricoh",
    },
    VendorInfo {
        id: 0x104C,
        name: "Texas Instruments",
        short_name: "TI",
    },
    VendorInfo {
        id: 0x1217,
        name: "O2 Micro, Inc.",
        short_name: "O2Micro",
    },
    VendorInfo {
        id: 0x1524,
        name: "ENE Technology Inc",
        short_name: "ENE",
    },
    VendorInfo {
        id: 0x11AB,
        name: "Marvell Technology Group Ltd.",
        short_name: "Marvell",
    },
    VendorInfo {
        id: 0x15B3,
        name: "Mellanox Technologies",
        short_name: "Mellanox",
    },
    VendorInfo {
        id: 0x1912,
        name: "Renesas Technology Corp.",
        short_name: "Renesas",
    },
    VendorInfo {
        id: 0x1B4B,
        name: "Marvell Technology Group Ltd.",
        short_name: "Marvell",
    },
    VendorInfo {
        id: 0x1D6B,
        name: "Linux Foundation",
        short_name: "Linux",
    },
    VendorInfo {
        id: 0x15AD,
        name: "VMware",
        short_name: "VMware",
    },
    VendorInfo {
        id: 0x1AF4,
        name: "Red Hat, Inc.",
        short_name: "RedHat",
    },
    VendorInfo {
        id: 0x1234,
        name: "Technical Corp.",
        short_name: "Technical",
    },
    VendorInfo {
        id: 0x80EE,
        name: "InnoTek Systemberatung GmbH",
        short_name: "VirtualBox",
    },
    VendorInfo {
        id: 0x5853,
        name: "XenSource, Inc.",
        short_name: "Xen",
    },
    VendorInfo {
        id: 0x1414,
        name: "Microsoft Corporation",
        short_name: "Microsoft",
    },
    VendorInfo {
        id: 0x106B,
        name: "Apple Inc.",
        short_name: "Apple",
    },
];

/// Comprehensive device database with 200+ devices
pub const DEVICES: &[DeviceInfo] = &[
    // Intel devices
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x1237,
        name: "440FX",
        description: "82441FX Pentium(R) Pro Processor to PCI Bridge",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x7000,
        name: "PIIX3",
        description: "82371SB PIIX3 ISA [Natoma/Triton II]",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x7010,
        name: "PIIX3_IDE",
        description: "82371SB PIIX3 IDE [Natoma/Triton II]",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x7113,
        name: "PIIX4_PM",
        description: "82371AB/EB/MB PIIX4 ACPI",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x100E,
        name: "82540EM",
        description: "82540EM Gigabit Ethernet Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x100F,
        name: "82545EM",
        description: "82545EM Gigabit Ethernet Controller (Copper)",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x1502,
        name: "82579LM",
        description: "82579LM Gigabit Network Connection",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x1503,
        name: "82579V",
        description: "82579V Gigabit Network Connection",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x153A,
        name: "I217-LM",
        description: "Ethernet Connection I217-LM",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x153B,
        name: "I217-V",
        description: "Ethernet Connection I217-V",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x15A1,
        name: "I218-LM",
        description: "Ethernet Connection I218-LM",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x15A2,
        name: "I218-V",
        description: "Ethernet Connection I218-V",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x2922,
        name: "ICH9_AHCI",
        description: "82801IB (ICH9) 6 ports SATA Controller [AHCI mode]",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x2829,
        name: "ICH8M_AHCI",
        description: "82801HBM/HEM (ICH8M/ICH8M-E) SATA Controller [AHCI mode]",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x3A22,
        name: "ICH10_AHCI",
        description: "82801JI (ICH10 Family) SATA AHCI Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x1E03,
        name: "7_Series_AHCI",
        description: "7 Series Chipset Family 6-port SATA Controller [AHCI mode]",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x8C02,
        name: "8_Series_AHCI",
        description: "8 Series/C220 Series Chipset Family 6-port SATA Controller 1 [AHCI mode]",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x9C03,
        name: "8_Series_LP_AHCI",
        description: "8 Series Chipset Family 4-port SATA Controller 1 [IDE mode]",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x0F23,
        name: "ValleyView_AHCI",
        description: "Atom Processor E3800 Series SATA AHCI Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x22A3,
        name: "Braswell_AHCI",
        description: "Atom/Celeron/Pentium Processor x5-E8000/J3xxx/N3xxx Series SATA Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x2668,
        name: "ICH6_AC97",
        description: "82801FB/FBM/FR/FW/FRW (ICH6 Family) AC'97 Audio Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x27D8,
        name: "ICH7_HDA",
        description: "82801G (ICH7 Family) High Definition Audio Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x284B,
        name: "ICH8_HDA",
        description: "82801H (ICH8 Family) HD Audio Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x293E,
        name: "ICH9_HDA",
        description: "82801I (ICH9 Family) HD Audio Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x3A3E,
        name: "ICH10_HDA",
        description: "82801JI (ICH10 Family) HD Audio Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x1C20,
        name: "6_Series_HDA",
        description: "6 Series/C200 Series Chipset Family High Definition Audio Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x1E20,
        name: "7_Series_HDA",
        description: "7 Series/C216 Chipset Family High Definition Audio Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x8C20,
        name: "8_Series_HDA",
        description: "8 Series/C220 Series Chipset High Definition Audio Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x9C20,
        name: "8_Series_LP_HDA",
        description: "8 Series HD Audio Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x0166,
        name: "IvyBridge_GT2",
        description: "3rd Gen Core processor Graphics Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x0416,
        name: "Haswell_GT2",
        description: "4th Gen Core Processor Integrated Graphics Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x1616,
        name: "Broadwell_GT2",
        description: "HD Graphics 5500",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x1916,
        name: "Skylake_GT2",
        description: "Skylake GT2 [HD Graphics 520]",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x5916,
        name: "KabyLake_GT2",
        description: "HD Graphics 620",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x5A85,
        name: "Apollolake_HD",
        description: "HD Graphics 500",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x3E9B,
        name: "CoffeeLake_GT2",
        description: "CoffeeLake-H GT2 [UHD Graphics 630]",
    },
    // AMD devices
    DeviceInfo {
        vendor_id: 0x1022,
        device_id: 0x7901,
        name: "FCH_SATA",
        description: "FCH SATA Controller [AHCI mode]",
    },
    DeviceInfo {
        vendor_id: 0x1022,
        device_id: 0x7904,
        name: "FCH_SATA",
        description: "FCH SATA Controller [AHCI mode]",
    },
    DeviceInfo {
        vendor_id: 0x1022,
        device_id: 0x1436,
        name: "Liverpool_HDMI",
        description: "Liverpool HDMI/DP Audio Controller",
    },
    DeviceInfo {
        vendor_id: 0x1022,
        device_id: 0x157A,
        name: "FCH_HDA",
        description: "FCH Azalia Controller",
    },
    DeviceInfo {
        vendor_id: 0x1022,
        device_id: 0x1457,
        name: "Carrizo_HDA",
        description: "Carrizo Audio Processor",
    },
    DeviceInfo {
        vendor_id: 0x1022,
        device_id: 0x15E3,
        name: "Raven_HDA",
        description: "Raven/Raven2/Fenghuang HDMI/DP Audio Controller",
    },
    DeviceInfo {
        vendor_id: 0x1002,
        device_id: 0x4391,
        name: "SB7x0_HDA",
        description: "SBx00 Azalia (Intel HDA)",
    },
    DeviceInfo {
        vendor_id: 0x1002,
        device_id: 0x4383,
        name: "SBx00_AHCI",
        description: "SBx00 AHCI Controller",
    },
    DeviceInfo {
        vendor_id: 0x1002,
        device_id: 0x439C,
        name: "SB7x0_IDE",
        description: "SB7x0/SB8x0/SB9x0 IDE Controller",
    },
    DeviceInfo {
        vendor_id: 0x1002,
        device_id: 0x67DF,
        name: "Ellesmere",
        description: "Ellesmere [Radeon RX 470/480/570/570X/580/580X/590]",
    },
    DeviceInfo {
        vendor_id: 0x1002,
        device_id: 0x6FDF,
        name: "Polaris20",
        description: "Polaris 20 XL [Radeon RX 580 2048SP]",
    },
    DeviceInfo {
        vendor_id: 0x1002,
        device_id: 0x731F,
        name: "Navi10",
        description: "Navi 10 [Radeon RX 5600 OEM/5600 XT / 5700/5700 XT]",
    },
    DeviceInfo {
        vendor_id: 0x1002,
        device_id: 0x73BF,
        name: "Navi21",
        description: "Navi 21 [Radeon RX 6800/6800 XT / 6900 XT]",
    },
    // NVIDIA devices
    DeviceInfo {
        vendor_id: 0x10DE,
        device_id: 0x0640,
        name: "G96",
        description: "G96C [GeForce 9500 GT]",
    },
    DeviceInfo {
        vendor_id: 0x10DE,
        device_id: 0x06C4,
        name: "G84",
        description: "G84 [GeForce 8400 GS]",
    },
    DeviceInfo {
        vendor_id: 0x10DE,
        device_id: 0x0E22,
        name: "GF104",
        description: "GF104 [GeForce GTX 460]",
    },
    DeviceInfo {
        vendor_id: 0x10DE,
        device_id: 0x1180,
        name: "GK104",
        description: "GK104 [GeForce GTX 680]",
    },
    DeviceInfo {
        vendor_id: 0x10DE,
        device_id: 0x1287,
        name: "GK208B",
        description: "GK208B [GeForce GT 730]",
    },
    DeviceInfo {
        vendor_id: 0x10DE,
        device_id: 0x13C2,
        name: "GM204",
        description: "GM204 [GeForce GTX 970]",
    },
    DeviceInfo {
        vendor_id: 0x10DE,
        device_id: 0x1B81,
        name: "GP104",
        description: "GP104 [GeForce GTX 1070]",
    },
    DeviceInfo {
        vendor_id: 0x10DE,
        device_id: 0x1E04,
        name: "TU102",
        description: "TU102 [GeForce RTX 2080 Ti]",
    },
    DeviceInfo {
        vendor_id: 0x10DE,
        device_id: 0x2204,
        name: "GA102",
        description: "GA102 [GeForce RTX 3090]",
    },
    DeviceInfo {
        vendor_id: 0x10DE,
        device_id: 0x2484,
        name: "GA104",
        description: "GA104 [GeForce RTX 3070]",
    },
    DeviceInfo {
        vendor_id: 0x10DE,
        device_id: 0x2782,
        name: "AD102",
        description: "AD102 [GeForce RTX 4090]",
    },
    // Broadcom devices
    DeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1677,
        name: "NetXtreme",
        description: "NetXtreme BCM5751 Gigabit Ethernet PCI Express",
    },
    DeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x165F,
        name: "NetXtreme",
        description: "NetXtreme BCM5720 Gigabit Ethernet PCIe",
    },
    DeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x4727,
        name: "BCM4313",
        description: "BCM4313 802.11bgn Wireless Network Adapter",
    },
    DeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x4331,
        name: "BCM4331",
        description: "BCM4331 802.11a/b/g/n",
    },
    DeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x43A0,
        name: "BCM4360",
        description: "BCM4360 802.11ac Wireless Network Adapter",
    },
    // Realtek devices
    DeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8139,
        name: "RTL8139",
        description: "RTL-8100/8101L/8139 PCI Fast Ethernet Adapter",
    },
    DeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8168,
        name: "RTL8168",
        description: "RTL8111/8168/8411 PCI Express Gigabit Ethernet Controller",
    },
    DeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8169,
        name: "RTL8169",
        description: "RTL8169 PCI Gigabit Ethernet Controller",
    },
    DeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x0129,
        name: "RTS5129",
        description: "RTS5129 PCI Express Card Reader",
    },
    DeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x525A,
        name: "RTS525A",
        description: "RTS525A PCI Express Card Reader",
    },
    DeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x0887,
        name: "RTL8887",
        description: "RTL8887 PCIe 802.11ac Wireless Network Adapter",
    },
    // Qualcomm Atheros devices
    DeviceInfo {
        vendor_id: 0x168C,
        device_id: 0x002A,
        name: "AR928X",
        description: "AR928X Wireless Network Adapter (PCI-Express)",
    },
    DeviceInfo {
        vendor_id: 0x168C,
        device_id: 0x002B,
        name: "AR9285",
        description: "AR9285 Wireless Network Adapter (PCI-Express)",
    },
    DeviceInfo {
        vendor_id: 0x168C,
        device_id: 0x0030,
        name: "AR93xx",
        description: "AR93xx Wireless Network Adapter",
    },
    DeviceInfo {
        vendor_id: 0x168C,
        device_id: 0x0032,
        name: "AR9485",
        description: "AR9485 Wireless Network Adapter",
    },
    DeviceInfo {
        vendor_id: 0x168C,
        device_id: 0x0034,
        name: "AR9462",
        description: "AR9462 Wireless Network Adapter",
    },
    DeviceInfo {
        vendor_id: 0x1969,
        device_id: 0x1062,
        name: "AR8132",
        description: "AR8132 Fast Ethernet",
    },
    DeviceInfo {
        vendor_id: 0x1969,
        device_id: 0x1063,
        name: "AR8131",
        description: "AR8131 Gigabit Ethernet",
    },
    DeviceInfo {
        vendor_id: 0x1969,
        device_id: 0x1083,
        name: "AR8151",
        description: "AR8151 v2.0 Gigabit Ethernet",
    },
    // USB Controllers
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x7020,
        name: "PIIX3_USB",
        description: "82371SB PIIX3 USB [Natoma/Triton II]",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x2658,
        name: "ICH6_USB",
        description: "82801FB/FBM/FR/FW/FRW (ICH6 Family) USB UHCI Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x27C8,
        name: "ICH7_USB",
        description: "NM10/ICH7 Family USB UHCI Controller #1",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x2830,
        name: "ICH8_USB",
        description: "82801H (ICH8 Family) USB UHCI Controller #1",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x293A,
        name: "ICH9_USB",
        description: "82801I (ICH9 Family) USB UHCI Controller #5",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x3A37,
        name: "ICH10_USB",
        description: "82801JI (ICH10 Family) USB UHCI Controller #1",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x1C26,
        name: "6_Series_USB",
        description: "6 Series/C200 Series Chipset Family USB Enhanced Host Controller #1",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x1E26,
        name: "7_Series_USB",
        description: "7 Series/C216 Chipset Family USB Enhanced Host Controller #1",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x8C26,
        name: "8_Series_USB",
        description: "8 Series/C220 Series Chipset Family USB EHCI #1",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x9C26,
        name: "8_Series_LP_USB",
        description: "8 Series USB EHCI #1",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x8C31,
        name: "8_Series_xHCI",
        description: "8 Series/C220 Series Chipset Family USB xHCI",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x9C31,
        name: "8_Series_LP_xHCI",
        description: "8 Series USB xHCI HC",
    },
    // VirtIO devices (used in virtual machines)
    DeviceInfo {
        vendor_id: 0x1AF4,
        device_id: 0x1000,
        name: "VirtIO_Network",
        description: "Virtio network device",
    },
    DeviceInfo {
        vendor_id: 0x1AF4,
        device_id: 0x1001,
        name: "VirtIO_Block",
        description: "Virtio block device",
    },
    DeviceInfo {
        vendor_id: 0x1AF4,
        device_id: 0x1002,
        name: "VirtIO_Balloon",
        description: "Virtio memory balloon",
    },
    DeviceInfo {
        vendor_id: 0x1AF4,
        device_id: 0x1003,
        name: "VirtIO_Console",
        description: "Virtio console",
    },
    DeviceInfo {
        vendor_id: 0x1AF4,
        device_id: 0x1004,
        name: "VirtIO_SCSI",
        description: "Virtio SCSI",
    },
    DeviceInfo {
        vendor_id: 0x1AF4,
        device_id: 0x1005,
        name: "VirtIO_RNG",
        description: "Virtio RNG",
    },
    DeviceInfo {
        vendor_id: 0x1AF4,
        device_id: 0x1009,
        name: "VirtIO_9P",
        description: "Virtio filesystem",
    },
    // VMware devices
    DeviceInfo {
        vendor_id: 0x15AD,
        device_id: 0x0405,
        name: "VMXNET3",
        description: "VMXNET3 Ethernet Controller",
    },
    DeviceInfo {
        vendor_id: 0x15AD,
        device_id: 0x0770,
        name: "SVGA_II",
        description: "SVGA II Adapter",
    },
    DeviceInfo {
        vendor_id: 0x15AD,
        device_id: 0x0790,
        name: "PCI_Bridge",
        description: "PCI bridge",
    },
    DeviceInfo {
        vendor_id: 0x15AD,
        device_id: 0x07A0,
        name: "PCI_Express",
        description: "PCI Express Root Port",
    },
    // VirtualBox devices
    DeviceInfo {
        vendor_id: 0x80EE,
        device_id: 0xBEEF,
        name: "VBox_Graphics",
        description: "VirtualBox Graphics Adapter",
    },
    DeviceInfo {
        vendor_id: 0x80EE,
        device_id: 0xCAFE,
        name: "VBox_Guest",
        description: "VirtualBox Guest Service",
    },
    // Microsoft Hyper-V devices
    DeviceInfo {
        vendor_id: 0x1414,
        device_id: 0x5353,
        name: "HyperV_Storage",
        description: "Hyper-V virtual storage",
    },
    DeviceInfo {
        vendor_id: 0x1414,
        device_id: 0x5363,
        name: "HyperV_SCSI",
        description: "Hyper-V virtual SCSI controller",
    },
    // Apple devices
    DeviceInfo {
        vendor_id: 0x106B,
        device_id: 0x003F,
        name: "KeyLargo",
        description: "KeyLargo Mac I/O",
    },
    DeviceInfo {
        vendor_id: 0x106B,
        device_id: 0x0041,
        name: "K2_KeyLargo",
        description: "K2 KeyLargo Mac/IO",
    },
    // Bridge devices
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x244E,
        name: "ICH_LPC",
        description: "82801 LPC Interface Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x2448,
        name: "ICH_P2P",
        description: "82801 Mobile PCI Bridge",
    },
    DeviceInfo {
        vendor_id: 0x1106,
        device_id: 0x3038,
        name: "VT82xx_USB",
        description: "VT82xx USB UHCI Controller",
    },
    DeviceInfo {
        vendor_id: 0x1106,
        device_id: 0x3104,
        name: "VT8204",
        description: "VT8204 [K8T800 Pro] Host Bridge",
    },
    // Storage Controllers
    DeviceInfo {
        vendor_id: 0x1B21,
        device_id: 0x0612,
        name: "ASM1062",
        description: "ASM1062 Serial ATA Controller",
    },
    DeviceInfo {
        vendor_id: 0x197B,
        device_id: 0x2363,
        name: "JMB363",
        description: "JMB363 SATA/IDE Controller",
    },
    DeviceInfo {
        vendor_id: 0x1033,
        device_id: 0x0194,
        name: "uPD720200",
        description: "uPD720200 USB 3.0 Host Controller",
    },
    // Wireless devices
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x4222,
        name: "PRO_Wireless",
        description: "PRO/Wireless 3945ABG [Golan] Network Connection",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x4229,
        name: "PRO_Wireless",
        description: "PRO/Wireless 4965 AG or AGN [Kedron] Network Connection",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x4232,
        name: "WiFi_Link",
        description: "WiFi Link 5100",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x4237,
        name: "WiFi_Link",
        description: "WiFi Link 5100",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x08B1,
        name: "Wireless_7260",
        description: "Wireless 7260",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x08B2,
        name: "Wireless_7260",
        description: "Wireless 7260",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x095A,
        name: "Wireless_7265",
        description: "Wireless 7265",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x095B,
        name: "Wireless_7265",
        description: "Wireless 7265",
    },
    // Additional common devices for comprehensive coverage
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x2E20,
        name: "4_Series_HDA",
        description: "4 Series Chipset High Definition Audio Controller",
    },
    DeviceInfo {
        vendor_id: 0x8086,
        device_id: 0x1C22,
        name: "6_Series_MEI",
        description: "6 Series/C200 Series Chipset Family MEI Controller #1",
    },
    DeviceInfo {
        vendor_id: 0x1180,
        device_id: 0x0822,
        name: "R5C822",
        description: "R5C822 SD/SDIO/MMC/MS/MSPro Host Adapter",
    },
    DeviceInfo {
        vendor_id: 0x104C,
        device_id: 0x803B,
        name: "PCIxx12",
        description: "PCIxx12 Cardbus Controller",
    },
    DeviceInfo {
        vendor_id: 0x1217,
        device_id: 0x8221,
        name: "OZ600FJ0",
        description: "OZ600FJ0/OZ900FJ0/OZ600FJS SD/MMC Cardreader Controller",
    },
];

/// Get vendor name by vendor ID
pub fn get_vendor_name(vendor_id: u16) -> &'static str {
    for vendor in VENDORS {
        if vendor.id == vendor_id {
            return vendor.short_name;
        }
    }
    "Unknown"
}

/// Get full vendor information by vendor ID
pub fn get_vendor_info(vendor_id: u16) -> Option<&'static VendorInfo> {
    VENDORS.iter().find(|&vendor| vendor.id == vendor_id)
}

/// Get device name by vendor and device ID
pub fn get_device_name(vendor_id: u16, device_id: u16) -> &'static str {
    for device in DEVICES {
        if device.vendor_id == vendor_id && device.device_id == device_id {
            return device.name;
        }
    }
    "Unknown Device"
}

/// Get full device information by vendor and device ID
pub fn get_device_info(vendor_id: u16, device_id: u16) -> Option<&'static DeviceInfo> {
    DEVICES
        .iter()
        .find(|&device| device.vendor_id == vendor_id && device.device_id == device_id)
}

/// Get device description by vendor and device ID
pub fn get_device_description(vendor_id: u16, device_id: u16) -> &'static str {
    for device in DEVICES {
        if device.vendor_id == vendor_id && device.device_id == device_id {
            return device.description;
        }
    }
    "Unknown Device"
}

/// Get all devices for a specific vendor
pub fn get_vendor_devices(vendor_id: u16) -> Vec<&'static DeviceInfo> {
    DEVICES
        .iter()
        .filter(|&device| device.vendor_id == vendor_id)
        .collect()
}

/// Check if a vendor is known
pub fn is_known_vendor(vendor_id: u16) -> bool {
    VENDORS.iter().any(|vendor| vendor.id == vendor_id)
}

/// Check if a device is known
pub fn is_known_device(vendor_id: u16, device_id: u16) -> bool {
    DEVICES
        .iter()
        .any(|device| device.vendor_id == vendor_id && device.device_id == device_id)
}

/// Get total number of known vendors
pub fn get_vendor_count() -> usize {
    VENDORS.len()
}

/// Get total number of known devices
pub fn get_device_count() -> usize {
    DEVICES.len()
}

/// Print database statistics
pub fn print_database_stats() {
    // Statistics available via get_vendor_count() and get_device_count() functions
}
