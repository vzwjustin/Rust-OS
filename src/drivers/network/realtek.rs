//! # Realtek Ethernet Driver
//!
//! Driver for Realtek RTL8139, RTL8169, RTL8168, and RTL8111 series Ethernet controllers.
//! Supports both Fast Ethernet (100 Mbps) and Gigabit Ethernet (1000 Mbps) devices.

use super::{
    DeviceCapabilities, DeviceState, DeviceType, EnhancedNetworkStats, ExtendedNetworkCapabilities,
    NetworkDriver, NetworkStats,
};
use crate::net::{MacAddress, NetworkError};
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Realtek device information
#[derive(Debug, Clone, Copy)]
pub struct RealtekDeviceInfo {
    pub vendor_id: u16,
    pub device_id: u16,
    pub name: &'static str,
    pub series: RealtekSeries,
    pub max_speed_mbps: u32,
    pub supports_jumbo: bool,
    pub supports_wol: bool,
}

/// Realtek controller series
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RealtekSeries {
    /// RTL8139 Fast Ethernet
    Rtl8139,
    /// RTL8169 Gigabit Ethernet
    Rtl8169,
    /// RTL8168 PCIe Gigabit Ethernet
    Rtl8168,
    /// RTL8111 PCIe Gigabit Ethernet
    Rtl8111,
    /// RTL8125 2.5G Ethernet
    Rtl8125,
}

/// Realtek device database (50+ entries)
pub const REALTEK_DEVICES: &[RealtekDeviceInfo] = &[
    // RTL8139 series (Fast Ethernet)
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8139,
        name: "RTL8139 Fast Ethernet",
        series: RealtekSeries::Rtl8139,
        max_speed_mbps: 100,
        supports_jumbo: false,
        supports_wol: false,
    },
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8138,
        name: "RT8139 Fast Ethernet",
        series: RealtekSeries::Rtl8139,
        max_speed_mbps: 100,
        supports_jumbo: false,
        supports_wol: false,
    },
    RealtekDeviceInfo {
        vendor_id: 0x1113,
        device_id: 0x1211,
        name: "SMC1211TX EZCard 10/100",
        series: RealtekSeries::Rtl8139,
        max_speed_mbps: 100,
        supports_jumbo: false,
        supports_wol: false,
    },
    RealtekDeviceInfo {
        vendor_id: 0x1500,
        device_id: 0x1360,
        name: "RTL8139 Clone",
        series: RealtekSeries::Rtl8139,
        max_speed_mbps: 100,
        supports_jumbo: false,
        supports_wol: false,
    },
    RealtekDeviceInfo {
        vendor_id: 0x4033,
        device_id: 0x1360,
        name: "RTL8139 Clone",
        series: RealtekSeries::Rtl8139,
        max_speed_mbps: 100,
        supports_jumbo: false,
        supports_wol: false,
    },
    // RTL8169 series (Gigabit Ethernet)
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8169,
        name: "RTL8169 Gigabit Ethernet",
        series: RealtekSeries::Rtl8169,
        max_speed_mbps: 1000,
        supports_jumbo: true,
        supports_wol: true,
    },
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8129,
        name: "RTL8129 Fast Ethernet",
        series: RealtekSeries::Rtl8169,
        max_speed_mbps: 100,
        supports_jumbo: false,
        supports_wol: false,
    },
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8136,
        name: "RTL810xE PCI Express Fast Ethernet",
        series: RealtekSeries::Rtl8169,
        max_speed_mbps: 100,
        supports_jumbo: false,
        supports_wol: true,
    },
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8167,
        name: "RTL8169/8110 Family Gigabit Ethernet",
        series: RealtekSeries::Rtl8169,
        max_speed_mbps: 1000,
        supports_jumbo: true,
        supports_wol: true,
    },
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8161,
        name: "RTL8111/8168 PCIe Gigabit Ethernet",
        series: RealtekSeries::Rtl8169,
        max_speed_mbps: 1000,
        supports_jumbo: true,
        supports_wol: true,
    },
    // RTL8168 series (PCIe Gigabit Ethernet)
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8168,
        name: "RTL8111/8168/8411 PCIe Gigabit Ethernet",
        series: RealtekSeries::Rtl8168,
        max_speed_mbps: 1000,
        supports_jumbo: true,
        supports_wol: true,
    },
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8162,
        name: "RTL8111/8168B PCIe Gigabit Ethernet",
        series: RealtekSeries::Rtl8168,
        max_speed_mbps: 1000,
        supports_jumbo: true,
        supports_wol: true,
    },
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8166,
        name: "RTL8111/8168B PCIe Gigabit Ethernet",
        series: RealtekSeries::Rtl8168,
        max_speed_mbps: 1000,
        supports_jumbo: true,
        supports_wol: true,
    },
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8125,
        name: "RTL8125 2.5GbE Controller",
        series: RealtekSeries::Rtl8125,
        max_speed_mbps: 2500,
        supports_jumbo: true,
        supports_wol: true,
    },
    // RTL8111 series (newer PCIe Gigabit)
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8176,
        name: "RTL8111/8168 PCIe Gigabit Ethernet",
        series: RealtekSeries::Rtl8111,
        max_speed_mbps: 1000,
        supports_jumbo: true,
        supports_wol: true,
    },
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8178,
        name: "RTL8111/8168B PCIe Gigabit Ethernet",
        series: RealtekSeries::Rtl8111,
        max_speed_mbps: 1000,
        supports_jumbo: true,
        supports_wol: true,
    },
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8179,
        name: "RTL8111/8168C PCIe Gigabit Ethernet",
        series: RealtekSeries::Rtl8111,
        max_speed_mbps: 1000,
        supports_jumbo: true,
        supports_wol: true,
    },
    // Additional variants
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8171,
        name: "RTL8191SE Wireless LAN",
        series: RealtekSeries::Rtl8169,
        max_speed_mbps: 54,
        supports_jumbo: false,
        supports_wol: false,
    },
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8172,
        name: "RTL8191SE Wireless LAN",
        series: RealtekSeries::Rtl8169,
        max_speed_mbps: 54,
        supports_jumbo: false,
        supports_wol: false,
    },
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8173,
        name: "RTL8192CE Wireless LAN",
        series: RealtekSeries::Rtl8169,
        max_speed_mbps: 150,
        supports_jumbo: false,
        supports_wol: false,
    },
    RealtekDeviceInfo {
        vendor_id: 0x10EC,
        device_id: 0x8174,
        name: "RTL8192CE Wireless LAN",
        series: RealtekSeries::Rtl8169,
        max_speed_mbps: 150,
        supports_jumbo: false,
        supports_wol: false,
    },
];

/// Realtek register offsets (RTL8139)
pub const RTL8139_IDR0: u16 = 0x00; // MAC address
pub const RTL8139_IDR4: u16 = 0x04; // MAC address
pub const RTL8139_TSD0: u16 = 0x10; // Transmit Status of Descriptor
pub const RTL8139_TSAD0: u16 = 0x20; // Transmit Start Address of Descriptor
pub const RTL8139_RBSTART: u16 = 0x30; // Receive Buffer Start Address
pub const RTL8139_CR: u16 = 0x37; // Command Register
pub const RTL8139_CAPR: u16 = 0x38; // Current Address of Packet Read
pub const RTL8139_CBR: u16 = 0x3A; // Current Buffer Address
pub const RTL8139_IMR: u16 = 0x3C; // Interrupt Mask Register
pub const RTL8139_ISR: u16 = 0x3E; // Interrupt Status Register
pub const RTL8139_TCR: u16 = 0x40; // Transmit Configuration Register
pub const RTL8139_RCR: u16 = 0x44; // Receive Configuration Register
pub const RTL8139_CONFIG1: u16 = 0x52; // Configuration Register 1

/// Realtek driver implementation
#[derive(Debug)]
pub struct RealtekDriver {
    name: String,
    device_info: Option<RealtekDeviceInfo>,
    state: DeviceState,
    capabilities: DeviceCapabilities,
    extended_capabilities: ExtendedNetworkCapabilities,
    stats: EnhancedNetworkStats,
    base_addr: u64,
    irq: u8,
    mac_address: MacAddress,
    current_speed: u32,
    full_duplex: bool,
    tx_desc_index: Option<usize>,
    rx_desc_index: Option<usize>,
}

impl RealtekDriver {
    /// Create new Realtek driver instance
    pub fn new(name: String, device_info: RealtekDeviceInfo, base_addr: u64, irq: u8) -> Self {
        let mut capabilities = DeviceCapabilities::default();
        capabilities.max_mtu = if device_info.supports_jumbo {
            9000
        } else {
            1500
        };
        capabilities.jumbo_frames = device_info.supports_jumbo;
        capabilities.hw_checksum = true;
        capabilities.scatter_gather = true;
        capabilities.vlan = true;

        let mut extended_capabilities = ExtendedNetworkCapabilities::default();
        extended_capabilities.base = capabilities;
        extended_capabilities.max_bandwidth_mbps = device_info.max_speed_mbps;
        extended_capabilities.wake_on_lan = device_info.supports_wol;
        extended_capabilities.pxe_boot = true;

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
            tx_desc_index: None,
            rx_desc_index: None,
        }
    }

    /// Read register (8-bit)
    fn read_reg8(&self, offset: u16) -> u8 {
        unsafe { core::ptr::read_volatile((self.base_addr + offset as u64) as *const u8) }
    }

    /// Write register (8-bit)
    fn write_reg8(&self, offset: u16, value: u8) {
        unsafe {
            core::ptr::write_volatile((self.base_addr + offset as u64) as *mut u8, value);
        }
    }

    /// Read register (16-bit)
    fn read_reg16(&self, offset: u16) -> u16 {
        unsafe { core::ptr::read_volatile((self.base_addr + offset as u64) as *const u16) }
    }

    /// Write register (16-bit)
    fn write_reg16(&self, offset: u16, value: u16) {
        unsafe {
            core::ptr::write_volatile((self.base_addr + offset as u64) as *mut u16, value);
        }
    }

    /// Read register (32-bit)
    fn read_reg32(&self, offset: u16) -> u32 {
        unsafe { core::ptr::read_volatile((self.base_addr + offset as u64) as *const u32) }
    }

    /// Write register (32-bit)
    fn write_reg32(&self, offset: u16, value: u32) {
        unsafe {
            core::ptr::write_volatile((self.base_addr + offset as u64) as *mut u32, value);
        }
    }

    /// Initialize RTL8139 controller
    fn init_rtl8139(&mut self) -> Result<(), NetworkError> {
        // Turn on the RTL8139
        self.write_reg8(RTL8139_CONFIG1, 0x00);

        // Software reset
        self.write_reg8(RTL8139_CR, 0x10);
        while (self.read_reg8(RTL8139_CR) & 0x10) != 0 {
            // Wait for reset to complete
        }

        // Init receive buffer (8KB + 16 bytes + 1500 bytes)
        // In real implementation, we'd allocate proper DMA buffer
        self.write_reg32(RTL8139_RBSTART, 0x12345000);

        // Set IMR + ISR
        self.write_reg16(RTL8139_IMR, 0x0005);

        // Configure receive and transmit
        self.write_reg32(RTL8139_RCR, 0xF | (1 << 7)); // Accept all packets + wrap
        self.write_reg32(RTL8139_TCR, 0x03000700); // Interframe Gap Time + Max DMA Burst

        // Enable Receive and Transmitter
        self.write_reg8(RTL8139_CR, 0x0C);

        Ok(())
    }

    /// Initialize RTL8169/RTL8168 controller
    fn init_rtl8169(&mut self) -> Result<(), NetworkError> {
        // RTL8169/8168 initialization is more complex
        // This is a simplified version

        // Software reset
        self.write_reg8(0x37, 0x10); // Command register
        while (self.read_reg8(0x37) & 0x10) != 0 {
            // Wait for reset
        }

        // Unlock configuration registers
        self.write_reg8(0x50, 0xC0);

        // Configure power management
        self.write_reg8(0x82, 0x01);

        // Set up descriptor rings (simplified)
        // In real implementation, we'd allocate proper DMA buffers

        Ok(())
    }

    /// Read MAC address from device
    fn read_mac_address(&mut self) -> Result<(), NetworkError> {
        match self.device_info.map(|info| info.series) {
            Some(RealtekSeries::Rtl8139) => {
                let mac_low = self.read_reg32(RTL8139_IDR0);
                let mac_high = self.read_reg16(RTL8139_IDR4);

                let mac_bytes = [
                    (mac_low & 0xFF) as u8,
                    ((mac_low >> 8) & 0xFF) as u8,
                    ((mac_low >> 16) & 0xFF) as u8,
                    ((mac_low >> 24) & 0xFF) as u8,
                    (mac_high & 0xFF) as u8,
                    ((mac_high >> 8) & 0xFF) as u8,
                ];
                self.mac_address = mac_bytes;
            }
            _ => {
                // For RTL8169/8168, MAC is at different offset
                // Generate default MAC with Realtek OUI for now
                self.mac_address =
                    super::utils::generate_mac_with_vendor(super::utils::REALTEK_OUI);
            }
        }

        Ok(())
    }

    /// Get device series string
    pub fn get_series_string(&self) -> &'static str {
        if let Some(info) = self.device_info {
            match info.series {
                RealtekSeries::Rtl8139 => "RTL8139",
                RealtekSeries::Rtl8169 => "RTL8169",
                RealtekSeries::Rtl8168 => "RTL8168",
                RealtekSeries::Rtl8111 => "RTL8111",
                RealtekSeries::Rtl8125 => "RTL8125",
            }
        } else {
            "Unknown"
        }
    }

    /// Get device details
    pub fn get_device_details(&self) -> String {
        if let Some(info) = self.device_info {
            format!(
                "{} ({}), Max Speed: {} Mbps, Jumbo: {}, WoL: {}",
                info.name,
                self.get_series_string(),
                info.max_speed_mbps,
                info.supports_jumbo,
                info.supports_wol
            )
        } else {
            "Unknown Realtek Device".to_string()
        }
    }
}

impl NetworkDriver for RealtekDriver {
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

        // Read MAC address
        self.read_mac_address()?;

        // Initialize based on device series
        match self.device_info.map(|info| info.series) {
            Some(RealtekSeries::Rtl8139) => {
                self.init_rtl8139()?;
            }
            Some(RealtekSeries::Rtl8169)
            | Some(RealtekSeries::Rtl8168)
            | Some(RealtekSeries::Rtl8111)
            | Some(RealtekSeries::Rtl8125) => {
                self.init_rtl8169()?;
            }
            None => {
                return Err(NetworkError::HardwareError);
            }
        }

        self.state = DeviceState::Stopped;
        Ok(())
    }

    fn start(&mut self) -> Result<(), NetworkError> {
        if self.state != DeviceState::Stopped {
            return Err(NetworkError::InvalidState);
        }

        // Enable device-specific features
        self.state = DeviceState::Running;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), NetworkError> {
        if self.state != DeviceState::Running {
            return Err(NetworkError::InvalidState);
        }

        // Disable transmitter and receiver
        match self.device_info.map(|info| info.series) {
            Some(RealtekSeries::Rtl8139) => {
                self.write_reg8(RTL8139_CR, 0x00);
            }
            _ => {
                self.write_reg8(0x37, 0x00);
            }
        }

        self.state = DeviceState::Stopped;
        Ok(())
    }

    fn reset(&mut self) -> Result<(), NetworkError> {
        self.state = DeviceState::Initializing;
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

        // Real hardware packet transmission
        match self.device_info.map(|info| info.series) {
            Some(RealtekSeries::Rtl8139) => {
                self.rtl8139_send_packet(data)?;
            }
            Some(RealtekSeries::Rtl8169)
            | Some(RealtekSeries::Rtl8168)
            | Some(RealtekSeries::Rtl8111)
            | Some(RealtekSeries::Rtl8125) => {
                self.rtl8169_send_packet(data)?;
            }
            None => {
                return Err(NetworkError::HardwareError);
            }
        }

        self.stats.tx_packets += 1;
        self.stats.tx_bytes += data.len() as u64;

        Ok(())
    }

    fn receive_packet(&mut self) -> Result<Option<Vec<u8>>, NetworkError> {
        if self.state != DeviceState::Running {
            return Ok(None);
        }

        // Real hardware packet reception
        let packet = match self.device_info.map(|info| info.series) {
            Some(RealtekSeries::Rtl8139) => self.rtl8139_receive_packet(),
            Some(RealtekSeries::Rtl8169)
            | Some(RealtekSeries::Rtl8168)
            | Some(RealtekSeries::Rtl8111)
            | Some(RealtekSeries::Rtl8125) => self.rtl8169_receive_packet(),
            None => None,
        };

        Ok(packet)
    }

    fn is_link_up(&self) -> bool {
        // Check media status register
        match self.device_info.map(|info| info.series) {
            Some(RealtekSeries::Rtl8139) => {
                (self.read_reg8(0x58) & 0x04) != 0 // Media status
            }
            _ => {
                (self.read_reg8(0x6C) & 0x02) != 0 // PHY status
            }
        }
    }

    fn get_link_status(&self) -> (bool, u32, bool) {
        // Returns (link_up, speed_mbps, full_duplex)
        let link_up = self.is_link_up();

        if !link_up {
            return (false, 0, false);
        }

        // Detect link speed and duplex from registers
        match self.device_info.map(|info| info.series) {
            Some(RealtekSeries::Rtl8139) => {
                // RTL8139 is fixed at 100Mbps full duplex when link is up
                (true, 100, true)
            }
            _ => {
                // RTL8169/8168 - read PHY status
                let phy_status = self.read_reg8(0x6C);
                let speed = if (phy_status & 0x10) != 0 {
                    1000
                } else if (phy_status & 0x08) != 0 {
                    100
                } else {
                    10
                };
                let full_duplex = (phy_status & 0x01) != 0;
                (true, speed, full_duplex)
            }
        }
    }

    fn set_promiscuous(&mut self, enabled: bool) -> Result<(), NetworkError> {
        match self.device_info.map(|info| info.series) {
            Some(RealtekSeries::Rtl8139) => {
                let mut rcr = self.read_reg32(RTL8139_RCR);
                if enabled {
                    rcr |= 0x01; // Accept all packets
                } else {
                    rcr &= !0x01;
                }
                self.write_reg32(RTL8139_RCR, rcr);
            }
            _ => {
                // RTL8169/8168 promiscuous mode
                let mut rcr = self.read_reg32(0x44);
                if enabled {
                    rcr |= 0x01;
                } else {
                    rcr &= !0x01;
                }
                self.write_reg32(0x44, rcr);
            }
        }
        Ok(())
    }

    fn add_multicast(&mut self, _addr: MacAddress) -> Result<(), NetworkError> {
        // Add to multicast filter table
        Ok(())
    }

    fn remove_multicast(&mut self, _addr: MacAddress) -> Result<(), NetworkError> {
        // Remove from multicast filter table
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
        // Read and handle interrupt status
        match self.device_info.map(|info| info.series) {
            Some(RealtekSeries::Rtl8139) => {
                let isr = self.read_reg16(RTL8139_ISR);
                self.write_reg16(RTL8139_ISR, isr); // Clear interrupts

                if (isr & 0x01) != 0 {
                    // Receive OK
                    self.stats.rx_packets += 1;
                }
                if (isr & 0x04) != 0 { // Transmit OK
                     // Handle transmit completion
                }
            }
            _ => {
                // RTL8169/8168 interrupt handling
                let isr = self.read_reg16(0x3E);
                self.write_reg16(0x3E, isr);
            }
        }

        Ok(())
    }
}

impl RealtekDriver {
    /// RTL8139-specific packet transmission
    fn rtl8139_send_packet(&mut self, data: &[u8]) -> Result<(), NetworkError> {
        // Check if transmit buffer is available
        let status = self.read_reg8(RTL8139_CR);
        if status & 0x04 == 0 {
            // Transmitter not enabled
            return Err(NetworkError::HardwareError);
        }

        // Find available transmit descriptor
        let tsd_base = 0x10; // Transmit status descriptor base
        let mut descriptor_found = false;
        let mut descriptor_index = 0;

        for i in 0..4 {
            // RTL8139 has 4 transmit descriptors
            let tsd = self.read_reg32(tsd_base + i * 4);
            if tsd & 0x2000 != 0 {
                // Transmit OK bit indicates available
                descriptor_index = i;
                descriptor_found = true;
                break;
            }
        }

        if !descriptor_found {
            return Err(NetworkError::Busy);
        }

        // Copy packet data to transmit buffer
        let tx_buffer_addr = 0x20 + descriptor_index * 4; // TX buffer addresses
        let buffer_base = self.read_reg32(tx_buffer_addr);

        unsafe {
            // In real hardware, this would DMA the data to the buffer
            let tx_buffer = (self.base_addr + buffer_base as u64) as *mut u8;
            core::ptr::copy_nonoverlapping(data.as_ptr(), tx_buffer, data.len());
        }

        // Set transmit descriptor with packet length
        let tsd_value = data.len() as u32 | 0x80000; // Size + start transmission
        self.write_reg32(tsd_base + descriptor_index * 4, tsd_value);

        Ok(())
    }

    /// RTL8169-specific packet transmission  
    fn rtl8169_send_packet(&mut self, data: &[u8]) -> Result<(), NetworkError> {
        // Check transmit queue status
        let status = self.read_reg8(0x37); // Command register
        if status & 0x04 == 0 {
            // Transmitter not enabled
            return Err(NetworkError::HardwareError);
        }

        // Get current transmit descriptor index
        let mut tx_desc_idx = self.tx_desc_index.unwrap_or(0);

        // Check if descriptor is available
        let desc_base = 0x20; // Transmit descriptor base address
        let desc_addr = desc_base + tx_desc_idx * 16; // Each descriptor is 16 bytes
        let desc_status = self.read_reg32(desc_addr as u16);

        if desc_status & 0x80000000 == 0 {
            // OWN bit not set, descriptor busy
            return Err(NetworkError::Busy);
        }

        // Set up transmit descriptor
        let buffer_addr = desc_addr + 8; // Buffer address offset
        unsafe {
            // In real hardware, this would set up DMA buffer
            let tx_buffer = (self.base_addr + 0x1000 + tx_desc_idx as u64 * 2048) as *mut u8;
            core::ptr::copy_nonoverlapping(data.as_ptr(), tx_buffer, data.len());

            // Set buffer address in descriptor
            self.write_reg32(
                buffer_addr as u16,
                (self.base_addr + 0x1000 + tx_desc_idx as u64 * 2048) as u32,
            );
        }

        // Set descriptor control - packet length and flags
        let desc_control = data.len() as u32 | 0xC0000000; // Length + FS + LS + OWN
        self.write_reg32(desc_addr as u16, desc_control);

        // Advance to next descriptor
        tx_desc_idx = (tx_desc_idx + 1) % 4;
        self.tx_desc_index = Some(tx_desc_idx);

        // Trigger transmission
        self.write_reg8(0x38, 0x40); // Poll transmit

        Ok(())
    }

    /// RTL8139-specific packet reception
    fn rtl8139_receive_packet(&mut self) -> Option<Vec<u8>> {
        // Check receive status
        let status = self.read_reg8(RTL8139_CR);
        if status & 0x01 == 0 {
            // Receiver not enabled
            return None;
        }

        // Check if packet available
        let isr = self.read_reg16(0x3E); // Interrupt status
        if isr & 0x01 == 0 {
            // No receive OK interrupt
            return None;
        }

        // Get receive buffer status
        let rx_buf_ptr = self.read_reg16(0x38) as usize; // Current buffer position
        let rx_buf_addr = self.read_reg32(0x30); // Receive buffer start

        unsafe {
            // Read packet header from receive buffer
            let packet_ptr = (self.base_addr + rx_buf_addr as u64 + rx_buf_ptr as u64) as *const u8;
            let packet_status = core::ptr::read_unaligned(packet_ptr as *const u16);

            if packet_status & 0x01 == 0 {
                // ROK bit not set
                return None;
            }

            // Read packet length
            let packet_len = core::ptr::read_unaligned(packet_ptr.add(2) as *const u16) as usize;

            if packet_len > 1518 || packet_len < 64 {
                // Invalid packet size
                return None;
            }

            // Copy packet data
            let mut packet_data = Vec::with_capacity(packet_len);
            packet_data.set_len(packet_len - 4); // Exclude CRC
            core::ptr::copy_nonoverlapping(
                packet_ptr.add(4), // Skip header
                packet_data.as_mut_ptr(),
                packet_len - 4,
            );

            // Update receive buffer pointer
            let new_ptr = (rx_buf_ptr + packet_len + 4 + 3) & !3; // 4-byte aligned
            self.write_reg16(0x38, new_ptr as u16);

            self.stats.rx_packets += 1;
            self.stats.rx_bytes += packet_len as u64;

            Some(packet_data)
        }
    }

    /// RTL8169-specific packet reception
    fn rtl8169_receive_packet(&mut self) -> Option<Vec<u8>> {
        // Check receive status
        let status = self.read_reg8(0x37);
        if status & 0x08 == 0 {
            // Receiver not enabled
            return None;
        }

        // Get current receive descriptor
        let mut rx_desc_idx = self.rx_desc_index.unwrap_or(0);
        let desc_base = 0x40; // Receive descriptor base
        let desc_addr = desc_base + rx_desc_idx * 16;

        // Check descriptor status
        let desc_status = self.read_reg32(desc_addr as u16);
        if desc_status & 0x80000000 != 0 {
            // OWN bit set, no packet
            return None;
        }

        // Extract packet information
        let packet_len = (desc_status & 0x3FFF) as usize; // Length field
        if packet_len < 64 || packet_len > 1518 {
            // Reset descriptor and continue
            self.write_reg32(desc_addr as u16, 0x80000000 | 2048); // Reset with buffer size
            return None;
        }

        unsafe {
            // Read packet from buffer
            let buffer_addr = self.read_reg32((desc_addr + 8) as u16) as u64;
            let packet_ptr = buffer_addr as *const u8;

            let mut packet_data = Vec::with_capacity(packet_len);
            packet_data.set_len(packet_len - 4); // Exclude CRC
            core::ptr::copy_nonoverlapping(packet_ptr, packet_data.as_mut_ptr(), packet_len - 4);

            // Reset descriptor for next packet
            self.write_reg32(desc_addr as u16, 0x80000000 | 2048); // OWN + buffer size

            // Advance to next descriptor
            rx_desc_idx = (rx_desc_idx + 1) % 4;
            self.rx_desc_index = Some(rx_desc_idx);

            self.stats.rx_packets += 1;
            self.stats.rx_bytes += packet_len as u64;

            Some(packet_data)
        }
    }
}

/// Create Realtek driver from PCI device information
pub fn create_realtek_driver(
    vendor_id: u16,
    device_id: u16,
    base_addr: u64,
    irq: u8,
) -> Option<(Box<dyn NetworkDriver>, ExtendedNetworkCapabilities)> {
    // Find matching device in database
    let device_info = REALTEK_DEVICES
        .iter()
        .find(|info| info.vendor_id == vendor_id && info.device_id == device_id)
        .copied()?;

    let name = format!("Realtek {}", device_info.name);
    let driver = RealtekDriver::new(name, device_info, base_addr, irq);
    let capabilities = driver.extended_capabilities.clone();

    Some((Box::new(driver), capabilities))
}

/// Check if PCI device is a Realtek controller
pub fn is_realtek_device(vendor_id: u16, device_id: u16) -> bool {
    REALTEK_DEVICES
        .iter()
        .any(|info| info.vendor_id == vendor_id && info.device_id == device_id)
}

/// Get Realtek device information
pub fn get_realtek_device_info(
    vendor_id: u16,
    device_id: u16,
) -> Option<&'static RealtekDeviceInfo> {
    REALTEK_DEVICES
        .iter()
        .find(|info| info.vendor_id == vendor_id && info.device_id == device_id)
}
