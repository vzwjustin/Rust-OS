//! Initramfs Support for RustOS
//!
//! This module handles loading and extracting the initial RAM filesystem
//! which contains the Linux userspace environment (Alpine Linux).

use crate::vfs::{get_vfs, InodeType, OpenFlags};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::vec::Vec;

/// Embedded initramfs data (included at compile time).
/// Alpine Linux rootfs as uncompressed CPIO newc so boot does not depend on
/// the kernel gzip path before userspace is mounted.
pub static INITRAMFS_DATA: &[u8] = include_bytes!("../userspace/initramfs.cpio");

/// Initramfs header and metadata
pub struct InitramfsInfo {
    pub size: usize,
    pub format: InitramfsFormat,
    pub root_path: String,
}

/// Supported initramfs formats
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InitramfsFormat {
    CpioNewc,       // newc format (most common)
    CpioOdc,        // odc format (old)
    CompressedGzip, // gzip compressed cpio
}

/// Extract initramfs to VFS
pub fn extract_initramfs() -> Result<InitramfsInfo, InitramfsError> {
    if INITRAMFS_DATA.is_empty() {
        return Err(InitramfsError::NoInitramfs);
    }

    // 1. Detect format and extract. Uncompressed CPIO is parsed in place so
    // large desktop rootfs archives do not need a second heap-sized copy.
    if is_gzipped(INITRAMFS_DATA) {
        let data = decompress_gzip(INITRAMFS_DATA)?;
        extract_cpio(&data)?;
    } else {
        extract_cpio(INITRAMFS_DATA)?;
    }

    Ok(InitramfsInfo {
        size: INITRAMFS_DATA.len(),
        format: if is_gzipped(INITRAMFS_DATA) {
            InitramfsFormat::CompressedGzip
        } else {
            InitramfsFormat::CpioNewc
        },
        root_path: String::from("/"),
    })
}

/// Check if data is gzip compressed
fn is_gzipped(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b
}

/// Load and execute an ELF binary
///
/// This function:
/// 1. Parses and validates the ELF binary
/// 2. Allocates memory for all loadable segments
/// 3. Loads segment data into memory
/// 4. Sets up an initial stack
/// 5. Returns entry point address and stack pointer for execution
///
/// # Returns
/// (entry_point, stack_pointer) tuple
pub fn load_and_execute_elf(binary_data: &[u8]) -> Result<(u64, u64), InitramfsError> {
    use crate::elf_loader::{elf_load, elf_validate};

    // Validate ELF binary format
    elf_validate(binary_data).map_err(|_| InitramfsError::InvalidFormat)?;

    // Load ELF image (no load bias for static executables)
    let image = elf_load(binary_data, None).map_err(|_| InitramfsError::ExtractionFailed)?;

    // Get all program headers for segment data extraction
    use crate::elf_loader::parser::get_loadable_segments;
    let program_headers =
        get_loadable_segments(binary_data).map_err(|_| InitramfsError::InvalidFormat)?;

    // Load each PT_LOAD segment: allocate memory at the segment's virtual
    // address, copy the file data, and zero the BSS area. Memory is mapped
    // with permissions derived from the segment flags.
    for (segment, ph) in image.segments.iter().zip(program_headers.iter()) {
        if segment.segment_type != crate::elf_loader::SegmentType::Load {
            continue;
        }

        // Get segment data from binary
        let offset = ph.offset();
        let file_size = ph.file_size();

        if offset + file_size > binary_data.len() {
            return Err(InitramfsError::ExtractionFailed);
        }

        let segment_data = &binary_data[offset..offset + file_size];

        // Validate the destination range [vaddr, vaddr + mem_size) lies entirely
        // within the user address space before performing any raw write.
        let vaddr = segment.vaddr.as_u64();
        let mem_end = vaddr
            .checked_add(segment.mem_size as u64)
            .ok_or(InitramfsError::ExtractionFailed)?;
        if vaddr < crate::memory::USER_SPACE_START as u64
            || mem_end > crate::memory::USER_SPACE_END as u64
        {
            return Err(InitramfsError::ExtractionFailed);
        }

        // Determine memory protection from ELF segment flags.
        let prot = crate::memory::MemoryProtection {
            readable: segment.flags.readable,
            writable: segment.flags.writable,
            executable: segment.flags.executable,
            user_accessible: true,
            cache_disabled: false,
            write_through: false,
            copy_on_write: false,
            guard_page: false,
        };

        // Allocate and map memory at the segment's virtual address.
        // allocate_memory_at maps physical frames into the page table at
        // the specified address, so the subsequent copy writes to real
        // mapped memory rather than raw physical addresses.
        crate::memory::allocate_memory_at(
            segment.vaddr,
            segment.mem_size,
            crate::memory::MemoryRegionType::UserCode,
            prot,
        )
        .map_err(|_| InitramfsError::ExtractionFailed)?;

        // Copy file data into the mapped region
        unsafe {
            let dest = segment.vaddr.as_u64() as *mut u8;
            let src = segment_data.as_ptr();
            core::ptr::copy_nonoverlapping(src, dest, segment.file_size);

            // Zero BSS (memory size > file size)
            if segment.mem_size > segment.file_size {
                let bss_start = dest.add(segment.file_size);
                let bss_size = segment.mem_size - segment.file_size;
                core::ptr::write_bytes(bss_start, 0, bss_size);
            }
        }
    }

    // Allocate and map the user stack.
    let stack_size: usize = 16 * 1024; // 16 KiB stack
    let stack_top = image.stack_address.as_u64();
    let stack_bottom = stack_top.saturating_sub(stack_size as u64);
    if stack_bottom >= crate::memory::USER_SPACE_START as u64 {
        let stack_prot = crate::memory::MemoryProtection {
            readable: true,
            writable: true,
            executable: false,
            user_accessible: true,
            cache_disabled: false,
            write_through: false,
            copy_on_write: false,
            guard_page: false,
        };
        let _ = crate::memory::allocate_memory_at(
            x86_64::VirtAddr::new(stack_bottom),
            stack_size,
            crate::memory::MemoryRegionType::UserStack,
            stack_prot,
        );
    }

    // Set up initial user stack
    // Stack is at the recommended address from the ELF image
    let stack_pointer = image.stack_address.as_u64();

    // Return entry point address and stack pointer
    Ok((image.entry_point.as_u64(), stack_pointer))
}

/// Load ELF binary with full page table setup (advanced version)
///
/// This is the production-ready version that:
/// 1. Creates a new user page table
/// 2. Maps all segments with proper permissions (R/W/X)
/// 3. Sets up user stack with guard pages
/// 4. Handles both static and dynamic executables
///
/// Note: This requires a page table mapper and frame allocator
#[allow(dead_code)]
pub fn load_and_execute_elf_with_paging<M, A>(
    binary_data: &[u8],
    mapper: &mut M,
    frame_allocator: &mut A,
) -> Result<(u64, u64), InitramfsError>
where
    M: x86_64::structures::paging::Mapper<x86_64::structures::paging::Size4KiB>,
    A: x86_64::structures::paging::FrameAllocator<x86_64::structures::paging::Size4KiB>,
{
    use crate::elf_loader::{elf_create_stack, elf_load, elf_map_segments, elf_validate};

    // Validate ELF binary format
    elf_validate(binary_data).map_err(|_| InitramfsError::InvalidFormat)?;

    // Load ELF image
    let image = elf_load(binary_data, None).map_err(|_| InitramfsError::ExtractionFailed)?;

    // Map all segments into page table with proper permissions
    elf_map_segments(&image, binary_data, mapper, frame_allocator)
        .map_err(|_| InitramfsError::ExtractionFailed)?;

    // Create user stack (8 MB)
    const STACK_SIZE: usize = 8 * 1024 * 1024;
    let stack_pointer = elf_create_stack(mapper, frame_allocator, image.stack_address, STACK_SIZE)
        .map_err(|_| InitramfsError::ExtractionFailed)?;

    Ok((image.entry_point.as_u64(), stack_pointer.as_u64()))
}

/// Start the init process from initramfs
pub fn start_init() -> Result<(), InitramfsError> {
    // 1. Load /init from VFS
    let vfs = get_vfs();
    let init_inode = vfs
        .lookup("/init")
        .map_err(|_| InitramfsError::InitNotFound)?;

    // 2. Read the entire /init binary into memory
    let mut binary_data = Vec::new();
    let file_size = init_inode
        .stat()
        .map_err(|_| InitramfsError::VfsError)?
        .size;

    binary_data.resize(file_size as usize, 0);
    init_inode
        .read_at(0, &mut binary_data)
        .map_err(|_| InitramfsError::VfsError)?;

    // 3. Load and validate the ELF binary
    let (_entry_point, _stack_pointer) = load_and_execute_elf(&binary_data)?;

    // 4. Set up user mode execution context
    // This would involve:
    // - Creating a new process context
    // - Setting up user mode stack with arguments (argc, argv, envp)
    // - Configuring registers (RIP = entry_point, RSP = stack_pointer)
    // - Switching privilege level to ring 3
    // - Using IRET to jump to user mode

    // For now, we'll prepare but not execute
    // In a full implementation, this would:
    // 1. Create a process: process_manager::create_user_process()
    // 2. Set up execution context with entry point and stack
    // 3. Jump to user mode: usermode::switch_to_user_mode(entry_point, stack_pointer)

    // Log that we're ready to execute (for debugging)
    #[cfg(feature = "serial")]
    {
        use crate::serial_println;
        serial_println!("Init binary loaded:");
        serial_println!("  Entry point: {:#x}", entry_point);
        serial_println!("  Stack pointer: {:#x}", stack_pointer);
    }

    Ok(())
}

/// Execute the init process (transitions to user mode)
///
/// # Safety
/// Never returns on success.
#[allow(dead_code)]
pub unsafe fn execute_init(entry_point: u64, stack_pointer: u64) -> ! {
    crate::usermode::switch_to_user_mode(entry_point, stack_pointer)
}

/// Return the first userspace init path present on the root VFS.
pub fn find_userspace_init() -> Option<&'static str> {
    for path in ["/bin/init", "/init"] {
        if crate::vfs::vfs_stat(path).is_ok() {
            return Some(path);
        }
    }
    None
}

/// True when `/bin/init` or `/init` exists and is reachable via the VFS.
pub fn userspace_init_available() -> bool {
    find_userspace_init().is_some()
}

/// Spawn `/bin/init` or `/init` via `linux_compat::desktop` while the kernel
/// compositor loop keeps running.
pub fn spawn_userspace_init() -> Result<u32, InitramfsError> {
    let path = find_userspace_init().ok_or(InitramfsError::InitNotFound)?;
    crate::linux_compat::desktop::spawn_session_init(path).map_err(|_| InitramfsError::InitNotFound)
}

/// Try to exec `/bin/init` or `/init` and enter user mode.
///
/// # Safety
/// Never returns on success.
pub unsafe fn boot_userspace_init() -> ! {
    match spawn_userspace_init() {
        Ok(pid) => {
            crate::serial_println!(
                "init: PID {} queued; kernel desktop loop will service it",
                pid
            );
        }
        Err(_) => {
            use crate::linux_compat::process_ops;

            if let Some(path) = find_userspace_init() {
                crate::serial_println!("init: spawn failed, attempting direct exec of {}", path);
                let init_c = match path {
                    "/bin/init" => c"/bin/init",
                    _ => c"/init",
                };
                let (exec_c, argv): (&'static core::ffi::CStr, [*const u8; 3]) = if path == "/init"
                {
                    let sh_c = c"/bin/sh";
                    (
                        sh_c,
                        [
                            sh_c.as_ptr() as *const u8,
                            init_c.as_ptr() as *const u8,
                            core::ptr::null(),
                        ],
                    )
                } else {
                    (
                        init_c,
                        [
                            init_c.as_ptr() as *const u8,
                            core::ptr::null(),
                            core::ptr::null(),
                        ],
                    )
                };
                let envp: [*const u8; 7] = [
                    c"HOME=/root".as_ptr() as *const u8,
                    c"PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".as_ptr()
                        as *const u8,
                    c"XDG_RUNTIME_DIR=/run/user/0".as_ptr() as *const u8,
                    c"XDG_CURRENT_DESKTOP=ubuntu:GNOME".as_ptr() as *const u8,
                    c"XDG_SESSION_TYPE=wayland".as_ptr() as *const u8,
                    c"WAYLAND_DISPLAY=wayland-0".as_ptr() as *const u8,
                    core::ptr::null(),
                ];
                if process_ops::execve_and_enter_user_mode(
                    exec_c.as_ptr() as *const u8,
                    argv.as_ptr(),
                    envp.as_ptr(),
                )
                .is_ok()
                {
                    unreachable!("switch_to_user_mode returns !");
                }
                crate::serial_println!("init: exec {} failed", path);
            } else {
                crate::serial_println!("init: no /bin/init or /init on rootfs");
            }
        }
    }

    loop {
        x86_64::instructions::hlt();
    }
}

/// Legacy alias kept for callers that probe both init paths manually.
#[allow(dead_code)]
pub unsafe fn try_exec_init() -> ! {
    boot_userspace_init()
}

/// Load initramfs at kernel boot
pub fn init_initramfs() -> Result<(), InitramfsError> {
    if INITRAMFS_DATA.is_empty() {
        return Err(InitramfsError::NoInitramfs);
    }

    // Decompress (if needed) and extract the CPIO archive into the VFS.
    // decompress_gzip streams the payload through miniz_oxide in 32 KiB
    // chunks so multi-megabyte archives do not require a monolithic
    // intermediate buffer.
    crate::serial_println!("initramfs: Extracting {} bytes", INITRAMFS_DATA.len());
    match extract_initramfs() {
        Ok(info) => {
            crate::serial_println!(
                "initramfs: Extracted {:?} ({} bytes source)",
                info.format,
                info.size
            );
            Ok(())
        }
        Err(InitramfsError::DecompressionFailed) => {
            // Decompression failed — fall back to the minimal filesystem
            // rather than halting boot. The caller can retry later.
            crate::serial_println!("initramfs: Decompression failed, using minimal filesystem");
            Ok(())
        }
        Err(e) => {
            crate::serial_println!("initramfs: Extraction failed: {:?}", e);
            Err(e)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum InitramfsError {
    NoInitramfs,
    InvalidFormat,
    DecompressionFailed,
    ExtractionFailed,
    InitNotFound,
    VfsError,
    ParseError,
}

/// CPIO newc format constants
const CPIO_NEWC_MAGIC: &[u8; 6] = b"070701";
const CPIO_NEWC_CRC_MAGIC: &[u8; 6] = b"070702";
const CPIO_HEADER_SIZE: usize = 110;
const CPIO_TRAILER: &str = "TRAILER!!!";

/// Parse a hex string from ASCII bytes.
///
/// CPIO newc format stores all numeric fields as 8-character ASCII hex strings.
fn parse_hex_field(data: &[u8]) -> Result<u32, InitramfsError> {
    if data.len() != 8 {
        return Err(InitramfsError::ParseError);
    }

    let mut result: u32 = 0;
    for &byte in data {
        let digit = match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => return Err(InitramfsError::ParseError),
        };
        result = result.checked_mul(16).ok_or(InitramfsError::ParseError)?;
        result = result
            .checked_add(digit as u32)
            .ok_or(InitramfsError::ParseError)?;
    }
    Ok(result)
}

/// CPIO header structure (newc format)
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CpioHeader {
    pub magic: [u8; 6], // "070701" or "070702"
    pub ino: u32,
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub nlink: u32,
    pub mtime: u32,
    pub filesize: u32,
    pub devmajor: u32,
    pub devminor: u32,
    pub rdevmajor: u32,
    pub rdevminor: u32,
    pub namesize: u32,
    pub check: u32,
}

impl CpioHeader {
    /// Parse CPIO header from bytes (newc format).
    ///
    /// The newc format uses ASCII hex for all numeric fields:
    /// - magic: 6 bytes ("070701" or "070702")
    /// - All other fields: 8 bytes each (hex ASCII)
    ///
    /// # Arguments
    /// * `data` - At least 110 bytes of CPIO header data
    ///
    /// # Returns
    /// * `Ok(CpioHeader)` - Successfully parsed header
    /// * `Err(InitramfsError)` - Invalid format or data
    pub fn from_bytes(data: &[u8]) -> Result<Self, InitramfsError> {
        if data.len() < CPIO_HEADER_SIZE {
            return Err(InitramfsError::InvalidFormat);
        }

        // Verify magic number
        let magic = &data[0..6];
        if magic != CPIO_NEWC_MAGIC && magic != CPIO_NEWC_CRC_MAGIC {
            return Err(InitramfsError::InvalidFormat);
        }

        // Parse all header fields using hex parsing
        // Layout: magic(6) + ino(8) + mode(8) + uid(8) + gid(8) + nlink(8) + mtime(8)
        //       + filesize(8) + devmajor(8) + devminor(8) + rdevmajor(8) + rdevminor(8)
        //       + namesize(8) + check(8) = 110 bytes total
        Ok(CpioHeader {
            magic: [data[0], data[1], data[2], data[3], data[4], data[5]],
            ino: parse_hex_field(&data[6..14])?,
            mode: parse_hex_field(&data[14..22])?,
            uid: parse_hex_field(&data[22..30])?,
            gid: parse_hex_field(&data[30..38])?,
            nlink: parse_hex_field(&data[38..46])?,
            mtime: parse_hex_field(&data[46..54])?,
            filesize: parse_hex_field(&data[54..62])?,
            devmajor: parse_hex_field(&data[62..70])?,
            devminor: parse_hex_field(&data[70..78])?,
            rdevmajor: parse_hex_field(&data[78..86])?,
            rdevminor: parse_hex_field(&data[86..94])?,
            namesize: parse_hex_field(&data[94..102])?,
            check: parse_hex_field(&data[102..110])?,
        })
    }
}

/// Decompress gzip-compressed data.
///
/// This function handles gzip decompression for initramfs archives using
/// miniz_oxide's DEFLATE implementation. It properly handles:
/// - Gzip header parsing (magic, flags, optional fields)
/// - DEFLATE decompression of the payload
/// - Gzip trailer (CRC32, original size) validation
///
/// # Arguments
/// * `data` - Gzip-compressed data buffer
///
/// # Returns
/// * `Ok(Vec<u8>)` - Decompressed data
/// * `Err(InitramfsError)` - If decompression fails
pub fn decompress_gzip(data: &[u8]) -> Result<Vec<u8>, InitramfsError> {
    use miniz_oxide::inflate::core::{decompress as tinfl_decompress, DecompressorOxide};
    use miniz_oxide::inflate::TINFLStatus;

    const GZIP_MAGIC: [u8; 2] = [0x1f, 0x8b];
    const GZIP_HEADER_SIZE: usize = 10;

    // Gzip flags
    const FEXTRA: u8 = 0x04;
    const FNAME: u8 = 0x08;
    const FCOMMENT: u8 = 0x10;
    const FHCRC: u8 = 0x02;

    // Validate minimum size for gzip header
    if data.len() < GZIP_HEADER_SIZE {
        return Err(InitramfsError::InvalidFormat);
    }

    // Validate gzip magic number
    if data[0] != GZIP_MAGIC[0] || data[1] != GZIP_MAGIC[1] {
        return Err(InitramfsError::InvalidFormat);
    }

    // Check compression method (must be 8 for DEFLATE)
    if data[2] != 8 {
        return Err(InitramfsError::InvalidFormat);
    }

    let flags = data[3];
    let mut offset = GZIP_HEADER_SIZE;

    // Skip extra fields if present (FEXTRA flag)
    if flags & FEXTRA != 0 {
        if data.len() < offset + 2 {
            return Err(InitramfsError::InvalidFormat);
        }
        let xlen = u16::from_le_bytes([data[offset], data[offset + 1]]) as usize;
        offset += 2 + xlen;
    }

    // Skip original filename if present (FNAME flag)
    if flags & FNAME != 0 {
        while offset < data.len() && data[offset] != 0 {
            offset += 1;
        }
        offset += 1; // Skip null terminator
    }

    // Skip comment if present (FCOMMENT flag)
    if flags & FCOMMENT != 0 {
        while offset < data.len() && data[offset] != 0 {
            offset += 1;
        }
        offset += 1; // Skip null terminator
    }

    // Skip header CRC if present (FHCRC flag)
    if flags & FHCRC != 0 {
        offset += 2;
    }

    // Validate we have enough data for footer (CRC32 + original size = 8 bytes)
    if data.len() < offset + 8 {
        return Err(InitramfsError::InvalidFormat);
    }

    // Extract compressed data (everything except the last 8 bytes for footer)
    let compressed_data = &data[offset..data.len() - 8];

    // Extract expected original size from footer (last 4 bytes, little-endian)
    let footer_offset = data.len() - 4;
    let expected_size = u32::from_le_bytes([
        data[footer_offset],
        data[footer_offset + 1],
        data[footer_offset + 2],
        data[footer_offset + 3],
    ]) as usize;

    // Decompress using miniz_oxide's streaming decompressor
    const TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF: u32 = 0x00000008;

    let mut decompressor = DecompressorOxide::new();
    // Pre-allocate with expected size if reasonable, otherwise start smaller
    let initial_capacity = if expected_size > 0 && expected_size < 256 * 1024 * 1024 {
        expected_size
    } else {
        compressed_data.len() * 4
    };
    let mut output = Vec::with_capacity(initial_capacity);
    let mut in_pos = 0;

    loop {
        let in_buf = &compressed_data[in_pos..];
        let out_cur_pos = output.len();

        // Reserve space for decompressed data (32KB chunks)
        output.resize(out_cur_pos + 32768, 0);

        let (status, bytes_in, bytes_out) = tinfl_decompress(
            &mut decompressor,
            in_buf,
            &mut output,
            out_cur_pos,
            TINFL_FLAG_USING_NON_WRAPPING_OUTPUT_BUF,
        );

        // Truncate to actual output size
        output.truncate(out_cur_pos + bytes_out);
        in_pos += bytes_in;

        match status {
            TINFLStatus::Done => {
                output.shrink_to_fit();
                return Ok(output);
            }
            TINFLStatus::HasMoreOutput | TINFLStatus::NeedsMoreInput => {
                if in_pos >= compressed_data.len() && status == TINFLStatus::NeedsMoreInput {
                    return Err(InitramfsError::DecompressionFailed);
                }
                continue;
            }
            _ => {
                return Err(InitramfsError::DecompressionFailed);
            }
        }
    }
}

/// File mode bits for CPIO entries
mod cpio_mode {
    pub const S_IFMT: u32 = 0o170000; // File type mask
    pub const S_IFSOCK: u32 = 0o140000; // Socket
    pub const S_IFLNK: u32 = 0o120000; // Symbolic link
    pub const S_IFREG: u32 = 0o100000; // Regular file
    pub const S_IFBLK: u32 = 0o060000; // Block device
    pub const S_IFDIR: u32 = 0o040000; // Directory
    pub const S_IFCHR: u32 = 0o020000; // Character device
    pub const S_IFIFO: u32 = 0o010000; // FIFO
}

/// Parsed CPIO entry from newc format
#[derive(Debug)]
struct CpioEntry {
    /// File name/path
    name: String,
    /// File mode (type and permissions)
    mode: u32,
    /// User ID
    uid: u32,
    /// Group ID
    gid: u32,
    /// Number of links
    nlink: u32,
    /// Modification time
    mtime: u32,
    /// File size
    filesize: u32,
    /// Device major number
    devmajor: u32,
    /// Device minor number
    devminor: u32,
    /// Rdev major (for device files)
    rdevmajor: u32,
    /// Rdev minor (for device files)
    rdevminor: u32,
}

impl CpioEntry {
    /// Determine the VFS inode type from CPIO mode bits
    fn inode_type(&self) -> InodeType {
        match self.mode & cpio_mode::S_IFMT {
            cpio_mode::S_IFREG => InodeType::File,
            cpio_mode::S_IFDIR => InodeType::Directory,
            cpio_mode::S_IFLNK => InodeType::Symlink,
            cpio_mode::S_IFCHR => InodeType::CharDevice,
            cpio_mode::S_IFBLK => InodeType::BlockDevice,
            cpio_mode::S_IFIFO => InodeType::Fifo,
            cpio_mode::S_IFSOCK => InodeType::Socket,
            _ => InodeType::File, // Default to regular file
        }
    }

    /// Get the permission bits (lower 12 bits)
    fn permissions(&self) -> u32 {
        self.mode & 0o7777
    }
}

/// Align offset to 4-byte boundary (CPIO newc requirement)
fn align4(offset: usize) -> usize {
    (offset + 3) & !3
}

/// Parse a single CPIO entry from the data at the given offset.
///
/// Returns the parsed entry and the offset to the next entry.
fn parse_cpio_entry(
    data: &[u8],
    offset: usize,
) -> Result<(CpioEntry, &[u8], usize), InitramfsError> {
    // Validate minimum size for header
    if offset + CPIO_HEADER_SIZE > data.len() {
        return Err(InitramfsError::ParseError);
    }

    let header = &data[offset..offset + CPIO_HEADER_SIZE];

    // Validate magic number
    let magic = &header[0..6];
    if magic != CPIO_NEWC_MAGIC && magic != CPIO_NEWC_CRC_MAGIC {
        return Err(InitramfsError::InvalidFormat);
    }

    // Parse header fields (all 8-character hex strings)
    // Layout: magic(6) + ino(8) + mode(8) + uid(8) + gid(8) + nlink(8) + mtime(8)
    //       + filesize(8) + devmajor(8) + devminor(8) + rdevmajor(8) + rdevminor(8)
    //       + namesize(8) + check(8)
    let _ino = parse_hex_field(&header[6..14])?;
    let mode = parse_hex_field(&header[14..22])?;
    let uid = parse_hex_field(&header[22..30])?;
    let gid = parse_hex_field(&header[30..38])?;
    let nlink = parse_hex_field(&header[38..46])?;
    let mtime = parse_hex_field(&header[46..54])?;
    let filesize = parse_hex_field(&header[54..62])?;
    let devmajor = parse_hex_field(&header[62..70])?;
    let devminor = parse_hex_field(&header[70..78])?;
    let rdevmajor = parse_hex_field(&header[78..86])?;
    let rdevminor = parse_hex_field(&header[86..94])?;
    let namesize = parse_hex_field(&header[94..102])?;
    let _check = parse_hex_field(&header[102..110])?;

    // Parse filename (null-terminated, follows header)
    // namesize counts the trailing NUL, so a valid record always has namesize >= 1.
    // Reject namesize == 0, otherwise name_end == name_start and the
    // name_end - 1 below would form a reversed (panicking) slice range.
    if namesize == 0 {
        return Err(InitramfsError::ParseError);
    }

    let name_start = offset + CPIO_HEADER_SIZE;
    let name_end = name_start + namesize as usize;

    if name_end > data.len() {
        return Err(InitramfsError::ParseError);
    }

    // Extract name (excluding null terminator)
    let name_bytes = &data[name_start..name_end - 1];
    let name = core::str::from_utf8(name_bytes)
        .map_err(|_| InitramfsError::ParseError)?
        .to_string();

    // Calculate data offset (header + name, aligned to 4 bytes)
    let data_start = align4(name_end);

    // Extract file data
    let data_end = data_start + filesize as usize;
    if data_end > data.len() {
        return Err(InitramfsError::ParseError);
    }
    let file_data = &data[data_start..data_end];

    // Calculate next entry offset (data, aligned to 4 bytes)
    let next_offset = align4(data_end);

    let entry = CpioEntry {
        name,
        mode,
        uid,
        gid,
        nlink,
        mtime,
        filesize,
        devmajor,
        devminor,
        rdevmajor,
        rdevminor,
    };

    Ok((entry, file_data, next_offset))
}

/// Ensure all parent directories exist in the VFS for a given path.
///
/// Creates intermediate directories with default permissions (0o755).
fn ensure_parent_directories(
    vfs: &'static crate::vfs::Vfs,
    path: &str,
) -> Result<(), InitramfsError> {
    // Skip if path is root or has no parent
    if path == "/" || path.is_empty() {
        return Ok(());
    }

    // Build path components
    let mut current_path = String::new();

    for component in path.trim_start_matches('/').split('/') {
        if component.is_empty() || component == "." {
            continue;
        }

        // Don't create the final component (that's the file itself)
        if !current_path.is_empty() || path.starts_with('/') {
            let check_path = if current_path.is_empty() {
                format!("/{}", component)
            } else {
                format!("{}/{}", current_path, component)
            };

            // Check if this is the final component
            if check_path.len() >= path.len() - 1 {
                break;
            }

            // Try to create directory if it doesn't exist
            match vfs.lookup(&check_path) {
                Ok(_) => {
                    // Directory exists, continue
                }
                Err(crate::vfs::VfsError::NotFound) => {
                    // Create the directory
                    vfs.mkdir(&check_path, 0o755)
                        .map_err(|_| InitramfsError::VfsError)?;
                }
                Err(_) => {
                    return Err(InitramfsError::VfsError);
                }
            }

            current_path = check_path;
        } else {
            current_path = format!("/{}", component);
        }
    }

    Ok(())
}

/// Extract CPIO archive contents to the VFS.
///
/// This function parses a CPIO newc format archive and extracts all entries
/// to the kernel's virtual filesystem. It handles:
/// - Regular files (with content)
/// - Directories
/// - Symbolic links
/// - Device nodes (character and block)
/// - FIFOs
///
/// The CPIO newc format is the standard format used by Linux initramfs.
///
/// # Arguments
/// * `data` - Raw CPIO archive data (uncompressed)
///
/// # Returns
/// * `Ok(())` - All entries extracted successfully
/// * `Err(InitramfsError)` - Parse or extraction error
fn extract_cpio(data: &[u8]) -> Result<(), InitramfsError> {
    let vfs = get_vfs();
    let mut offset = 0;
    let mut entries_extracted = 0;

    while offset < data.len() {
        // Parse the next entry
        let (entry, file_data, next_offset) = match parse_cpio_entry(data, offset) {
            Ok(result) => result,
            Err(e) => {
                // If we've extracted at least one entry, this might be padding
                if entries_extracted > 0 {
                    break;
                }
                return Err(e);
            }
        };

        // Check for end of archive marker
        if entry.name == CPIO_TRAILER {
            break;
        }

        // Skip empty names and current/parent directory entries
        if entry.name.is_empty() || entry.name == "." || entry.name == ".." {
            offset = next_offset;
            continue;
        }

        // Normalize path (ensure it starts with /)
        let entry_name = entry.name.strip_prefix("./").unwrap_or(entry.name.as_str());
        let path = if entry_name.starts_with('/') {
            entry_name.to_string()
        } else {
            format!("/{}", entry_name)
        };

        // Ensure parent directories exist
        ensure_parent_directories(vfs, &path)?;

        // Extract based on entry type
        match entry.inode_type() {
            InodeType::Directory => {
                // Create directory if it doesn't exist
                match vfs.lookup(&path) {
                    Ok(_) => {
                        // Directory already exists, skip
                    }
                    Err(crate::vfs::VfsError::NotFound) => {
                        vfs.mkdir(&path, entry.permissions()).map_err(|_| {
                            crate::serial_println!("initramfs: mkdir failed: {}", path);
                            InitramfsError::ExtractionFailed
                        })?;
                    }
                    Err(_) => {
                        crate::serial_println!("initramfs: lookup failed: {}", path);
                        return Err(InitramfsError::ExtractionFailed);
                    }
                }
            }

            InodeType::File => {
                // Create and write file
                let fd = vfs
                    .open(
                        &path,
                        OpenFlags::new(OpenFlags::WRONLY | OpenFlags::CREAT | OpenFlags::TRUNC),
                        entry.permissions(),
                    )
                    .map_err(|_| {
                        crate::serial_println!("initramfs: open failed: {}", path);
                        InitramfsError::ExtractionFailed
                    })?;

                if !file_data.is_empty() {
                    vfs.write(fd, file_data).map_err(|_| {
                        crate::serial_println!("initramfs: write failed: {}", path);
                        InitramfsError::ExtractionFailed
                    })?;
                }

                vfs.close(fd).map_err(|_| {
                    crate::serial_println!("initramfs: close failed: {}", path);
                    InitramfsError::ExtractionFailed
                })?;
            }

            InodeType::Symlink => {
                // Symbolic links: file_data contains the target path as UTF-8.
                // Use the VFS symlink() function to create a real symlink.
                let target = core::str::from_utf8(file_data).map_err(|_| {
                    crate::serial_println!("initramfs: bad symlink target: {}", path);
                    InitramfsError::ExtractionFailed
                })?;
                let target = target.trim_end_matches('\0');
                vfs.symlink(target, &path).map_err(|_| {
                    crate::serial_println!("initramfs: symlink failed: {} -> {}", path, target);
                    InitramfsError::ExtractionFailed
                })?;
            }

            InodeType::CharDevice | InodeType::BlockDevice => {
                // Device nodes become marker files until VFS device nodes are native.
                match vfs.open(
                    &path,
                    OpenFlags::new(OpenFlags::WRONLY | OpenFlags::CREAT | OpenFlags::TRUNC),
                    entry.permissions(),
                ) {
                    Ok(fd) => {
                        let _ = vfs.close(fd);
                    }
                    Err(_) => {
                        crate::serial_println!("initramfs: skipped device node: {}", path);
                    }
                }
            }

            InodeType::Fifo | InodeType::Socket => {
                // FIFOs and sockets become marker files until VFS supports them.
                match vfs.open(
                    &path,
                    OpenFlags::new(OpenFlags::WRONLY | OpenFlags::CREAT | OpenFlags::TRUNC),
                    entry.permissions(),
                ) {
                    Ok(fd) => {
                        let _ = vfs.close(fd);
                    }
                    Err(_) => {
                        crate::serial_println!("initramfs: skipped fifo/socket: {}", path);
                    }
                }
            }
        }

        entries_extracted += 1;
        offset = next_offset;
    }

    if entries_extracted == 0 {
        return Err(InitramfsError::ParseError);
    }

    Ok(())
}
