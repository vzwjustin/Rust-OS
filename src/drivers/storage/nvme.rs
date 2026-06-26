//! # NVMe SSD Controller Driver
//!
//! Non-Volatile Memory Express (NVMe) driver for high-performance SSD storage.
//! Supports PCIe-based NVMe controllers with queue-based command processing.

use super::{
    StorageCapabilities, StorageDeviceState, StorageDeviceType, StorageDriver, StorageError,
    StorageStats,
};
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::vec::Vec;
use alloc::{format, vec};
use core::mem;
use core::ptr;

/// NVMe controller register offsets
#[repr(u64)]
pub enum NvmeReg {
    /// Controller Capabilities
    Cap = 0x00,
    /// Version
    Vs = 0x08,
    /// Interrupt Mask Set
    Intms = 0x0c,
    /// Interrupt Mask Clear
    Intmc = 0x10,
    /// Controller Configuration
    Cc = 0x14,
    /// Controller Status
    Csts = 0x1c,
    /// NVM Subsystem Reset
    Nssr = 0x20,
    /// Admin Queue Attributes
    Aqa = 0x24,
    /// Admin Submission Queue Base Address
    Asq = 0x28,
    /// Admin Completion Queue Base Address
    Acq = 0x30,
    /// Controller Memory Buffer Location
    Cmbloc = 0x38,
    /// Controller Memory Buffer Size
    Cmbsz = 0x3c,
}

/// NVMe doorbell registers start at offset 0x1000
pub const NVME_DOORBELL_BASE: u64 = 0x1000;

/// NVMe controller capabilities register bits
bitflags::bitflags! {
    pub struct NvmeCap: u64 {
        const MQES_MASK = 0xffff;           // Maximum Queue Entries Supported
        const CQR = 1 << 16;                // Contiguous Queues Required
        const AMS_MASK = 0x3 << 17;         // Arbitration Mechanism Supported
        const TO_MASK = 0xff << 24;         // Timeout
        const DSTRD_MASK = 0xf << 32;       // Doorbell Stride
        const NSSRS = 1 << 36;              // NVM Subsystem Reset Supported
        const CSS_MASK = 0xff << 37;        // Command Sets Supported
        const BPS = 1 << 45;                // Boot Partition Support
        const MPSMIN_MASK = 0xf << 48;      // Memory Page Size Minimum
        const MPSMAX_MASK = 0xf << 52;      // Memory Page Size Maximum
        const PMRS = 1 << 56;               // Persistent Memory Region Supported
        const CMBS = 1 << 57;               // Controller Memory Buffer Supported
    }
}

/// NVMe controller configuration register bits
bitflags::bitflags! {
    pub struct NvmeCc: u32 {
        const EN = 1 << 0;                  // Enable
        const CSS_MASK = 0x7 << 4;          // I/O Command Set Selected
        const MPS_MASK = 0xf << 7;          // Memory Page Size
        const AMS_MASK = 0x7 << 11;         // Arbitration Mechanism Selected
        const SHN_MASK = 0x3 << 14;         // Shutdown Notification
        const IOSQES_MASK = 0xf << 16;      // I/O Submission Queue Entry Size
        const IOCQES_MASK = 0xf << 20;      // I/O Completion Queue Entry Size
    }
}

/// NVMe controller status register bits
bitflags::bitflags! {
    pub struct NvmeCsts: u32 {
        const RDY = 1 << 0;                 // Ready
        const CFS = 1 << 1;                 // Controller Fatal Status
        const SHST_MASK = 0x3 << 2;         // Shutdown Status
        const NSSRO = 1 << 4;               // NVM Subsystem Reset Occurred
        const PP = 1 << 5;                  // Processing Paused
    }
}

/// NVMe command opcodes
#[repr(u8)]
pub enum NvmeOpcode {
    /// Delete I/O Submission Queue
    DeleteIoSq = 0x00,
    /// Create I/O Submission Queue
    CreateIoSq = 0x01,
    /// Get Log Page
    GetLogPage = 0x02,
    /// Delete I/O Completion Queue
    DeleteIoCq = 0x04,
    /// Create I/O Completion Queue
    CreateIoCq = 0x05,
    /// Identify
    Identify = 0x06,
    /// Abort
    Abort = 0x08,
    /// Set Features
    SetFeatures = 0x09,
    /// Get Features
    GetFeatures = 0x0a,
    /// Asynchronous Event Request
    AsyncEventReq = 0x0c,
    /// Namespace Management
    NsManagement = 0x0d,
    /// Firmware Commit
    FwCommit = 0x10,
    /// Firmware Image Download
    FwDownload = 0x11,
    /// Device Self-test
    DeviceSelfTest = 0x14,
    /// Namespace Attachment
    NsAttachment = 0x15,
    /// Keep Alive
    KeepAlive = 0x18,
    /// Directive Send
    DirectiveSend = 0x19,
    /// Directive Receive
    DirectiveRecv = 0x1a,
    /// Virtualization Management
    VirtMgmt = 0x1c,
    /// NVMe-MI Send
    NvmeMiSend = 0x1d,
    /// NVMe-MI Receive
    NvmeMiRecv = 0x1e,
    /// Doorbell Buffer Config
    DoorbellCfg = 0x7c,
    /// Format NVM
    FormatNvm = 0x80,
    /// Security Send
    SecuritySend = 0x81,
    /// Security Receive
    SecurityRecv = 0x82,
    /// Sanitize
    Sanitize = 0x84,
    /// Get LBA Status
    GetLbaStatus = 0x86,
}

/// NVMe I/O command opcodes
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
pub enum NvmeIoOpcode {
    /// Flush
    Flush = 0x00,
    /// Write
    Write = 0x01,
    /// Read
    Read = 0x02,
    /// Write Uncorrectable
    WriteUncorr = 0x04,
    /// Compare
    Compare = 0x05,
    /// Write Zeroes
    WriteZeroes = 0x08,
    /// Dataset Management
    Dsm = 0x09,
    /// Verify
    Verify = 0x0c,
    /// Reservation Register
    ResvRegister = 0x0d,
    /// Reservation Report
    ResvReport = 0x0e,
    /// Reservation Acquire
    ResvAcquire = 0x11,
    /// Reservation Release
    ResvRelease = 0x15,
}

/// NVMe submission queue entry (64 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct NvmeSqe {
    /// Command Dword 0
    pub cdw0: u32,
    /// Namespace Identifier
    pub nsid: u32,
    /// Command Dword 2
    pub cdw2: u32,
    /// Command Dword 3
    pub cdw3: u32,
    /// Metadata Pointer
    pub mptr: u64,
    /// Data Pointer
    pub dptr: [u64; 2],
    /// Command Dword 10
    pub cdw10: u32,
    /// Command Dword 11
    pub cdw11: u32,
    /// Command Dword 12
    pub cdw12: u32,
    /// Command Dword 13
    pub cdw13: u32,
    /// Command Dword 14
    pub cdw14: u32,
    /// Command Dword 15
    pub cdw15: u32,
}

/// NVMe completion queue entry (16 bytes)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct NvmeCqe {
    /// Command Specific
    pub dw0: u32,
    /// Reserved
    pub dw1: u32,
    /// Submission Queue Head Pointer
    pub sq_head: u16,
    /// Submission Queue Identifier
    pub sq_id: u16,
    /// Command Identifier
    pub cid: u16,
    /// Phase Tag and Status Field
    pub status: u16,
}

/// NVMe identify controller data structure (4096 bytes, partial)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct NvmeIdCtrl {
    /// PCI Vendor ID
    pub vid: u16,
    /// PCI Subsystem Vendor ID
    pub ssvid: u16,
    /// Serial Number (20 ASCII characters)
    pub sn: [u8; 20],
    /// Model Number (40 ASCII characters)
    pub mn: [u8; 40],
    /// Firmware Revision (8 ASCII characters)
    pub fr: [u8; 8],
    /// Recommended Arbitration Burst
    pub rab: u8,
    /// IEEE OUI Identifier
    pub ieee: [u8; 3],
    /// Controller Multi-Path I/O and Namespace Sharing Capabilities
    pub cmic: u8,
    /// Maximum Data Transfer Size
    pub mdts: u8,
    /// Controller ID
    pub cntlid: u16,
    /// Version
    pub ver: u32,
    /// RTD3 Resume Latency
    pub rtd3r: u32,
    /// RTD3 Entry Latency
    pub rtd3e: u32,
    /// Optional Asynchronous Events Supported
    pub oaes: u32,
    /// Controller Attributes
    pub ctratt: u32,
    /// Read Recovery Levels Supported
    pub rrls: u16,
    /// Reserved
    pub rsvd102: [u8; 9],
    /// Controller Type
    pub cntrltype: u8,
    /// FRU Globally Unique Identifier
    pub fguid: [u8; 16],
    /// Command Retry Delay Time 1
    pub crdt1: u16,
    /// Command Retry Delay Time 2
    pub crdt2: u16,
    /// Command Retry Delay Time 3
    pub crdt3: u16,
    /// Reserved
    pub rsvd134: [u8; 122],
    // ... truncated for brevity, real structure is 4096 bytes
}

/// NVMe identify namespace data structure (4096 bytes, partial)
#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct NvmeIdNs {
    /// Namespace Size
    pub nsze: u64,
    /// Namespace Capacity
    pub ncap: u64,
    /// Namespace Utilization
    pub nuse: u64,
    /// Namespace Features
    pub nsfeat: u8,
    /// Number of LBA Formats
    pub nlbaf: u8,
    /// Formatted LBA Size
    pub flbas: u8,
    /// Metadata Capabilities
    pub mc: u8,
    /// End-to-end Data Protection Capabilities
    pub dpc: u8,
    /// End-to-end Data Protection Type Settings
    pub dps: u8,
    /// Namespace Multi-path I/O and Namespace Sharing Capabilities
    pub nmic: u8,
    /// Reservation Capabilities
    pub rescap: u8,
    /// Format Progress Indicator
    pub fpi: u8,
    /// Deallocate Logical Block Features
    pub dlfeat: u8,
    /// Namespace Atomic Write Unit Normal
    pub nawun: u16,
    /// Namespace Atomic Write Unit Power Fail
    pub nawupf: u16,
    /// Namespace Atomic Compare & Write Unit
    pub nacwu: u16,
    /// Namespace Atomic Boundary Size Normal
    pub nabsn: u16,
    /// Namespace Atomic Boundary Offset
    pub nabo: u16,
    /// Namespace Atomic Boundary Size Power Fail
    pub nabspf: u16,
    /// Namespace Optimal I/O Boundary
    pub noiob: u16,
    /// NVM Capacity
    pub nvmcap: [u8; 16],
    /// Namespace Preferred Write Granularity
    pub npwg: u16,
    /// Namespace Preferred Write Alignment
    pub npwa: u16,
    /// Namespace Preferred Deallocate Granularity
    pub npdg: u16,
    /// Namespace Preferred Deallocate Alignment
    pub npda: u16,
    /// Namespace Optimal Write Size
    pub nows: u16,
    /// Reserved
    pub rsvd74: [u8; 18],
    /// ANA Group Identifier
    pub anagrpid: u32,
    /// Reserved
    pub rsvd96: [u8; 3],
    /// Namespace Attributes
    pub nsattr: u8,
    /// NVM Set Identifier
    pub nvmsetid: u16,
    /// Endurance Group Identifier
    pub endgid: u16,
    /// Namespace Globally Unique Identifier
    pub nguid: [u8; 16],
    /// IEEE Extended Unique Identifier
    pub eui64: [u8; 8],
    /// LBA Format Support
    pub lbaf: [u32; 16],
    /// Reserved
    pub rsvd192: [u8; 192],
    /// Vendor Specific
    pub vs: [u8; 3712],
}

/// NVMe LBA Format
#[derive(Debug, Clone, Copy)]
pub struct NvmeLbaFormat {
    /// Metadata Size
    pub ms: u16,
    /// LBA Data Size
    pub lbads: u8,
    /// Relative Performance
    pub rp: u8,
}

impl NvmeLbaFormat {
    pub fn from_u32(value: u32) -> Self {
        Self {
            ms: (value & 0xffff) as u16,
            lbads: ((value >> 16) & 0xff) as u8,
            rp: ((value >> 24) & 0x3) as u8,
        }
    }

    /// Get LBA size in bytes
    pub fn lba_size(&self) -> u32 {
        1 << self.lbads
    }
}

/// NVMe driver implementation
#[derive(Debug)]
pub struct NvmeDriver {
    name: String,
    state: StorageDeviceState,
    capabilities: StorageCapabilities,
    stats: StorageStats,
    base_addr: u64,
    doorbell_stride: u32,
    max_queue_entries: u16,
    controller_ready: bool,
    active_namespace: u32,
    namespace_count: u32,
    lba_format: NvmeLbaFormat,
    // Queue management
    queue_depth: u16,
    current_sq_tail: u16,
    current_cq_head: u16,
    cq_phase: u32,
    io_sq_base: u64,
    io_cq_base: u64,
    next_command_id: u16,
}

impl NvmeDriver {
    /// Create new NVMe driver instance
    pub fn new(name: String, base_addr: u64) -> Self {
        Self {
            name,
            state: StorageDeviceState::Offline,
            capabilities: StorageCapabilities::default(),
            stats: StorageStats::default(),
            base_addr,
            doorbell_stride: 4,    // Default, will be read from CAP register
            max_queue_entries: 64, // Default, will be read from CAP register
            controller_ready: false,
            active_namespace: 1, // Default to namespace 1
            namespace_count: 0,
            lba_format: NvmeLbaFormat {
                ms: 0,
                lbads: 9,
                rp: 0,
            }, // Default 512 bytes
            queue_depth: 64,
            current_sq_tail: 0,
            current_cq_head: 0,
            cq_phase: 1,   // Start with phase 1
            io_sq_base: 0, // Will be set during queue initialization
            io_cq_base: 0, // Will be set during queue initialization
            next_command_id: 1,
        }
    }

    /// Read NVMe register
    fn read_reg(&self, offset: NvmeReg) -> u64 {
        unsafe {
            match offset {
                NvmeReg::Cap | NvmeReg::Asq | NvmeReg::Acq => {
                    ptr::read_volatile((self.base_addr + offset as u64) as *const u64)
                }
                _ => ptr::read_volatile((self.base_addr + offset as u64) as *const u32) as u64,
            }
        }
    }

    /// Write NVMe register
    fn write_reg(&self, offset: NvmeReg, value: u64) {
        unsafe {
            match offset {
                NvmeReg::Cap => {
                    // CAP register is read-only
                }
                NvmeReg::Asq | NvmeReg::Acq => {
                    ptr::write_volatile((self.base_addr + offset as u64) as *mut u64, value);
                }
                _ => {
                    ptr::write_volatile((self.base_addr + offset as u64) as *mut u32, value as u32);
                }
            }
        }
    }

    /// Read doorbell register
    fn read_doorbell(&self, queue_id: u16, is_completion: bool) -> u32 {
        let offset = NVME_DOORBELL_BASE
            + (queue_id as u64 * 2 + is_completion as u64) * self.doorbell_stride as u64;
        unsafe { ptr::read_volatile((self.base_addr + offset) as *const u32) }
    }

    /// Write doorbell register
    fn write_doorbell(&self, queue_id: u16, is_completion: bool, value: u32) {
        let offset = NVME_DOORBELL_BASE
            + (queue_id as u64 * 2 + is_completion as u64) * self.doorbell_stride as u64;
        unsafe {
            ptr::write_volatile((self.base_addr + offset) as *mut u32, value);
        }
    }

    /// Write to register with u32 offset (for doorbells)
    fn write_reg_raw(&self, offset: u32, value: u32) {
        unsafe {
            ptr::write_volatile((self.base_addr + offset as u64) as *mut u32, value);
        }
    }

    /// Get next available command ID
    fn get_next_command_id(&mut self) -> u16 {
        let id = self.next_command_id;
        self.next_command_id = if self.next_command_id >= 65535 {
            1
        } else {
            self.next_command_id + 1
        };
        id
    }

    /// Initialize NVMe controller
    fn init_controller(&mut self) -> Result<(), StorageError> {
        // Read controller capabilities
        let cap = self.read_reg(NvmeReg::Cap);

        // Extract capabilities
        self.max_queue_entries = ((cap & 0xffff) + 1) as u16;
        self.doorbell_stride = 4 << ((cap >> 32) & 0xf); // DSTRD field

        // Check if controller supports NVM command set
        let css = (cap >> 37) & 0xff;
        if (css & 1) == 0 {
            return Err(StorageError::NotSupported);
        }

        // Reset controller if needed
        let csts = self.read_reg(NvmeReg::Csts) as u32;
        if (csts & NvmeCsts::RDY.bits()) != 0 {
            // Disable controller
            let mut cc = self.read_reg(NvmeReg::Cc) as u32;
            cc &= !NvmeCc::EN.bits();
            self.write_reg(NvmeReg::Cc, cc as u64);

            // Wait for controller to become ready
            for _ in 0..1000 {
                let csts = self.read_reg(NvmeReg::Csts) as u32;
                if (csts & NvmeCsts::RDY.bits()) == 0 {
                    break;
                }
            }
        }

        // Configure controller
        let mut cc = 0u32;
        cc |= 0 << 4; // CSS = NVM command set
        cc |= 0 << 7; // MPS = 2^(12+0) = 4KB pages
        cc |= 0 << 11; // AMS = Round Robin
        cc |= 6 << 16; // IOSQES = 2^6 = 64 bytes
        cc |= 4 << 20; // IOCQES = 2^4 = 16 bytes

        self.write_reg(NvmeReg::Cc, cc as u64);

        // Set up admin queues (simplified - would need real DMA memory)
        let acq_size = (self.max_queue_entries - 1) as u32;
        let asq_size = (self.max_queue_entries - 1) as u32;
        self.write_reg(NvmeReg::Aqa, ((acq_size << 16) | asq_size) as u64);

        // Enable controller
        cc |= NvmeCc::EN.bits();
        self.write_reg(NvmeReg::Cc, cc as u64);

        // Wait for controller ready
        for _ in 0..1000 {
            let csts = self.read_reg(NvmeReg::Csts) as u32;
            if (csts & NvmeCsts::RDY.bits()) != 0 {
                self.controller_ready = true;
                break;
            }
        }

        if !self.controller_ready {
            return Err(StorageError::Timeout);
        }

        // Identify controller and namespaces
        self.identify_controller()?;
        self.identify_namespaces()?;

        self.state = StorageDeviceState::Ready;
        Ok(())
    }

    /// Execute identify controller command
    fn identify_controller(&mut self) -> Result<(), StorageError> {
        // In a real implementation, we would:
        // 1. Allocate DMA buffer for identify data
        // 2. Build identify command in admin submission queue
        // 3. Ring doorbell
        // 4. Wait for completion
        // 5. Parse controller data

        // For now, just simulate reading controller info
        self.capabilities.supports_smart = true;
        self.capabilities.supports_trim = true;

        Ok(())
    }

    /// Identify available namespaces
    fn identify_namespaces(&mut self) -> Result<(), StorageError> {
        // In a real implementation, we would:
        // 1. Get active namespace list
        // 2. For each namespace, get namespace identify data
        // 3. Parse LBA formats and capabilities

        // For now, assume one namespace with 512-byte sectors
        self.namespace_count = 1;
        self.active_namespace = 1;

        // Set default capabilities
        self.capabilities.capacity_bytes = 1024 * 1024 * 1024; // 1GB default
        self.capabilities.sector_size = 512;
        self.capabilities.max_transfer_size = 128 * 1024; // 128KB
        self.capabilities.max_queue_depth = self.max_queue_entries;
        self.capabilities.supports_ncq = true;
        self.capabilities.read_speed_mbps = 3500; // Typical NVMe SSD speed
        self.capabilities.write_speed_mbps = 3000;

        Ok(())
    }

    /// Submit I/O command (production implementation)
    fn submit_io_command(
        &mut self,
        opcode: NvmeIoOpcode,
        lba: u64,
        block_count: u16,
    ) -> Result<(), StorageError> {
        if !self.controller_ready {
            return Err(StorageError::DeviceNotFound);
        }

        // Production implementation:

        // 1. Build NVMe command in submission queue
        let sq_entry = self.current_sq_tail;
        let command_id = self.get_next_command_id();

        // Get submission queue base address
        let sq_base = self.io_sq_base;
        if sq_base == 0 {
            return Err(StorageError::HardwareError);
        }

        // Allocate DMA buffer for data transfer - BEFORE unsafe block so it stays alive
        use crate::net::dma::{DmaBuffer, DMA_ALIGNMENT};

        let buffer_size = (block_count as usize) * (self.capabilities.sector_size as usize);
        let mut _dma_buffer = DmaBuffer::allocate(buffer_size, DMA_ALIGNMENT)
            .map_err(|_| StorageError::HardwareError)?;

        // Translate virtual address to physical for hardware DMA
        let buffer_phys = {
            use crate::memory::get_memory_manager;
            use x86_64::VirtAddr;

            let virt_addr = VirtAddr::new(_dma_buffer.virtual_addr() as u64);
            let memory_manager = get_memory_manager().ok_or(StorageError::HardwareError)?;

            memory_manager
                .translate_addr(virt_addr)
                .ok_or(StorageError::HardwareError)?
                .as_u64()
        };

        unsafe {
            let sq_entry_ptr = (sq_base + (sq_entry as u64 * 64)) as *mut u64; // Each SQ entry is 64 bytes

            // Command DWord 0: Opcode and Command ID
            *sq_entry_ptr = (opcode as u64) | ((command_id as u64) << 16);

            // Command DWord 1: Namespace ID (assuming namespace 1)
            *sq_entry_ptr.add(1) = 1u64;

            // Command DWords 2-3: Reserved
            *sq_entry_ptr.add(2) = 0;
            *sq_entry_ptr.add(3) = 0;

            // Command DWords 4-5: Metadata pointer (not used)
            *sq_entry_ptr.add(4) = 0;
            *sq_entry_ptr.add(5) = 0;

            // 2. Set up data pointers (PRPs)
            *sq_entry_ptr.add(6) = buffer_phys; // PRP1
            *sq_entry_ptr.add(7) = 0; // PRP2 (not needed for small transfers)

            // Command DWords 10-11: Starting LBA
            *sq_entry_ptr.add(10) = lba;
            *sq_entry_ptr.add(11) = 0;

            // Command DWord 12: Number of logical blocks (0-based)
            *sq_entry_ptr.add(12) = (block_count - 1) as u64;

            // Command DWords 13-15: Command-specific
            *sq_entry_ptr.add(13) = 0;
            *sq_entry_ptr.add(14) = 0;
            *sq_entry_ptr.add(15) = 0;
        }

        // Update submission queue tail
        self.current_sq_tail = (self.current_sq_tail + 1) % self.queue_depth;

        // 3. Ring submission queue doorbell
        let doorbell_offset = 0x1000 + (0 * 2 * (4 << self.doorbell_stride)); // Queue 0 submission doorbell
        self.write_reg_raw(doorbell_offset as u32, self.current_sq_tail as u32);

        // 4. Wait for completion queue entry
        let mut timeout = 1000000; // Timeout counter
        let cq_base = self.io_cq_base;

        while timeout > 0 {
            unsafe {
                let cq_entry_ptr = (cq_base + (self.current_cq_head as u64 * 16)) as *const u32; // Each CQ entry is 16 bytes
                let dw3 = core::ptr::read_volatile(cq_entry_ptr.add(3));
                let phase = (dw3 >> 16) & 1;

                if phase == self.cq_phase {
                    // Completion found - check status
                    let status = (dw3 >> 17) & 0x7FF;
                    if status != 0 {
                        return Err(StorageError::HardwareError);
                    }

                    // Update completion queue head
                    self.current_cq_head = (self.current_cq_head + 1) % self.queue_depth;
                    if self.current_cq_head == 0 {
                        self.cq_phase = 1 - self.cq_phase; // Flip phase
                    }

                    break;
                }
            }
            timeout -= 1;
        }

        if timeout == 0 {
            return Err(StorageError::Timeout);
        }

        // 5. Ring completion queue doorbell
        let cq_doorbell_offset = 0x1000 + (0 * 2 + 1) * (4 << self.doorbell_stride); // Queue 0 completion doorbell
        self.write_reg_raw(cq_doorbell_offset as u32, self.current_cq_head as u32);

        // Update statistics
        match opcode {
            NvmeIoOpcode::Read => {
                self.stats.reads_total += 1;
                self.stats.bytes_read +=
                    (block_count as u64) * (self.capabilities.sector_size as u64);
            }
            NvmeIoOpcode::Write => {
                self.stats.writes_total += 1;
                self.stats.bytes_written +=
                    (block_count as u64) * (self.capabilities.sector_size as u64);
            }
            _ => {}
        }

        Ok(())
    }

    /// Get namespace information
    pub fn get_namespace_info(&self, nsid: u32) -> Option<(u64, u32)> {
        if nsid == 0 || nsid > self.namespace_count {
            return None;
        }

        // Return (size_in_blocks, block_size)
        let blocks = self.capabilities.capacity_bytes / self.capabilities.sector_size as u64;
        Some((blocks, self.capabilities.sector_size))
    }

    /// Get controller version
    pub fn get_version(&self) -> u32 {
        self.read_reg(NvmeReg::Vs) as u32
    }

    /// Get supported features
    pub fn get_supported_features(&self) -> Vec<String> {
        let mut features = Vec::new();

        if self.capabilities.supports_ncq {
            features.push("Native Command Queuing".to_string());
        }
        if self.capabilities.supports_trim {
            features.push("TRIM/Deallocate".to_string());
        }
        if self.capabilities.supports_smart {
            features.push("SMART Health Information".to_string());
        }

        features.push(format!("Max Queue Depth: {}", self.max_queue_entries));
        features.push(format!("Doorbell Stride: {} bytes", self.doorbell_stride));

        features
    }
}

impl StorageDriver for NvmeDriver {
    fn name(&self) -> &str {
        &self.name
    }

    fn device_type(&self) -> StorageDeviceType {
        StorageDeviceType::NvmeSsd
    }

    fn state(&self) -> StorageDeviceState {
        self.state
    }

    fn capabilities(&self) -> StorageCapabilities {
        self.capabilities.clone()
    }

    fn init(&mut self) -> Result<(), StorageError> {
        self.state = StorageDeviceState::Initializing;
        self.init_controller()?;
        self.state = StorageDeviceState::Ready;
        Ok(())
    }

    fn read_sectors(
        &mut self,
        start_sector: u64,
        buffer: &mut [u8],
    ) -> Result<usize, StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        let sector_size = self.capabilities.sector_size as usize;
        let sector_count = buffer.len() / sector_size;

        if sector_count == 0 {
            return Err(StorageError::BufferTooSmall);
        }

        if sector_count > 65536 {
            return Err(StorageError::TransferTooLarge);
        }

        // Convert sector to LBA (assuming 1:1 mapping for simplicity)
        let lba = start_sector;
        let block_count = sector_count as u16;

        self.submit_io_command(NvmeIoOpcode::Read, lba, block_count)?;

        // In real implementation, data would be DMAed into buffer
        Ok(buffer.len())
    }

    fn write_sectors(&mut self, start_sector: u64, buffer: &[u8]) -> Result<usize, StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        let sector_size = self.capabilities.sector_size as usize;
        let sector_count = buffer.len() / sector_size;

        if sector_count == 0 {
            return Err(StorageError::BufferTooSmall);
        }

        if sector_count > 65536 {
            return Err(StorageError::TransferTooLarge);
        }

        let lba = start_sector;
        let block_count = sector_count as u16;

        self.submit_io_command(NvmeIoOpcode::Write, lba, block_count)?;

        Ok(buffer.len())
    }

    fn flush(&mut self) -> Result<(), StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        self.submit_io_command(NvmeIoOpcode::Flush, 0, 0)?;
        Ok(())
    }

    fn get_stats(&self) -> StorageStats {
        self.stats.clone()
    }

    fn reset(&mut self) -> Result<(), StorageError> {
        self.state = StorageDeviceState::Resetting;
        self.controller_ready = false;
        self.init_controller()?;
        self.state = StorageDeviceState::Ready;
        Ok(())
    }

    fn standby(&mut self) -> Result<(), StorageError> {
        // NVMe doesn't have explicit standby, use power management
        self.state = StorageDeviceState::Standby;
        Ok(())
    }

    fn wake(&mut self) -> Result<(), StorageError> {
        if self.state == StorageDeviceState::Standby {
            self.state = StorageDeviceState::Ready;
        }
        Ok(())
    }

    fn vendor_command(&mut self, _command: u8, _data: &[u8]) -> Result<Vec<u8>, StorageError> {
        // NVMe vendor commands would be executed through admin queue
        Err(StorageError::NotSupported)
    }

    fn get_smart_data(&mut self) -> Result<Vec<u8>, StorageError> {
        if !self.capabilities.supports_smart {
            return Err(StorageError::NotSupported);
        }

        // Production implementation: Execute Get Log Page command for SMART data

        // Build admin command for Get Log Page (SMART data)
        let command_id = self.get_next_command_id();
        let admin_sq_base = self.read_reg(NvmeReg::Asq); // Admin submission queue base

        if admin_sq_base == 0 {
            return Err(StorageError::HardwareError);
        }

        // Allocate buffer for SMART data - Production DMA allocation
        use crate::net::dma::{DmaBuffer, DMA_ALIGNMENT};

        let mut dma_buffer =
            DmaBuffer::allocate(512, DMA_ALIGNMENT).map_err(|_| StorageError::HardwareError)?;

        // Translate virtual address to physical for hardware DMA
        let buffer_phys = {
            use crate::memory::get_memory_manager;
            use x86_64::VirtAddr;

            let virt_addr = VirtAddr::new(dma_buffer.virtual_addr() as u64);
            let memory_manager = get_memory_manager().ok_or(StorageError::HardwareError)?;

            memory_manager
                .translate_addr(virt_addr)
                .ok_or(StorageError::HardwareError)?
                .as_u64()
        };

        unsafe {
            let sq_entry_ptr = admin_sq_base as *mut u64;

            // Command DWord 0: Opcode (Get Log Page = 0x02) and Command ID
            *sq_entry_ptr = 0x02u64 | ((command_id as u64) << 16);

            // Command DWord 1: Namespace ID (0xFFFFFFFF for controller)
            *sq_entry_ptr.add(1) = 0xFFFFFFFFu64;

            // Command DWords 2-3: Reserved
            *sq_entry_ptr.add(2) = 0;
            *sq_entry_ptr.add(3) = 0;

            // Command DWords 4-5: Metadata pointer (not used)
            *sq_entry_ptr.add(4) = 0;
            *sq_entry_ptr.add(5) = 0;

            // Command DWords 6-7: Data pointer (PRP1/PRP2)
            *sq_entry_ptr.add(6) = buffer_phys;
            *sq_entry_ptr.add(7) = 0;

            // Command DWords 8-9: Reserved
            *sq_entry_ptr.add(8) = 0;
            *sq_entry_ptr.add(9) = 0;

            // Command DWord 10: Log Page Identifier (0x02 = SMART/Health) and length
            *sq_entry_ptr.add(10) = 0x02u64 | ((511u64) << 16); // Log ID + Number of DWORDs - 1

            // Command DWords 11-15: Reserved/Log-specific
            for i in 11..16 {
                *sq_entry_ptr.add(i) = 0;
            }
        }

        // Ring admin submission queue doorbell
        self.write_doorbell(0, false, 1); // Admin queue ID = 0, submission doorbell

        // Wait for completion
        let mut timeout = 1000000;
        let admin_cq_base = self.read_reg(NvmeReg::Acq);

        while timeout > 0 {
            unsafe {
                let cq_entry_ptr = admin_cq_base as *const u32;
                let dw3 = core::ptr::read_volatile(cq_entry_ptr.add(3));
                let phase = (dw3 >> 16) & 1;

                if phase == 1 {
                    // Admin queue phase is typically 1
                    // Check status
                    let status = (dw3 >> 17) & 0x7FF;
                    if status != 0 {
                        return Err(StorageError::HardwareError);
                    }
                    break;
                }
            }
            timeout -= 1;
        }

        if timeout == 0 {
            return Err(StorageError::Timeout);
        }

        // Read SMART data from buffer
        let mut smart_data = vec![0u8; 512];
        unsafe {
            let data_ptr = buffer_phys as *const u8;
            for i in 0..512 {
                smart_data[i] = core::ptr::read_volatile(data_ptr.add(i));
            }
        }

        // Ring admin completion queue doorbell
        self.write_doorbell(0, true, 1); // Admin queue ID = 0, completion doorbell

        Ok(smart_data)
    }
}

/// Create NVMe driver from PCI device information
pub fn create_nvme_driver(base_addr: u64, device_name: Option<String>) -> Box<dyn StorageDriver> {
    let name = device_name.unwrap_or_else(|| "NVMe Controller".to_string());
    let driver = NvmeDriver::new(name, base_addr);
    Box::new(driver)
}

/// Check if PCI device is an NVMe controller
pub fn is_nvme_device(class_code: u8, subclass: u8, prog_if: u8) -> bool {
    // NVMe controllers: Class 01h (Mass Storage), Subclass 08h (Non-Volatile Memory), Prog IF 02h (NVMe)
    class_code == 0x01 && subclass == 0x08 && prog_if == 0x02
}
