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

// NVMe controller capabilities register bits
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

// NVMe controller configuration register bits
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

// NVMe controller status register bits
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
    io_sq_virt: u64,
    io_cq_virt: u64,
    next_command_id: u16,
    // Admin queue tracking
    admin_sq_base: u64,
    admin_cq_base: u64,
    admin_sq_tail: u16,
    admin_cq_head: u16,
    admin_cq_phase: u32,
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
            io_sq_virt: 0,
            io_cq_virt: 0,
            next_command_id: 1,
            admin_sq_base: 0,
            admin_cq_base: 0,
            admin_sq_tail: 0,
            admin_cq_head: 0,
            admin_cq_phase: 1,
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
        self.next_command_id = if self.next_command_id == u16::MAX {
            1
        } else {
            self.next_command_id + 1
        };
        id
    }

    fn nvme_status_to_error(status: u16) -> StorageError {
        let sc = ((status >> 1) & 0xff) as u8;
        match sc {
            0x00 => StorageError::HardwareError,
            0x02 | 0x80 => StorageError::InvalidSector,
            0x04 => StorageError::DeviceBusy,
            0x05 => StorageError::InvalidSector,
            0x0b => StorageError::NotSupported,
            _ => StorageError::HardwareError,
        }
    }

    fn dma_phys(virt: usize) -> Result<u64, StorageError> {
        use crate::memory::get_memory_manager;
        use x86_64::VirtAddr;
        let mm = get_memory_manager().ok_or(StorageError::HardwareError)?;
        mm.translate_addr(VirtAddr::new(virt as u64))
            .ok_or(StorageError::HardwareError)
            .map(|p| p.as_u64())
    }

    fn submit_admin_command(&mut self, mut cmd: NvmeSqe) -> Result<u32, StorageError> {
        if self.admin_sq_base == 0 || self.admin_cq_base == 0 {
            return Err(StorageError::HardwareError);
        }
        let cmd_id = self.get_next_command_id();
        cmd.cdw0 = (cmd.cdw0 & !0xffff_0000) | ((cmd_id as u32) << 16);
        let sqe = self.admin_sq_tail;
        unsafe {
            let ptr = (self.admin_sq_base + sqe as u64 * core::mem::size_of::<NvmeSqe>() as u64)
                as *mut NvmeSqe;
            core::ptr::write_volatile(ptr, cmd);
        }
        self.admin_sq_tail = (self.admin_sq_tail + 1) % self.max_queue_entries;
        self.write_doorbell(0, false, self.admin_sq_tail as u32);

        let mut timeout = 1_000_000u32;
        loop {
            unsafe {
                let cqe = (self.admin_cq_base
                    + self.admin_cq_head as u64 * core::mem::size_of::<NvmeCqe>() as u64)
                    as *const u32;
                let dw0 = core::ptr::read_volatile(cqe);
                let dw2 = core::ptr::read_volatile(cqe.add(2));
                let dw3 = core::ptr::read_volatile(cqe.add(3));
                let cid = (dw3 & 0xffff) as u16;
                let status_field = (dw3 >> 16) as u16;
                let phase = (status_field & 1) as u32;
                if phase == self.admin_cq_phase {
                    if cid != cmd_id {
                        return Err(StorageError::HardwareError);
                    }
                    let status = status_field >> 1;
                    let _sq_head = dw2 & 0xffff;
                    self.admin_cq_head = (self.admin_cq_head + 1) % self.max_queue_entries;
                    if self.admin_cq_head == 0 {
                        self.admin_cq_phase ^= 1;
                    }
                    self.write_doorbell(0, true, self.admin_cq_head as u32);
                    if status != 0 {
                        return Err(Self::nvme_status_to_error(status_field));
                    }
                    return Ok(dw0);
                }
            }
            if timeout == 0 {
                return Err(StorageError::Timeout);
            }
            timeout -= 1;
            core::hint::spin_loop();
        }
    }

    /// Initialize NVMe controller
    fn init_controller(&mut self) -> Result<(), StorageError> {
        // Read controller capabilities
        let cap = self.read_reg(NvmeReg::Cap);

        // Extract capabilities
        let mqes = ((cap & 0xffff) + 1).min(64) as u16;
        self.max_queue_entries = mqes;
        self.queue_depth = core::cmp::min(self.max_queue_entries, 64);
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

        // Set up admin queues with real DMA memory
        let acq_size = (self.max_queue_entries - 1) as u32;
        let asq_size = (self.max_queue_entries - 1) as u32;
        self.write_reg(NvmeReg::Aqa, ((acq_size << 16) | asq_size) as u64);

        // Allocate DMA memory for admin submission queue and completion queue
        // SQ entries are 64 bytes, CQ entries are 16 bytes
        let asq_bytes = self.max_queue_entries as usize * 64;
        let acq_bytes = self.max_queue_entries as usize * 16;
        let total_dma = ((asq_bytes + 0xFFF) & !0xFFF) + ((acq_bytes + 0xFFF) & !0xFFF);

        let layout = alloc::alloc::Layout::from_size_align(total_dma, 4096)
            .map_err(|_| StorageError::HardwareError)?;
        let dma_ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        if dma_ptr.is_null() {
            return Err(StorageError::HardwareError);
        }
        let asq_phys = dma_ptr as u64;
        let acq_phys = asq_phys + ((asq_bytes + 0xFFF) & !0xFFF) as u64;
        self.admin_sq_base = asq_phys;
        self.admin_cq_base = acq_phys;

        // Program ASQ and ACQ base addresses
        self.write_reg(NvmeReg::Asq, asq_phys);
        self.write_reg(NvmeReg::Acq, acq_phys);

        crate::serial_println!(
            "nvme: admin queues ASQ=0x{:X} ACQ=0x{:X} entries={}",
            asq_phys,
            acq_phys,
            self.max_queue_entries
        );

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
        self.init_io_queues()?;

        self.state = StorageDeviceState::Ready;
        Ok(())
    }

    /// Execute identify controller command via admin queue
    fn identify_controller(&mut self) -> Result<(), StorageError> {
        // Allocate DMA buffer for identify data (4096 bytes)
        use crate::net::dma::{DmaBuffer, DMA_ALIGNMENT};
        let dma_buf =
            DmaBuffer::allocate(4096, DMA_ALIGNMENT).map_err(|_| StorageError::HardwareError)?;

        let buffer_phys = Self::dma_phys(dma_buf.virtual_addr() as usize)?;

        let cmd = NvmeSqe {
            cdw0: NvmeOpcode::Identify as u32,
            nsid: 0,
            cdw2: 0,
            cdw3: 0,
            mptr: 0,
            dptr: [buffer_phys, 0],
            cdw10: 0x01,
            cdw11: 0,
            cdw12: 0,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };
        self.submit_admin_command(cmd)?;

        // Parse identify controller data from DMA buffer
        // The identify data is at offset 0 in the DMA buffer
        let buf_ptr = dma_buf.virtual_addr() as *const u8;

        unsafe {
            let oacs = core::ptr::read_unaligned(buf_ptr.add(256) as *const u16);
            // NVMe SMART/Health log (log page 0x02) is mandatory for NVM
            // controllers; OACS is still parsed for optional admin features.
            let _oacs = oacs;
            self.capabilities.supports_smart = true;

            // Byte 519: ONCS (Offset 519-520)
            let oncs = core::ptr::read_unaligned(buf_ptr.add(520) as *const u16);
            // Bit 0 of ONCS indicates Dataset Management (TRIM) support
            self.capabilities.supports_trim = (oncs & 0x01) != 0;

            // Bytes 4-7: Serial Number (20 bytes ASCII)
            let sn_ptr = buf_ptr.add(4) as *const u8;

            // Bytes 24-43: Model Number (40 bytes ASCII)
            let mn_ptr = buf_ptr.add(24) as *const u8;

            crate::serial_println!(
                "nvme: identify controller SMART={} TRIM={} SN={:02X}{:02X}{:02X}{:02X}",
                self.capabilities.supports_smart,
                self.capabilities.supports_trim,
                *sn_ptr,
                *sn_ptr.add(1),
                *sn_ptr.add(2),
                *sn_ptr.add(3)
            );

            // Read max transfer size from MDTS (byte 77+4=81)
            let mdts = *buf_ptr.add(77);
            if mdts > 0 {
                let shift = core::cmp::min(12 + mdts as u32, 31);
                self.capabilities.max_transfer_size = 1u32 << shift;
            } else {
                self.capabilities.max_transfer_size = 128 * 1024;
            }

            // Suppress unused warning
            let _ = mn_ptr;
        }

        Ok(())
    }

    /// Identify available namespaces via admin queue
    fn identify_namespaces(&mut self) -> Result<(), StorageError> {
        // Allocate DMA buffer for identify namespace data (4096 bytes)
        use crate::net::dma::{DmaBuffer, DMA_ALIGNMENT};
        let dma_buf =
            DmaBuffer::allocate(4096, DMA_ALIGNMENT).map_err(|_| StorageError::HardwareError)?;

        let buffer_phys = Self::dma_phys(dma_buf.virtual_addr() as usize)?;

        let cmd = NvmeSqe {
            cdw0: NvmeOpcode::Identify as u32,
            nsid: 1,
            cdw2: 0,
            cdw3: 0,
            mptr: 0,
            dptr: [buffer_phys, 0],
            cdw10: 0x00,
            cdw11: 0,
            cdw12: 0,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };
        self.submit_admin_command(cmd)?;

        // Parse identify namespace data from DMA buffer
        let buf_ptr = dma_buf.virtual_addr() as *const u8;

        unsafe {
            // NSZE (Namespace Size): bytes 0-7 (number of logical blocks)
            let nsze = core::ptr::read_unaligned(buf_ptr as *const u64);
            if nsze == 0 {
                return Err(StorageError::DeviceNotFound);
            }

            // NCAP (Namespace Capacity): bytes 8-15
            let _ncap = core::ptr::read_unaligned(buf_ptr.add(8) as *const u64);

            // LBA Format: byte 26 (LBA data size index in LBAF array)
            // The LBAF array starts at offset 128, each entry is 4 bytes
            // lbaf[0] = offset 128: bits[23:16] = ms, bits[15:0] = lbads (2^lbads = sector size)
            let lbaf_index = (*buf_ptr.add(26) & 0x0f) as usize;
            let lbaf_offset = 128 + (lbaf_index * 4);
            let lbaf_entry = core::ptr::read_unaligned(buf_ptr.add(lbaf_offset) as *const u32);
            let lbads = ((lbaf_entry >> 16) & 0xFF) as u32;
            let sector_size = if lbads > 0 { 1u32 << lbads } else { 512 };
            if sector_size < 512 || !sector_size.is_power_of_two() {
                return Err(StorageError::HardwareError);
            }

            self.namespace_count = 1;
            self.active_namespace = 1;
            self.capabilities.capacity_bytes = nsze * (sector_size as u64);
            self.capabilities.sector_size = sector_size;
            self.capabilities.max_queue_depth = self.max_queue_entries;
            self.capabilities.supports_ncq = true;

            crate::serial_println!(
                "nvme: namespace 1: capacity={} blocks ({} bytes), sector_size={}",
                nsze,
                self.capabilities.capacity_bytes,
                sector_size
            );
        }

        Ok(())
    }

    fn init_io_queues(&mut self) -> Result<(), StorageError> {
        if self.max_queue_entries < 2 {
            return Err(StorageError::HardwareError);
        }
        let qd = self.queue_depth.max(2);
        self.queue_depth = qd;
        let sq_bytes = qd as usize * core::mem::size_of::<NvmeSqe>();
        let cq_bytes = qd as usize * core::mem::size_of::<NvmeCqe>();
        let total = ((sq_bytes + 0xFFF) & !0xFFF) + ((cq_bytes + 0xFFF) & !0xFFF);
        let layout = alloc::alloc::Layout::from_size_align(total, 4096)
            .map_err(|_| StorageError::HardwareError)?;
        let ptr = unsafe { alloc::alloc::alloc_zeroed(layout) };
        if ptr.is_null() {
            return Err(StorageError::HardwareError);
        }
        let sq_phys = Self::dma_phys(ptr as usize)?;
        let cq_ptr = unsafe { ptr.add((sq_bytes + 0xFFF) & !0xFFF) };
        let cq_phys = Self::dma_phys(cq_ptr as usize)?;
        self.io_sq_base = sq_phys;
        self.io_cq_base = cq_phys;
        self.io_sq_virt = ptr as u64;
        self.io_cq_virt = cq_ptr as u64;
        self.current_sq_tail = 0;
        self.current_cq_head = 0;
        self.cq_phase = 1;

        let create_cq = NvmeSqe {
            cdw0: NvmeOpcode::CreateIoCq as u32,
            nsid: 0,
            cdw2: 0,
            cdw3: 0,
            mptr: 0,
            dptr: [cq_phys, 0],
            cdw10: 1 | (((qd - 1) as u32) << 16),
            cdw11: 1, // physically contiguous, interrupts disabled
            cdw12: 0,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };
        self.submit_admin_command(create_cq)?;

        let create_sq = NvmeSqe {
            cdw0: NvmeOpcode::CreateIoSq as u32,
            nsid: 0,
            cdw2: 0,
            cdw3: 0,
            mptr: 0,
            dptr: [sq_phys, 0],
            cdw10: 1 | (((qd - 1) as u32) << 16),
            cdw11: 1 | (1 << 16), // physically contiguous, CQID=1
            cdw12: 0,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };
        self.submit_admin_command(create_sq)?;
        Ok(())
    }

    /// Submit I/O command (production implementation)
    /// Submit a single IO command. `data_ptr`/`data_len` describe the caller's
    /// buffer: for writes it is copied into the DMA buffer before the doorbell,
    /// for reads the DMA buffer is copied back into it after completion.
    fn submit_io_command(
        &mut self,
        opcode: NvmeIoOpcode,
        lba: u64,
        block_count: u16,
        data_ptr: *mut u8,
        data_len: usize,
    ) -> Result<(), StorageError> {
        if !self.controller_ready {
            return Err(StorageError::DeviceNotFound);
        }

        // Production implementation:

        // 1. Build NVMe command in submission queue
        let sq_entry = self.current_sq_tail;
        let command_id = self.get_next_command_id();

        // Get submission queue base address
        let sq_base = self.io_sq_virt;
        if sq_base == 0 || self.io_cq_virt == 0 {
            return Err(StorageError::HardwareError);
        }

        // Allocate DMA buffer for data transfer - BEFORE unsafe block so it stays alive
        use crate::net::dma::{DmaBuffer, DMA_ALIGNMENT};

        let buffer_size = if matches!(opcode, NvmeIoOpcode::Flush) {
            0
        } else {
            (block_count as usize) * (self.capabilities.sector_size as usize)
        };
        let mut _dma_buffer = if buffer_size > 0 {
            Some(
                DmaBuffer::allocate(buffer_size, DMA_ALIGNMENT)
                    .map_err(|_| StorageError::HardwareError)?,
            )
        } else {
            None
        };

        // Translate virtual address to physical for hardware DMA
        let buffer_phys = if let Some(ref dma) = _dma_buffer {
            Self::dma_phys(dma.virtual_addr() as usize)?
        } else {
            0
        };

        // For writes, stage the caller's data into the DMA buffer before the
        // device reads it. Bounded by the DMA buffer size.
        if matches!(opcode, NvmeIoOpcode::Write) && !data_ptr.is_null() {
            let n = core::cmp::min(data_len, buffer_size);
            unsafe {
                core::ptr::copy_nonoverlapping(
                    data_ptr as *const u8,
                    _dma_buffer.as_ref().unwrap().virtual_addr() as *mut u8,
                    n,
                );
            }
        }

        unsafe {
            let sq_entry_ptr = (sq_base + (sq_entry as u64 * 64)) as *mut NvmeSqe;
            let cmd = NvmeSqe {
                cdw0: (opcode as u32) | ((command_id as u32) << 16),
                nsid: self.active_namespace,
                cdw2: 0,
                cdw3: 0,
                mptr: 0,
                dptr: [buffer_phys, 0],
                cdw10: lba as u32,
                cdw11: (lba >> 32) as u32,
                cdw12: if matches!(opcode, NvmeIoOpcode::Flush) {
                    0
                } else {
                    (block_count - 1) as u32
                },
                cdw13: 0,
                cdw14: 0,
                cdw15: 0,
            };
            core::ptr::write_volatile(sq_entry_ptr, cmd);
        }

        // Update submission queue tail
        self.current_sq_tail = (self.current_sq_tail + 1) % self.queue_depth;

        // 3. Ring submission queue doorbell
        self.write_doorbell(1, false, self.current_sq_tail as u32);

        // 4. Wait for completion queue entry
        let mut timeout = 1000000; // Timeout counter
        let cq_base = self.io_cq_virt;

        while timeout > 0 {
            unsafe {
                let cq_entry_ptr = (cq_base + (self.current_cq_head as u64 * 16)) as *const u32; // Each CQ entry is 16 bytes
                let dw3 = core::ptr::read_volatile(cq_entry_ptr.add(3));
                let phase = (dw3 >> 16) & 1;

                if phase == self.cq_phase {
                    let completed_cid = (dw3 & 0xffff) as u16;
                    if completed_cid != command_id {
                        return Err(StorageError::HardwareError);
                    }
                    // Completion found - check status
                    let status = (dw3 >> 17) & 0x7FF;
                    if status != 0 {
                        return Err(Self::nvme_status_to_error((dw3 >> 16) as u16));
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
        self.write_doorbell(1, true, self.current_cq_head as u32);

        // For reads, copy the DMA buffer back into the caller's buffer now that
        // the device has filled it.
        if matches!(opcode, NvmeIoOpcode::Read) && !data_ptr.is_null() {
            let n = core::cmp::min(data_len, buffer_size);
            unsafe {
                core::ptr::copy_nonoverlapping(
                    _dma_buffer.as_ref().unwrap().virtual_addr() as *const u8,
                    data_ptr,
                    n,
                );
            }
        }

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
        if sector_size == 0 || buffer.len() % sector_size != 0 {
            return Err(StorageError::BufferTooSmall);
        }
        let sector_count = buffer.len() / sector_size;

        if sector_count == 0 {
            return Err(StorageError::BufferTooSmall);
        }

        if sector_count > u16::MAX as usize {
            return Err(StorageError::TransferTooLarge);
        }
        if buffer.len() > self.capabilities.max_transfer_size as usize {
            return Err(StorageError::TransferTooLarge);
        }

        let end = start_sector
            .checked_add(sector_count as u64)
            .ok_or(StorageError::InvalidSector)?;
        let total = self.capabilities.capacity_bytes / self.capabilities.sector_size as u64;
        if total != 0 && end > total {
            return Err(StorageError::InvalidSector);
        }

        // Convert sector to LBA (assuming 1:1 mapping for simplicity)
        let lba = start_sector;
        let block_count = sector_count as u16;

        let buf_len = buffer.len();
        self.submit_io_command(
            NvmeIoOpcode::Read,
            lba,
            block_count,
            buffer.as_mut_ptr(),
            buf_len,
        )?;

        Ok(buffer.len())
    }

    fn write_sectors(&mut self, start_sector: u64, buffer: &[u8]) -> Result<usize, StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        let sector_size = self.capabilities.sector_size as usize;
        if sector_size == 0 || buffer.len() % sector_size != 0 {
            return Err(StorageError::BufferTooSmall);
        }
        let sector_count = buffer.len() / sector_size;

        if sector_count == 0 {
            return Err(StorageError::BufferTooSmall);
        }

        if sector_count > u16::MAX as usize {
            return Err(StorageError::TransferTooLarge);
        }
        if buffer.len() > self.capabilities.max_transfer_size as usize {
            return Err(StorageError::TransferTooLarge);
        }

        let end = start_sector
            .checked_add(sector_count as u64)
            .ok_or(StorageError::InvalidSector)?;
        let total = self.capabilities.capacity_bytes / self.capabilities.sector_size as u64;
        if total != 0 && end > total {
            return Err(StorageError::InvalidSector);
        }

        let lba = start_sector;
        let block_count = sector_count as u16;

        self.submit_io_command(
            NvmeIoOpcode::Write,
            lba,
            block_count,
            buffer.as_ptr() as *mut u8,
            buffer.len(),
        )?;

        Ok(buffer.len())
    }

    fn flush(&mut self) -> Result<(), StorageError> {
        if self.state != StorageDeviceState::Ready {
            return Err(StorageError::DeviceBusy);
        }

        self.submit_io_command(NvmeIoOpcode::Flush, 0, 0, core::ptr::null_mut(), 0)?;
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

        use crate::net::dma::{DmaBuffer, DMA_ALIGNMENT};

        let dma_buffer =
            DmaBuffer::allocate(512, DMA_ALIGNMENT).map_err(|_| StorageError::HardwareError)?;
        let buffer_phys = Self::dma_phys(dma_buffer.virtual_addr() as usize)?;
        let cmd = NvmeSqe {
            cdw0: NvmeOpcode::GetLogPage as u32,
            nsid: 0xFFFF_FFFF,
            cdw2: 0,
            cdw3: 0,
            mptr: 0,
            dptr: [buffer_phys, 0],
            cdw10: 0x02 | (((512 / 4 - 1) as u32) << 16),
            cdw11: 0,
            cdw12: 0,
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };
        self.submit_admin_command(cmd)?;

        // Read SMART data from buffer
        let mut smart_data = vec![0u8; 512];
        unsafe {
            let data_ptr = dma_buffer.virtual_addr() as *const u8;
            for i in 0..512 {
                smart_data[i] = core::ptr::read_volatile(data_ptr.add(i));
            }
        }

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
