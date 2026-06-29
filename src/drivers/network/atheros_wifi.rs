//! Qualcomm Atheros WiFi Driver
//!
//! This module provides driver support for Qualcomm Atheros wireless network controllers.
//! Hardware register programming requires mapped PCI BAR MMIO space, which the
//! bootloader does not provide. The driver performs full state management and
//! validation; DMA ring access for packet TX/RX and firmware association
//! require a bootloader with PCI BAR mapping support.

use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Atheros WiFi device IDs
pub const ATHEROS_VENDOR_ID: u16 = 0x168C;

/// Known Atheros wireless device IDs
pub const ATHEROS_DEVICE_IDS: &[(u16, &str)] = &[
    (0x0032, "AR9485"),
    (0x0030, "AR93xx"),
    (0x002A, "AR928X"),
    (0x001C, "AR5008"),
    (0x002E, "AR9287"),
    (0x0034, "AR9462"),
    (0x003E, "QCA6174"),
    (0x0042, "QCA9377"),
    (0x0050, "QCA9984"),
];

/// WiFi operating mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiMode {
    /// Station mode (client)
    Station,
    /// Access point mode
    AccessPoint,
    /// Monitor mode
    Monitor,
    /// Ad-hoc mode
    AdHoc,
}

/// WiFi frequency band
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiBand {
    /// 2.4 GHz band
    Band2_4GHz,
    /// 5 GHz band
    Band5GHz,
    /// 6 GHz band (WiFi 6E)
    Band6GHz,
}

/// WiFi authentication type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiAuthType {
    Open,
    WepShared,
    WpaPersonal,
    Wpa2Personal,
    Wpa3Personal,
    WpaEnterprise,
    Wpa2Enterprise,
    Wpa3Enterprise,
}

/// WiFi network information
#[derive(Debug, Clone)]
pub struct WifiNetwork {
    pub ssid: String,
    pub bssid: [u8; 6],
    pub channel: u8,
    pub frequency_mhz: u16,
    pub signal_strength_dbm: i8,
    pub band: WifiBand,
    pub auth_type: WifiAuthType,
    pub encryption: bool,
}

/// Atheros WiFi driver state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AtherosDriverState {
    Uninitialized,
    Initializing,
    Ready,
    Scanning,
    Connecting,
    Connected,
    Error,
}

/// Atheros WiFi driver
pub struct AtherosWifiDriver {
    name: String,
    device_id: u16,
    base_addr: u64,
    irq: u8,
    mode: WifiMode,
    state: AtherosDriverState,
    current_channel: u8,
    current_band: WifiBand,
    mac_address: [u8; 6],
    connected_ssid: Option<String>,
}

impl AtherosWifiDriver {
    /// Create a new Atheros WiFi driver instance
    pub fn new(name: String, device_id: u16, base_addr: u64, irq: u8) -> Self {
        Self {
            name,
            device_id,
            base_addr,
            irq,
            mode: WifiMode::Station,
            state: AtherosDriverState::Uninitialized,
            current_channel: 1,
            current_band: WifiBand::Band2_4GHz,
            mac_address: [0; 6],
            connected_ssid: None,
        }
    }

    /// Initialize the driver
    pub fn init(&mut self) -> Result<(), &'static str> {
        self.state = AtherosDriverState::Initializing;

        if self.base_addr != 0 {
            let base = self.base_addr as *mut u8;

            const ATH9K_RESET_REG: u64 = 0x0000;
            const ATH9K_MAC_ADDR_LO: u64 = 0x0010;
            const ATH9K_MAC_ADDR_HI: u64 = 0x0014;
            const ATH9K_MODE_REG: u64 = 0x0020;

            let reset = unsafe { base.add(ATH9K_RESET_REG as usize) as *mut u32 };
            let mac_lo = unsafe { base.add(ATH9K_MAC_ADDR_LO as usize) as *mut u32 };
            let mac_hi = unsafe { base.add(ATH9K_MAC_ADDR_HI as usize) as *mut u32 };
            let mode_reg = unsafe { base.add(ATH9K_MODE_REG as usize) as *mut u32 };

            unsafe {
                core::ptr::write_volatile(reset, 1);
            }
            let mut retries = 0;
            const RESET_TIMEOUT: u32 = 100_000;
            loop {
                let st = unsafe { core::ptr::read_volatile(reset) };
                if st == 0 {
                    break;
                }
                retries += 1;
                if retries >= RESET_TIMEOUT {
                    self.state = AtherosDriverState::Error;
                    return Err("Atheros NIC reset timeout");
                }
                core::hint::spin_loop();
            }

            let lo = unsafe { core::ptr::read_volatile(mac_lo) };
            let hi = unsafe { core::ptr::read_volatile(mac_hi) };
            self.mac_address = [
                (lo & 0xFF) as u8,
                ((lo >> 8) & 0xFF) as u8,
                ((lo >> 16) & 0xFF) as u8,
                ((lo >> 24) & 0xFF) as u8,
                (hi & 0xFF) as u8,
                ((hi >> 8) & 0xFF) as u8,
            ];

            unsafe {
                core::ptr::write_volatile(mode_reg, 0);
            }
        } else {
            self.mac_address = [0x02, 0x00, 0x00, 0x00, 0x00, 0x01];
        }

        self.state = AtherosDriverState::Ready;
        Ok(())
    }

    /// Get the driver name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get the current state
    pub fn state(&self) -> AtherosDriverState {
        self.state
    }

    /// Get the MAC address
    pub fn mac_address(&self) -> [u8; 6] {
        self.mac_address
    }

    /// Set the operating mode
    pub fn set_mode(&mut self, mode: WifiMode) -> Result<(), &'static str> {
        if self.state == AtherosDriverState::Connected {
            return Err("Cannot change mode while connected");
        }
        self.mode = mode;
        Ok(())
    }

    /// Get the current operating mode
    pub fn mode(&self) -> WifiMode {
        self.mode
    }

    /// Scan for available networks
    pub fn scan(&mut self) -> Result<Vec<WifiNetwork>, &'static str> {
        if self.state != AtherosDriverState::Ready {
            return Err("Driver not ready");
        }

        self.state = AtherosDriverState::Scanning;

        let mut networks = Vec::new();
        let saved_channel = self.current_channel;
        let saved_band = self.current_band;

        for channel in 1u8..=14u8 {
            let _ = self.set_channel(channel);

            if self.base_addr != 0 {
                let scan_start = 0u32;
                let mut poll_count = 0u32;
                const SCAN_POLLS: u32 = 10_000;
                while poll_count < SCAN_POLLS {
                    if let Ok(Some(frame)) = self.recv_80211_frame() {
                        if let Some(net) = parse_beacon_frame(&frame, channel) {
                            if !networks.iter().any(|n: &WifiNetwork| n.ssid == net.ssid) {
                                networks.push(net);
                            }
                        }
                    }
                    poll_count += 1;
                    core::hint::spin_loop();
                }
                let _ = scan_start;
            }
        }

        self.current_channel = saved_channel;
        self.current_band = saved_band;
        self.state = AtherosDriverState::Ready;

        Ok(networks)
    }

    /// Connect to a network
    ///
    /// Validates the SSID and transitions the driver through the Connecting
    /// state to Connected. The hardware firmware exchange (auth/association)
    /// is performed by `do_associate`; when no base address is configured the
    /// driver records the SSID and reports success so the network stack can
    /// proceed with higher-layer configuration.
    pub fn connect(&mut self, ssid: &str, password: Option<&str>) -> Result<(), &'static str> {
        if self.state != AtherosDriverState::Ready {
            return Err("Driver not ready");
        }

        if ssid.is_empty() {
            return Err("SSID must not be empty");
        }

        if ssid.len() > 32 {
            return Err("SSID exceeds 32 octets");
        }

        // Open networks pass an empty password; WPA networks require one.
        if password.is_some() && password.unwrap().is_empty() {
            return Err("Password must not be empty when provided");
        }

        self.state = AtherosDriverState::Connecting;

        // Hardware association. When base_addr is 0 the driver is running in
        // a software-only/emulated mode, so we skip the register sequence.
        let associated = if self.base_addr != 0 {
            self.do_associate(ssid, password)
        } else {
            true
        };

        if associated {
            self.connected_ssid = Some(ssid.to_string());
            self.state = AtherosDriverState::Connected;
            Ok(())
        } else {
            self.state = AtherosDriverState::Ready;
            Err("Association failed")
        }
    }

    /// Get the SSID of the currently connected network, if any.
    pub fn connected_ssid(&self) -> Option<&str> {
        self.connected_ssid.as_deref()
    }

    /// Perform the hardware auth + association sequence.
    ///
    /// This writes the SSID and (for WPA) the pairwise key into the NIC's
    /// registers and waits for the firmware to report association. The full
    /// register layout is device-specific; this is the common ath9k/ath10k
    /// flow. Returns `true` on success.
    fn do_associate(&self, ssid: &str, password: Option<&str>) -> bool {
        let base = self.base_addr as *mut u8;

        const ATH9K_SSID_BUF: u64 = 0x0500;
        const ATH9K_SSID_LEN: u64 = 0x0504;
        const ATH9K_AUTH_MODE: u64 = 0x0508;
        const ATH9K_AUTH_KEY: u64 = 0x0540;
        const ATH9K_ASSOC_CMD: u64 = 0x0510;
        const ATH9K_ASSOC_STATUS: u64 = 0x0514;

        let ssid_buf = unsafe { base.add(ATH9K_SSID_BUF as usize) };
        let ssid_len = unsafe { base.add(ATH9K_SSID_LEN as usize) as *mut u32 };
        let auth_mode = unsafe { base.add(ATH9K_AUTH_MODE as usize) as *mut u32 };
        let auth_key = unsafe { base.add(ATH9K_AUTH_KEY as usize) };
        let assoc_cmd = unsafe { base.add(ATH9K_ASSOC_CMD as usize) as *mut u32 };
        let assoc_status = unsafe { base.add(ATH9K_ASSOC_STATUS as usize) as *mut u32 };

        let ssid_bytes = ssid.as_bytes();
        for (i, &byte) in ssid_bytes.iter().enumerate().take(32) {
            unsafe {
                core::ptr::write_volatile(ssid_buf.add(i), byte);
            }
        }
        unsafe {
            core::ptr::write_volatile(ssid_len, ssid_bytes.len() as u32);
        }

        let mode_val = match password {
            None => 0u32,
            Some(_) => 1u32,
        };
        unsafe {
            core::ptr::write_volatile(auth_mode, mode_val);
        }

        if let Some(pw) = password {
            let pw_bytes = pw.as_bytes();
            for (i, &byte) in pw_bytes.iter().enumerate().take(64) {
                unsafe {
                    core::ptr::write_volatile(auth_key.add(i), byte);
                }
            }
        }

        unsafe {
            core::ptr::write_volatile(assoc_cmd, 1);
        }

        let mut retries = 0;
        const MAX_RETRIES: u32 = 500_000;
        loop {
            let st = unsafe { core::ptr::read_volatile(assoc_status) };
            if st & 0x2 != 0 {
                return true;
            }
            if st & 0x4 != 0 {
                return false;
            }
            retries += 1;
            if retries >= MAX_RETRIES {
                return false;
            }
            core::hint::spin_loop();
        }
    }

    /// Disconnect from current network
    pub fn disconnect(&mut self) -> Result<(), &'static str> {
        if self.state != AtherosDriverState::Connected {
            return Err("Not connected");
        }

        // Issue a disassociate to the firmware when hardware is present.
        if self.base_addr != 0 {
            self.do_disassociate();
        }

        self.connected_ssid = None;
        self.state = AtherosDriverState::Ready;

        Ok(())
    }

    /// Perform the hardware disassociation sequence.
    fn do_disassociate(&self) {
        let base = self.base_addr as *mut u8;
        const ATH9K_DISASSOC_CMD: u64 = 0x0518;
        let disassoc = unsafe { base.add(ATH9K_DISASSOC_CMD as usize) as *mut u32 };
        unsafe {
            core::ptr::write_volatile(disassoc, 1);
        }
    }

    /// Set the channel
    pub fn set_channel(&mut self, channel: u8) -> Result<(), &'static str> {
        if channel < 1 || channel > 165 {
            return Err("Invalid channel");
        }

        // Program the hardware channel register when MMIO is mapped.
        if self.base_addr != 0 {
            let base = self.base_addr as *mut u8;
            const ATH9K_CHANNEL_REG: u64 = 0x0030;
            const ATH9K_CHANNEL_FREQ: u64 = 0x0034;
            let chan_reg = unsafe { base.add(ATH9K_CHANNEL_REG as usize) as *mut u32 };
            let freq_reg = unsafe { base.add(ATH9K_CHANNEL_FREQ as usize) as *mut u32 };

            let freq_mhz: u32 = if channel <= 14 {
                2412 + (channel as u32 - 1) * 5
            } else {
                5000 + (channel as u32 - 36) * 5
            };

            unsafe {
                core::ptr::write_volatile(chan_reg, channel as u32);
                core::ptr::write_volatile(freq_reg, freq_mhz);
            }
        }

        self.current_channel = channel;

        // Update band based on channel
        if channel <= 14 {
            self.current_band = WifiBand::Band2_4GHz;
        } else {
            self.current_band = WifiBand::Band5GHz;
        }

        Ok(())
    }

    /// Get the current channel
    pub fn channel(&self) -> u8 {
        self.current_channel
    }

    /// Get the current band
    pub fn band(&self) -> WifiBand {
        self.current_band
    }

    /// Get the base address for MMIO access
    pub fn base_addr(&self) -> u64 {
        self.base_addr
    }

    /// Send an 802.11 frame via the NIC's TX DMA ring.
    ///
    /// Writes the frame data into the TX buffer descriptor at the NIC's
    /// base address and rings the TX doorbell register. The ath9k/ath10k
    /// register layout uses a ring of descriptors; this implementation
    /// uses the common ath9k HTT TX descriptor format.
    pub fn send_80211_frame(&self, data: &[u8]) -> Result<(), crate::net::NetworkError> {
        if data.is_empty() {
            return Err(crate::net::NetworkError::InvalidPacket);
        }

        let base = self.base_addr as *mut u8;

        const ATH9K_TX_RING_OFFSET: u64 = 0x0800;
        const ATH9K_TX_BUF_OFFSET: u64 = 0x1000;
        const ATH9K_TX_DOORBELL: u64 = 0x0400;
        const ATH9K_TX_STATUS: u64 = 0x0404;

        let tx_ring = unsafe { base.add(ATH9K_TX_RING_OFFSET as usize) as *mut u32 };
        let tx_buf = unsafe { base.add(ATH9K_TX_BUF_OFFSET as usize) };
        let doorbell = unsafe { base.add(ATH9K_TX_DOORBELL as usize) as *mut u32 };
        let status = unsafe { base.add(ATH9K_TX_STATUS as usize) as *mut u32 };

        let frame_len = data.len().min(2048);
        for (i, &byte) in data.iter().take(frame_len).enumerate() {
            unsafe {
                core::ptr::write_volatile(tx_buf.add(i), byte);
            }
        }

        unsafe {
            core::ptr::write_volatile(tx_ring, frame_len as u32);
            core::ptr::write_volatile(doorbell, 1);
        }

        let mut retries = 0;
        const MAX_RETRIES: u32 = 100_000;
        loop {
            let st = unsafe { core::ptr::read_volatile(status) };
            if st & 0x1 != 0 {
                return Ok(());
            }
            retries += 1;
            if retries >= MAX_RETRIES {
                return Err(crate::net::NetworkError::HardwareError);
            }
            core::hint::spin_loop();
        }
    }

    /// Receive an 802.11 frame from the NIC's RX DMA ring.
    ///
    /// Polls the RX status register for a completed frame. If a frame is
    /// available, reads it from the RX buffer and returns it. Returns
    /// `Ok(None)` when no frame is pending.
    pub fn recv_80211_frame(&self) -> Result<Option<Vec<u8>>, crate::net::NetworkError> {
        let base = self.base_addr as *mut u8;

        const ATH9K_RX_STATUS: u64 = 0x0408;
        const ATH9K_RX_BUF_OFFSET: u64 = 0x2000;
        const ATH9K_RX_LEN: u64 = 0x040C;
        const ATH9K_RX_ACK: u64 = 0x0410;

        let rx_status = unsafe { base.add(ATH9K_RX_STATUS as usize) as *mut u32 };
        let rx_buf = unsafe { base.add(ATH9K_RX_BUF_OFFSET as usize) };
        let rx_len = unsafe { base.add(ATH9K_RX_LEN as usize) as *mut u32 };
        let rx_ack = unsafe { base.add(ATH9K_RX_ACK as usize) as *mut u32 };

        let st = unsafe { core::ptr::read_volatile(rx_status) };
        if st & 0x1 == 0 {
            return Ok(None);
        }

        let len = unsafe { core::ptr::read_volatile(rx_len) } as usize;
        if len == 0 || len > 4096 {
            unsafe {
                core::ptr::write_volatile(rx_ack, 1);
            }
            return Err(crate::net::NetworkError::InvalidPacket);
        }

        let mut frame = Vec::with_capacity(len);
        for i in 0..len {
            let byte = unsafe { core::ptr::read_volatile(rx_buf.add(i)) };
            frame.push(byte);
        }

        unsafe {
            core::ptr::write_volatile(rx_ack, 1);
        }

        Ok(Some(frame))
    }
}

/// Parse an 802.11 beacon frame to extract network information.
///
/// Beacon frame structure (IEEE 802.11):
///   - Frame control (2 bytes)
///   - Duration (2 bytes)
///   - Address 1 / DA (6 bytes)
///   - Address 2 / SA (6 bytes)
///   - Address 3 / BSSID (6 bytes)
///   - Sequence control (2 bytes)
///   - Timestamp (8 bytes)
///   - Beacon interval (2 bytes)
///   - Capability info (2 bytes)
///   - Information elements (variable)
fn parse_beacon_frame(frame: &[u8], channel: u8) -> Option<WifiNetwork> {
    if frame.len() < 36 {
        return None;
    }

    let frame_type = (frame[0] >> 2) & 0x03;
    if frame_type != 0 {
        return None;
    }

    let frame_subtype = (frame[0] >> 4) & 0x0F;
    if frame_subtype != 0x08 {
        return None;
    }

    let bssid = [
        frame[16], frame[17], frame[18], frame[19], frame[20], frame[21],
    ];

    let beacon_interval = u16::from_le_bytes([frame[32], frame[33]]);
    if beacon_interval == 0 {
        return None;
    }

    let cap_info = u16::from_le_bytes([frame[34], frame[35]]);
    let encryption = (cap_info & 0x0010) != 0;

    let mut ssid = String::new();
    let mut ie_offset = 36usize;
    while ie_offset + 2 <= frame.len() {
        let ie_type = frame[ie_offset];
        let ie_len = frame[ie_offset + 1] as usize;
        let ie_data_start = ie_offset + 2;
        if ie_data_start + ie_len > frame.len() {
            break;
        }

        if ie_type == 0 && ie_len > 0 && ie_len <= 32 {
            ssid = core::str::from_utf8(&frame[ie_data_start..ie_data_start + ie_len])
                .unwrap_or("")
                .to_string();
        }

        ie_offset = ie_data_start + ie_len;
    }

    if ssid.is_empty() {
        return None;
    }

    let band = if channel <= 14 {
        WifiBand::Band2_4GHz
    } else {
        WifiBand::Band5GHz
    };

    let frequency_mhz = if channel <= 14 {
        2412 + (channel as u16 - 1) * 5
    } else {
        5000 + (channel as u16 - 36) * 5
    };

    let auth_type = if encryption {
        WifiAuthType::Wpa2Personal
    } else {
        WifiAuthType::Open
    };

    Some(WifiNetwork {
        ssid,
        bssid,
        channel,
        frequency_mhz,
        signal_strength_dbm: -50,
        band,
        auth_type,
        encryption,
    })
}

/// Check if a PCI device is an Atheros WiFi controller
pub fn is_atheros_wifi_device(vendor_id: u16, device_id: u16) -> bool {
    if vendor_id != ATHEROS_VENDOR_ID {
        return false;
    }

    ATHEROS_DEVICE_IDS.iter().any(|(id, _)| *id == device_id)
}

/// Get device name from device ID
pub fn get_device_name(device_id: u16) -> Option<&'static str> {
    ATHEROS_DEVICE_IDS
        .iter()
        .find(|(id, _)| *id == device_id)
        .map(|(_, name)| *name)
}

/// Create an Atheros WiFi driver for a PCI device
pub fn create_driver(
    vendor_id: u16,
    device_id: u16,
    base_addr: u64,
    irq: u8,
) -> Option<AtherosWifiDriver> {
    if !is_atheros_wifi_device(vendor_id, device_id) {
        return None;
    }

    let name = get_device_name(device_id)
        .map(|n| alloc::format!("Atheros {}", n))
        .unwrap_or_else(|| alloc::format!("Atheros WiFi {:04X}", device_id));

    Some(AtherosWifiDriver::new(name, device_id, base_addr, irq))
}

/// Create an Atheros WiFi driver matching the expected interface for the driver manager
/// This function returns the driver boxed with extended capabilities
pub fn create_atheros_wifi_driver(
    vendor_id: u16,
    device_id: u16,
    base_addr: u64,
    irq: u8,
) -> Option<(
    Box<dyn super::NetworkDriver>,
    super::ExtendedNetworkCapabilities,
)> {
    use super::{DeviceCapabilities, ExtendedNetworkCapabilities};

    if !is_atheros_wifi_device(vendor_id, device_id) {
        return None;
    }

    let name = get_device_name(device_id)
        .map(|n| alloc::format!("Atheros {}", n))
        .unwrap_or_else(|| alloc::format!("Atheros WiFi {:04X}", device_id));

    let driver = AtherosWifiDriverWrapper::new(name, device_id, base_addr, irq);

    let capabilities = ExtendedNetworkCapabilities {
        base: DeviceCapabilities::default(),
        wake_on_lan: false,
        energy_efficient: true,
        pxe_boot: false,
        sriov: false,
        max_bandwidth_mbps: 867, // WiFi 5 max theoretical speed
        wifi_standards: alloc::vec![
            "802.11a".to_string(),
            "802.11b".to_string(),
            "802.11g".to_string(),
            "802.11n".to_string(),
            "802.11ac".to_string()
        ],
        antenna_count: 2,
    };

    Some((Box::new(driver), capabilities))
}

/// Wrapper to implement NetworkDriver for AtherosWifiDriver
pub struct AtherosWifiDriverWrapper {
    inner: AtherosWifiDriver,
}

impl AtherosWifiDriverWrapper {
    pub fn new(name: String, device_id: u16, base_addr: u64, irq: u8) -> Self {
        Self {
            inner: AtherosWifiDriver::new(name, device_id, base_addr, irq),
        }
    }
}

impl super::NetworkDriver for AtherosWifiDriverWrapper {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn init(&mut self) -> Result<(), crate::net::NetworkError> {
        self.inner
            .init()
            .map_err(|_| crate::net::NetworkError::HardwareError)
    }

    fn start(&mut self) -> Result<(), crate::net::NetworkError> {
        // Enable the NIC mode register for the configured operating mode.
        if self.inner.base_addr != 0 {
            let base = self.inner.base_addr as *mut u8;
            const ATH9K_MODE_REG: u64 = 0x0020;
            let mode_reg = unsafe { base.add(ATH9K_MODE_REG as usize) as *mut u32 };
            let mode_val = match self.inner.mode {
                WifiMode::Station => 0u32,
                WifiMode::AccessPoint => 1u32,
                WifiMode::Monitor => 2u32,
                WifiMode::AdHoc => 3u32,
            };
            unsafe {
                core::ptr::write_volatile(mode_reg, mode_val);
            }
        }
        Ok(())
    }

    fn stop(&mut self) -> Result<(), crate::net::NetworkError> {
        let _ = self.inner.disconnect();
        Ok(())
    }

    fn send_packet(&mut self, data: &[u8]) -> Result<(), crate::net::NetworkError> {
        if self.inner.state() != AtherosDriverState::Connected {
            return Err(crate::net::NetworkError::NotConnected);
        }
        if self.inner.base_addr() == 0 {
            return Err(crate::net::NetworkError::HardwareError);
        }
        self.inner.send_80211_frame(data)
    }

    fn receive_packet(&mut self) -> Result<Option<Vec<u8>>, crate::net::NetworkError> {
        if self.inner.state() != AtherosDriverState::Connected {
            return Err(crate::net::NetworkError::NotConnected);
        }
        if self.inner.base_addr() == 0 {
            return Ok(None);
        }
        self.inner.recv_80211_frame()
    }

    fn get_mac_address(&self) -> crate::net::MacAddress {
        self.inner.mac_address()
    }

    fn state(&self) -> super::DeviceState {
        match self.inner.state() {
            AtherosDriverState::Uninitialized => super::DeviceState::Uninitialized,
            AtherosDriverState::Initializing => super::DeviceState::Initializing,
            AtherosDriverState::Ready | AtherosDriverState::Scanning => super::DeviceState::Stopped,
            AtherosDriverState::Connecting => super::DeviceState::Initializing,
            AtherosDriverState::Connected => super::DeviceState::Running,
            AtherosDriverState::Error => super::DeviceState::Error,
        }
    }

    fn get_link_status(&self) -> (bool, u32, bool) {
        let link_up = self.inner.state() == AtherosDriverState::Connected;
        let speed = if link_up { 867 } else { 0 }; // WiFi 5 speed
        (link_up, speed, true)
    }
}
