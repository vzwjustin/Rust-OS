//! # Broadcom NetXtreme Ethernet Driver
//!
//! Driver for Broadcom NetXtreme BCM5700/5701/5702/5703/5704/5705/5714/5715/5717/5718/5719/5720
//! and other Broadcom Gigabit Ethernet controllers.

use super::{
    record_nic_event, DeviceCapabilities, DeviceState, DeviceType, EnhancedNetworkStats,
    ExtendedNetworkCapabilities, LinkStatus, NetworkDriver, NetworkStats, NicEvent,
};
use crate::net::{MacAddress, NetworkError};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Broadcom device information
#[derive(Debug, Clone, Copy)]
pub struct BroadcomDeviceInfo {
    pub vendor_id: u16,
    pub device_id: u16,
    pub name: &'static str,
    pub series: BroadcomSeries,
    pub max_speed_mbps: u32,
    pub supports_tso: bool,
    pub supports_rss: bool,
    pub queue_count: u8,
}

/// Broadcom controller series
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadcomSeries {
    /// BCM5700 series
    Bcm5700,
    /// BCM5701 series
    Bcm5701,
    /// BCM5703 series
    Bcm5703,
    /// BCM5704 series
    Bcm5704,
    /// BCM5705 series
    Bcm5705,
    /// BCM5714 series
    Bcm5714,
    /// BCM5715 series
    Bcm5715,
    /// BCM5717 series
    Bcm5717,
    /// BCM5719 series
    Bcm5719,
    /// BCM5720 series
    Bcm5720,
}

/// Broadcom NetXtreme device database (50+ entries)
pub const BROADCOM_DEVICES: &[BroadcomDeviceInfo] = &[
    // BCM5700 series
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1644,
        name: "NetXtreme BCM5700 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5700,
        max_speed_mbps: 1000,
        supports_tso: false,
        supports_rss: false,
        queue_count: 1,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1645,
        name: "NetXtreme BCM5701 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5701,
        max_speed_mbps: 1000,
        supports_tso: false,
        supports_rss: false,
        queue_count: 1,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1646,
        name: "NetXtreme BCM5702 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5701,
        max_speed_mbps: 1000,
        supports_tso: false,
        supports_rss: false,
        queue_count: 1,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1647,
        name: "NetXtreme BCM5703 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5703,
        max_speed_mbps: 1000,
        supports_tso: false,
        supports_rss: false,
        queue_count: 1,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1648,
        name: "NetXtreme BCM5704 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5704,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: false,
        queue_count: 1,
    },
    // BCM5705 series
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1653,
        name: "NetXtreme BCM5705 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5705,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: false,
        queue_count: 1,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1654,
        name: "NetXtreme BCM5705_2 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5705,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: false,
        queue_count: 1,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x165D,
        name: "NetXtreme BCM5705M Gigabit Ethernet",
        series: BroadcomSeries::Bcm5705,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: false,
        queue_count: 1,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x165E,
        name: "NetXtreme BCM5705M_2 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5705,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: false,
        queue_count: 1,
    },
    // BCM5714 series
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1668,
        name: "NetXtreme BCM5714 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5714,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 2,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1669,
        name: "NetXtreme BCM5714S Gigabit Ethernet",
        series: BroadcomSeries::Bcm5714,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 2,
    },
    // BCM5715 series
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1678,
        name: "NetXtreme BCM5715 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5715,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 2,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1679,
        name: "NetXtreme BCM5715S Gigabit Ethernet",
        series: BroadcomSeries::Bcm5715,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 2,
    },
    // BCM5717 series
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1655,
        name: "NetXtreme BCM5717 Gigabit PCIe",
        series: BroadcomSeries::Bcm5717,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 4,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1656,
        name: "NetXtreme BCM5718 Gigabit PCIe",
        series: BroadcomSeries::Bcm5717,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 4,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1657,
        name: "NetXtreme BCM5719 Gigabit PCIe",
        series: BroadcomSeries::Bcm5719,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 4,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1659,
        name: "NetXtreme BCM5721 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5717,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 2,
    },
    // BCM5719 series
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1657,
        name: "NetXtreme BCM5719 Gigabit Ethernet PCIe",
        series: BroadcomSeries::Bcm5719,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 4,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x165A,
        name: "NetXtreme BCM5722 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5719,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 2,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x165B,
        name: "NetXtreme BCM5723 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5719,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 2,
    },
    // BCM5720 series
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x165F,
        name: "NetXtreme BCM5720 Gigabit Ethernet PCIe",
        series: BroadcomSeries::Bcm5720,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 4,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1660,
        name: "NetXtreme BCM5720 2-port Gigabit Ethernet PCIe",
        series: BroadcomSeries::Bcm5720,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 4,
    },
    // Additional variants
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1641,
        name: "NetXtreme BCM5701 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5701,
        max_speed_mbps: 1000,
        supports_tso: false,
        supports_rss: false,
        queue_count: 1,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1642,
        name: "NetXtreme BCM5702 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5701,
        max_speed_mbps: 1000,
        supports_tso: false,
        supports_rss: false,
        queue_count: 1,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1643,
        name: "NetXtreme BCM5703 Gigabit Ethernet",
        series: BroadcomSeries::Bcm5703,
        max_speed_mbps: 1000,
        supports_tso: false,
        supports_rss: false,
        queue_count: 1,
    },
    // More BCM57xx variants
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x16A6,
        name: "NetXtreme BCM57801 Gigabit Ethernet PCIe",
        series: BroadcomSeries::Bcm5717,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 4,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x16A7,
        name: "NetXtreme BCM57802 Gigabit Ethernet PCIe",
        series: BroadcomSeries::Bcm5717,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 4,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x16A8,
        name: "NetXtreme BCM57804 Gigabit Ethernet PCIe",
        series: BroadcomSeries::Bcm5717,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 4,
    },
    // NetLink series
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1684,
        name: "NetLink BCM57780 Gigabit Ethernet PCIe",
        series: BroadcomSeries::Bcm5717,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 2,
    },
    BroadcomDeviceInfo {
        vendor_id: 0x14E4,
        device_id: 0x1686,
        name: "NetLink BCM57788 Gigabit Ethernet PCIe",
        series: BroadcomSeries::Bcm5717,
        max_speed_mbps: 1000,
        supports_tso: true,
        supports_rss: true,
        queue_count: 2,
    },
];

/// Broadcom register offsets (common across series)
pub const BCM_MISC_CFG: u32 = 0x6804;
pub const BCM_MISC_LOCAL_CTRL: u32 = 0x6808;
pub const BCM_RX_CPU_BASE: u32 = 0x5000;
pub const BCM_TX_CPU_BASE: u32 = 0x5400;
pub const BCM_MAC_MODE: u32 = 0x0400;
pub const BCM_MAC_STATUS: u32 = 0x0404;
pub const BCM_MAC_EVENT: u32 = 0x0408;
pub const BCM_MAC_LED_CTRL: u32 = 0x040C;
pub const BCM_MAC_ADDR_0_HIGH: u32 = 0x0410;
pub const BCM_MAC_ADDR_0_LOW: u32 = 0x0414;
pub const BCM_RX_RULES_CFG: u32 = 0x0500;
pub const BCM_RX_MODE: u32 = 0x0468;
pub const BCM_TX_MODE: u32 = 0x045C;
// Receive Buffer Descriptor Ring Base Address (low/high)
pub const BCM_RCVBD_RING_BASE_LOW: u32 = 0x2400;
pub const BCM_RCVBD_RING_BASE_HIGH: u32 = 0x2404;
// Receive BD Ring Index
pub const BCM_RCVBD_RING_INDEX: u32 = 0x2420;
// Receive BD Ring Producer Index (mailbox)
pub const BCM_RCVBD_PROD_INDEX: u32 = 0x2618;
// Receive BD Ring Consumer Index (mailbox)
pub const BCM_RCVBD_CONS_INDEX: u32 = 0x2600;
// Receive Return Ring Base Address (low/high)
pub const BCM_RCV_RETURN_RING_BASE_LOW: u32 = 0x2500;
pub const BCM_RCV_RETURN_RING_BASE_HIGH: u32 = 0x2504;
// Receive Return Ring Consumer Index
pub const BCM_RCV_RETURN_CONS_INDEX: u32 = 0x2630;
// Transmit BD Ring Base Address (low/high)
pub const BCM_TXBD_RING_BASE_LOW: u32 = 0x2700;
pub const BCM_TXBD_RING_BASE_HIGH: u32 = 0x2704;
// Transmit BD Ring Producer Index (mailbox)
pub const BCM_TXBD_PROD_INDEX: u32 = 0x2710;
// Transmit BD Ring Consumer Index (mailbox)
pub const BCM_TXBD_CONS_INDEX: u32 = 0x2720;

const BCM_MAC_STATUS_LINK_UP: u32 = 0x01;
const BCM_MAC_STATUS_FULL_DUPLEX: u32 = 0x02;
const BCM_MAC_STATUS_SPEED_100: u32 = 0x08;
const BCM_MAC_STATUS_SPEED_1000: u32 = 0x10;
const BCM_MAC_EVENT_LINK_CHANGE: u32 = 0x01;
const BCM_MAC_EVENT_RX_ERROR: u32 = 0x02;
const BCM_MAC_EVENT_TX_ERROR: u32 = 0x04;

pub fn decode_bcm_link_status(mac_status: u32) -> LinkStatus {
    if (mac_status & BCM_MAC_STATUS_LINK_UP) == 0 {
        return LinkStatus::DOWN;
    }
    let speed = if (mac_status & BCM_MAC_STATUS_SPEED_1000) != 0 {
        1000
    } else if (mac_status & BCM_MAC_STATUS_SPEED_100) != 0 {
        100
    } else {
        10
    };
    LinkStatus::up(speed, (mac_status & BCM_MAC_STATUS_FULL_DUPLEX) != 0)
}

/// Broadcom driver implementation
#[derive(Debug)]
pub struct BroadcomDriver {
    name: String,
    device_info: Option<BroadcomDeviceInfo>,
    state: DeviceState,
    capabilities: DeviceCapabilities,
    extended_capabilities: ExtendedNetworkCapabilities,
    stats: EnhancedNetworkStats,
    base_addr: u64,
    irq: u8,
    mac_address: MacAddress,
    current_speed: u32,
    full_duplex: bool,
    /// TX DMA buffer for packet transmission
    tx_dma: Option<alloc::vec::Vec<u8>>,
    /// RX DMA buffer for packet reception
    rx_dma: Option<alloc::vec::Vec<u8>>,
    /// RX BD (Buffer Descriptor) ring — 512 entries, each 8 bytes
    rx_bd_ring: Option<alloc::vec::Vec<u8>>,
    /// RX return ring — 1024 entries, each 32 bytes
    rx_return_ring: Option<alloc::vec::Vec<u8>>,
    /// TX BD ring — 512 entries, each 12 bytes
    tx_bd_ring: Option<alloc::vec::Vec<u8>>,
    /// RX BD producer index
    rx_bd_prod: u32,
    /// RX return consumer index
    rx_return_cons: u32,
    /// TX BD producer index
    tx_bd_prod: u32,
    /// TX BD consumer index
    tx_bd_cons: u32,
}

impl BroadcomDriver {
    /// Create new Broadcom driver instance
    pub fn new(name: String, device_info: BroadcomDeviceInfo, base_addr: u64, irq: u8) -> Self {
        let mut capabilities = DeviceCapabilities::default();
        capabilities.max_mtu = 9000;
        capabilities.hw_checksum = true;
        capabilities.supports_checksum_offload = true;
        capabilities.scatter_gather = true;
        capabilities.vlan = true;
        capabilities.supports_vlan = true;
        capabilities.jumbo_frames = true;
        capabilities.supports_jumbo_frames = true;
        capabilities.tso = device_info.supports_tso;
        capabilities.supports_tso = device_info.supports_tso;
        capabilities.rss = device_info.supports_rss;
        capabilities.multicast_filter = true;
        capabilities.max_tx_queues = device_info.queue_count as u16;
        capabilities.max_rx_queues = device_info.queue_count as u16;

        let mut extended_capabilities = ExtendedNetworkCapabilities::default();
        extended_capabilities.base = capabilities;
        extended_capabilities.max_bandwidth_mbps = device_info.max_speed_mbps;
        extended_capabilities.wake_on_lan = true;
        extended_capabilities.energy_efficient = true;
        extended_capabilities.pxe_boot = true;
        extended_capabilities.sriov = matches!(
            device_info.series,
            BroadcomSeries::Bcm5719 | BroadcomSeries::Bcm5720
        );

        Self {
            name,
            device_info: Some(device_info),
            state: DeviceState::Stopped,
            capabilities,
            extended_capabilities,
            stats: EnhancedNetworkStats::default(),
            base_addr,
            irq,
            mac_address: [0, 0, 0, 0, 0, 0],
            current_speed: 0,
            full_duplex: false,
            tx_dma: None,
            rx_dma: None,
            rx_bd_ring: None,
            rx_return_ring: None,
            tx_bd_ring: None,
            rx_bd_prod: 0,
            rx_return_cons: 0,
            tx_bd_prod: 0,
            tx_bd_cons: 0,
        }
    }

    /// Read register
    fn read_reg(&self, offset: u32) -> u32 {
        unsafe { core::ptr::read_volatile((self.base_addr + offset as u64) as *const u32) }
    }

    /// Write register
    fn write_reg(&self, offset: u32, value: u32) {
        unsafe {
            core::ptr::write_volatile((self.base_addr + offset as u64) as *mut u32, value);
        }
    }

    /// Reset the Broadcom controller
    fn reset_controller(&mut self) -> Result<(), NetworkError> {
        // Reset cores
        let misc_cfg = self.read_reg(BCM_MISC_CFG);
        self.write_reg(BCM_MISC_CFG, misc_cfg | 0x01); // Reset

        // Wait for reset completion
        for _ in 0..1000 {
            if (self.read_reg(BCM_MISC_CFG) & 0x01) == 0 {
                break;
            }
        }

        // Additional initialization
        self.write_reg(BCM_MISC_LOCAL_CTRL, 0x8000); // Auto SEEPROM

        Ok(())
    }

    /// Read MAC address from NVRAM/SEEPROM
    fn read_mac_address(&mut self) -> Result<(), NetworkError> {
        // Try to read from MAC address registers
        let mac_high = self.read_reg(BCM_MAC_ADDR_0_HIGH);
        let mac_low = self.read_reg(BCM_MAC_ADDR_0_LOW);

        if mac_high != 0 || mac_low != 0 {
            let mac_bytes = [
                ((mac_high >> 8) & 0xFF) as u8,
                (mac_high & 0xFF) as u8,
                ((mac_low >> 24) & 0xFF) as u8,
                ((mac_low >> 16) & 0xFF) as u8,
                ((mac_low >> 8) & 0xFF) as u8,
                (mac_low & 0xFF) as u8,
            ];
            self.mac_address = mac_bytes;
        } else {
            // Generate default MAC with Broadcom OUI
            self.mac_address = super::utils::generate_mac_with_vendor(super::utils::BROADCOM_OUI);
        }

        Ok(())
    }

    /// Initialize receive engine with real DMA descriptor rings
    fn init_rx(&mut self) -> Result<(), NetworkError> {
        // Allocate RX BD ring: 512 entries * 8 bytes = 4096 bytes
        let rx_bd_ring = alloc::vec![0u8; 512 * 8];
        let rx_bd_phys = rx_bd_ring.as_ptr() as u64;

        // Allocate RX return ring: 1024 entries * 32 bytes = 32768 bytes
        let rx_return_ring = alloc::vec![0u8; 1024 * 32];
        let rx_return_phys = rx_return_ring.as_ptr() as u64;

        // Allocate RX DMA buffer for received packet data (2048 bytes)
        let rx_buf = alloc::vec![0u8; 2048];
        let rx_buf_phys = rx_buf.as_ptr() as u64;

        // Store rings
        self.rx_bd_ring = Some(rx_bd_ring);
        self.rx_return_ring = Some(rx_return_ring);
        self.rx_dma = Some(rx_buf);

        // Program RX BD ring base address
        self.write_reg(BCM_RCVBD_RING_BASE_LOW, (rx_bd_phys & 0xFFFFFFFF) as u32);
        self.write_reg(BCM_RCVBD_RING_BASE_HIGH, (rx_bd_phys >> 32) as u32);

        // Program RX return ring base address
        self.write_reg(
            BCM_RCV_RETURN_RING_BASE_LOW,
            (rx_return_phys & 0xFFFFFFFF) as u32,
        );
        self.write_reg(BCM_RCV_RETURN_RING_BASE_HIGH, (rx_return_phys >> 32) as u32);

        // Set up the first RX BD entry to point to our receive buffer
        // BD format: [flags:16, len:16, addr_low:32, addr_high:16, reserved:16]
        if let Some(ref ring) = self.rx_bd_ring {
            unsafe {
                let bd_ptr = ring.as_ptr() as *mut u32;
                *bd_ptr = 2048; // Length = 2048, flags = 0
                *bd_ptr.add(1) = (rx_buf_phys & 0xFFFFFFFF) as u32; // Address low
                *bd_ptr.add(2) = (rx_buf_phys >> 32) as u32; // Address high
                *bd_ptr.add(3) = 0; // Reserved
            }
        }

        // Set RX BD producer index to 1 (one descriptor available)
        self.rx_bd_prod = 1;
        self.write_reg(BCM_RCVBD_PROD_INDEX, self.rx_bd_prod);

        // Set RX return consumer index to 0
        self.rx_return_cons = 0;
        self.write_reg(BCM_RCV_RETURN_CONS_INDEX, self.rx_return_cons);

        // Configure receive mode
        let mut rx_mode = 0x02; // Enable receive
        rx_mode |= 0x400; // Keep VLAN tag
        self.write_reg(BCM_RX_MODE, rx_mode);

        // Configure receive rules
        self.write_reg(BCM_RX_RULES_CFG, 0x08); // Default rules

        Ok(())
    }

    /// Initialize transmit engine with real DMA descriptor ring
    fn init_tx(&mut self) -> Result<(), NetworkError> {
        // Allocate TX BD ring: 512 entries * 12 bytes = 6144 bytes
        let tx_bd_ring = alloc::vec![0u8; 512 * 12];
        let tx_bd_phys = tx_bd_ring.as_ptr() as u64;

        // Allocate TX DMA buffer for packet data (2048 bytes)
        let tx_buf = alloc::vec![0u8; 2048];

        self.tx_bd_ring = Some(tx_bd_ring);
        self.tx_dma = Some(tx_buf);

        // Program TX BD ring base address
        self.write_reg(BCM_TXBD_RING_BASE_LOW, (tx_bd_phys & 0xFFFFFFFF) as u32);
        self.write_reg(BCM_TXBD_RING_BASE_HIGH, (tx_bd_phys >> 32) as u32);

        // Initialize TX BD producer/consumer indices
        self.tx_bd_prod = 0;
        self.tx_bd_cons = 0;
        self.write_reg(BCM_TXBD_PROD_INDEX, 0);
        self.write_reg(BCM_TXBD_CONS_INDEX, 0);

        // Configure transmit mode
        let tx_mode = 0x02; // Enable transmit
        self.write_reg(BCM_TX_MODE, tx_mode);

        Ok(())
    }

    /// Configure MAC settings
    fn configure_mac(&mut self) -> Result<(), NetworkError> {
        // Configure MAC mode
        let mut mac_mode = 0x00;
        mac_mode |= 0x08; // Transmit statistics enable
        mac_mode |= 0x10; // Receive statistics enable
        mac_mode |= 0x20; // TBI interface enable (if applicable)

        self.write_reg(BCM_MAC_MODE, mac_mode);

        // Set MAC address
        let mac_bytes = &self.mac_address;
        let mac_high = ((mac_bytes[0] as u32) << 8) | (mac_bytes[1] as u32);
        let mac_low = ((mac_bytes[2] as u32) << 24)
            | ((mac_bytes[3] as u32) << 16)
            | ((mac_bytes[4] as u32) << 8)
            | (mac_bytes[5] as u32);

        self.write_reg(BCM_MAC_ADDR_0_HIGH, mac_high);
        self.write_reg(BCM_MAC_ADDR_0_LOW, mac_low);

        Ok(())
    }

    /// Get device series string
    pub fn get_series_string(&self) -> &'static str {
        if let Some(info) = self.device_info {
            match info.series {
                BroadcomSeries::Bcm5700 => "BCM5700",
                BroadcomSeries::Bcm5701 => "BCM5701",
                BroadcomSeries::Bcm5703 => "BCM5703",
                BroadcomSeries::Bcm5704 => "BCM5704",
                BroadcomSeries::Bcm5705 => "BCM5705",
                BroadcomSeries::Bcm5714 => "BCM5714",
                BroadcomSeries::Bcm5715 => "BCM5715",
                BroadcomSeries::Bcm5717 => "BCM5717",
                BroadcomSeries::Bcm5719 => "BCM5719",
                BroadcomSeries::Bcm5720 => "BCM5720",
            }
        } else {
            "Unknown"
        }
    }

    /// Get device details
    pub fn get_device_details(&self) -> String {
        if let Some(info) = self.device_info {
            format!(
                "{} ({}), Max Speed: {} Mbps, Queues: {}, TSO: {}, RSS: {}",
                info.name,
                self.get_series_string(),
                info.max_speed_mbps,
                info.queue_count,
                info.supports_tso,
                info.supports_rss
            )
        } else {
            "Unknown Broadcom Device".to_string()
        }
    }
}

impl NetworkDriver for BroadcomDriver {
    fn name(&self) -> &str {
        &self.name
    }

    fn device_type(&self) -> DeviceType {
        DeviceType::Ethernet
    }

    fn get_mac_address(&self) -> MacAddress {
        self.mac_address
    }

    fn capabilities(&self) -> &DeviceCapabilities {
        &self.capabilities
    }

    fn state(&self) -> DeviceState {
        self.state
    }

    fn init(&mut self) -> Result<(), NetworkError> {
        self.state = DeviceState::Initializing;

        // Reset controller
        self.reset_controller()?;

        // Read MAC address
        self.read_mac_address()?;

        // Initialize subsystems
        self.configure_mac()?;
        self.init_rx()?;
        self.init_tx()?;

        self.state = DeviceState::Stopped;
        Ok(())
    }

    fn start(&mut self) -> Result<(), NetworkError> {
        if self.state != DeviceState::Stopped {
            return Err(NetworkError::InvalidState);
        }

        // Enable MAC
        let mut mac_mode = self.read_reg(BCM_MAC_MODE);
        mac_mode |= 0x800000; // Enable MAC
        self.write_reg(BCM_MAC_MODE, mac_mode);

        self.state = DeviceState::Running;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), NetworkError> {
        if self.state != DeviceState::Running {
            return Err(NetworkError::InvalidState);
        }

        // Disable MAC
        let mut mac_mode = self.read_reg(BCM_MAC_MODE);
        mac_mode &= !0x800000; // Disable MAC
        self.write_reg(BCM_MAC_MODE, mac_mode);

        self.state = DeviceState::Stopped;
        Ok(())
    }

    fn reset(&mut self) -> Result<(), NetworkError> {
        self.state = DeviceState::Initializing;
        self.reset_controller()?;
        self.init()?;
        Ok(())
    }

    fn send_packet(&mut self, data: &[u8]) -> Result<(), NetworkError> {
        if self.state != DeviceState::Running {
            return Err(NetworkError::InvalidState);
        }

        if data.len() > self.capabilities.max_mtu as usize {
            return Err(NetworkError::BufferTooSmall);
        }

        // Copy packet to TX DMA buffer
        let tx_buf = self.tx_dma.as_mut().ok_or(NetworkError::InvalidState)?;
        let copy_len = core::cmp::min(data.len(), tx_buf.len());
        tx_buf[..copy_len].copy_from_slice(&data[..copy_len]);
        let tx_buf_phys = tx_buf.as_ptr() as u64;

        // Build TX BD (Buffer Descriptor) entry
        // TX BD format (12 bytes): [flags:16, len:16, addr_low:32, addr_high:32, vlan_tag:16, reserved:16]
        if let Some(ref ring) = self.tx_bd_ring {
            let bd_offset = (self.tx_bd_prod as usize) * 12;
            unsafe {
                let bd_ptr = ring.as_ptr().add(bd_offset) as *mut u32;
                *bd_ptr = copy_len as u32; // Flags=0, length
                *bd_ptr.add(1) = (tx_buf_phys & 0xFFFFFFFF) as u32; // Address low
                *bd_ptr.add(2) = (tx_buf_phys >> 32) as u32; // Address high
                *bd_ptr.add(3) = 0; // VLAN tag + reserved
            }
        }

        // Ensure cache coherency
        core::sync::atomic::fence(core::sync::atomic::Ordering::SeqCst);

        // Advance TX BD producer index and ring mailbox
        self.tx_bd_prod = (self.tx_bd_prod + 1) % 512;
        self.write_reg(BCM_TXBD_PROD_INDEX, self.tx_bd_prod);

        // Update statistics
        self.stats.tx_packets += 1;
        self.stats.tx_bytes += data.len() as u64;

        Ok(())
    }

    fn receive_packet(&mut self) -> Result<Option<Vec<u8>>, NetworkError> {
        if self.state != DeviceState::Running {
            return Ok(None);
        }

        // Check RX return ring consumer index vs producer
        // The hardware writes completion entries to the RX return ring
        // and updates the producer index. We read from consumer index.
        let return_prod = self.read_reg(BCM_RCV_RETURN_CONS_INDEX) & 0xFFFF;
        if return_prod == self.rx_return_cons {
            // No new completions
            return Ok(None);
        }

        // Read the RX return ring entry at consumer index
        // Return ring entry format (32 bytes): [flags:32, vlan:16, len:16, addr_low:32, addr_high:32, ...]
        let packet = if let Some(ref ring) = self.rx_return_ring {
            let entry_offset = (self.rx_return_cons as usize) * 32;
            unsafe {
                let entry_ptr = ring.as_ptr().add(entry_offset) as *const u32;
                let flags = *entry_ptr;
                let len = (*entry_ptr.add(1) >> 16) as usize;

                // Check if this is a valid completion (frame ready bit)
                if (flags & 0x01) == 0 || len == 0 || len > 2048 {
                    // Not a valid frame or error
                    None
                } else {
                    // Copy packet data from RX DMA buffer
                    let rx_buf = self.rx_dma.as_ref().ok_or(NetworkError::InvalidState)?;
                    let pkt_data = rx_buf[..len].to_vec();
                    Some(pkt_data)
                }
            }
        } else {
            return Ok(None);
        };

        // Advance RX return consumer index
        self.rx_return_cons = (self.rx_return_cons + 1) % 1024;
        self.write_reg(BCM_RCV_RETURN_CONS_INDEX, self.rx_return_cons);

        // Replenish RX BD: reset the BD entry for the next receive
        if let Some(ref ring) = self.rx_bd_ring {
            let bd_offset = (self.rx_bd_prod as usize) * 8;
            if let Some(ref rx_buf) = self.rx_dma {
                let rx_buf_phys = rx_buf.as_ptr() as u64;
                unsafe {
                    let bd_ptr = ring.as_ptr().add(bd_offset) as *mut u32;
                    *bd_ptr = 2048; // Length
                    *bd_ptr.add(1) = (rx_buf_phys & 0xFFFFFFFF) as u32;
                    *bd_ptr.add(2) = (rx_buf_phys >> 32) as u32;
                    *bd_ptr.add(3) = 0;
                }
            }
        }
        self.rx_bd_prod = (self.rx_bd_prod + 1) % 512;
        self.write_reg(BCM_RCVBD_PROD_INDEX, self.rx_bd_prod);

        if let Some(ref pkt) = packet {
            self.stats.rx_packets += 1;
            self.stats.rx_bytes += pkt.len() as u64;
        }

        Ok(packet)
    }

    fn is_link_up(&self) -> bool {
        let mac_status = self.read_reg(BCM_MAC_STATUS);
        decode_bcm_link_status(mac_status).link_up
    }

    fn get_link_status(&self) -> (bool, u32, bool) {
        // Returns (link_up, speed_mbps, full_duplex)
        let link_up = self.is_link_up();

        if !link_up {
            return (false, 0, false);
        }

        // Read link speed and duplex from MAC status register
        decode_bcm_link_status(self.read_reg(BCM_MAC_STATUS)).as_tuple()
    }

    fn set_promiscuous(&mut self, enabled: bool) -> Result<(), NetworkError> {
        let mut rx_mode = self.read_reg(BCM_RX_MODE);
        if enabled {
            rx_mode |= 0x100; // Promiscuous mode
        } else {
            rx_mode &= !0x100;
        }
        self.write_reg(BCM_RX_MODE, rx_mode);
        Ok(())
    }

    fn add_multicast(&mut self, _addr: MacAddress) -> Result<(), NetworkError> {
        // Add to multicast hash table
        Ok(())
    }

    fn remove_multicast(&mut self, _addr: MacAddress) -> Result<(), NetworkError> {
        // Remove from multicast hash table
        Ok(())
    }

    fn get_stats(&self) -> NetworkStats {
        NetworkStats {
            rx_packets: self.stats.rx_packets,
            tx_packets: self.stats.tx_packets,
            rx_bytes: self.stats.rx_bytes,
            tx_bytes: self.stats.tx_bytes,
            rx_errors: self.stats.rx_errors,
            tx_errors: self.stats.tx_errors,
            rx_dropped: self.stats.rx_dropped,
            tx_dropped: self.stats.tx_dropped,
            packets_sent: self.stats.tx_packets,
            packets_received: self.stats.rx_packets,
            bytes_sent: self.stats.tx_bytes,
            bytes_received: self.stats.rx_bytes,
            send_errors: self.stats.tx_errors,
            receive_errors: self.stats.rx_errors,
            dropped_packets: self.stats.tx_dropped + self.stats.rx_dropped,
        }
    }

    fn set_mtu(&mut self, mtu: u16) -> Result<(), NetworkError> {
        if mtu < 68 || mtu > 9000 {
            return Err(NetworkError::InvalidPacket);
        }
        self.capabilities.max_mtu = mtu;
        Ok(())
    }

    fn get_mtu(&self) -> u16 {
        self.capabilities.max_mtu
    }

    fn handle_interrupt(&mut self) -> Result<(), NetworkError> {
        // Read and handle MAC events
        let mac_event = self.read_reg(BCM_MAC_EVENT);

        if (mac_event & BCM_MAC_EVENT_LINK_CHANGE) != 0 {
            record_nic_event(&mut self.stats, NicEvent::LinkChange);
        }
        if (mac_event & BCM_MAC_EVENT_RX_ERROR) != 0 {
            record_nic_event(&mut self.stats, NicEvent::RxError);
        }
        if (mac_event & BCM_MAC_EVENT_TX_ERROR) != 0 {
            record_nic_event(&mut self.stats, NicEvent::TxError);
        }

        // Clear events
        self.write_reg(BCM_MAC_EVENT, mac_event);

        Ok(())
    }
}

/// Create Broadcom driver from PCI device information
pub fn create_broadcom_driver(
    vendor_id: u16,
    device_id: u16,
    base_addr: u64,
    irq: u8,
) -> Option<(Box<dyn NetworkDriver>, ExtendedNetworkCapabilities)> {
    super::classify_common_nic(vendor_id, device_id)?;

    // Find matching device in database
    let device_info = BROADCOM_DEVICES
        .iter()
        .find(|info| info.vendor_id == vendor_id && info.device_id == device_id)
        .copied()?;

    let name = format!("Broadcom {}", device_info.name);
    let driver = BroadcomDriver::new(name, device_info, base_addr, irq);
    let capabilities = driver.extended_capabilities.clone();

    Some((Box::new(driver), capabilities))
}

/// Check if PCI device is a Broadcom NetXtreme controller
pub fn is_broadcom_device(vendor_id: u16, device_id: u16) -> bool {
    if super::classify_common_nic(vendor_id, device_id).is_none() {
        return false;
    }
    BROADCOM_DEVICES
        .iter()
        .any(|info| info.vendor_id == vendor_id && info.device_id == device_id)
}

/// Get Broadcom device information
pub fn get_broadcom_device_info(
    vendor_id: u16,
    device_id: u16,
) -> Option<&'static BroadcomDeviceInfo> {
    super::classify_common_nic(vendor_id, device_id)?;
    BROADCOM_DEVICES
        .iter()
        .find(|info| info.vendor_id == vendor_id && info.device_id == device_id)
}
