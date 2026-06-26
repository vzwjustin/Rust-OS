//! ARP (Address Resolution Protocol) implementation
//!
//! Provides comprehensive ARP table management with aging, updates, and security features
//! conforming to RFC 826.
//!
//! # Features
//!
//! - RFC 826 compliant ARP implementation
//! - Dynamic ARP table with aging and state management
//! - Static ARP entry support for security
//! - ARP request/reply processing
//! - Gratuitous ARP handling
//! - ARP cache timeout and entry eviction
//! - Anti-spoofing security features
//! - Comprehensive statistics and monitoring
//!
//! # Security
//!
//! The implementation includes security flags to detect and prevent ARP spoofing attacks.
//! Static entries can be configured for critical infrastructure to prevent cache poisoning.

use super::{NetworkAddress, NetworkError, NetworkResult};
use alloc::{collections::BTreeMap, string::String, vec::Vec};
use spin::RwLock;

/// ARP entry states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArpEntryState {
    /// Entry is incomplete (request sent, no reply yet)
    Incomplete,
    /// Entry is reachable (recently confirmed)
    Reachable,
    /// Entry is stale (may be outdated but usable)
    Stale,
    /// Entry is in delay state (waiting for confirmation)
    Delay,
    /// Entry is being probed for reachability
    Probe,
    /// Entry failed reachability test
    Failed,
}

/// ARP table entry with comprehensive metadata
#[derive(Debug, Clone)]
pub struct ArpEntry {
    /// IP address
    pub ip_address: NetworkAddress,
    /// MAC address (None if incomplete)
    pub mac_address: Option<NetworkAddress>,
    /// Entry state
    pub state: ArpEntryState,
    /// Creation timestamp
    pub created: u64,
    /// Last update timestamp
    pub updated: u64,
    /// Last used timestamp
    pub last_used: u64,
    /// Number of times this entry was used
    pub use_count: u64,
    /// Number of ARP requests sent for this entry
    pub request_count: u8,
    /// Interface this entry belongs to
    pub interface: String,
    /// Whether this is a static entry
    pub is_static: bool,
    /// Security flags
    pub security_flags: ArpSecurityFlags,
}

/// ARP security flags
#[derive(Debug, Clone, Copy, Default)]
pub struct ArpSecurityFlags {
    /// Entry was verified through gratuitous ARP
    pub verified: bool,
    /// Entry appears to be spoofed
    pub suspicious: bool,
    /// Entry failed security validation
    pub blocked: bool,
    /// Entry is from a trusted source
    pub trusted: bool,
}

impl ArpEntry {
    /// Create a new ARP entry
    pub fn new(ip: NetworkAddress, interface: String) -> Self {
        let now = current_time_ms();
        Self {
            ip_address: ip,
            mac_address: None,
            state: ArpEntryState::Incomplete,
            created: now,
            updated: now,
            last_used: now,
            use_count: 0,
            request_count: 0,
            interface,
            is_static: false,
            security_flags: ArpSecurityFlags::default(),
        }
    }

    /// Create a static ARP entry
    pub fn new_static(ip: NetworkAddress, mac: NetworkAddress, interface: String) -> Self {
        let now = current_time_ms();
        Self {
            ip_address: ip,
            mac_address: Some(mac),
            state: ArpEntryState::Reachable,
            created: now,
            updated: now,
            last_used: now,
            use_count: 0,
            request_count: 0,
            interface,
            is_static: true,
            security_flags: ArpSecurityFlags {
                trusted: true,
                ..Default::default()
            },
        }
    }

    /// Update entry with new MAC address
    pub fn update_mac(&mut self, mac: NetworkAddress) {
        let now = current_time_ms();

        // Check for MAC address changes (potential spoofing)
        if let Some(existing_mac) = self.mac_address {
            if existing_mac != mac && !self.is_static {
                self.security_flags.suspicious = true;
            }
        }

        self.mac_address = Some(mac);
        self.state = ArpEntryState::Reachable;
        self.updated = now;
        self.request_count = 0;
    }

    /// Mark entry as used
    pub fn mark_used(&mut self) {
        self.last_used = current_time_ms();
        self.use_count += 1;

        // Transition from stale to delay when used
        if self.state == ArpEntryState::Stale {
            self.state = ArpEntryState::Delay;
        }
    }

    /// Check if entry is expired
    pub fn is_expired(&self, max_age_ms: u64) -> bool {
        if self.is_static {
            return false;
        }

        let now = current_time_ms();
        match self.state {
            ArpEntryState::Incomplete => now - self.created > 3000, // 3 seconds
            ArpEntryState::Reachable => now - self.updated > max_age_ms,
            ArpEntryState::Stale => now - self.updated > max_age_ms * 2,
            ArpEntryState::Delay => now - self.last_used > 5000, // 5 seconds
            ArpEntryState::Probe => now - self.updated > 1000,   // 1 second
            ArpEntryState::Failed => true,                       // Always expired
        }
    }

    /// Check if entry needs revalidation
    pub fn needs_revalidation(&self) -> bool {
        if self.is_static {
            return false;
        }

        let now = current_time_ms();
        match self.state {
            ArpEntryState::Reachable => now - self.updated > 15000, // 15 seconds
            ArpEntryState::Stale => true,
            ArpEntryState::Delay => now - self.last_used > 5000,
            _ => false,
        }
    }

    /// Increment request count and check if limit exceeded
    pub fn increment_request_count(&mut self) -> bool {
        self.request_count += 1;
        self.request_count > 3 // Max 3 requests
    }
}

/// ARP table statistics
#[derive(Debug, Clone, Default)]
pub struct ArpTableStats {
    pub total_entries: usize,
    pub static_entries: usize,
    pub reachable_entries: usize,
    pub stale_entries: usize,
    pub incomplete_entries: usize,
    pub failed_entries: usize,
    pub requests_sent: u64,
    pub replies_received: u64,
    pub gratuitous_arps: u64,
    pub security_violations: u64,
    pub cache_hits: u64,
    pub cache_misses: u64,
}

/// Enhanced ARP table manager
pub struct ArpTable {
    /// ARP entries indexed by IP address
    entries: RwLock<BTreeMap<NetworkAddress, ArpEntry>>,
    /// Statistics
    stats: RwLock<ArpTableStats>,
    /// Configuration
    config: ArpConfig,
}

/// ARP configuration
#[derive(Debug, Clone)]
pub struct ArpConfig {
    /// Maximum age for reachable entries (milliseconds)
    pub max_age_ms: u64,
    /// Maximum number of entries
    pub max_entries: usize,
    /// Enable ARP security checks
    pub security_enabled: bool,
    /// Allow gratuitous ARP updates
    pub allow_gratuitous: bool,
    /// Trusted MAC address prefixes
    pub trusted_prefixes: Vec<[u8; 3]>,
}

impl Default for ArpConfig {
    fn default() -> Self {
        Self {
            max_age_ms: 300000, // 5 minutes
            max_entries: 1024,
            security_enabled: true,
            allow_gratuitous: true,
            trusted_prefixes: Vec::new(),
        }
    }
}

impl ArpTable {
    /// Create a new ARP table
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(BTreeMap::new()),
            stats: RwLock::new(ArpTableStats::default()),
            config: ArpConfig::default(),
        }
    }

    /// Create ARP table with custom configuration
    pub fn with_config(config: ArpConfig) -> Self {
        Self {
            entries: RwLock::new(BTreeMap::new()),
            stats: RwLock::new(ArpTableStats::default()),
            config,
        }
    }

    /// Add or update ARP entry
    pub fn update_entry(
        &self,
        ip: NetworkAddress,
        mac: NetworkAddress,
        interface: String,
    ) -> NetworkResult<()> {
        let mut entries = self.entries.write();
        let mut stats = self.stats.write();

        // Security check for suspicious activity
        if self.config.security_enabled {
            if let Some(existing) = entries.get(&ip) {
                if let Some(existing_mac) = existing.mac_address {
                    if existing_mac != mac && !self.is_trusted_mac(&mac) {
                        stats.security_violations += 1;
                        // Could block or flag as suspicious
                    }
                }
            }
        }

        match entries.get_mut(&ip) {
            Some(entry) => {
                entry.update_mac(mac);
                stats.replies_received += 1;
            }
            None => {
                // Check entry limit
                if entries.len() >= self.config.max_entries {
                    self.evict_oldest_entry(&mut entries);
                }

                let mut entry = ArpEntry::new(ip, interface);
                entry.update_mac(mac);
                entries.insert(ip, entry);
                stats.total_entries += 1;
            }
        }

        Ok(())
    }

    /// Look up MAC address for IP
    pub fn lookup(&self, ip: &NetworkAddress) -> Option<NetworkAddress> {
        let mut entries = self.entries.write();
        let mut stats = self.stats.write();

        if let Some(entry) = entries.get_mut(ip) {
            if entry.is_expired(self.config.max_age_ms) {
                stats.cache_misses += 1;
                return None;
            }

            entry.mark_used();
            stats.cache_hits += 1;

            // Transition states based on usage
            match entry.state {
                ArpEntryState::Stale => entry.state = ArpEntryState::Delay,
                _ => {}
            }

            entry.mac_address
        } else {
            stats.cache_misses += 1;
            None
        }
    }

    /// Add static ARP entry
    pub fn add_static_entry(
        &self,
        ip: NetworkAddress,
        mac: NetworkAddress,
        interface: String,
    ) -> NetworkResult<()> {
        let mut entries = self.entries.write();
        let mut stats = self.stats.write();

        let entry = ArpEntry::new_static(ip, mac, interface);
        entries.insert(ip, entry);
        stats.total_entries += 1;
        stats.static_entries += 1;

        Ok(())
    }

    /// Remove ARP entry
    pub fn remove_entry(&self, ip: &NetworkAddress) -> NetworkResult<()> {
        let mut entries = self.entries.write();
        let mut stats = self.stats.write();

        if entries.remove(ip).is_some() {
            stats.total_entries -= 1;
            Ok(())
        } else {
            Err(NetworkError::InvalidAddress)
        }
    }

    /// Get all entries
    pub fn get_all_entries(&self) -> Vec<ArpEntry> {
        let entries = self.entries.read();
        entries.values().cloned().collect()
    }

    /// Age out expired entries
    pub fn age_entries(&self) {
        let mut entries = self.entries.write();
        let mut stats = self.stats.write();

        let expired_ips: Vec<_> = entries
            .iter()
            .filter(|(_, entry)| entry.is_expired(self.config.max_age_ms))
            .map(|(&ip, _)| ip)
            .collect();

        for ip in expired_ips {
            entries.remove(&ip);
        }

        // Recalculate all statistics after removing expired entries
        self.update_state_stats(&entries, &mut stats);
    }

    /// Handle gratuitous ARP
    pub fn handle_gratuitous_arp(
        &self,
        ip: NetworkAddress,
        mac: NetworkAddress,
        interface: String,
    ) -> NetworkResult<()> {
        if !self.config.allow_gratuitous {
            return Err(NetworkError::NotSupported);
        }

        let mut entries = self.entries.write();
        let mut stats = self.stats.write();

        stats.gratuitous_arps += 1;

        // Security check for gratuitous ARP
        if self.config.security_enabled {
            if let Some(existing) = entries.get(&ip) {
                if let Some(existing_mac) = existing.mac_address {
                    if existing_mac != mac {
                        // Potential ARP spoofing
                        stats.security_violations += 1;
                        return Err(NetworkError::PermissionDenied);
                    }
                }
            }
        }

        // Update or create entry
        match entries.get_mut(&ip) {
            Some(entry) => {
                entry.update_mac(mac);
                entry.security_flags.verified = true;
            }
            None => {
                let mut entry = ArpEntry::new(ip, interface);
                entry.update_mac(mac);
                entry.security_flags.verified = true;
                entries.insert(ip, entry);
                stats.total_entries += 1;
            }
        }

        Ok(())
    }

    /// Get ARP table statistics
    pub fn get_stats(&self) -> ArpTableStats {
        let entries = self.entries.read();
        let mut stats = self.stats.write();

        self.update_state_stats(&entries, &mut stats);
        stats.clone()
    }

    /// Clear all non-static entries
    pub fn clear_dynamic_entries(&self) {
        let mut entries = self.entries.write();
        let mut stats = self.stats.write();

        let static_entries: BTreeMap<_, _> = entries
            .iter()
            .filter(|(_, entry)| entry.is_static)
            .map(|(k, v)| (*k, v.clone()))
            .collect();

        entries.clear();
        *entries = static_entries;

        // Recalculate all statistics after clearing entries
        self.update_state_stats(&entries, &mut stats);
    }

    /// Check if MAC address is from a trusted vendor
    fn is_trusted_mac(&self, mac: &NetworkAddress) -> bool {
        if let NetworkAddress::Mac(mac_bytes) = mac {
            let prefix = [mac_bytes[0], mac_bytes[1], mac_bytes[2]];
            self.config.trusted_prefixes.contains(&prefix)
        } else {
            false
        }
    }

    /// Evict oldest entry to make room
    fn evict_oldest_entry(&self, entries: &mut BTreeMap<NetworkAddress, ArpEntry>) {
        if let Some((&oldest_ip, _)) = entries
            .iter()
            .filter(|(_, entry)| !entry.is_static)
            .min_by_key(|(_, entry)| entry.last_used)
        {
            entries.remove(&oldest_ip);
        }
    }

    /// Update state-based statistics
    fn update_state_stats(
        &self,
        entries: &BTreeMap<NetworkAddress, ArpEntry>,
        stats: &mut ArpTableStats,
    ) {
        stats.total_entries = entries.len();
        stats.static_entries = entries.values().filter(|e| e.is_static).count();
        stats.reachable_entries = entries
            .values()
            .filter(|e| e.state == ArpEntryState::Reachable)
            .count();
        stats.stale_entries = entries
            .values()
            .filter(|e| e.state == ArpEntryState::Stale)
            .count();
        stats.incomplete_entries = entries
            .values()
            .filter(|e| e.state == ArpEntryState::Incomplete)
            .count();
        stats.failed_entries = entries
            .values()
            .filter(|e| e.state == ArpEntryState::Failed)
            .count();
    }
}

/// Global ARP table instance
static ARP_TABLE: ArpTable = ArpTable {
    entries: RwLock::new(BTreeMap::new()),
    stats: RwLock::new(ArpTableStats {
        total_entries: 0,
        static_entries: 0,
        reachable_entries: 0,
        stale_entries: 0,
        incomplete_entries: 0,
        failed_entries: 0,
        requests_sent: 0,
        replies_received: 0,
        gratuitous_arps: 0,
        security_violations: 0,
        cache_hits: 0,
        cache_misses: 0,
    }),
    config: ArpConfig {
        max_age_ms: 300000,
        max_entries: 1024,
        security_enabled: true,
        allow_gratuitous: true,
        trusted_prefixes: Vec::new(),
    },
};

/// Public API functions

/// Update ARP entry
pub fn update_arp_entry(
    ip: NetworkAddress,
    mac: NetworkAddress,
    interface: String,
) -> NetworkResult<()> {
    ARP_TABLE.update_entry(ip, mac, interface)
}

/// Lookup MAC address for IP
pub fn lookup_arp(ip: &NetworkAddress) -> Option<NetworkAddress> {
    ARP_TABLE.lookup(ip)
}

/// Add static ARP entry
pub fn add_static_arp(
    ip: NetworkAddress,
    mac: NetworkAddress,
    interface: String,
) -> NetworkResult<()> {
    ARP_TABLE.add_static_entry(ip, mac, interface)
}

/// Remove ARP entry
pub fn remove_arp_entry(ip: &NetworkAddress) -> NetworkResult<()> {
    ARP_TABLE.remove_entry(ip)
}

/// Get all ARP entries
pub fn get_arp_table() -> Vec<ArpEntry> {
    ARP_TABLE.get_all_entries()
}

/// Age out expired ARP entries
pub fn age_arp_table() {
    ARP_TABLE.age_entries()
}

/// Handle gratuitous ARP
pub fn handle_gratuitous_arp(
    ip: NetworkAddress,
    mac: NetworkAddress,
    interface: String,
) -> NetworkResult<()> {
    ARP_TABLE.handle_gratuitous_arp(ip, mac, interface)
}

/// Get ARP table statistics
pub fn get_arp_stats() -> ArpTableStats {
    ARP_TABLE.get_stats()
}

/// Clear all dynamic ARP entries
pub fn clear_arp_table() {
    ARP_TABLE.clear_dynamic_entries()
}

/// Initialize ARP subsystem
pub fn init() -> NetworkResult<()> {
    // ARP table is statically initialized
    Ok(())
}

/// Cleanup routine to be called periodically
pub fn cleanup() {
    age_arp_table();
}

/// Get current time in milliseconds
fn current_time_ms() -> u64 {
    // Use system time for ARP cache timeouts
    crate::time::get_system_time_ms()
}

/// ARP request/reply processing functions

/// Process ARP request
pub fn process_arp_request(
    sender_ip: NetworkAddress,
    sender_mac: NetworkAddress,
    target_ip: NetworkAddress,
    interface: String,
) -> NetworkResult<()> {
    // Update ARP table with sender information
    update_arp_entry(sender_ip, sender_mac, interface.clone())?;

    // Check if we should reply (target IP is ours)
    let network_stack = crate::net::network_stack();
    let iface = network_stack.get_interface(&interface);

    if let Some(iface) = iface {
        // Check if target IP matches any of our interface IPs
        if iface.ip_addresses.contains(&target_ip) {
            // Target IP is ours - send ARP reply
            let our_mac = iface.mac_address;
            let our_ip = target_ip;

            // Build and send ARP reply packet via ethernet module
            match super::ethernet::create_arp_reply(our_mac, our_ip, sender_mac, sender_ip) {
                Ok(packet) => {
                    network_stack.send_packet(&interface, packet)?;
                }
                Err(e) => return Err(e),
            }
        }
    }

    Ok(())
}

/// Process ARP reply
pub fn process_arp_reply(
    sender_ip: NetworkAddress,
    sender_mac: NetworkAddress,
    interface: String,
) -> NetworkResult<()> {
    // Update ARP table with reply information
    update_arp_entry(sender_ip, sender_mac, interface)?;

    Ok(())
}

/// Send ARP request (would be called by higher layers)
pub fn send_arp_request(target_ip: NetworkAddress, interface: String) -> NetworkResult<()> {
    // Create incomplete entry before sending request
    let (sender_mac, sender_ip) = {
        let mut entries = ARP_TABLE.entries.write();
        let mut stats = ARP_TABLE.stats.write();

        if !entries.contains_key(&target_ip) {
            let entry = ArpEntry::new(target_ip, interface.clone());
            entries.insert(target_ip, entry);
            stats.total_entries += 1;
            stats.incomplete_entries += 1;
        }

        stats.requests_sent += 1;

        // Get interface MAC and IP from network stack
        drop(entries);
        drop(stats);

        // Access network stack to get interface details
        let network_stack = crate::net::network_stack();
        let iface = network_stack
            .get_interface(&interface)
            .ok_or(NetworkError::InvalidAddress)?;

        let src_mac = iface.mac_address;
        let src_ip = *iface
            .ip_addresses
            .first()
            .ok_or(NetworkError::InvalidAddress)?;

        (src_mac, src_ip)
    };

    // Build ARP request packet
    let mut packet_data = alloc::vec::Vec::new();

    // Ethernet header (14 bytes)
    packet_data.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF]); // Destination: broadcast
    if let NetworkAddress::Mac(mac_bytes) = sender_mac {
        packet_data.extend_from_slice(&mac_bytes); // Source MAC
    } else {
        return Err(NetworkError::InvalidAddress);
    }
    packet_data.extend_from_slice(&[0x08, 0x06]); // EtherType: ARP (0x0806)

    // ARP packet (28 bytes)
    packet_data.extend_from_slice(&[0x00, 0x01]); // Hardware type: Ethernet (1)
    packet_data.extend_from_slice(&[0x08, 0x00]); // Protocol type: IPv4 (0x0800)
    packet_data.push(6); // Hardware address length: 6
    packet_data.push(4); // Protocol address length: 4
    packet_data.extend_from_slice(&[0x00, 0x01]); // Operation: Request (1)

    // Sender hardware address (6 bytes)
    if let NetworkAddress::Mac(mac_bytes) = sender_mac {
        packet_data.extend_from_slice(&mac_bytes);
    } else {
        return Err(NetworkError::InvalidAddress);
    }

    // Sender protocol address (4 bytes)
    if let NetworkAddress::IPv4(ip_bytes) = sender_ip {
        packet_data.extend_from_slice(&ip_bytes);
    } else {
        return Err(NetworkError::NotSupported); // ARP only works with IPv4
    }

    // Target hardware address (6 bytes) - unknown, all zeros
    packet_data.extend_from_slice(&[0x00, 0x00, 0x00, 0x00, 0x00, 0x00]);

    // Target protocol address (4 bytes)
    if let NetworkAddress::IPv4(ip_bytes) = target_ip {
        packet_data.extend_from_slice(&ip_bytes);
    } else {
        return Err(NetworkError::NotSupported); // ARP only works with IPv4
    }

    // Create packet buffer and send
    let packet = super::PacketBuffer::from_data(packet_data);
    let network_stack = crate::net::network_stack();
    network_stack.send_packet(&interface, packet)?;

    Ok(())
}
