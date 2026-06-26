//! Memory Region Management
//!
//! Tracks virtual memory regions with their properties, permissions, and backing.

use alloc::boxed::Box;
use alloc::string::String;
use core::ops::Range;
use x86_64::VirtAddr;

/// Memory region type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    /// Anonymous memory (not backed by file)
    Anonymous,
    /// File-backed memory
    FileBacked,
    /// Shared memory
    Shared,
    /// Stack memory
    Stack,
    /// Heap memory
    Heap,
    /// Code (text) section
    Code,
    /// Data section
    Data,
    /// Device memory (MMIO)
    Device,
}

/// Memory protection flags (POSIX-style)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProtectionFlags {
    bits: u8,
}

impl ProtectionFlags {
    /// No access
    pub const NONE: Self = Self { bits: 0 };
    /// Read permission
    pub const READ: Self = Self { bits: 1 << 0 };
    /// Write permission
    pub const WRITE: Self = Self { bits: 1 << 1 };
    /// Execute permission
    pub const EXECUTE: Self = Self { bits: 1 << 2 };

    /// Common combinations
    pub const READ_WRITE: Self = Self {
        bits: Self::READ.bits | Self::WRITE.bits,
    };
    pub const READ_EXEC: Self = Self {
        bits: Self::READ.bits | Self::EXECUTE.bits,
    };
    pub const READ_WRITE_EXEC: Self = Self {
        bits: Self::READ.bits | Self::WRITE.bits | Self::EXECUTE.bits,
    };

    /// Create empty protection flags
    pub const fn empty() -> Self {
        Self::NONE
    }

    /// Check if flags contain specific permission
    pub const fn contains(&self, other: Self) -> bool {
        (self.bits & other.bits) == other.bits
    }

    /// Check if readable
    pub const fn is_readable(&self) -> bool {
        self.contains(Self::READ)
    }

    /// Check if writable
    pub const fn is_writable(&self) -> bool {
        self.contains(Self::WRITE)
    }

    /// Check if executable
    pub const fn is_executable(&self) -> bool {
        self.contains(Self::EXECUTE)
    }

    /// Combine flags
    pub const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    /// Remove flags
    pub const fn difference(self, other: Self) -> Self {
        Self {
            bits: self.bits & !other.bits,
        }
    }
}

impl core::ops::BitOr for ProtectionFlags {
    type Output = Self;

    fn bitor(self, rhs: Self) -> Self::Output {
        self.union(rhs)
    }
}

impl core::ops::BitOrAssign for ProtectionFlags {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

impl core::ops::BitAnd for ProtectionFlags {
    type Output = Self;

    fn bitand(self, rhs: Self) -> Self::Output {
        Self {
            bits: self.bits & rhs.bits,
        }
    }
}

/// Memory region descriptor
#[derive(Debug, Clone)]
pub struct MemoryRegion {
    /// Virtual address range
    pub start: VirtAddr,
    pub end: VirtAddr,
    /// Protection flags
    pub protection: ProtectionFlags,
    /// Memory type
    pub memory_type: MemoryType,
    /// Whether this is a shared mapping
    pub shared: bool,
    /// Copy-on-write flag
    pub copy_on_write: bool,
    /// File descriptor for file-backed regions
    pub file_descriptor: Option<usize>,
    /// Offset in file for file-backed regions
    pub file_offset: usize,
    /// Region name (for debugging)
    pub name: Option<String>,
}

impl MemoryRegion {
    /// Create a new memory region
    pub fn new(
        start: VirtAddr,
        end: VirtAddr,
        protection: ProtectionFlags,
        memory_type: MemoryType,
    ) -> Self {
        Self {
            start,
            end,
            protection,
            memory_type,
            shared: false,
            copy_on_write: false,
            file_descriptor: None,
            file_offset: 0,
            name: None,
        }
    }

    /// Create an anonymous memory region
    pub fn anonymous(start: VirtAddr, end: VirtAddr, protection: ProtectionFlags) -> Self {
        Self::new(start, end, protection, MemoryType::Anonymous)
    }

    /// Create a file-backed memory region
    pub fn file_backed(
        start: VirtAddr,
        end: VirtAddr,
        protection: ProtectionFlags,
        fd: usize,
        offset: usize,
    ) -> Self {
        let mut region = Self::new(start, end, protection, MemoryType::FileBacked);
        region.file_descriptor = Some(fd);
        region.file_offset = offset;
        region
    }

    /// Create a shared memory region
    pub fn shared(start: VirtAddr, end: VirtAddr, protection: ProtectionFlags) -> Self {
        let mut region = Self::new(start, end, protection, MemoryType::Shared);
        region.shared = true;
        region
    }

    /// Get the size of the region in bytes
    pub fn size(&self) -> usize {
        (self.end.as_u64() - self.start.as_u64()) as usize
    }

    /// Get the size in pages
    pub fn size_in_pages(&self) -> usize {
        (self.size() + 4095) / 4096
    }

    /// Check if address is within region
    pub fn contains(&self, addr: VirtAddr) -> bool {
        addr >= self.start && addr < self.end
    }

    /// Check if region overlaps with another
    pub fn overlaps(&self, other: &MemoryRegion) -> bool {
        self.start < other.end && other.start < self.end
    }

    /// Check if region overlaps with address range
    pub fn overlaps_range(&self, start: VirtAddr, end: VirtAddr) -> bool {
        self.start < end && start < self.end
    }

    /// Split region at address
    pub fn split_at(&self, addr: VirtAddr) -> Option<(MemoryRegion, MemoryRegion)> {
        if !self.contains(addr) || addr == self.start {
            return None;
        }

        let mut first = self.clone();
        let mut second = self.clone();

        first.end = addr;
        second.start = addr;

        // Adjust file offset for second region if file-backed
        if let Some(_fd) = self.file_descriptor {
            let offset_increase = (addr.as_u64() - self.start.as_u64()) as usize;
            second.file_offset += offset_increase;
        }

        Some((first, second))
    }

    /// Merge with another region if contiguous and compatible
    pub fn try_merge(&self, other: &MemoryRegion) -> Option<MemoryRegion> {
        // Check if contiguous
        if self.end != other.start && other.end != self.start {
            return None;
        }

        // Check if properties match
        if self.protection != other.protection
            || self.memory_type != other.memory_type
            || self.shared != other.shared
            || self.copy_on_write != other.copy_on_write
        {
            return None;
        }

        // Check file-backed properties
        if self.file_descriptor != other.file_descriptor {
            return None;
        }

        if let Some(_fd) = self.file_descriptor {
            // Check if file offsets are contiguous
            let expected_offset = if self.end == other.start {
                self.file_offset + self.size()
            } else {
                other.file_offset + other.size()
            };

            let actual_offset = if self.end == other.start {
                other.file_offset
            } else {
                self.file_offset
            };

            if expected_offset != actual_offset {
                return None;
            }
        }

        // Create merged region
        let (start, end) = if self.start < other.start {
            (self.start, other.end)
        } else {
            (other.start, self.end)
        };

        let mut merged = self.clone();
        merged.start = start;
        merged.end = end;

        Some(merged)
    }

    /// Change protection flags
    pub fn set_protection(&mut self, protection: ProtectionFlags) {
        self.protection = protection;
    }

    /// Mark as copy-on-write
    pub fn set_copy_on_write(&mut self, cow: bool) {
        self.copy_on_write = cow;
    }

    /// Set region name
    pub fn set_name(&mut self, name: String) {
        self.name = Some(name);
    }

    /// Get address range
    pub fn range(&self) -> Range<u64> {
        self.start.as_u64()..self.end.as_u64()
    }

    /// Check if region is valid
    pub fn is_valid(&self) -> bool {
        self.start < self.end && self.start.as_u64() % 4096 == 0
    }
}

/// Memory region tree node for efficient lookup
#[derive(Debug)]
pub struct MemoryRegionNode {
    pub region: MemoryRegion,
    pub left: Option<Box<MemoryRegionNode>>,
    pub right: Option<Box<MemoryRegionNode>>,
}

impl MemoryRegionNode {
    /// Create a new node
    pub fn new(region: MemoryRegion) -> Self {
        Self {
            region,
            left: None,
            right: None,
        }
    }

    /// Insert a region into the tree
    pub fn insert(&mut self, region: MemoryRegion) -> Result<(), &'static str> {
        if region.overlaps(&self.region) {
            return Err("Region overlaps with existing region");
        }

        if region.start < self.region.start {
            if let Some(ref mut left) = self.left {
                left.insert(region)
            } else {
                self.left = Some(Box::new(MemoryRegionNode::new(region)));
                Ok(())
            }
        } else {
            if let Some(ref mut right) = self.right {
                right.insert(region)
            } else {
                self.right = Some(Box::new(MemoryRegionNode::new(region)));
                Ok(())
            }
        }
    }

    /// Find region containing address
    pub fn find(&self, addr: VirtAddr) -> Option<&MemoryRegion> {
        if self.region.contains(addr) {
            Some(&self.region)
        } else if addr < self.region.start {
            self.left.as_ref().and_then(|left| left.find(addr))
        } else {
            self.right.as_ref().and_then(|right| right.find(addr))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_protection_flags() {
        let rw = ProtectionFlags::READ | ProtectionFlags::WRITE;
        assert!(rw.is_readable());
        assert!(rw.is_writable());
        assert!(!rw.is_executable());

        let rwx = rw | ProtectionFlags::EXECUTE;
        assert!(rwx.is_executable());
    }

    #[test_case]
    fn test_memory_region() {
        let start = VirtAddr::new(0x1000);
        let end = VirtAddr::new(0x2000);
        let region = MemoryRegion::anonymous(start, end, ProtectionFlags::READ_WRITE);

        assert_eq!(region.size(), 0x1000);
        assert!(region.contains(VirtAddr::new(0x1500)));
        assert!(!region.contains(VirtAddr::new(0x2500)));
    }

    #[test_case]
    fn test_region_split() {
        let start = VirtAddr::new(0x1000);
        let end = VirtAddr::new(0x3000);
        let region = MemoryRegion::anonymous(start, end, ProtectionFlags::READ_WRITE);

        let split_addr = VirtAddr::new(0x2000);
        let (first, second) = region.split_at(split_addr).unwrap();

        assert_eq!(first.end, split_addr);
        assert_eq!(second.start, split_addr);
        assert_eq!(first.size() + second.size(), region.size());
    }

    #[test_case]
    fn test_region_merge() {
        let region1 = MemoryRegion::anonymous(
            VirtAddr::new(0x1000),
            VirtAddr::new(0x2000),
            ProtectionFlags::READ_WRITE,
        );
        let region2 = MemoryRegion::anonymous(
            VirtAddr::new(0x2000),
            VirtAddr::new(0x3000),
            ProtectionFlags::READ_WRITE,
        );

        let merged = region1.try_merge(&region2).unwrap();
        assert_eq!(merged.start, VirtAddr::new(0x1000));
        assert_eq!(merged.end, VirtAddr::new(0x3000));
    }
}
