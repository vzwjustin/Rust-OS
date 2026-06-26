//! # DMA Buffer Management for Network Operations
//!
//! This module provides Direct Memory Access (DMA) buffer management
//! for efficient network packet transmission and reception.

use alloc::{boxed::Box, vec::Vec};
use core::ptr;
use lazy_static::lazy_static;
use spin::{Mutex, RwLock};

use super::{NetworkError, NetworkResult, PacketBuffer};

/// DMA buffer alignment requirement (typically 64 bytes for optimal performance)
pub const DMA_ALIGNMENT: usize = 64;

/// Maximum DMA buffer size
pub const MAX_DMA_BUFFER_SIZE: usize = 65536;

/// DMA buffer descriptor for hardware
#[repr(C, align(16))]
#[derive(Debug, Clone, Copy)]
pub struct DmaDescriptor {
    /// Physical address of buffer
    pub buffer_addr: u64,
    /// Buffer length
    pub length: u16,
    /// Control flags
    pub flags: u16,
    /// Status flags (updated by hardware)
    pub status: u16,
    /// Reserved field
    pub reserved: u16,
}

impl DmaDescriptor {
    /// Create new DMA descriptor
    pub fn new(buffer_addr: u64, length: u16) -> Self {
        Self {
            buffer_addr,
            length,
            flags: 0,
            status: 0,
            reserved: 0,
        }
    }

    /// Set end of packet flag
    pub fn set_eop(&mut self) {
        self.flags |= 1 << 0;
    }

    /// Set interrupt on completion flag
    pub fn set_interrupt(&mut self) {
        self.flags |= 1 << 1;
    }

    /// Check if descriptor is done (processed by hardware)
    pub fn is_done(&self) -> bool {
        (self.status & (1 << 0)) != 0
    }

    /// Get error status
    pub fn has_error(&self) -> bool {
        (self.status & (1 << 1)) != 0
    }
}

/// DMA-coherent buffer for network operations
#[derive(Debug)]
pub struct DmaBuffer {
    /// Virtual address of buffer
    virtual_addr: *mut u8,
    /// Physical address of buffer (for hardware)
    physical_addr: u64,
    /// Buffer size
    size: usize,
    /// Buffer alignment
    alignment: usize,
}

impl DmaBuffer {
    /// Allocate DMA-coherent buffer
    pub fn allocate(size: usize, alignment: usize) -> NetworkResult<Self> {
        if size == 0 || size > MAX_DMA_BUFFER_SIZE {
            return Err(NetworkError::InvalidArgument);
        }

        // Align size to alignment boundary
        let aligned_size = (size + alignment - 1) & !(alignment - 1);

        // Allocate aligned memory
        // In a real implementation, this would use DMA-coherent allocation
        let layout = core::alloc::Layout::from_size_align(aligned_size, alignment)
            .map_err(|_| NetworkError::InsufficientMemory)?;

        let virtual_addr = unsafe { alloc::alloc::alloc_zeroed(layout) };

        if virtual_addr.is_null() {
            return Err(NetworkError::InsufficientMemory);
        }

        // Get physical address from virtual address using memory manager
        let physical_addr = {
            use crate::memory::get_memory_manager;
            use x86_64::VirtAddr;

            let virt_addr = VirtAddr::new(virtual_addr as u64);
            let memory_manager = get_memory_manager().ok_or(NetworkError::InternalError)?;

            memory_manager
                .translate_addr(virt_addr)
                .ok_or(NetworkError::InternalError)?
                .as_u64()
        };

        Ok(Self {
            virtual_addr,
            physical_addr,
            size: aligned_size,
            alignment,
        })
    }

    /// Get virtual address
    pub fn virtual_addr(&self) -> *mut u8 {
        self.virtual_addr
    }

    /// Get physical address
    pub fn physical_addr(&self) -> u64 {
        self.physical_addr
    }

    /// Get buffer size
    pub fn size(&self) -> usize {
        self.size
    }

    /// Get buffer as slice
    pub fn as_slice(&self) -> &[u8] {
        unsafe { core::slice::from_raw_parts(self.virtual_addr, self.size) }
    }

    /// Get buffer as mutable slice
    pub fn as_mut_slice(&mut self) -> &mut [u8] {
        unsafe { core::slice::from_raw_parts_mut(self.virtual_addr, self.size) }
    }

    /// Copy data to DMA buffer
    pub fn copy_from_slice(&mut self, data: &[u8]) -> NetworkResult<()> {
        if data.len() > self.size {
            return Err(NetworkError::BufferOverflow);
        }

        unsafe {
            ptr::copy_nonoverlapping(data.as_ptr(), self.virtual_addr, data.len());
        }

        Ok(())
    }

    /// Copy data from DMA buffer
    pub fn copy_to_slice(&self, data: &mut [u8]) -> usize {
        let copy_len = core::cmp::min(data.len(), self.size);

        unsafe {
            ptr::copy_nonoverlapping(self.virtual_addr, data.as_mut_ptr(), copy_len);
        }

        copy_len
    }

    /// Ensure cache coherency (flush to memory)
    pub fn flush_cache(&self) {
        // In real implementation, this would flush CPU cache to ensure
        // hardware sees the latest data
        unsafe {
            core::arch::x86_64::_mm_mfence();
        }
    }

    /// Invalidate cache (ensure CPU sees latest hardware updates)
    pub fn invalidate_cache(&self) {
        // In real implementation, this would invalidate CPU cache to ensure
        // CPU sees the latest hardware updates
        unsafe {
            core::arch::x86_64::_mm_mfence();
        }
    }
}

impl Drop for DmaBuffer {
    fn drop(&mut self) {
        if !self.virtual_addr.is_null() {
            let layout = core::alloc::Layout::from_size_align(self.size, self.alignment)
                .expect("Invalid layout");
            unsafe {
                alloc::alloc::dealloc(self.virtual_addr, layout);
            }
        }
    }
}

// SAFETY: DmaBuffer owns its memory and ensures proper cleanup via Drop.
// The raw pointer is just an implementation detail for DMA memory management.
// Multiple threads can safely own separate DmaBuffers.
unsafe impl Send for DmaBuffer {}

// SAFETY: DmaBuffer provides interior mutability through its methods,
// and all access to the underlying memory is properly synchronized.
// Multiple threads can safely share references to a DmaBuffer.
unsafe impl Sync for DmaBuffer {}

/// DMA ring buffer for efficient packet processing
#[derive(Debug)]
pub struct DmaRing {
    /// DMA descriptors
    descriptors: Vec<DmaDescriptor>,
    /// DMA buffers
    buffers: Vec<DmaBuffer>,
    /// Current head index (next to process)
    head: usize,
    /// Current tail index (next to fill)
    tail: usize,
    /// Ring size
    size: usize,
    /// Buffer size for each entry
    buffer_size: usize,
}

impl DmaRing {
    /// Create new DMA ring
    pub fn new(ring_size: usize, buffer_size: usize) -> NetworkResult<Self> {
        let mut descriptors = Vec::with_capacity(ring_size);
        let mut buffers = Vec::with_capacity(ring_size);

        // Allocate DMA buffers and create descriptors
        for _ in 0..ring_size {
            let buffer = DmaBuffer::allocate(buffer_size, DMA_ALIGNMENT)?;
            let descriptor = DmaDescriptor::new(buffer.physical_addr(), buffer_size as u16);

            descriptors.push(descriptor);
            buffers.push(buffer);
        }

        Ok(Self {
            descriptors,
            buffers,
            head: 0,
            tail: 0,
            size: ring_size,
            buffer_size,
        })
    }

    /// Get next available descriptor for transmission
    pub fn get_tx_descriptor(&mut self) -> Option<(&mut DmaDescriptor, &mut DmaBuffer)> {
        let next_tail = (self.tail + 1) % self.size;

        // Check if ring is full
        if next_tail == self.head {
            return None;
        }

        let descriptor = &mut self.descriptors[self.tail];
        let buffer = &mut self.buffers[self.tail];

        Some((descriptor, buffer))
    }

    /// Advance tail pointer after filling descriptor
    pub fn advance_tail(&mut self) {
        self.tail = (self.tail + 1) % self.size;
    }

    /// Get next completed descriptor for reception
    pub fn get_rx_descriptor(&mut self) -> Option<(&mut DmaDescriptor, &mut DmaBuffer)> {
        let descriptor = &mut self.descriptors[self.head];

        // Check if descriptor is completed by hardware
        if !descriptor.is_done() {
            return None;
        }

        let buffer = &mut self.buffers[self.head];
        Some((descriptor, buffer))
    }

    /// Advance head pointer after processing descriptor
    pub fn advance_head(&mut self) {
        self.head = (self.head + 1) % self.size;
    }

    /// Get physical address of descriptor ring
    pub fn descriptor_ring_addr(&self) -> u64 {
        self.descriptors.as_ptr() as u64
    }

    /// Get ring size
    pub fn size(&self) -> usize {
        self.size
    }

    /// Check if ring is empty
    pub fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    /// Check if ring is full
    pub fn is_full(&self) -> bool {
        (self.tail + 1) % self.size == self.head
    }

    /// Get number of available slots
    pub fn available_slots(&self) -> usize {
        if self.tail >= self.head {
            self.size - (self.tail - self.head) - 1
        } else {
            self.head - self.tail - 1
        }
    }

    /// Get number of used slots
    pub fn used_slots(&self) -> usize {
        if self.tail >= self.head {
            self.tail - self.head
        } else {
            self.size - (self.head - self.tail)
        }
    }
}

/// Hardware-specific packet formatting
pub trait PacketFormatter {
    /// Format packet for hardware transmission
    fn format_tx_packet(
        &self,
        packet: &PacketBuffer,
        dma_buffer: &mut DmaBuffer,
    ) -> NetworkResult<u16>;

    /// Parse received packet from hardware
    fn parse_rx_packet(&self, dma_buffer: &DmaBuffer, length: u16) -> NetworkResult<PacketBuffer>;

    /// Calculate hardware checksum
    fn calculate_hw_checksum(&self, data: &[u8]) -> u16;

    /// Validate hardware checksum
    fn validate_hw_checksum(&self, data: &[u8], checksum: u16) -> bool;
}

/// Ethernet packet formatter
pub struct EthernetFormatter;

impl PacketFormatter for EthernetFormatter {
    fn format_tx_packet(
        &self,
        packet: &PacketBuffer,
        dma_buffer: &mut DmaBuffer,
    ) -> NetworkResult<u16> {
        let packet_data = packet.as_slice();

        // Validate minimum Ethernet frame size
        if packet_data.len() < 14 {
            return Err(NetworkError::InvalidPacket);
        }

        // Copy packet data to DMA buffer
        dma_buffer.copy_from_slice(packet_data)?;

        // Pad to minimum frame size if necessary
        let mut frame_size = packet_data.len();
        if frame_size < 60 {
            // Minimum Ethernet frame size (without CRC)
            let padding_size = 60 - frame_size;
            let buffer_slice = dma_buffer.as_mut_slice();

            // Zero-fill padding
            for i in frame_size..frame_size + padding_size {
                if i < buffer_slice.len() {
                    buffer_slice[i] = 0;
                }
            }
            frame_size = 60;
        }

        // Ensure cache coherency
        dma_buffer.flush_cache();

        Ok(frame_size as u16)
    }

    fn parse_rx_packet(&self, dma_buffer: &DmaBuffer, length: u16) -> NetworkResult<PacketBuffer> {
        if length < 14 {
            return Err(NetworkError::InvalidPacket);
        }

        // Ensure cache coherency
        dma_buffer.invalidate_cache();

        // Copy data from DMA buffer
        let buffer_data = &dma_buffer.as_slice()[..length as usize];
        let packet = PacketBuffer::from_data(buffer_data.to_vec());

        Ok(packet)
    }

    fn calculate_hw_checksum(&self, data: &[u8]) -> u16 {
        let mut sum = 0u32;

        // Calculate Internet checksum
        for chunk in data.chunks(2) {
            if chunk.len() == 2 {
                sum += ((chunk[0] as u32) << 8) | (chunk[1] as u32);
            } else {
                sum += (chunk[0] as u32) << 8;
            }
        }

        // Fold 32-bit sum to 16 bits
        while (sum >> 16) != 0 {
            sum = (sum & 0xFFFF) + (sum >> 16);
        }

        !sum as u16
    }

    fn validate_hw_checksum(&self, data: &[u8], expected_checksum: u16) -> bool {
        let calculated = self.calculate_hw_checksum(data);
        calculated == expected_checksum
    }
}

/// DMA operations for network devices
pub struct DmaOperations {
    /// Transmit ring
    tx_ring: Mutex<DmaRing>,
    /// Receive ring
    rx_ring: Mutex<DmaRing>,
    /// Packet formatter
    formatter: Box<dyn PacketFormatter + Send + Sync>,
    /// Statistics
    stats: RwLock<DmaStats>,
}

impl DmaOperations {
    /// Create new DMA operations
    pub fn new(
        tx_ring_size: usize,
        rx_ring_size: usize,
        buffer_size: usize,
        formatter: Box<dyn PacketFormatter + Send + Sync>,
    ) -> NetworkResult<Self> {
        let tx_ring = DmaRing::new(tx_ring_size, buffer_size)?;
        let rx_ring = DmaRing::new(rx_ring_size, buffer_size)?;

        Ok(Self {
            tx_ring: Mutex::new(tx_ring),
            rx_ring: Mutex::new(rx_ring),
            formatter,
            stats: RwLock::new(DmaStats::default()),
        })
    }

    /// Transmit packet using DMA
    pub fn transmit_packet(&self, packet: PacketBuffer) -> NetworkResult<()> {
        let mut tx_ring = self.tx_ring.lock();
        let mut stats = self.stats.write();

        // Get next available descriptor
        let (descriptor, dma_buffer) = tx_ring.get_tx_descriptor().ok_or(NetworkError::Busy)?;

        // Format packet for hardware
        let frame_size = self.formatter.format_tx_packet(&packet, dma_buffer)?;

        // Setup descriptor
        descriptor.length = frame_size;
        descriptor.set_eop();
        descriptor.flags |= 1 << 2; // Ready for transmission

        // Advance tail pointer
        tx_ring.advance_tail();

        // Update statistics
        stats.tx_packets += 1;
        stats.tx_bytes += frame_size as u64;

        Ok(())
    }

    /// Receive packet using DMA
    pub fn receive_packet(&self) -> NetworkResult<Option<PacketBuffer>> {
        let mut rx_ring = self.rx_ring.lock();
        let mut stats = self.stats.write();

        // Get next completed descriptor
        if let Some((descriptor, dma_buffer)) = rx_ring.get_rx_descriptor() {
            // Extract needed data from descriptor before releasing the borrow
            let has_error = descriptor.has_error();
            let length = descriptor.length;

            // Reset descriptor for reuse
            descriptor.status = 0;
            descriptor.flags = 1 << 2; // Ready for reception

            // Check for errors
            if has_error {
                stats.rx_errors += 1;
                rx_ring.advance_head();
                return Err(NetworkError::InvalidPacket);
            }

            // Parse received packet
            let packet = self.formatter.parse_rx_packet(dma_buffer, length)?;

            // Advance head pointer
            rx_ring.advance_head();

            // Update statistics
            stats.rx_packets += 1;
            stats.rx_bytes += length as u64;

            Ok(Some(packet))
        } else {
            Ok(None)
        }
    }

    /// Get DMA statistics
    pub fn get_stats(&self) -> DmaStats {
        self.stats.read().clone()
    }

    /// Get transmit ring physical address
    pub fn get_tx_ring_addr(&self) -> u64 {
        self.tx_ring.lock().descriptor_ring_addr()
    }

    /// Get receive ring physical address
    pub fn get_rx_ring_addr(&self) -> u64 {
        self.rx_ring.lock().descriptor_ring_addr()
    }

    /// Get ring sizes
    pub fn get_ring_sizes(&self) -> (usize, usize) {
        let tx_ring = self.tx_ring.lock();
        let rx_ring = self.rx_ring.lock();
        (tx_ring.size(), rx_ring.size())
    }
}

/// DMA statistics
#[derive(Debug, Default, Clone)]
pub struct DmaStats {
    pub tx_packets: u64,
    pub tx_bytes: u64,
    pub rx_packets: u64,
    pub rx_bytes: u64,
    pub tx_errors: u64,
    pub rx_errors: u64,
    pub tx_ring_full: u64,
    pub rx_ring_empty: u64,
}

/// Create DMA operations for Ethernet
pub fn create_ethernet_dma(
    tx_ring_size: usize,
    rx_ring_size: usize,
    buffer_size: usize,
) -> NetworkResult<DmaOperations> {
    let formatter = Box::new(EthernetFormatter);
    DmaOperations::new(tx_ring_size, rx_ring_size, buffer_size, formatter)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[cfg(feature = "disabled-tests")]
    #[test_case]
    fn test_dma_buffer_allocation() {
        let buffer = DmaBuffer::allocate(1024, DMA_ALIGNMENT).unwrap();
        assert_eq!(buffer.size(), 1024);
        assert!(!buffer.virtual_addr().is_null());
    }

    #[test_case]
    fn test_dma_descriptor() {
        let mut desc = DmaDescriptor::new(0x1000, 1500);
        assert_eq!(desc.buffer_addr, 0x1000);
        assert_eq!(desc.length, 1500);

        desc.set_eop();
        assert_eq!(desc.flags & 1, 1);
    }

    #[test_case]
    fn test_ethernet_formatter() {
        let formatter = EthernetFormatter;
        let data = vec![0u8; 64]; // Valid Ethernet frame
        let checksum = formatter.calculate_hw_checksum(&data);
        assert!(formatter.validate_hw_checksum(&data, checksum));
    }
}
