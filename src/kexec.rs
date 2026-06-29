//! kexec image loading and staging.
//!
//! This ports the load-time half of Linux kexec: syscall argument validation,
//! segment copying, file-based image capture, and unload semantics. RustOS does
//! not yet have the architecture handoff code that disables devices, installs
//! identity mappings, and jumps to the loaded image, so this module deliberately
//! exposes a staged image but does not execute it.

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use spin::RwLock;

use crate::linux_compat::LinuxError;
use crate::memory::user_space::UserSpaceMemory;
use crate::process;
use crate::vfs;
use x86_64::{
    structures::paging::{FrameAllocator, Page, PageTableFlags, PhysFrame, Size4KiB},
    PhysAddr, VirtAddr,
};

const KEXEC_ON_CRASH: u64 = 0x0000_0001;
const KEXEC_PRESERVE_CONTEXT: u64 = 0x0000_0002;
const KEXEC_UPDATE_ELFCOREHDR: u64 = 0x0000_0004;
const KEXEC_CRASH_HOTPLUG_SUPPORT: u64 = 0x0000_0008;
const KEXEC_ARCH_MASK: u64 = 0xffff_0000;
const KEXEC_ARCH_DEFAULT: u64 = 0 << 16;
const KEXEC_ARCH_X86_64: u64 = 62 << 16;
const KEXEC_VALID_FLAGS: u64 = KEXEC_ON_CRASH
    | KEXEC_PRESERVE_CONTEXT
    | KEXEC_UPDATE_ELFCOREHDR
    | KEXEC_CRASH_HOTPLUG_SUPPORT
    | KEXEC_ARCH_MASK;

const KEXEC_FILE_UNLOAD: u64 = 0x0000_0001;
const KEXEC_FILE_ON_CRASH: u64 = 0x0000_0002;
const KEXEC_FILE_NO_INITRAMFS: u64 = 0x0000_0004;
const KEXEC_FILE_DEBUG: u64 = 0x0000_0008;
const KEXEC_FILE_NO_CMA: u64 = 0x0000_0010;
const KEXEC_FILE_FORCE_DTB: u64 = 0x0000_0020;
const KEXEC_FILE_VALID_FLAGS: u64 = KEXEC_FILE_UNLOAD
    | KEXEC_FILE_ON_CRASH
    | KEXEC_FILE_NO_INITRAMFS
    | KEXEC_FILE_DEBUG
    | KEXEC_FILE_NO_CMA
    | KEXEC_FILE_FORCE_DTB;

const MAX_SEGMENTS: usize = 16;
const MAX_SEGMENT_BYTES: usize = 64 * 1024 * 1024;
const MAX_FILE_IMAGE_BYTES: usize = 128 * 1024 * 1024;
const MAX_CMDLINE_BYTES: usize = 64 * 1024;
const KEXEC_STACK_BYTES: usize = 4096;
const LOW_CANONICAL_LIMIT: u64 = 0x0000_8000_0000_0000;

static STAGED_IMAGE: RwLock<Option<KexecImage>> = RwLock::new(None);

#[derive(Clone, Copy)]
struct PreparedKexec {
    entry: u64,
    stack_top: u64,
    bytes_written: usize,
}

#[derive(Clone)]
pub struct KexecLoadedSegment {
    pub mem: u64,
    pub memsz: usize,
    pub data: Vec<u8>,
}

#[derive(Clone)]
pub enum KexecImageSource {
    Segments {
        entry: u64,
    },
    File {
        cmdline: Vec<u8>,
        initrd: Option<Vec<u8>>,
    },
}

#[derive(Clone)]
pub struct KexecImage {
    pub flags: u64,
    pub source: KexecImageSource,
    pub segments: Vec<KexecLoadedSegment>,
    pub loaded_by_pid: u32,
}

#[derive(Clone, Copy, Default)]
#[repr(C)]
struct KexecSegmentUser {
    buf: u64,
    bufsz: usize,
    mem: u64,
    memsz: usize,
}

fn current_is_privileged() -> bool {
    let pid = process::current_pid();
    process::get_process_manager()
        .get_process(pid)
        .map(|pcb| pcb.euid == 0)
        .unwrap_or(true)
}

fn copy_from_user<T: Copy + Default>(addr: u64) -> Result<T, LinuxError> {
    if addr == 0 {
        return Err(LinuxError::EFAULT);
    }
    let mut value = T::default();
    let bytes = unsafe {
        core::slice::from_raw_parts_mut(
            (&mut value as *mut T) as *mut u8,
            core::mem::size_of::<T>(),
        )
    };
    UserSpaceMemory::copy_from_user(addr, bytes).map_err(|_| LinuxError::EFAULT)?;
    Ok(value)
}

fn copy_user_bytes(addr: u64, len: usize) -> Result<Vec<u8>, LinuxError> {
    if len == 0 {
        return Ok(Vec::new());
    }
    if addr == 0 {
        return Err(LinuxError::EFAULT);
    }
    let mut out = vec![0u8; len];
    UserSpaceMemory::copy_from_user(addr, &mut out).map_err(|_| LinuxError::EFAULT)?;
    Ok(out)
}

fn validate_kexec_flags(flags: u64) -> Result<(), LinuxError> {
    if flags & !KEXEC_VALID_FLAGS != 0 {
        return Err(LinuxError::EINVAL);
    }
    let arch = flags & KEXEC_ARCH_MASK;
    if arch != KEXEC_ARCH_DEFAULT && arch != KEXEC_ARCH_X86_64 {
        return Err(LinuxError::EINVAL);
    }
    Ok(())
}

fn validate_file_flags(flags: u64) -> Result<(), LinuxError> {
    if flags & !KEXEC_FILE_VALID_FLAGS != 0 {
        return Err(LinuxError::EINVAL);
    }
    Ok(())
}

fn validate_segment(seg: KexecSegmentUser) -> Result<(), LinuxError> {
    if seg.bufsz > seg.memsz || seg.bufsz > MAX_SEGMENT_BYTES || seg.memsz > MAX_SEGMENT_BYTES {
        return Err(LinuxError::EINVAL);
    }
    if seg.bufsz > 0 && seg.buf == 0 {
        return Err(LinuxError::EFAULT);
    }
    if seg.mem == 0 || seg.mem & 0xfff != 0 || seg.memsz & 0xfff != 0 {
        return Err(LinuxError::EINVAL);
    }
    seg.mem
        .checked_add(seg.memsz as u64)
        .ok_or(LinuxError::EINVAL)?;
    Ok(())
}

fn ranges_overlap(a: &KexecLoadedSegment, b: &KexecLoadedSegment) -> bool {
    let a_end = a.mem.saturating_add(a.memsz as u64);
    let b_end = b.mem.saturating_add(b.memsz as u64);
    a.mem < b_end && b.mem < a_end
}

fn read_fd_all(fd: i32, max_bytes: usize) -> Result<Vec<u8>, LinuxError> {
    if fd < 0 {
        return Err(LinuxError::EBADF);
    }
    let stat = vfs::vfs_fstat(fd).map_err(|_| LinuxError::EBADF)?;
    if stat.size as usize > max_bytes {
        return Err(LinuxError::EFBIG);
    }

    let mut out = vec![0u8; stat.size as usize];
    let mut offset = 0usize;
    while offset < out.len() {
        let n =
            vfs::vfs_pread(fd, &mut out[offset..], offset as u64).map_err(|_| LinuxError::EIO)?;
        if n == 0 {
            out.truncate(offset);
            break;
        }
        offset += n;
    }
    Ok(out)
}

fn errno(ret: Result<(), LinuxError>) -> i32 {
    match ret {
        Ok(()) => 0,
        Err(err) => -(err as i32),
    }
}

pub fn kexec_load(entry: u64, nr_segments: usize, segments: *const u8, flags: u64) -> i32 {
    errno(kexec_load_inner(entry, nr_segments, segments as u64, flags))
}

fn kexec_load_inner(
    entry: u64,
    nr_segments: usize,
    segments_addr: u64,
    flags: u64,
) -> Result<(), LinuxError> {
    if !current_is_privileged() {
        return Err(LinuxError::EPERM);
    }
    validate_kexec_flags(flags)?;

    if nr_segments == 0 {
        *STAGED_IMAGE.write() = None;
        return Ok(());
    }
    if nr_segments > MAX_SEGMENTS || segments_addr == 0 || entry == 0 {
        return Err(LinuxError::EINVAL);
    }

    let mut staged_segments = Vec::new();
    for index in 0..nr_segments {
        let seg_addr = segments_addr + (index * core::mem::size_of::<KexecSegmentUser>()) as u64;
        let seg: KexecSegmentUser = copy_from_user(seg_addr)?;
        validate_segment(seg)?;
        let data = copy_user_bytes(seg.buf, seg.bufsz)?;
        let loaded = KexecLoadedSegment {
            mem: seg.mem,
            memsz: seg.memsz,
            data,
        };
        if staged_segments
            .iter()
            .any(|old| ranges_overlap(old, &loaded))
        {
            return Err(LinuxError::EINVAL);
        }
        staged_segments.push(loaded);
    }

    *STAGED_IMAGE.write() = Some(KexecImage {
        flags,
        source: KexecImageSource::Segments { entry },
        segments: staged_segments,
        loaded_by_pid: process::current_pid(),
    });
    Ok(())
}

pub fn kexec_file_load(
    kernel_fd: i32,
    initrd_fd: i32,
    cmdline_len: usize,
    cmdline: *const u8,
    flags: u64,
) -> i32 {
    errno(kexec_file_load_inner(
        kernel_fd,
        initrd_fd,
        cmdline_len,
        cmdline as u64,
        flags,
    ))
}

fn kexec_file_load_inner(
    kernel_fd: i32,
    initrd_fd: i32,
    cmdline_len: usize,
    cmdline_addr: u64,
    flags: u64,
) -> Result<(), LinuxError> {
    if !current_is_privileged() {
        return Err(LinuxError::EPERM);
    }
    validate_file_flags(flags)?;

    if flags & KEXEC_FILE_UNLOAD != 0 {
        *STAGED_IMAGE.write() = None;
        return Ok(());
    }
    if cmdline_len > MAX_CMDLINE_BYTES {
        return Err(LinuxError::E2BIG);
    }

    let kernel = read_fd_all(kernel_fd, MAX_FILE_IMAGE_BYTES)?;
    if kernel.is_empty() {
        return Err(LinuxError::ENOEXEC);
    }
    let initrd = if flags & KEXEC_FILE_NO_INITRAMFS != 0 {
        None
    } else if initrd_fd >= 0 {
        Some(read_fd_all(initrd_fd, MAX_FILE_IMAGE_BYTES)?)
    } else {
        None
    };
    let cmdline = copy_user_bytes(cmdline_addr, cmdline_len)?;

    *STAGED_IMAGE.write() = Some(KexecImage {
        flags,
        source: KexecImageSource::File { cmdline, initrd },
        segments: vec![KexecLoadedSegment {
            mem: 0,
            memsz: kernel.len(),
            data: kernel,
        }],
        loaded_by_pid: process::current_pid(),
    });
    Ok(())
}

fn range_end(start: u64, len: usize) -> Result<u64, LinuxError> {
    let end = start.checked_add(len as u64).ok_or(LinuxError::EINVAL)?;
    if end > LOW_CANONICAL_LIMIT {
        return Err(LinuxError::EINVAL);
    }
    Ok(end)
}

fn align_down(addr: u64) -> u64 {
    addr & !(crate::memory::PAGE_SIZE as u64 - 1)
}

fn align_up(addr: u64) -> Result<u64, LinuxError> {
    addr.checked_add(crate::memory::PAGE_SIZE as u64 - 1)
        .map(align_down)
        .ok_or(LinuxError::EINVAL)
}

fn entry_in_segments(entry: u64, segments: &[KexecLoadedSegment]) -> bool {
    segments.iter().any(|seg| {
        let Ok(end) = range_end(seg.mem, seg.memsz) else {
            return false;
        };
        entry >= seg.mem && entry < end
    })
}

fn map_identity_page(
    page_table_manager: &mut crate::memory::PageTableManager,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    phys: u64,
) -> Result<(), LinuxError> {
    let page_addr = align_down(phys);
    let virt = VirtAddr::new(page_addr);
    let expected = PhysAddr::new(page_addr);
    if let Some(mapped) = page_table_manager.translate_addr(virt) {
        return if mapped == expected {
            Ok(())
        } else {
            Err(LinuxError::EBUSY)
        };
    }

    let page: Page<Size4KiB> = Page::containing_address(virt);
    let frame: PhysFrame<Size4KiB> = PhysFrame::containing_address(expected);
    let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    page_table_manager
        .map_page(page, frame, flags, frame_allocator)
        .map_err(|_| LinuxError::ENOMEM)
}

fn map_identity_range(
    page_table_manager: &mut crate::memory::PageTableManager,
    frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    start: u64,
    len: usize,
) -> Result<(), LinuxError> {
    if len == 0 {
        return Ok(());
    }
    let end = range_end(start, len)?;
    let mut page = align_down(start);
    let last = align_up(end)?;
    while page < last {
        map_identity_page(page_table_manager, frame_allocator, page)?;
        page = page
            .checked_add(crate::memory::PAGE_SIZE as u64)
            .ok_or(LinuxError::EINVAL)?;
    }
    Ok(())
}

fn prepare_segment_image(image: &KexecImage) -> Result<PreparedKexec, LinuxError> {
    if image.flags & KEXEC_ON_CRASH != 0 {
        return Err(LinuxError::EINVAL);
    }
    if image.flags & KEXEC_PRESERVE_CONTEXT != 0 {
        return Err(LinuxError::ENOTSUP);
    }

    let entry = match image.source {
        KexecImageSource::Segments { entry } => entry,
        KexecImageSource::File { .. } => return Err(LinuxError::ENOEXEC),
    };
    if !entry_in_segments(entry, &image.segments) {
        return Err(LinuxError::EINVAL);
    }

    let memory_manager = crate::memory::get_memory_manager().ok_or(LinuxError::ENOMEM)?;
    let mut page_table_manager = memory_manager.page_table_manager.lock();
    let mut frame_allocator = memory_manager.frame_allocator.lock();

    for segment in &image.segments {
        range_end(segment.mem, segment.memsz)?;
        map_identity_range(
            &mut page_table_manager,
            &mut *frame_allocator,
            segment.mem,
            segment.memsz,
        )?;
    }
    map_identity_page(&mut page_table_manager, &mut *frame_allocator, entry)?;

    let stack_frame = frame_allocator.allocate_frame().ok_or(LinuxError::ENOMEM)?;
    let stack_phys = stack_frame.start_address().as_u64();
    map_identity_range(
        &mut page_table_manager,
        &mut *frame_allocator,
        stack_phys,
        KEXEC_STACK_BYTES,
    )?;

    let phys_offset = crate::memory::get_physical_memory_offset();
    if phys_offset == 0 {
        return Err(LinuxError::ENOMEM);
    }
    let stack_ptr = phys_offset
        .checked_add(stack_phys)
        .ok_or(LinuxError::EINVAL)? as *mut u8;
    unsafe {
        core::ptr::write_bytes(stack_ptr, 0, KEXEC_STACK_BYTES);
    }

    Ok(PreparedKexec {
        entry,
        stack_top: stack_phys + KEXEC_STACK_BYTES as u64,
        bytes_written: image.segments.iter().map(|seg| seg.memsz).sum(),
    })
}

fn commit_segments(image: &KexecImage) -> Result<(), LinuxError> {
    let phys_offset = crate::memory::get_physical_memory_offset();
    if phys_offset == 0 {
        return Err(LinuxError::ENOMEM);
    }

    for segment in &image.segments {
        range_end(segment.mem, segment.memsz)?;
        if segment.data.len() > segment.memsz {
            return Err(LinuxError::EINVAL);
        }
        let dest = phys_offset
            .checked_add(segment.mem)
            .ok_or(LinuxError::EINVAL)? as *mut u8;
        unsafe {
            if !segment.data.is_empty() {
                core::ptr::copy_nonoverlapping(segment.data.as_ptr(), dest, segment.data.len());
            }
            let zero_len = segment.memsz - segment.data.len();
            if zero_len != 0 {
                core::ptr::write_bytes(dest.add(segment.data.len()), 0, zero_len);
            }
        }
    }

    unsafe {
        core::arch::asm!(
            "mfence",
            "sfence",
            "wbinvd",
            options(nostack, preserves_flags)
        );
    }
    Ok(())
}

unsafe fn jump_to_image(entry: u64, stack_top: u64) -> ! {
    unsafe {
        core::arch::asm!(
            "cli",
            "cld",
            "mov rsp, {stack}",
            "xor rbp, rbp",
            "xor rdi, rdi",
            "xor rsi, rsi",
            "xor rdx, rdx",
            "jmp {entry}",
            stack = in(reg) stack_top,
            entry = in(reg) entry,
            options(noreturn)
        );
    }
}

pub fn execute_loaded_image() -> Result<(), LinuxError> {
    if !current_is_privileged() {
        return Err(LinuxError::EPERM);
    }
    let image = staged_image().ok_or(LinuxError::EINVAL)?;
    let prepared = prepare_segment_image(&image)?;
    crate::serial_println!(
        "[kexec] executing staged image: entry=0x{:x} bytes={}",
        prepared.entry,
        prepared.bytes_written
    );
    let _ = crate::kernel::shutdown();
    commit_segments(&image)?;
    unsafe { jump_to_image(prepared.entry, prepared.stack_top) }
}

pub fn staged_image() -> Option<KexecImage> {
    STAGED_IMAGE.read().clone()
}

pub fn init() {
    *STAGED_IMAGE.write() = None;
    crate::serial_println!("[kexec] image staging subsystem initialized");
}

pub fn staged_summary() -> Option<String> {
    staged_image().map(|image| match image.source {
        KexecImageSource::Segments { entry } => alloc::format!(
            "segments={} entry=0x{:x} flags=0x{:x}",
            image.segments.len(),
            entry,
            image.flags
        ),
        KexecImageSource::File {
            ref cmdline,
            ref initrd,
        } => alloc::format!(
            "file kernel_bytes={} initrd_bytes={} cmdline_bytes={} flags=0x{:x}",
            image
                .segments
                .first()
                .map(|seg| seg.data.len())
                .unwrap_or(0),
            initrd.as_ref().map(|data| data.len()).unwrap_or(0),
            cmdline.len(),
            image.flags
        ),
    })
}
