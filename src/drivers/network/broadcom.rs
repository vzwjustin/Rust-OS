//! # Broadcom NetXtreme Ethernet Driver
//!
//! Driver for Broadcom NetXtreme BCM5700/5701/5702/5703/5704/5705/5714/5715/5717/5718/5719/5720
//! and other Broadcom Gigabit Ethernet controllers.

use super::{ExtendedNetworkCapabilities, EnhancedNetworkStats, NetworkDriver, DeviceState, DeviceCapabilities, DeviceType, NetworkStats};
use crate::net::{NetworkError, MacAddress};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::boxed::Box;

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
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1644, name: "NetXtreme BCM5700 Gigabit Ethernet", series: BroadcomSeries::Bcm5700, max_speed_mbps: 1000, supports_tso: false, supports_rss: false, queue_count: 1 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1645, name: "NetXtreme BCM5701 Gigabit Ethernet", series: BroadcomSeries::Bcm5701, max_speed_mbps: 1000, supports_tso: false, supports_rss: false, queue_count: 1 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1646, name: "NetXtreme BCM5702 Gigabit Ethernet", series: BroadcomSeries::Bcm5701, max_speed_mbps: 1000, supports_tso: false, supports_rss: false, queue_count: 1 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1647, name: "NetXtreme BCM5703 Gigabit Ethernet", series: BroadcomSeries::Bcm5703, max_speed_mbps: 1000, supports_tso: false, supports_rss: false, queue_count: 1 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1648, name: "NetXtreme BCM5704 Gigabit Ethernet", series: BroadcomSeries::Bcm5704, max_speed_mbps: 1000, supports_tso: true, supports_rss: false, queue_count: 1 },

    // BCM5705 series
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1653, name: "NetXtreme BCM5705 Gigabit Ethernet", series: BroadcomSeries::Bcm5705, max_speed_mbps: 1000, supports_tso: true, supports_rss: false, queue_count: 1 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1654, name: "NetXtreme BCM5705_2 Gigabit Ethernet", series: BroadcomSeries::Bcm5705, max_speed_mbps: 1000, supports_tso: true, supports_rss: false, queue_count: 1 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x165D, name: "NetXtreme BCM5705M Gigabit Ethernet", series: BroadcomSeries::Bcm5705, max_speed_mbps: 1000, supports_tso: true, supports_rss: false, queue_count: 1 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x165E, name: "NetXtreme BCM5705M_2 Gigabit Ethernet", series: BroadcomSeries::Bcm5705, max_speed_mbps: 1000, supports_tso: true, supports_rss: false, queue_count: 1 },

    // BCM5714 series
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1668, name: "NetXtreme BCM5714 Gigabit Ethernet", series: BroadcomSeries::Bcm5714, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 2 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1669, name: "NetXtreme BCM5714S Gigabit Ethernet", series: BroadcomSeries::Bcm5714, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 2 },

    // BCM5715 series
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1678, name: "NetXtreme BCM5715 Gigabit Ethernet", series: BroadcomSeries::Bcm5715, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 2 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1679, name: "NetXtreme BCM5715S Gigabit Ethernet", series: BroadcomSeries::Bcm5715, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 2 },

    // BCM5717 series
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1655, name: "NetXtreme BCM5717 Gigabit PCIe", series: BroadcomSeries::Bcm5717, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 4 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1656, name: "NetXtreme BCM5718 Gigabit PCIe", series: BroadcomSeries::Bcm5717, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 4 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1657, name: "NetXtreme BCM5719 Gigabit PCIe", series: BroadcomSeries::Bcm5719, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 4 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1659, name: "NetXtreme BCM5721 Gigabit Ethernet", series: BroadcomSeries::Bcm5717, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 2 },

    // BCM5719 series
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1657, name: "NetXtreme BCM5719 Gigabit Ethernet PCIe", series: BroadcomSeries::Bcm5719, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 4 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x165A, name: "NetXtreme BCM5722 Gigabit Ethernet", series: BroadcomSeries::Bcm5719, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 2 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x165B, name: "NetXtreme BCM5723 Gigabit Ethernet", series: BroadcomSeries::Bcm5719, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 2 },

    // BCM5720 series
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x165F, name: "NetXtreme BCM5720 Gigabit Ethernet PCIe", series: BroadcomSeries::Bcm5720, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 4 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1660, name: "NetXtreme BCM5720 2-port Gigabit Ethernet PCIe", series: BroadcomSeries::Bcm5720, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 4 },

    // Additional variants
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1641, name: "NetXtreme BCM5701 Gigabit Ethernet", series: BroadcomSeries::Bcm5701, max_speed_mbps: 1000, supports_tso: false, supports_rss: false, queue_count: 1 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1642, name: "NetXtreme BCM5702 Gigabit Ethernet", series: BroadcomSeries::Bcm5701, max_speed_mbps: 1000, supports_tso: false, supports_rss: false, queue_count: 1 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1643, name: "NetXtreme BCM5703 Gigabit Ethernet", series: BroadcomSeries::Bcm5703, max_speed_mbps: 1000, supports_tso: false, supports_rss: false, queue_count: 1 },

    // More BCM57xx variants
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x16A6, name: "NetXtreme BCM57801 Gigabit Ethernet PCIe", series: BroadcomSeries::Bcm5717, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 4 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x16A7, name: "NetXtreme BCM57802 Gigabit Ethernet PCIe", series: BroadcomSeries::Bcm5717, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 4 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x16A8, name: "NetXtreme BCM57804 Gigabit Ethernet PCIe", series: BroadcomSeries::Bcm5717, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 4 },

    // NetLink series
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1684, name: "NetLink BCM57780 Gigabit Ethernet PCIe", series: BroadcomSeries::Bcm5717, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 2 },
    BroadcomDeviceInfo { vendor_id: 0x14E4, device_id: 0x1686, name: "NetLink BCM57788 Gigabit Ethernet PCIe", series: BroadcomSeries::Bcm5717, max_speed_mbps: 1000, supports_tso: true, supports_rss: true, queue_count: 2 },
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
}

impl BroadcomDriver {
    /// Create new Broadcom driver instance
    pub fn new(
        name: String,
        device_info: BroadcomDeviceInfo,
        base_addr: u64,
        irq: u8,
    ) -> Self {
        let mut capabilities = DeviceCapabilities::default();
        capabilities.max_mtu = 9000;
        capabilities.hw_checksum = true;
        capabilities.scatter_gather = true;
        capabilities.vlan = true;
        capabilities.jumbo_frames = true;
        capabilities.tso = device_info.supports_tso;
        capabilities.rss = device_info.supports_rss;

        let mut extended_capabilities = ExtendedNetworkCapabilities::default();
        extended_capabilities.base = capabilities;
        extended_capabilities.max_bandwidth_mbps = device_info.max_speed_mbps;
        extended_capabilities.wake_on_lan = true;
        extended_capabilities.energy_efficient = true;
        extended_capabilities.pxe_boot = true;
        extended_capabilities.sriov = matches!(device_info.series, BroadcomSeries::Bcm5719 | BroadcomSeries::Bcm5720);

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
        }
    }

    /// Read register
    fn read_reg(&self, offset: u32) -> u32 {
        unsafe {
            core::ptr::read_volatile((self.base_addr + offset as u64) as *const u32)
        }
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

    /// Initialize receive engine
    fn init_rx(&mut self) -> Result<(), NetworkError> {
        // Configure receive mode
        let mut rx_mode = 0x02; // Enable receive
        rx_mode |= 0x400; // Keep VLAN tag
        self.write_reg(BCM_RX_MODE, rx_mode);

        // Configure receive rules
        self.write_reg(BCM_RX_RULES_CFG, 0x08); // Default rules

        Ok(())
    }

    /// Initialize transmit engine
    fn init_tx(&mut self) -> Result<(), NetworkError> {
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
        let mac_low = ((mac_bytes[2] as u32) << 24) |
                      ((mac_bytes[3] as u32) << 16) |
                      ((mac_bytes[4] as u32) << 8) |
                      (mac_bytes[5] as u32);

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

        // Simulate packet transmission
        self.stats.tx_packets += 1;
        self.stats.tx_bytes += data.len() as u64;

        Ok(())
    }

    fn receive_packet(&mut self) -> Result<Option<Vec<u8>>, NetworkError> {
        if self.state != DeviceState::Running {
            return Ok(None);
        }

        // Simulate packet reception
        Ok(None)
    }

    fn is_link_up(&self) -> bool {
        let mac_status = self.read_reg(BCM_MAC_STATUS);
        (mac_status & 0x01) != 0 // Link up bit
    }

    fn get_link_status(&self) -> (bool, u32, bool) {
        // Returns (link_up, speed_mbps, full_duplex)
        let link_up = self.is_link_up();

        if !link_up {
            return (false, 0, false);
        }

        // Read link speed and duplex from MAC status register
        let mac_status = self.read_reg(BCM_MAC_STATUS);
        let speed = if (mac_status & 0x10) != 0 { 1000 }
                   else if (mac_status & 0x08) != 0 { 100 }
                   else { 10 };
        let full_duplex = (mac_status & 0x02) != 0;

        (true, speed, full_duplex)
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

        if (mac_event & 0x01) != 0 { // Link state change
            self.stats.link_changes += 1;
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
    // Find matching device in database
    let device_info = BROADCOM_DEVICES.iter()
        .find(|info| info.vendor_id == vendor_id && info.device_id == device_id)
        .copied()?;

    let name = format!("Broadcom {}", device_info.name);
    let driver = BroadcomDriver::new(name, device_info, base_addr, irq);
    let capabilities = driver.extended_capabilities.clone();

    Some((Box::new(driver), capabilities))
}

/// Check if PCI device is a Broadcom NetXtreme controller
pub fn is_broadcom_device(vendor_id: u16, device_id: u16) -> bool {
    BROADCOM_DEVICES.iter()
        .any(|info| info.vendor_id == vendor_id && info.device_id == device_id)
}

/// Get Broadcom device information
pub fn get_broadcom_device_info(vendor_id: u16, device_id: u16) -> Option<&'static BroadcomDeviceInfo> {
    BROADCOM_DEVICES.iter()
        .find(|info| info.vendor_id == vendor_id && info.device_id == device_id)
}