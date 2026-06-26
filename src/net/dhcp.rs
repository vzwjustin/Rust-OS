//! # Simple DHCP Implementation for RustOS
//!
//! A simplified DHCP client implementation for no_std environments

use super::{Ipv4Address, MacAddress, NetworkError};
use core::fmt;

/// DHCP message types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DhcpMessageType {
    Discover = 1,
    Offer = 2,
    Request = 3,
    Decline = 4,
    Ack = 5,
    Nak = 6,
    Release = 7,
    Inform = 8,
}

impl From<u8> for DhcpMessageType {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Discover,
            2 => Self::Offer,
            3 => Self::Request,
            4 => Self::Decline,
            5 => Self::Ack,
            6 => Self::Nak,
            7 => Self::Release,
            8 => Self::Inform,
            _ => Self::Discover, // Default fallback
        }
    }
}

impl From<DhcpMessageType> for u8 {
    fn from(msg_type: DhcpMessageType) -> u8 {
        msg_type as u8
    }
}

/// DHCP operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DhcpOperation {
    BootRequest = 1,
    BootReply = 2,
}

impl From<u8> for DhcpOperation {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::BootRequest,
            2 => Self::BootReply,
            _ => Self::BootRequest,
        }
    }
}

impl From<DhcpOperation> for u8 {
    fn from(op: DhcpOperation) -> u8 {
        op as u8
    }
}

/// DHCP hardware types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DhcpHardwareType {
    Ethernet = 1,
}

impl From<DhcpHardwareType> for u8 {
    fn from(hw_type: DhcpHardwareType) -> u8 {
        hw_type as u8
    }
}

/// DHCP client states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum DhcpClientState {
    Init = 0,
    Selecting = 1,
    Requesting = 2,
    Bound = 3,
    Renewing = 4,
    Rebinding = 5,
    InitReboot = 6,
    Rebooting = 7,
}

impl fmt::Display for DhcpClientState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state_str = match self {
            Self::Init => "INIT",
            Self::Selecting => "SELECTING",
            Self::Requesting => "REQUESTING",
            Self::Bound => "BOUND",
            Self::Renewing => "RENEWING",
            Self::Rebinding => "REBINDING",
            Self::InitReboot => "INIT-REBOOT",
            Self::Rebooting => "REBOOTING",
        };
        write!(f, "{}", state_str)
    }
}

/// Simplified DHCP packet structure
#[derive(Debug, Clone)]
pub struct DhcpPacket {
    pub operation: DhcpOperation,
    pub hardware_type: DhcpHardwareType,
    pub hardware_length: u8,
    pub hops: u8,
    pub transaction_id: u32,
    pub seconds: u16,
    pub flags: u16,
    pub client_ip: Ipv4Address,
    pub your_ip: Ipv4Address,
    pub server_ip: Ipv4Address,
    pub gateway_ip: Ipv4Address,
    pub client_hardware_address: [u8; 16],
    pub server_name: [u8; 64],
    pub boot_filename: [u8; 128],
    pub magic_cookie: u32,
    pub message_type: Option<DhcpMessageType>,
    pub lease_time: Option<u32>,
}

impl DhcpPacket {
    /// DHCP magic cookie value
    pub const MAGIC_COOKIE: u32 = 0x63825363;

    /// Create new DHCP packet
    pub fn new(operation: DhcpOperation, transaction_id: u32, client_mac: MacAddress) -> Self {
        let mut client_hw = [0u8; 16];
        client_hw[..6].copy_from_slice(&client_mac);

        Self {
            operation,
            hardware_type: DhcpHardwareType::Ethernet,
            hardware_length: 6,
            hops: 0,
            transaction_id,
            seconds: 0,
            flags: 0,
            client_ip: [0, 0, 0, 0],
            your_ip: [0, 0, 0, 0],
            server_ip: [0, 0, 0, 0],
            gateway_ip: [0, 0, 0, 0],
            client_hardware_address: client_hw,
            server_name: [0; 64],
            boot_filename: [0; 128],
            magic_cookie: Self::MAGIC_COOKIE,
            message_type: None,
            lease_time: None,
        }
    }

    /// Create DHCP Discover packet
    pub fn create_discover(transaction_id: u32, client_mac: MacAddress) -> Self {
        let mut packet = Self::new(DhcpOperation::BootRequest, transaction_id, client_mac);
        packet.flags = 0x8000; // Set broadcast flag
        packet.message_type = Some(DhcpMessageType::Discover);
        packet
    }

    /// Create DHCP Request packet
    pub fn create_request(
        transaction_id: u32,
        client_mac: MacAddress,
        requested_ip: Ipv4Address,
        server_ip: Ipv4Address,
    ) -> Self {
        let mut packet = Self::new(DhcpOperation::BootRequest, transaction_id, client_mac);
        packet.flags = 0x8000; // Set broadcast flag
        packet.message_type = Some(DhcpMessageType::Request);
        packet.your_ip = requested_ip;
        packet.server_ip = server_ip;
        packet
    }

    /// Get message type
    pub fn message_type(&self) -> Option<DhcpMessageType> {
        self.message_type
    }

    /// Get lease time
    pub fn get_lease_time(&self) -> Option<u32> {
        self.lease_time
    }

    /// Convert to bytes (simplified)
    pub fn to_bytes(&self) -> [u8; 240] {
        let mut bytes = [0u8; 240];

        bytes[0] = self.operation.into();
        bytes[1] = self.hardware_type.into();
        bytes[2] = self.hardware_length;
        bytes[3] = self.hops;

        bytes[4..8].copy_from_slice(&self.transaction_id.to_be_bytes());
        bytes[8..10].copy_from_slice(&self.seconds.to_be_bytes());
        bytes[10..12].copy_from_slice(&self.flags.to_be_bytes());

        bytes[12..16].copy_from_slice(&self.client_ip);
        bytes[16..20].copy_from_slice(&self.your_ip);
        bytes[20..24].copy_from_slice(&self.server_ip);
        bytes[24..28].copy_from_slice(&self.gateway_ip);

        bytes[28..44].copy_from_slice(&self.client_hardware_address);
        bytes[44..108].copy_from_slice(&self.server_name);
        bytes[108..236].copy_from_slice(&self.boot_filename);
        bytes[236..240].copy_from_slice(&self.magic_cookie.to_be_bytes());

        bytes
    }

    /// Parse from bytes (simplified)
    pub fn from_bytes(data: &[u8]) -> Result<Self, NetworkError> {
        if data.len() < 240 {
            return Err(NetworkError::InvalidPacket);
        }

        let operation = DhcpOperation::from(data[0]);
        let hardware_type = DhcpHardwareType::Ethernet; // Assume Ethernet
        let hardware_length = data[2];
        let hops = data[3];

        let transaction_id = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        let seconds = u16::from_be_bytes([data[8], data[9]]);
        let flags = u16::from_be_bytes([data[10], data[11]]);

        let client_ip = [data[12], data[13], data[14], data[15]];
        let your_ip = [data[16], data[17], data[18], data[19]];
        let server_ip = [data[20], data[21], data[22], data[23]];
        let gateway_ip = [data[24], data[25], data[26], data[27]];

        let mut client_hardware_address = [0u8; 16];
        client_hardware_address.copy_from_slice(&data[28..44]);

        let mut server_name = [0u8; 64];
        server_name.copy_from_slice(&data[44..108]);

        let mut boot_filename = [0u8; 128];
        boot_filename.copy_from_slice(&data[108..236]);

        let magic_cookie = u32::from_be_bytes([data[236], data[237], data[238], data[239]]);

        if magic_cookie != Self::MAGIC_COOKIE {
            return Err(NetworkError::ProtocolError);
        }

        Ok(Self {
            operation,
            hardware_type,
            hardware_length,
            hops,
            transaction_id,
            seconds,
            flags,
            client_ip,
            your_ip,
            server_ip,
            gateway_ip,
            client_hardware_address,
            server_name,
            boot_filename,
            magic_cookie,
            message_type: None, // Would need option parsing
            lease_time: None,
        })
    }
}

/// Simplified DHCP lease information
#[derive(Debug, Clone)]
pub struct DhcpLease {
    pub ip_address: Ipv4Address,
    pub subnet_mask: Ipv4Address,
    pub gateway: Ipv4Address,
    pub dns_servers: [Ipv4Address; 2],
    pub lease_time: u32,
    pub renewal_time: u32,
    pub rebinding_time: u32,
    pub start_time: u32,
}

impl DhcpLease {
    /// Check if lease has expired
    pub fn is_expired(&self, current_time: u32) -> bool {
        current_time > self.start_time + self.lease_time
    }

    /// Check if renewal time has passed
    pub fn needs_renewal(&self, current_time: u32) -> bool {
        current_time > self.start_time + self.renewal_time
    }

    /// Check if rebinding time has passed
    pub fn needs_rebinding(&self, current_time: u32) -> bool {
        current_time > self.start_time + self.rebinding_time
    }
}

/// Simplified DHCP client
pub struct DhcpClient {
    mac_address: MacAddress,
    state: DhcpClientState,
    transaction_id: u32,
    lease: Option<DhcpLease>,
    server_ip: Option<Ipv4Address>,
    discover_count: u32,
    request_count: u32,
}

impl DhcpClient {
    /// Create new DHCP client
    pub fn new(mac_address: MacAddress) -> Self {
        Self {
            mac_address,
            state: DhcpClientState::Init,
            transaction_id: 0,
            lease: None,
            server_ip: None,
            discover_count: 0,
            request_count: 0,
        }
    }

    /// Get current state
    pub fn state(&self) -> DhcpClientState {
        self.state
    }

    /// Start DHCP discovery
    pub fn start_discovery(&mut self) -> DhcpPacket {
        self.state = DhcpClientState::Selecting;
        self.transaction_id = self.transaction_id.wrapping_add(1);
        self.discover_count += 1;

        DhcpPacket::create_discover(self.transaction_id, self.mac_address)
    }

    /// Handle DHCP offer
    pub fn handle_offer(&mut self, offer: &DhcpPacket) -> Result<DhcpPacket, NetworkError> {
        if self.state != DhcpClientState::Selecting {
            return Err(NetworkError::InvalidPacket);
        }

        if offer.transaction_id != self.transaction_id {
            return Err(NetworkError::InvalidPacket);
        }

        self.state = DhcpClientState::Requesting;
        self.server_ip = Some(offer.server_ip);
        self.request_count += 1;

        Ok(DhcpPacket::create_request(
            self.transaction_id,
            self.mac_address,
            offer.your_ip,
            offer.server_ip,
        ))
    }

    /// Handle DHCP ACK
    pub fn handle_ack(&mut self, ack: &DhcpPacket, current_time: u32) -> Result<(), NetworkError> {
        if self.state != DhcpClientState::Requesting {
            return Err(NetworkError::InvalidPacket);
        }

        if ack.transaction_id != self.transaction_id {
            return Err(NetworkError::InvalidPacket);
        }

        let lease_time = ack.get_lease_time().unwrap_or(3600);
        let renewal_time = lease_time / 2;
        let rebinding_time = (lease_time * 7) / 8;

        self.lease = Some(DhcpLease {
            ip_address: ack.your_ip,
            subnet_mask: [255, 255, 255, 0], // Default
            gateway: ack.gateway_ip,
            dns_servers: [[8, 8, 8, 8], [8, 8, 4, 4]],
            lease_time,
            renewal_time,
            rebinding_time,
            start_time: current_time,
        });

        self.state = DhcpClientState::Bound;
        Ok(())
    }

    /// Get current lease
    pub fn lease(&self) -> Option<&DhcpLease> {
        self.lease.as_ref()
    }

    /// Update client state based on time
    pub fn update(&mut self, current_time: u32) {
        if let Some(ref lease) = self.lease {
            match self.state {
                DhcpClientState::Bound => {
                    if lease.needs_renewal(current_time) {
                        self.state = DhcpClientState::Renewing;
                    }
                }
                DhcpClientState::Renewing => {
                    if lease.needs_rebinding(current_time) {
                        self.state = DhcpClientState::Rebinding;
                    }
                }
                DhcpClientState::Rebinding => {
                    if lease.is_expired(current_time) {
                        self.state = DhcpClientState::Init;
                        self.lease = None;
                    }
                }
                _ => {}
            }
        }
    }

    /// Get client statistics
    pub fn stats(&self) -> DhcpClientStats {
        DhcpClientStats {
            discovers_sent: self.discover_count,
            offers_received: if self.state as u8 > DhcpClientState::Selecting as u8 {
                1
            } else {
                0
            },
            requests_sent: self.request_count,
            acks_received: if self.lease.is_some() { 1 } else { 0 },
            current_state: self.state,
        }
    }
}

/// DHCP client statistics
#[derive(Debug, Clone)]
pub struct DhcpClientStats {
    pub discovers_sent: u32,
    pub offers_received: u32,
    pub requests_sent: u32,
    pub acks_received: u32,
    pub current_state: DhcpClientState,
}

/// DHCP error types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DhcpError {
    InvalidPacket,
    InvalidState,
    Timeout,
    NetworkError,
}

impl fmt::Display for DhcpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPacket => write!(f, "Invalid DHCP packet"),
            Self::InvalidState => write!(f, "Invalid client state"),
            Self::Timeout => write!(f, "DHCP timeout"),
            Self::NetworkError => write!(f, "Network error"),
        }
    }
}

// Test functions (simplified, without #[cfg(feature = "std-tests")] // Disabled: #[cfg(feature = "disabled-tests")] // #[cfg(feature = "disabled-tests")] // #[test_case] attributes)
#[cfg(test)]
mod tests {
    use super::*;

    fn test_dhcp_message_types() {
        assert_eq!(u8::from(DhcpMessageType::Discover), 1);
        assert_eq!(u8::from(DhcpMessageType::Offer), 2);
        assert_eq!(u8::from(DhcpMessageType::Request), 3);
        assert_eq!(DhcpMessageType::from(1u8), DhcpMessageType::Discover);
    }

    fn test_dhcp_packet_creation() {
        let mac = [0x00, 0x11, 0x22, 0x33, 0x44, 0x55];
        let packet = DhcpPacket::create_discover(12345, mac);

        assert_eq!(packet.operation, DhcpOperation::BootRequest);
        assert_eq!(packet.transaction_id, 12345);
        assert_eq!(packet.message_type(), Some(DhcpMessageType::Discover));
    }

    fn test_dhcp_client_state_machine() {
        let mac = [0x01, 0x02, 0x03, 0x04, 0x05, 0x06];
        let mut client = DhcpClient::new(mac);

        assert_eq!(client.state(), DhcpClientState::Init);

        let _discover = client.start_discovery();
        assert_eq!(client.state(), DhcpClientState::Selecting);
    }
}
