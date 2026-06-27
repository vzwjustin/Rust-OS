//! # VirtIO Network Driver
//!
//! Implements virtio-net for paravirtualized network access in QEMU/KVM.
//! Uses virtqueues for RX (receive) and TX (transmit) with scatter-gather DMA.

use super::*;
use crate::net::{MacAddress, NetworkError};
use alloc::string::ToString;
use alloc::vec::Vec;
use spin::Mutex;

/// virtio-net feature bits
const VIRTIO_NET_F_CSUM: u64 = 1 << 0;
const VIRTIO_NET_F_GUEST_CSUM: u64 = 1 << 1;
const VIRTIO_NET_F_MAC: u64 = 1 << 5;
const VIRTIO_NET_F_MRG_RXBUF: u64 = 1 << 15;
const VIRTIO_NET_F_STATUS: u64 = 1 << 16;

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

/// Queue indices for virtio-net
const RX_QUEUE: u16 = 0;
const TX_QUEUE: u16 = 1;

/// Default queue size
const QUEUE_SIZE: u16 = 32;

/// Packet buffer size (1518 = max Ethernet frame)
const PACKET_BUF_SIZE: usize = 1518 + 12; // +12 for virtio-net header

/// VirtIO network driver state
pub struct VirtioNet {
    transport: VirtioTransport,
    mac_address: MacAddress,
    rx_queue: Option<VirtQueue>,
    tx_queue: Option<VirtQueue>,
    /// Receive buffer storage
    rx_buffers: Vec<[u8; PACKET_BUF_SIZE]>,
}

impl VirtioNet {
    /// Create and initialize a virtio-net device
    pub fn new(transport: VirtioTransport) -> Result<Self, &'static str> {
        // Negotiate features: we want MAC address support
        let driver_features = VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS | VIRTIO_NET_F_GUEST_CSUM;
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
            let notify_off =
                unsafe { core::ptr::read_volatile((transport.common_base + 30) as *const u16) };
            notify_off
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
        let tx_notify_off = {
            let notify_off =
                unsafe { core::ptr::read_volatile((transport.common_base + 30) as *const u16) };
            notify_off
        };
        let mut tx_queue = VirtQueue::new(tx_size, tx_notify_off)?;
        transport.setup_queue(&tx_queue);

        // Allocate RX buffers and fill the receive queue
        let mut rx_buffers = Vec::new();
        for i in 0..rx_size as usize {
            let buf = [0u8; PACKET_BUF_SIZE];
            rx_buffers.push(buf);

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
            rx_queue: Some(rx_queue),
            tx_queue: Some(tx_queue),
            rx_buffers,
        })
    }

    /// Get MAC address
    pub fn mac_address(&self) -> MacAddress {
        self.mac_address
    }

    /// Send a packet
    pub fn send(&mut self, data: &[u8]) -> Result<(), &'static str> {
        let tx_queue = self.tx_queue.as_mut().unwrap();

        // Allocate a descriptor
        let desc_idx = tx_queue
            .alloc_desc()
            .ok_or("virtio-net: no free TX descriptors")?;

        // Build virtio-net header + data in a temporary buffer
        let mut buf = [0u8; PACKET_BUF_SIZE];
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

        let total_len = core::mem::size_of::<VirtioNetHdr>() + data.len();
        let buf_phys = super::virt_to_phys(buf.as_ptr() as usize);

        tx_queue.set_desc(desc_idx, buf_phys, total_len as u32, 0, 0);
        tx_queue.submit(desc_idx);
        self.transport.notify(tx_queue);

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
        if packet_len <= hdr_size {
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
        if self.transport.device_base != 0 {
            let status = self.transport.read_device_config8(6) as u16;
            (status & 1) == 1
        } else {
            true
        }
    }
}

/// Global virtio-net driver instance
static VIRTIO_NET: Mutex<Option<VirtioNet>> = Mutex::new(None);

/// Initialize virtio-net from a transport
pub fn init_virtio_net(transport: VirtioTransport) -> Result<(), &'static str> {
    let mut net = VirtioNet::new(transport)?;

    // Register with the network stack
    let mac = net.mac_address();
    crate::serial_println!("virtio-net: registering interface with network stack");

    // Add as a network interface
    let interface = crate::net::NetworkInterface {
        name: "eth0".to_string(),
        mac_address: crate::net::NetworkAddress::mac(mac),
        ip_addresses: alloc::vec![],
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
