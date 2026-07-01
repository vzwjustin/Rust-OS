//! # VirtIO Network Driver
//!
//! Implements virtio-net for paravirtualized network access in QEMU/KVM.
//! Uses virtqueues for RX (receive) and TX (transmit) with scatter-gather DMA.

use super::*;
use crate::net::device::{
    DeviceCapabilities, DeviceInfo, DeviceType, DuplexMode, LinkMode, NetworkDevice,
};
use crate::net::MacAddress;
use crate::net::{InterfaceStats, NetworkAddress, NetworkError, NetworkResult, PacketBuffer};
use alloc::boxed::Box;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;
use spin::Mutex;

/// virtio-net feature bits
const VIRTIO_NET_F_CSUM: u64 = 1 << 0;
const VIRTIO_NET_F_GUEST_CSUM: u64 = 1 << 1;
const VIRTIO_NET_F_MTU: u64 = 1 << 3;
const VIRTIO_NET_F_MAC: u64 = 1 << 5;
const VIRTIO_NET_F_MRG_RXBUF: u64 = 1 << 15;
const VIRTIO_NET_F_STATUS: u64 = 1 << 16;
const VIRTIO_NET_F_CTRL_VQ: u64 = 1 << 17;
const VIRTIO_NET_F_CTRL_RX: u64 = 1 << 18;
const VIRTIO_NET_F_CTRL_VLAN: u64 = 1 << 19;
const VIRTIO_NET_F_MQ: u64 = 1 << 22;

const VIRTIO_NET_S_LINK_UP: u16 = 1;
const VIRTIO_NET_S_ANNOUNCE: u16 = 2;

/// virtio-net header (mandatory prefix for all packets)
#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtioNetHdr {
    pub flags: u8,
    pub gso_type: u8,
    pub hdr_len: u16,
    pub gso_size: u16,
    pub csum_start: u16,
    pub csum_offset: u16,
}

/// virtio-net device config
#[repr(C)]
pub struct VirtioNetConfig {
    pub mac: [u8; 6],
    pub status: u16,
    pub max_virtqueue_pairs: u16,
    pub mtu: u16,
}

/// Normalized virtio-net capabilities from negotiated feature bits.
#[derive(Debug, Clone, Copy, Default)]
pub struct VirtioNetCapabilities {
    pub checksum_offload: bool,
    pub guest_checksum: bool,
    pub mac_config: bool,
    pub status_config: bool,
    pub mtu_config: bool,
    pub control_queue: bool,
    pub rx_mode_control: bool,
    pub vlan_control: bool,
    pub multiqueue: bool,
    pub mergeable_rx_buffers: bool,
    pub max_mtu: u16,
    pub queue_pairs: u16,
}

impl VirtioNetCapabilities {
    pub fn from_features(features: u64, max_mtu: u16, queue_pairs: u16) -> Self {
        Self {
            checksum_offload: (features & VIRTIO_NET_F_CSUM) != 0,
            guest_checksum: (features & VIRTIO_NET_F_GUEST_CSUM) != 0,
            mac_config: (features & VIRTIO_NET_F_MAC) != 0,
            status_config: (features & VIRTIO_NET_F_STATUS) != 0,
            mtu_config: (features & VIRTIO_NET_F_MTU) != 0,
            control_queue: (features & VIRTIO_NET_F_CTRL_VQ) != 0,
            rx_mode_control: (features & VIRTIO_NET_F_CTRL_RX) != 0,
            vlan_control: (features & VIRTIO_NET_F_CTRL_VLAN) != 0,
            multiqueue: (features & VIRTIO_NET_F_MQ) != 0,
            mergeable_rx_buffers: (features & VIRTIO_NET_F_MRG_RXBUF) != 0,
            max_mtu,
            queue_pairs,
        }
    }
}

/// Decode the virtio-net config status field.
pub fn decode_virtio_net_status(status: u16, status_feature: bool) -> (bool, bool) {
    if !status_feature {
        return (true, false);
    }
    (
        (status & VIRTIO_NET_S_LINK_UP) != 0,
        (status & VIRTIO_NET_S_ANNOUNCE) != 0,
    )
}

/// Queue indices for virtio-net
const RX_QUEUE: u16 = 0;
const TX_QUEUE: u16 = 1;

/// Default queue size
const QUEUE_SIZE: u16 = 32;

/// Packet buffer size (1518 = max Ethernet frame)
const PACKET_BUF_SIZE: usize = 1518 + 12; // +12 for virtio-net header

/// Number of static TX buffers in the pool
const TX_BUF_COUNT: usize = 32;

/// Static TX buffer pool — buffers must persist until the device has consumed them.
/// Stack-local buffers would be deallocated before the device finishes DMA.
static TX_BUFFERS: Mutex<[[u8; PACKET_BUF_SIZE]; TX_BUF_COUNT]> =
    Mutex::new([[0u8; PACKET_BUF_SIZE]; TX_BUF_COUNT]);

/// Track which TX buffers are in use
static TX_BUF_USED: Mutex<[bool; TX_BUF_COUNT]> = Mutex::new([false; TX_BUF_COUNT]);

/// Allocate a TX buffer from the static pool
fn alloc_tx_buffer() -> Option<usize> {
    let mut used = TX_BUF_USED.lock();
    for i in 0..TX_BUF_COUNT {
        if !used[i] {
            used[i] = true;
            return Some(i);
        }
    }
    None
}

/// Free a TX buffer back to the pool
fn free_tx_buffer(idx: usize) {
    TX_BUF_USED.lock()[idx] = false;
}

fn reap_tx_completions(tx_queue: &mut VirtQueue) {
    while tx_queue.has_used() {
        if let Some((id, _len)) = tx_queue.pop_used() {
            let desc_idx = id as usize;
            let buf_idx = {
                let mut map = TX_DESC_TO_BUF.lock();
                let Some(slot) = map.get_mut(desc_idx) else {
                    tx_queue.free_desc(id as u16);
                    continue;
                };
                let buf_idx = *slot;
                *slot = TX_BUF_SENTINEL;
                buf_idx
            };
            if buf_idx != TX_BUF_SENTINEL {
                free_tx_buffer(buf_idx as usize);
            }
            tx_queue.free_desc(id as u16);
        }
    }
}

/// VirtIO network driver state
pub struct VirtioNet {
    transport: VirtioTransport,
    mac_address: MacAddress,
    features: u64,
    capabilities: VirtioNetCapabilities,
    rx_queue: Option<VirtQueue>,
    tx_queue: Option<VirtQueue>,
    /// Receive buffer storage.  Each buffer is boxed so its DMA address remains
    /// stable even if the Vec metadata moves.
    rx_buffers: Vec<Box<[u8; PACKET_BUF_SIZE]>>,
}

impl VirtioNet {
    /// Create and initialize a virtio-net device
    pub fn new(transport: VirtioTransport) -> Result<Self, &'static str> {
        // Negotiate only the common virtio-net features this driver implements.
        // Do not request MRG_RXBUF unless the larger header format is wired.
        let device_features = transport.read_device_features();
        let driver_features = VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS | VIRTIO_NET_F_MTU;
        let negotiated_features = device_features & driver_features;
        transport.init_device(driver_features)?;

        // Read MAC address from device config
        let mut mac = [0u8; 6];
        for i in 0..6 {
            mac[i] = transport.read_device_config8(i as u32);
        }

        // If MAC is all zeros, use a default
        if mac.iter().all(|&b| b == 0) {
            mac = [0x52, 0x54, 0x00, 0x12, 0x34, 0x56];
        }

        let configured_mtu = if (negotiated_features & VIRTIO_NET_F_MTU) != 0 {
            read_config16(&transport, 10)
        } else {
            1500
        };
        let max_mtu = if configured_mtu >= 68 {
            configured_mtu.min(1500)
        } else {
            1500
        };
        let queue_pairs = if (negotiated_features & VIRTIO_NET_F_MQ) != 0 {
            read_config16(&transport, 8).max(1)
        } else {
            1
        };
        let capabilities =
            VirtioNetCapabilities::from_features(negotiated_features, max_mtu, queue_pairs);

        crate::serial_println!(
            "virtio-net: MAC={:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            mac[0],
            mac[1],
            mac[2],
            mac[3],
            mac[4],
            mac[5]
        );

        // Set up RX queue
        let rx_size = transport.select_queue(RX_QUEUE);
        let rx_size = if rx_size == 0 {
            QUEUE_SIZE
        } else {
            rx_size.min(QUEUE_SIZE)
        };
        let rx_notify_off = {
            // Read notify_off from common config after selecting queue
            transport.selected_queue_notify_off()
        };
        let mut rx_queue = VirtQueue::new(rx_size, rx_notify_off)?;
        transport.setup_queue(&rx_queue);

        // Set up TX queue
        let tx_size = transport.select_queue(TX_QUEUE);
        let tx_size = if tx_size == 0 {
            QUEUE_SIZE
        } else {
            tx_size.min(QUEUE_SIZE)
        };
        let tx_notify_off = { transport.selected_queue_notify_off() };
        let tx_queue = VirtQueue::new(tx_size, tx_notify_off)?;
        transport.setup_queue(&tx_queue);

        // Allocate all RX buffers before publishing any DMA descriptors.  The
        // boxed backing storage keeps each buffer's address stable for the
        // lifetime of the descriptor, and preallocating avoids Vec growth while
        // descriptors are visible to the device.
        let mut rx_buffers = Vec::with_capacity(rx_size as usize);
        for _ in 0..rx_size as usize {
            rx_buffers.push(Box::new([0u8; PACKET_BUF_SIZE]));
        }

        for i in 0..rx_size as usize {
            let buf_phys = super::virt_to_phys(rx_buffers[i].as_ptr() as usize);
            let desc_idx = i as u16;
            rx_queue.set_desc(
                desc_idx,
                buf_phys,
                PACKET_BUF_SIZE as u32,
                desc_flags::WRITE,
                0,
            );
            rx_queue.submit(desc_idx);
        }
        transport.notify(&rx_queue);

        // Set DRIVER_OK to complete initialization
        transport.set_driver_ok();

        crate::serial_println!(
            "virtio-net: initialized (rx_queue={}, tx_queue={})",
            rx_size,
            tx_size
        );

        Ok(VirtioNet {
            transport,
            mac_address: mac,
            features: negotiated_features,
            capabilities,
            rx_queue: Some(rx_queue),
            tx_queue: Some(tx_queue),
            rx_buffers,
        })
    }

    /// Get MAC address
    pub fn mac_address(&self) -> MacAddress {
        self.mac_address
    }

    pub fn capabilities(&self) -> VirtioNetCapabilities {
        self.capabilities
    }

    /// Send a packet
    pub fn send(&mut self, data: &[u8]) -> Result<(), &'static str> {
        if data.is_empty() {
            return Err("virtio-net: empty packet");
        }
        let hdr_len = core::mem::size_of::<VirtioNetHdr>();
        if data.len() > self.capabilities.max_mtu as usize + 14
            || data.len() + hdr_len > PACKET_BUF_SIZE
        {
            return Err("virtio-net: packet too large");
        }

        let tx_queue = self.tx_queue.as_mut().unwrap();

        // Allocate a descriptor
        reap_tx_completions(tx_queue);

        let desc_idx = tx_queue
            .alloc_desc()
            .ok_or("virtio-net: no free TX descriptors")?;

        // Allocate a TX buffer from the static pool (must persist for DMA)
        let Some(buf_idx) = alloc_tx_buffer() else {
            tx_queue.free_desc(desc_idx);
            return Err("virtio-net: no free TX buffers");
        };

        // Build virtio-net header + data in the static buffer
        let mut buffers = TX_BUFFERS.lock();
        let buf = &mut buffers[buf_idx];
        let hdr = VirtioNetHdr::default();
        unsafe {
            core::ptr::copy_nonoverlapping(
                &hdr as *const VirtioNetHdr as *const u8,
                buf.as_mut_ptr(),
                core::mem::size_of::<VirtioNetHdr>(),
            );
            let data_offset = core::mem::size_of::<VirtioNetHdr>();
            let copy_len = data.len().min(PACKET_BUF_SIZE - data_offset);
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                buf.as_mut_ptr().add(data_offset),
                copy_len,
            );
        }

        let total_len = hdr_len + data.len();
        let buf_phys = super::virt_to_phys(buffers[buf_idx].as_ptr() as usize);

        tx_queue.set_desc(desc_idx, buf_phys, total_len as u32, 0, 0);
        TX_DESC_TO_BUF.lock()[desc_idx as usize] = buf_idx as u16;
        tx_queue.submit(desc_idx);
        self.transport.notify(tx_queue);

        drop(buffers);

        Ok(())
    }

    /// Check for received packets
    pub fn recv(&mut self) -> Option<Vec<u8>> {
        let rx_queue = self.rx_queue.as_mut()?;

        if !rx_queue.has_used() {
            return None;
        }

        let (id, _len) = rx_queue.pop_used()?;
        let desc_idx = id as u16;

        // Read the packet data from the buffer
        let buf_idx = desc_idx as usize;
        if buf_idx >= self.rx_buffers.len() {
            return None;
        }

        let hdr_size = core::mem::size_of::<VirtioNetHdr>();
        let packet_len = _len as usize;
        if packet_len <= hdr_size || packet_len > PACKET_BUF_SIZE {
            // Re-submit the buffer
            let buf_phys = super::virt_to_phys(self.rx_buffers[buf_idx].as_ptr() as usize);
            rx_queue.set_desc(
                desc_idx,
                buf_phys,
                PACKET_BUF_SIZE as u32,
                desc_flags::WRITE,
                0,
            );
            rx_queue.submit(desc_idx);
            self.transport.notify(rx_queue);
            return None;
        }

        let data_len = packet_len - hdr_size;
        let data = self.rx_buffers[buf_idx]
            [hdr_size..hdr_size + data_len.min(PACKET_BUF_SIZE - hdr_size)]
            .to_vec();

        // Re-submit the receive buffer
        let buf_phys = super::virt_to_phys(self.rx_buffers[buf_idx].as_ptr() as usize);
        rx_queue.set_desc(
            desc_idx,
            buf_phys,
            PACKET_BUF_SIZE as u32,
            desc_flags::WRITE,
            0,
        );
        rx_queue.submit(desc_idx);
        self.transport.notify(rx_queue);

        Some(data)
    }

    /// Check link status
    pub fn link_up(&self) -> bool {
        let status = read_config16(&self.transport, 6);
        decode_virtio_net_status(status, (self.features & VIRTIO_NET_F_STATUS) != 0).0
    }
}

fn read_config16(transport: &VirtioTransport, offset: u32) -> u16 {
    let lo = transport.read_device_config8(offset) as u16;
    let hi = transport.read_device_config8(offset + 1) as u16;
    lo | (hi << 8)
}

const TX_BUF_SENTINEL: u16 = u16::MAX;

/// Mapping from TX descriptor index to TX buffer index for cleanup.
static TX_DESC_TO_BUF: Mutex<[u16; 256]> = Mutex::new([TX_BUF_SENTINEL; 256]);

/// Global virtio-net driver instance
static VIRTIO_NET: Mutex<Option<VirtioNet>> = Mutex::new(None);

/// Initialize virtio-net from a transport
pub fn init_virtio_net(transport: VirtioTransport) -> Result<(), &'static str> {
    let net = VirtioNet::new(transport)?;

    // Register with the network stack
    let mac = net.mac_address();
    crate::serial_println!("virtio-net: registering interface with network stack");

    // Add as a network interface
    let interface = crate::net::NetworkInterface {
        name: "eth0".to_string(),
        mac_address: crate::net::NetworkAddress::mac(mac),
        ip_addresses: Vec::new(),
        netmask: crate::net::NetworkAddress::ipv4(255, 255, 255, 0),
        mtu: 1500,
        flags: crate::net::InterfaceFlags {
            up: true,
            broadcast: true,
            loopback: false,
            multicast: false,
            point_to_point: false,
        },
        stats: crate::net::InterfaceStats::default(),
    };

    if let Err(e) = crate::net::network_stack().add_interface(interface) {
        crate::serial_println!("virtio-net: failed to register interface: {:?}", e);
    }

    // Register as a NetworkDevice with the DeviceManager so the network stack
    // can route send/recv calls through the virtio-net driver.
    let device = VirtioNetDevice::new(mac);
    if let Err(e) = crate::net::device::device_manager().register_device(Box::new(device)) {
        crate::serial_println!("virtio-net: failed to register device: {:?}", e);
    }

    *VIRTIO_NET.lock() = Some(net);
    Ok(())
}

/// Get the global virtio-net driver
pub fn with_virtio_net<F, R>(f: F) -> Option<R>
where
    F: FnOnce(&mut VirtioNet) -> R,
{
    let mut guard = VIRTIO_NET.lock();
    guard.as_mut().map(f)
}

/// Check if virtio-net is initialized
pub fn is_available() -> bool {
    VIRTIO_NET.lock().is_some()
}

/// Network device adapter for virtio-net
/// Implements the NetworkDevice trait so the network stack can route packets
/// through the virtio-net driver via the DeviceManager.
pub struct VirtioNetDevice {
    name: alloc::string::String,
    mac_address: NetworkAddress,
    mtu: u16,
    up: bool,
    stats: InterfaceStats,
}

impl VirtioNetDevice {
    pub fn new(mac: MacAddress) -> Self {
        Self {
            name: alloc::string::String::from("eth0"),
            mac_address: NetworkAddress::mac(mac),
            mtu: 1500,
            up: true,
            stats: InterfaceStats::default(),
        }
    }
}

impl NetworkDevice for VirtioNetDevice {
    fn name(&self) -> &str {
        &self.name
    }

    fn device_type(&self) -> DeviceType {
        DeviceType::Ethernet
    }

    fn mac_address(&self) -> NetworkAddress {
        self.mac_address
    }

    fn capabilities(&self) -> DeviceCapabilities {
        let virtio_caps = with_virtio_net(|net| net.capabilities()).unwrap_or_default();
        let max_mtu = if virtio_caps.max_mtu == 0 {
            1500
        } else {
            virtio_caps.max_mtu
        };
        DeviceCapabilities {
            max_mtu,
            min_mtu: 68,
            hw_checksum: virtio_caps.checksum_offload,
            supports_checksum_offload: virtio_caps.checksum_offload || virtio_caps.guest_checksum,
            scatter_gather: true,
            tso: false,
            supports_tso: false,
            supports_lro: false,
            rss: virtio_caps.multiqueue,
            vlan: virtio_caps.vlan_control,
            supports_vlan: virtio_caps.vlan_control,
            jumbo_frames: false,
            supports_jumbo_frames: false,
            multicast_filter: virtio_caps.rx_mode_control,
            max_tx_queues: virtio_caps.queue_pairs.max(1),
            max_rx_queues: virtio_caps.queue_pairs.max(1),
        }
    }

    fn mtu(&self) -> u16 {
        self.mtu
    }

    fn set_mtu(&mut self, mtu: u16) -> NetworkResult<()> {
        let max_mtu = with_virtio_net(|net| net.capabilities().max_mtu).unwrap_or(1500);
        if mtu < 68 || mtu > max_mtu {
            return Err(NetworkError::InvalidArgument);
        }
        self.mtu = mtu;
        Ok(())
    }

    fn is_up(&self) -> bool {
        self.up
    }

    fn up(&mut self) -> NetworkResult<()> {
        self.up = true;
        Ok(())
    }

    fn down(&mut self) -> NetworkResult<()> {
        self.up = false;
        Ok(())
    }

    fn send(&mut self, packet: PacketBuffer) -> NetworkResult<()> {
        if !self.up {
            return Err(NetworkError::NetworkUnreachable);
        }
        let data = packet.as_slice();
        if data.is_empty() {
            return Err(NetworkError::InvalidPacket);
        }
        if data.len() > self.mtu as usize + 14 {
            return Err(NetworkError::BufferOverflow);
        }

        match with_virtio_net(|net| net.send(data)) {
            Some(Ok(())) => {
                self.stats.tx_packets += 1;
                self.stats.tx_bytes += data.len() as u64;
                Ok(())
            }
            Some(Err(e)) => {
                self.stats.tx_errors += 1;
                crate::serial_println!("virtio-net: send failed: {}", e);
                Err(NetworkError::NetworkUnreachable)
            }
            None => {
                self.stats.tx_errors += 1;
                Err(NetworkError::NetworkUnreachable)
            }
        }
    }

    fn recv(&mut self) -> NetworkResult<Option<PacketBuffer>> {
        if !self.up {
            return Ok(None);
        }

        match with_virtio_net(|net| net.recv()) {
            Some(Some(data)) => {
                self.stats.rx_packets += 1;
                self.stats.rx_bytes += data.len() as u64;
                Ok(Some(PacketBuffer::from_data(data)))
            }
            Some(None) => Ok(None),
            None => Ok(None),
        }
    }

    fn stats(&self) -> InterfaceStats {
        self.stats.clone()
    }

    fn reset_stats(&mut self) {
        self.stats = InterfaceStats::default();
    }

    fn device_info(&self) -> DeviceInfo {
        DeviceInfo {
            driver: alloc::string::String::from("virtio-net"),
            version: alloc::string::String::from("1.0.0"),
            firmware: None,
            bus_info: Some(alloc::string::String::from("virtio-pci")),
            link_modes: vec![LinkMode::Mode1000BaseTFull],
            link_speed: Some(1000),
            duplex: Some(DuplexMode::Full),
        }
    }
}
