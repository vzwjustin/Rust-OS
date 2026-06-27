//! # Network Drivers Module
//!
//! Comprehensive network driver support for Ethernet, WiFi, and other network interfaces.
//! Includes Intel E1000/E1000E, Realtek RTL8139/RTL8169, Broadcom NetXtreme,
//! and Qualcomm Atheros wireless drivers.

pub mod atheros_wifi;
pub mod broadcom;
pub mod intel_e1000;
pub mod realtek;

use crate::net::{MacAddress, NetworkError};
use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

// Re-export types from net::device for compatibility
pub use crate::net::device::{DeviceCapabilities, DeviceType, NetworkDevice as NetworkDeviceTrait};

/// Device state enumeration for network drivers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    /// Device is not initialized
    Uninitialized,
    /// Device is initializing
    Initializing,
    /// Device is ready but stopped
    Stopped,
    /// Device is running
    Running,
    /// Device is in error state
    Error,
    /// Device is suspended
    Suspended,
}

/// Network driver trait for hardware-specific drivers
pub trait NetworkDriver: Send + Sync {
    /// Get driver name
    fn name(&self) -> &str;

    /// Get device type
    fn device_type(&self) -> DeviceType {
        DeviceType::Ethernet
    }

    /// Get current device state
    fn state(&self) -> DeviceState;

    /// Get device capabilities
    fn capabilities(&self) -> &DeviceCapabilities {
        // Default implementation - drivers should override
        static DEFAULT_CAPS: DeviceCapabilities = DeviceCapabilities {
            max_mtu: 1500,
            min_mtu: 68,
            hw_checksum: false,
            supports_checksum_offload: false,
            scatter_gather: false,
            tso: false,
            supports_tso: false,
            supports_lro: false,
            rss: false,
            vlan: false,
            supports_vlan: false,
            jumbo_frames: false,
            supports_jumbo_frames: false,
            multicast_filter: false,
            max_tx_queues: 1,
            max_rx_queues: 1,
        };
        &DEFAULT_CAPS
    }

    /// Initialize the driver
    fn init(&mut self) -> Result<(), NetworkError>;

    /// Start the driver (enable packet transmission/reception)
    fn start(&mut self) -> Result<(), NetworkError>;

    /// Stop the driver
    fn stop(&mut self) -> Result<(), NetworkError>;

    /// Reset the device
    fn reset(&mut self) -> Result<(), NetworkError> {
        self.stop()?;
        self.init()?;
        self.start()
    }

    /// Send a packet
    fn send_packet(&mut self, data: &[u8]) -> Result<(), NetworkError>;

    /// Receive a packet (returns None if no packet available)
    fn receive_packet(&mut self) -> Result<Option<Vec<u8>>, NetworkError>;

    /// Get MAC address
    fn get_mac_address(&self) -> MacAddress;

    /// Set MAC address
    fn set_mac_address(&mut self, _mac: MacAddress) -> Result<(), NetworkError> {
        Err(NetworkError::NotSupported)
    }

    /// Get link status (returns: link_up, speed_mbps, full_duplex)
    fn get_link_status(&self) -> (bool, u32, bool);

    /// Check if link is up
    fn is_link_up(&self) -> bool {
        let (link_up, _, _) = self.get_link_status();
        link_up
    }

    /// Get network statistics
    fn get_stats(&self) -> NetworkStats {
        // Default empty stats
        NetworkStats {
            rx_packets: 0,
            tx_packets: 0,
            rx_bytes: 0,
            tx_bytes: 0,
            rx_errors: 0,
            tx_errors: 0,
            rx_dropped: 0,
            tx_dropped: 0,
            packets_sent: 0,
            packets_received: 0,
            bytes_sent: 0,
            bytes_received: 0,
            send_errors: 0,
            receive_errors: 0,
            dropped_packets: 0,
        }
    }

    /// Set promiscuous mode
    fn set_promiscuous(&mut self, _enabled: bool) -> Result<(), NetworkError> {
        Err(NetworkError::NotSupported)
    }

    /// Add multicast address
    fn add_multicast(&mut self, _mac: MacAddress) -> Result<(), NetworkError> {
        Err(NetworkError::NotSupported)
    }

    /// Remove multicast address
    fn remove_multicast(&mut self, _mac: MacAddress) -> Result<(), NetworkError> {
        Err(NetworkError::NotSupported)
    }

    /// Set MTU (Maximum Transmission Unit)
    fn set_mtu(&mut self, _mtu: u16) -> Result<(), NetworkError> {
        Err(NetworkError::NotSupported)
    }

    /// Get current MTU
    fn get_mtu(&self) -> u16 {
        1500 // Default MTU
    }

    /// Handle interrupt from device
    fn handle_interrupt(&mut self) -> Result<(), NetworkError> {
        Ok(())
    }

    /// Set power state
    fn set_power_state(&mut self, _state: PowerState) -> Result<(), NetworkError> {
        Err(NetworkError::NotSupported)
    }

    /// Configure Wake-on-LAN
    fn configure_wol(&mut self, _config: WakeOnLanConfig) -> Result<(), NetworkError> {
        Err(NetworkError::NotSupported)
    }
}

/// Network statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct NetworkStats {
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_errors: u64,
    pub tx_errors: u64,
    pub rx_dropped: u64,
    pub tx_dropped: u64,
    pub packets_sent: u64,
    pub packets_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub send_errors: u64,
    pub receive_errors: u64,
    pub dropped_packets: u64,
}

/// Dummy Ethernet driver for testing only — not used in production.
/// Real NIC drivers are loaded via PCI scanning in `init_network_drivers()`.
#[cfg(test)]
pub struct DummyEthernetDriver {
    name: String,
    mac: MacAddress,
    state: DeviceState,
}

#[cfg(test)]
impl DummyEthernetDriver {
    pub fn new(name: String, mac: MacAddress) -> Self {
        Self {
            name,
            mac,
            state: DeviceState::Uninitialized,
        }
    }
}

#[cfg(test)]
impl NetworkDriver for DummyEthernetDriver {
    fn name(&self) -> &str {
        &self.name
    }

    fn init(&mut self) -> Result<(), NetworkError> {
        self.state = DeviceState::Stopped;
        Ok(())
    }

    fn start(&mut self) -> Result<(), NetworkError> {
        self.state = DeviceState::Running;
        Ok(())
    }

    fn stop(&mut self) -> Result<(), NetworkError> {
        self.state = DeviceState::Stopped;
        Ok(())
    }

    fn send_packet(&mut self, _data: &[u8]) -> Result<(), NetworkError> {
        if self.state != DeviceState::Running {
            return Err(NetworkError::NotConnected);
        }
        Ok(())
    }

    fn receive_packet(&mut self) -> Result<Option<Vec<u8>>, NetworkError> {
        if self.state != DeviceState::Running {
            return Err(NetworkError::NotConnected);
        }
        Ok(None)
    }

    fn get_mac_address(&self) -> MacAddress {
        self.mac
    }

    fn state(&self) -> DeviceState {
        self.state
    }

    fn get_link_status(&self) -> (bool, u32, bool) {
        let link_up = self.state == DeviceState::Running;
        (link_up, 1000, true)
    }
}

/// Network driver types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkDriverType {
    /// Intel Gigabit Ethernet
    IntelGigabit,
    /// Realtek Fast/Gigabit Ethernet
    RealtekEthernet,
    /// Broadcom NetXtreme Gigabit Ethernet
    BroadcomNetXtreme,
    /// Qualcomm Atheros WiFi
    AtherosWifi,
    /// Generic Ethernet
    GenericEthernet,
    /// Generic WiFi
    GenericWifi,
}

/// Network device capabilities extended
#[derive(Debug, Clone)]
pub struct ExtendedNetworkCapabilities {
    /// Base capabilities
    pub base: DeviceCapabilities,
    /// Wake-on-LAN support
    pub wake_on_lan: bool,
    /// Energy Efficient Ethernet support
    pub energy_efficient: bool,
    /// PXE boot support
    pub pxe_boot: bool,
    /// SRIOV support
    pub sriov: bool,
    /// Maximum bandwidth in Mbps
    pub max_bandwidth_mbps: u32,
    /// Supported WiFi standards (if applicable)
    pub wifi_standards: Vec<String>,
    /// Antenna count (for WiFi)
    pub antenna_count: u8,
}

impl Default for ExtendedNetworkCapabilities {
    fn default() -> Self {
        Self {
            base: DeviceCapabilities::default(),
            wake_on_lan: false,
            energy_efficient: false,
            pxe_boot: false,
            sriov: false,
            max_bandwidth_mbps: 1000,
            wifi_standards: Vec::new(),
            antenna_count: 0,
        }
    }
}

/// Network driver manager for hardware drivers
pub struct NetworkDriverManager {
    /// Registered drivers
    drivers: BTreeMap<u32, Box<dyn NetworkDriver>>,
    /// Driver capabilities
    capabilities: BTreeMap<u32, ExtendedNetworkCapabilities>,
    /// Next driver ID
    next_id: u32,
}

impl NetworkDriverManager {
    pub fn new() -> Self {
        Self {
            drivers: BTreeMap::new(),
            capabilities: BTreeMap::new(),
            next_id: 1,
        }
    }

    /// Register a network driver
    pub fn register_driver(
        &mut self,
        driver: Box<dyn NetworkDriver>,
        capabilities: ExtendedNetworkCapabilities,
    ) -> u32 {
        let id = self.next_id;
        self.next_id += 1;

        self.drivers.insert(id, driver);
        self.capabilities.insert(id, capabilities);

        id
    }

    /// Get driver by ID
    pub fn get_driver(&self, id: u32) -> Option<&dyn NetworkDriver> {
        self.drivers.get(&id).map(|d| d.as_ref())
    }

    /// Get mutable driver by ID
    pub fn get_driver_mut(&mut self, id: u32) -> Option<&mut (dyn NetworkDriver + '_)> {
        match self.drivers.get_mut(&id) {
            Some(driver) => Some(&mut **driver),
            None => None,
        }
    }

    /// Get driver capabilities
    pub fn get_capabilities(&self, id: u32) -> Option<&ExtendedNetworkCapabilities> {
        self.capabilities.get(&id)
    }

    /// List all drivers
    pub fn list_drivers(&self) -> Vec<(u32, &str, NetworkDriverType)> {
        let mut drivers = Vec::new();
        for (&id, driver) in &self.drivers {
            let driver_type = self.get_driver_type_from_name(driver.name());
            drivers.push((id, driver.name(), driver_type));
        }
        drivers
    }

    /// Initialize all drivers
    pub fn init_all_drivers(&mut self) -> Result<(), NetworkError> {
        for driver in self.drivers.values_mut() {
            driver.init()?;
        }
        Ok(())
    }

    /// Start all drivers
    pub fn start_all_drivers(&mut self) -> Result<(), NetworkError> {
        for driver in self.drivers.values_mut() {
            driver.start()?;
        }
        Ok(())
    }

    /// Get driver count by type
    pub fn get_driver_count_by_type(&self, driver_type: NetworkDriverType) -> usize {
        self.drivers
            .values()
            .filter(|driver| self.get_driver_type_from_name(driver.name()) == driver_type)
            .count()
    }

    /// Get driver type from name
    fn get_driver_type_from_name(&self, name: &str) -> NetworkDriverType {
        let name_lower = name.to_lowercase();
        if name_lower.contains("intel") || name_lower.contains("e1000") {
            NetworkDriverType::IntelGigabit
        } else if name_lower.contains("realtek") || name_lower.contains("rtl") {
            NetworkDriverType::RealtekEthernet
        } else if name_lower.contains("broadcom") || name_lower.contains("netxtreme") {
            NetworkDriverType::BroadcomNetXtreme
        } else if name_lower.contains("atheros") || name_lower.contains("wifi") {
            NetworkDriverType::AtherosWifi
        } else if name_lower.contains("ethernet") {
            NetworkDriverType::GenericEthernet
        } else {
            NetworkDriverType::GenericWifi
        }
    }
}

/// Enhanced network statistics
#[derive(Debug, Clone, Default)]
pub struct EnhancedNetworkStats {
    /// Packets transmitted
    pub tx_packets: u64,
    /// Packets received
    pub rx_packets: u64,
    /// Bytes transmitted
    pub tx_bytes: u64,
    /// Bytes received
    pub rx_bytes: u64,
    /// Transmission errors
    pub tx_errors: u64,
    /// Reception errors
    pub rx_errors: u64,
    /// Dropped packets (TX)
    pub tx_dropped: u64,
    /// Dropped packets (RX)
    pub rx_dropped: u64,
    /// Multicast packets
    pub multicast: u64,
    /// Collisions
    pub collisions: u64,
    /// Length errors
    pub rx_length_errors: u64,
    /// Frame errors
    pub rx_frame_errors: u64,
    /// CRC errors
    pub rx_crc_errors: u64,
    /// FIFO errors
    pub rx_fifo_errors: u64,
    /// Missed packets
    pub rx_missed_errors: u64,
    /// Aborted transmissions
    pub tx_aborted_errors: u64,
    /// Carrier errors
    pub tx_carrier_errors: u64,
    /// FIFO errors (TX)
    pub tx_fifo_errors: u64,
    /// Link up/down events
    pub link_changes: u64,
    /// Current link speed in Mbps
    pub link_speed_mbps: u32,
    /// Link duplex (true = full, false = half)
    pub link_duplex_full: bool,
}

/// Power management states for network devices
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerState {
    /// Device fully operational
    D0,
    /// Device in low power state
    D1,
    /// Device in lower power state
    D2,
    /// Device in lowest power state
    D3Hot,
    /// Device completely powered off
    D3Cold,
}

/// Wake-on-LAN configuration
#[derive(Debug, Clone)]
pub struct WakeOnLanConfig {
    /// Enable WoL
    pub enabled: bool,
    /// Wake on magic packet
    pub magic_packet: bool,
    /// Wake on pattern match
    pub pattern_match: bool,
    /// Wake on link change
    pub link_change: bool,
    /// Secure on password
    pub secure_on: bool,
    /// Password for secure WoL
    pub password: Option<[u8; 6]>,
}

impl Default for WakeOnLanConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            magic_packet: false,
            pattern_match: false,
            link_change: false,
            secure_on: false,
            password: None,
        }
    }
}

/// Create network drivers based on PCI device information
pub fn create_network_driver_from_pci(
    vendor_id: u16,
    device_id: u16,
    base_addr: u64,
    irq: u8,
) -> Option<(Box<dyn NetworkDriver>, ExtendedNetworkCapabilities)> {
    // Try Intel E1000 series
    if let Some((driver, caps)) =
        intel_e1000::create_intel_e1000_driver(vendor_id, device_id, base_addr, irq)
    {
        return Some((driver, caps));
    }

    // Try Realtek series
    if let Some((driver, caps)) =
        realtek::create_realtek_driver(vendor_id, device_id, base_addr, irq)
    {
        return Some((driver, caps));
    }

    // Try Broadcom NetXtreme series
    if let Some((driver, caps)) =
        broadcom::create_broadcom_driver(vendor_id, device_id, base_addr, irq)
    {
        return Some((driver, caps));
    }

    // Atheros WiFi driver is a stub — not loaded in production

    None
}

/// Initialize network driver system by scanning PCI for real NICs
pub fn init_network_drivers() -> Result<NetworkDriverManager, NetworkError> {
    let mut manager = NetworkDriverManager::new();

    use crate::pci::{get_devices_by_class, PciClass};

    let net_devices = get_devices_by_class(PciClass::Network);

    for dev in net_devices.iter() {
        let bar0 = dev.bars[0];
        if bar0 == 0 {
            continue;
        }

        let base_addr = if bar0 & 0x1 == 0 {
            (bar0 & 0xFFFF_FFF0) as u64
        } else {
            continue;
        };

        let irq = 0;

        crate::serial_println!(
            "net: PCI {:02x}:{:02x}.{} vendor=0x{:04X} device=0x{:04X} bar0=0x{:08X} mmio=0x{:X}",
            dev.bus,
            dev.device,
            dev.function,
            dev.vendor_id,
            dev.device_id,
            bar0,
            base_addr
        );

        let mapped = base_addr;

        if let Some((driver, caps)) =
            create_network_driver_from_pci(dev.vendor_id, dev.device_id, mapped, irq)
        {
            crate::serial_println!(
                "net: loaded driver '{}' for {:04X}:{:04X}",
                driver.name(),
                dev.vendor_id,
                dev.device_id
            );
            manager.register_driver(driver, caps);
        } else {
            crate::serial_println!(
                "net: no driver for vendor=0x{:04X} device=0x{:04X}",
                dev.vendor_id,
                dev.device_id
            );
        }
    }

    if manager.list_drivers().is_empty() {
        crate::serial_println!("net: no NIC drivers loaded");
    }

    Ok(manager)
}

/// Network driver utilities
pub mod utils {
    use super::*;

    /// Convert link speed to human readable format
    pub fn format_link_speed(speed_mbps: u32) -> String {
        match speed_mbps {
            0 => "Unknown".to_string(),
            10 => "10 Mbps".to_string(),
            100 => "100 Mbps".to_string(),
            1000 => "1 Gbps".to_string(),
            10000 => "10 Gbps".to_string(),
            25000 => "25 Gbps".to_string(),
            40000 => "40 Gbps".to_string(),
            100000 => "100 Gbps".to_string(),
            _ => format!("{} Mbps", speed_mbps),
        }
    }

    /// Validate MAC address
    pub fn is_valid_mac_address(mac: &MacAddress) -> bool {
        // Check for invalid addresses
        let bytes = mac;

        // All zeros
        if bytes.iter().all(|&b| b == 0) {
            return false;
        }

        // All ones (broadcast)
        if bytes.iter().all(|&b| b == 0xFF) {
            return false;
        }

        // Multicast bit check (bit 0 of first byte should be 0 for unicast)
        if (bytes[0] & 1) != 0 {
            return false;
        }

        true
    }

    /// Generate random MAC address with specific vendor prefix
    pub fn generate_mac_with_vendor(vendor_prefix: [u8; 3]) -> MacAddress {
        use core::sync::atomic::{AtomicU32, Ordering};
        static MAC_COUNTER: AtomicU32 = AtomicU32::new(0);

        let mut mac = [0u8; 6];
        mac[0] = vendor_prefix[0] & 0xFE; // Ensure unicast
        mac[1] = vendor_prefix[1];
        mac[2] = vendor_prefix[2];

        // Generate unique lower 3 bytes from a monotonic counter
        let counter = MAC_COUNTER.fetch_add(1, Ordering::Relaxed);
        mac[3] = ((counter >> 16) & 0xFF) as u8;
        mac[4] = ((counter >> 8) & 0xFF) as u8;
        mac[5] = (counter & 0xFF) as u8;

        mac
    }

    /// Common vendor prefixes
    pub const INTEL_OUI: [u8; 3] = [0x00, 0x1B, 0x21];
    pub const REALTEK_OUI: [u8; 3] = [0x00, 0x0E, 0x2E];
    pub const BROADCOM_OUI: [u8; 3] = [0x00, 0x10, 0x18];
    pub const ATHEROS_OUI: [u8; 3] = [0x00, 0x03, 0x7F];
}

/// Global network driver manager
static mut NETWORK_DRIVER_MANAGER: Option<NetworkDriverManager> = None;

/// Initialize global network driver manager
pub fn init_global_network_drivers() -> Result<(), NetworkError> {
    unsafe {
        NETWORK_DRIVER_MANAGER = Some(init_network_drivers()?);
    }
    Ok(())
}

/// Get global network driver manager
pub fn get_network_driver_manager() -> Option<&'static mut NetworkDriverManager> {
    unsafe { NETWORK_DRIVER_MANAGER.as_mut() }
}

/// Network driver detection and loading
pub fn detect_and_load_network_drivers() -> Result<Vec<String>, NetworkError> {
    let mut loaded_drivers = Vec::new();

    use crate::pci::{get_devices_by_class, PciClass};

    let network_devices = get_devices_by_class(PciClass::Network);

    for device in network_devices.iter() {
        let bar0 = device.bars[0];
        if bar0 == 0 || (bar0 & 0x1) != 0 {
            continue;
        }

        let base_addr = (bar0 & 0xFFFF_FFF0) as u64;
        let irq = 0u8;

        if let Some((mut driver, _caps)) =
            create_network_driver_from_pci(device.vendor_id, device.device_id, base_addr, irq)
        {
            let name = driver.name().to_string();
            crate::serial_println!(
                "net: initializing '{}' for {:04X}:{:04X} at 0x{:X}",
                name,
                device.vendor_id,
                device.device_id,
                base_addr
            );

            match driver.init() {
                Ok(()) => {
                    crate::serial_println!("net: '{}' initialized, starting...", name);
                    match driver.start() {
                        Ok(()) => {
                            let (link_up, speed, full_duplex) = driver.get_link_status();
                            crate::serial_println!(
                                "net: '{}' started, link={} speed={}Mbps duplex={}",
                                name,
                                link_up,
                                speed,
                                if full_duplex { "full" } else { "half" }
                            );
                        }
                        Err(e) => {
                            crate::serial_println!("net: '{}' start failed: {:?}", name, e);
                        }
                    }
                }
                Err(e) => {
                    crate::serial_println!("net: '{}' init failed: {:?}", name, e);
                }
            }

            loaded_drivers.push(name);
        } else {
            let vendor_name = match device.vendor_id {
                0x8086 => "Intel",
                0x10EC => "Realtek",
                0x14E4 => "Broadcom",
                0x168C => "Qualcomm Atheros",
                _ => "Unknown",
            };
            loaded_drivers.push(alloc::format!(
                "{} Network Controller ({}:{:04X}:{:04X}) — no driver",
                vendor_name,
                device.bus,
                device.vendor_id,
                device.device_id
            ));
        }
    }

    if loaded_drivers.is_empty() {
        crate::serial_println!("net: no network devices detected on PCI bus");
    }

    Ok(loaded_drivers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_network_driver_manager() {
        let mut manager = NetworkDriverManager::new();

        let dummy_driver = DummyEthernetDriver::new(
            "Test Driver".to_string(),
            [0x00, 0x11, 0x22, 0x33, 0x44, 0x55],
        );

        let caps = ExtendedNetworkCapabilities::default();
        let id = manager.register_driver(Box::new(dummy_driver), caps);

        assert_eq!(id, 1);
        assert!(manager.get_driver(id).is_some());
        assert!(manager.get_capabilities(id).is_some());
    }

    #[test_case]
    fn test_mac_address_validation() {
        // Valid MAC
        let valid_mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
        assert!(utils::is_valid_mac_address(&valid_mac));

        // Invalid MAC (all zeros)
        let invalid_mac = [0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(!utils::is_valid_mac_address(&invalid_mac));

        // Invalid MAC (multicast bit set)
        let multicast_mac = [0x01, 0x11, 0x22, 0x33, 0x44, 0x55];
        assert!(!utils::is_valid_mac_address(&multicast_mac));
    }

    #[test_case]
    fn test_link_speed_formatting() {
        assert_eq!(utils::format_link_speed(0), "Unknown");
        assert_eq!(utils::format_link_speed(10), "10 Mbps");
        assert_eq!(utils::format_link_speed(100), "100 Mbps");
        assert_eq!(utils::format_link_speed(1000), "1 Gbps");
        assert_eq!(utils::format_link_speed(10000), "10 Gbps");
    }
}
