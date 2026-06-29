//! HugeTLB page pool and hugetlbfs support.
//!
//! Maintains a pre-reserved pool of 2 MiB frames and exposes them through
//! `MAP_HUGETLB` mmap and a ramfs-like hugetlbfs mounted at `/dev/hugepages`.

use alloc::{collections::BTreeMap, format, string::String, string::ToString, vec::Vec};
use core::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use spin::RwLock;
use x86_64::{structures::paging::PageTableFlags, PhysAddr};

use crate::fs::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use crate::linux_compat::{LinuxError, LinuxResult};
use crate::memory::{self, HUGEPAGE_SIZE as MEM_HUGEPAGE_SIZE};
use crate::memory_manager::{MmapFlags, ProtectionFlags, VmError};

/// Re-export the canonical huge page size.
pub const HUGEPAGE_SIZE: usize = MEM_HUGEPAGE_SIZE as usize;

/// Default number of 2 MiB pages in the boot-time pool (64 MiB).
const DEFAULT_POOL_PAGES: usize = 32;

static INITIALIZED: AtomicBool = AtomicBool::new(false);
static NEXT_INODE: AtomicU64 = AtomicU64::new(1);
static NEXT_MAP_ADDR: AtomicU64 = AtomicU64::new(0x0000_6000_0000);
static POOL_TOTAL: AtomicUsize = AtomicUsize::new(0);
static POOL_FREE: AtomicUsize = AtomicUsize::new(0);

static HUGEPAGE_POOL: RwLock<Vec<PhysAddr>> = RwLock::new(Vec::new());
static HUGEPAGE_ALLOCATED: RwLock<BTreeMap<PhysAddr, HugeMapping>> = RwLock::new(BTreeMap::new());
static HUGETLB_FILE_BACKINGS: RwLock<BTreeMap<InodeNumber, HugeFileBacking>> =
    RwLock::new(BTreeMap::new());

#[derive(Debug, Clone, Copy)]
struct HugeMapping {
    virt: usize,
    owner_pid: u32,
    owned: bool,
}

#[derive(Debug, Clone)]
struct HugeFileBacking {
    pages: Vec<PhysAddr>,
    size: u64,
}

/// Pool statistics for /proc or debug.
#[derive(Debug, Clone, Copy, Default)]
pub struct HugetlbStats {
    pub total_pages: usize,
    pub free_pages: usize,
    pub mapped_pages: usize,
}

pub fn stats() -> HugetlbStats {
    HugetlbStats {
        total_pages: POOL_TOTAL.load(Ordering::Relaxed),
        free_pages: POOL_FREE.load(Ordering::Relaxed),
        mapped_pages: HUGEPAGE_ALLOCATED.read().len(),
    }
}

fn protection_to_pt_flags(prot: ProtectionFlags) -> PageTableFlags {
    let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
    if prot.is_writable() {
        flags |= PageTableFlags::WRITABLE;
    }
    if !prot.is_executable() {
        flags |= PageTableFlags::NO_EXECUTE;
    }
    flags
}

fn reserve_from_pool(count: usize) -> Result<Vec<PhysAddr>, LinuxError> {
    if count == 0 {
        return Ok(Vec::new());
    }
    let mut pool = HUGEPAGE_POOL.write();
    if pool.len() < count {
        return Err(LinuxError::ENOMEM);
    }
    let start = pool.len() - count;
    let reserved: Vec<PhysAddr> = pool.drain(start..).collect();
    POOL_FREE.store(pool.len(), Ordering::Relaxed);
    Ok(reserved)
}

fn release_to_pool(mut pages: Vec<PhysAddr>) {
    if pages.is_empty() {
        return;
    }
    pages.sort_by_key(|p| p.as_u64());
    let mut pool = HUGEPAGE_POOL.write();
    pool.extend(pages);
    POOL_FREE.store(pool.len(), Ordering::Relaxed);
}

fn choose_map_addr(hint: usize, length: usize, fixed: bool) -> Result<usize, LinuxError> {
    if fixed {
        if hint == 0 || hint % HUGEPAGE_SIZE != 0 {
            return Err(LinuxError::EINVAL);
        }
        return Ok(hint);
    }
    if hint != 0 {
        if hint % HUGEPAGE_SIZE != 0 {
            return Err(LinuxError::EINVAL);
        }
        return Ok(hint);
    }
    let mut addr = NEXT_MAP_ADDR.fetch_add(length as u64, Ordering::SeqCst) as usize;
    addr = (addr + HUGEPAGE_SIZE - 1) & !(HUGEPAGE_SIZE - 1);
    Ok(addr)
}

fn current_pid() -> u32 {
    crate::process::current_pid()
}

/// Initialize the huge page pool from physically contiguous 2 MiB frames.
pub fn init() {
    if INITIALIZED.swap(true, Ordering::SeqCst) {
        return;
    }
    let mut pool = Vec::with_capacity(DEFAULT_POOL_PAGES);
    for _ in 0..DEFAULT_POOL_PAGES {
        if let Some(phys) = memory::allocate_huge_frame() {
            unsafe {
                let mm = crate::memory::get_memory_manager().expect("memory manager");
                let ptr = (mm.physical_memory_offset() + phys.as_u64()).as_mut_ptr::<u8>();
                core::ptr::write_bytes(ptr, 0, HUGEPAGE_SIZE);
            }
            pool.push(phys);
        }
    }
    POOL_TOTAL.store(pool.len(), Ordering::Relaxed);
    POOL_FREE.store(pool.len(), Ordering::Relaxed);
    *HUGEPAGE_POOL.write() = pool;
    HUGEPAGE_ALLOCATED.write().clear();
    HUGETLB_FILE_BACKINGS.write().clear();
    NEXT_INODE.store(1, Ordering::Relaxed);
}

/// Reserve `count` pages for a hugetlbfs inode (used at truncate/create time).
pub fn reserve_file_pages(inode: InodeNumber, count: usize) -> FsResult<()> {
    let pages = reserve_from_pool(count).map_err(|_| FsError::NoSpaceLeft)?;
    let mut table = HUGETLB_FILE_BACKINGS.write();
    let backing = table.entry(inode).or_insert_with(|| HugeFileBacking {
        pages: Vec::new(),
        size: 0,
    });
    backing.pages.extend(pages);
    backing.size = (backing.pages.len() as u64) * HUGEPAGE_SIZE as u64;
    Ok(())
}

/// Release all pages held by a hugetlbfs inode back to the pool.
pub fn release_file_pages(inode: InodeNumber) {
    if let Some(backing) = HUGETLB_FILE_BACKINGS.write().remove(&inode) {
        release_to_pool(backing.pages);
    }
}

fn map_huge_range(
    start_virt: usize,
    pages: &[PhysAddr],
    prot: ProtectionFlags,
    owned: bool,
) -> Result<(), LinuxError> {
    let flags = protection_to_pt_flags(prot);
    let mut virt = start_virt;
    for &phys in pages {
        memory::map_user_huge_page(virt, phys, flags).map_err(|_| LinuxError::ENOMEM)?;
        HUGEPAGE_ALLOCATED.write().insert(
            phys,
            HugeMapping {
                virt,
                owner_pid: current_pid(),
                owned,
            },
        );
        virt += HUGEPAGE_SIZE;
    }
    Ok(())
}

fn unmap_huge_range(start_virt: usize, page_count: usize) -> Result<(), LinuxError> {
    let mut virt = start_virt;
    for _ in 0..page_count {
        if let Ok(Some(phys)) = memory::unmap_user_huge_page(virt) {
            let owned = HUGEPAGE_ALLOCATED
                .write()
                .remove(&phys)
                .map(|m| m.owned)
                .unwrap_or(false);
            if owned {
                release_to_pool(alloc::vec![phys]);
            }
        }
        virt += HUGEPAGE_SIZE;
    }
    Ok(())
}

/// Return true when `[addr, addr+length)` overlaps a hugetlb mapping.
pub fn contains_mapping(addr: usize, length: usize) -> bool {
    if length == 0 {
        return false;
    }
    let end = addr.saturating_add(length);
    HUGEPAGE_ALLOCATED
        .read()
        .values()
        .any(|m| m.virt < end && m.virt.saturating_add(HUGEPAGE_SIZE) > addr)
}

/// `MAP_HUGETLB` entry point from linux_compat memory_ops.
pub fn mmap(
    addr: *mut u8,
    length: usize,
    prot: ProtectionFlags,
    flags: MmapFlags,
    fd: i32,
    offset: usize,
) -> LinuxResult<*mut u8> {
    if length == 0 || length % HUGEPAGE_SIZE != 0 {
        return Err(LinuxError::EINVAL);
    }
    if offset % HUGEPAGE_SIZE != 0 {
        return Err(LinuxError::EINVAL);
    }

    let page_count = length / HUGEPAGE_SIZE;
    let start_virt = choose_map_addr(addr as usize, length, flags.fixed)?;

    if flags.anonymous || fd < 0 {
        let pages = reserve_from_pool(page_count)?;
        if map_huge_range(start_virt, &pages, prot, true).is_err() {
            release_to_pool(pages);
            return Err(LinuxError::ENOMEM);
        }
        return Ok(start_virt as *mut u8);
    }

    // File-backed hugetlbfs mmap: pages must already be reserved on the inode.
    let inode = lookup_fd_inode(fd).ok_or(LinuxError::EBADF)?;
    let table = HUGETLB_FILE_BACKINGS.read();
    let backing = table.get(&inode).ok_or(LinuxError::EINVAL)?;
    let start_index = offset / HUGEPAGE_SIZE;
    let end_index = start_index + page_count;
    if end_index > backing.pages.len() {
        return Err(LinuxError::EINVAL);
    }
    let slice = &backing.pages[start_index..end_index];
    map_huge_range(start_virt, slice, prot, false)?;
    Ok(start_virt as *mut u8)
}

/// Unmap a huge-page mapping and return pages to the pool.
pub fn munmap(addr: *mut u8, length: usize) -> LinuxResult<i32> {
    if addr.is_null() || length == 0 || (addr as usize) % HUGEPAGE_SIZE != 0 {
        return Err(LinuxError::EINVAL);
    }
    if length % HUGEPAGE_SIZE != 0 {
        return Err(LinuxError::EINVAL);
    }
    unmap_huge_range(addr as usize, length / HUGEPAGE_SIZE)?;
    Ok(0)
}

/// Track which hugetlbfs inode a VFS fd refers to (registered at open time).
static FD_INODE_MAP: RwLock<BTreeMap<i32, InodeNumber>> = RwLock::new(BTreeMap::new());

pub fn register_fd_inode(fd: i32, inode: InodeNumber) {
    FD_INODE_MAP.write().insert(fd, inode);
}

pub fn unregister_fd_inode(fd: i32) {
    FD_INODE_MAP.write().remove(&fd);
}

pub fn lookup_fd_inode(fd: i32) -> Option<InodeNumber> {
    FD_INODE_MAP.read().get(&fd).copied()
}

// ── hugetlbfs (ramfs-like, backed by the huge page pool) ─────────────────

#[derive(Debug)]
struct HugetlbInode {
    metadata: FileMetadata,
    entries: BTreeMap<String, InodeNumber>,
}

#[derive(Debug)]
pub struct HugetlbFs {
    inodes: RwLock<BTreeMap<InodeNumber, HugetlbInode>>,
    next_inode: RwLock<u64>,
}

impl HugetlbFs {
    pub fn new() -> Self {
        let fs = Self {
            inodes: RwLock::new(BTreeMap::new()),
            next_inode: RwLock::new(1),
        };
        fs.create_dir_node(1, FilePermissions::from_octal(0o1777));
        fs
    }

    fn alloc_inode(&self) -> InodeNumber {
        let mut next = self.next_inode.write();
        let inode = *next;
        *next += 1;
        inode
    }

    fn create_dir_node(&self, inode: InodeNumber, permissions: FilePermissions) {
        let mut inodes = self.inodes.write();
        inodes.insert(
            inode,
            HugetlbInode {
                metadata: FileMetadata {
                    inode,
                    file_type: FileType::Directory,
                    size: 0,
                    permissions,
                    uid: 0,
                    gid: 0,
                    created: get_current_time(),
                    modified: get_current_time(),
                    accessed: get_current_time(),
                    link_count: 2,
                    device_id: None,
                },
                entries: {
                    let mut e = BTreeMap::new();
                    e.insert(".".to_string(), inode);
                    e
                },
            },
        );
    }

    fn reserve_size(&self, inode: InodeNumber, new_size: u64) -> FsResult<()> {
        if new_size % HUGEPAGE_SIZE as u64 != 0 {
            return Err(FsError::InvalidArgument);
        }
        let needed = (new_size / HUGEPAGE_SIZE as u64) as usize;
        let current = HUGETLB_FILE_BACKINGS
            .read()
            .get(&inode)
            .map(|b| b.pages.len())
            .unwrap_or(0);
        if needed > current {
            reserve_file_pages(inode, needed - current)?;
        } else if needed < current {
            let mut table = HUGETLB_FILE_BACKINGS.write();
            if let Some(backing) = table.get_mut(&inode) {
                let released: Vec<PhysAddr> = backing.pages.drain(needed..).collect();
                backing.size = new_size;
                release_to_pool(released);
            }
        } else if let Some(backing) = HUGETLB_FILE_BACKINGS.write().get_mut(&inode) {
            backing.size = new_size;
        }
        Ok(())
    }
}

impl FileSystem for HugetlbFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::HugetlbFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        Ok(FileSystemStats {
            total_blocks: POOL_TOTAL.load(Ordering::Relaxed) as u64,
            free_blocks: POOL_FREE.load(Ordering::Relaxed) as u64,
            available_blocks: POOL_FREE.load(Ordering::Relaxed) as u64,
            total_inodes: self.inodes.read().len() as u64,
            free_inodes: 0,
            block_size: HUGEPAGE_SIZE as u32,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, name) = split_path(path)?;
        let parent_inode = self.open(&parent_path, OpenFlags::read_only())?;
        let mut inodes = self.inodes.write();
        {
            let parent = inodes.get(&parent_inode).ok_or(FsError::NotFound)?;
            if parent.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            if parent.entries.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
        }
        let child_inode = {
            let mut next = self.next_inode.write();
            let inode = *next;
            *next += 1;
            inode
        };
        inodes.insert(
            child_inode,
            HugetlbInode {
                metadata: FileMetadata {
                    inode: child_inode,
                    file_type: FileType::Regular,
                    size: 0,
                    permissions,
                    uid: 0,
                    gid: 0,
                    created: get_current_time(),
                    modified: get_current_time(),
                    accessed: get_current_time(),
                    link_count: 1,
                    device_id: None,
                },
                entries: BTreeMap::new(),
            },
        );
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.insert(name, child_inode);
        Ok(child_inode)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        let inodes = self.inodes.read();
        let mut current_inode = 1;
        for part in path.split('/').filter(|s| !s.is_empty()) {
            let node = inodes.get(&current_inode).ok_or(FsError::NotFound)?;
            if node.metadata.file_type != FileType::Directory {
                return Err(FsError::NotFound);
            }
            current_inode = *node.entries.get(part).ok_or(FsError::NotFound)?;
        }
        Ok(current_inode)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Regular {
            return Err(FsError::InvalidArgument);
        }
        if offset >= node.metadata.size {
            return Ok(0);
        }
        let len = core::cmp::min(buffer.len(), (node.metadata.size - offset) as usize);
        buffer[..len].fill(0);
        Ok(len)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        let new_size = offset.saturating_add(buffer.len() as u64);
        let aligned =
            ((new_size + HUGEPAGE_SIZE as u64 - 1) / HUGEPAGE_SIZE as u64) * HUGEPAGE_SIZE as u64;
        self.reserve_size(inode, aligned)?;
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.metadata.size = aligned;
        node.metadata.modified = get_current_time();
        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        self.inodes
            .read()
            .get(&inode)
            .map(|n| n.metadata.clone())
            .ok_or(FsError::NotFound)
    }

    fn set_metadata(&self, inode: InodeNumber, metadata: &FileMetadata) -> FsResult<()> {
        if metadata.size % HUGEPAGE_SIZE as u64 != 0 {
            return Err(FsError::InvalidArgument);
        }
        self.reserve_size(inode, metadata.size)?;
        let mut inodes = self.inodes.write();
        let node = inodes.get_mut(&inode).ok_or(FsError::NotFound)?;
        node.metadata.size = metadata.size;
        node.metadata.permissions = metadata.permissions;
        node.metadata.modified = get_current_time();
        Ok(())
    }

    fn mkdir(&self, path: &str, permissions: FilePermissions) -> FsResult<InodeNumber> {
        let (parent_path, name) = split_path(path)?;
        let parent_inode = self.open(&parent_path, OpenFlags::read_only())?;
        let mut inodes = self.inodes.write();
        {
            let parent = inodes.get(&parent_inode).ok_or(FsError::NotFound)?;
            if parent.metadata.file_type != FileType::Directory {
                return Err(FsError::NotADirectory);
            }
            if parent.entries.contains_key(&name) {
                return Err(FsError::AlreadyExists);
            }
        }
        let child_inode = {
            let mut next = self.next_inode.write();
            let inode = *next;
            *next += 1;
            inode
        };
        let mut entries = BTreeMap::new();
        entries.insert(".".to_string(), child_inode);
        entries.insert("..".to_string(), parent_inode);
        inodes.insert(
            child_inode,
            HugetlbInode {
                metadata: FileMetadata {
                    inode: child_inode,
                    file_type: FileType::Directory,
                    size: 0,
                    permissions,
                    uid: 0,
                    gid: 0,
                    created: get_current_time(),
                    modified: get_current_time(),
                    accessed: get_current_time(),
                    link_count: 2,
                    device_id: None,
                },
                entries,
            },
        );
        let parent = inodes.get_mut(&parent_inode).ok_or(FsError::NotFound)?;
        parent.entries.insert(name, child_inode);
        parent.metadata.link_count += 1;
        Ok(child_inode)
    }

    fn rmdir(&self, path: &str) -> FsResult<()> {
        let inode = self.open(path, OpenFlags::read_only())?;
        let mut inodes = self.inodes.write();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        if node.entries.len() > 2 {
            return Err(FsError::DirectoryNotEmpty);
        }
        let (parent_path, name) = split_path(path)?;
        let parent_inode = self.open(&parent_path, OpenFlags::read_only())?;
        inodes.remove(&inode);
        if let Some(parent) = inodes.get_mut(&parent_inode) {
            parent.entries.remove(&name);
        }
        Ok(())
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let inode = self.open(path, OpenFlags::read_only())?;
        release_file_pages(inode);
        let (parent_path, name) = split_path(path)?;
        let parent_inode = self.open(&parent_path, OpenFlags::read_only())?;
        let mut inodes = self.inodes.write();
        inodes.remove(&inode);
        if let Some(parent) = inodes.get_mut(&parent_inode) {
            parent.entries.remove(&name);
        }
        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        let inodes = self.inodes.read();
        let node = inodes.get(&inode).ok_or(FsError::NotFound)?;
        if node.metadata.file_type != FileType::Directory {
            return Err(FsError::NotADirectory);
        }
        let mut entries = Vec::new();
        for (name, &child_inode) in &node.entries {
            let child = inodes.get(&child_inode).ok_or(FsError::NotFound)?;
            entries.push(DirectoryEntry {
                name: name.clone(),
                inode: child_inode,
                file_type: child.metadata.file_type,
            });
        }
        Ok(entries)
    }

    fn rename(&self, old_path: &str, new_path: &str) -> FsResult<()> {
        let inode = self.open(old_path, OpenFlags::read_only())?;
        let (old_parent, old_name) = split_path(old_path)?;
        let (new_parent, new_name) = split_path(new_path)?;
        let old_parent_inode = self.open(&old_parent, OpenFlags::read_only())?;
        let new_parent_inode = self.open(&new_parent, OpenFlags::read_only())?;
        let mut inodes = self.inodes.write();
        if let Some(new_parent_node) = inodes.get(&new_parent_inode) {
            if new_parent_node.entries.contains_key(&new_name) {
                return Err(FsError::AlreadyExists);
            }
        }
        if let Some(old_parent_node) = inodes.get_mut(&old_parent_inode) {
            old_parent_node.entries.remove(&old_name);
        }
        if let Some(new_parent_node) = inodes.get_mut(&new_parent_inode) {
            new_parent_node.entries.insert(new_name, inode);
        }
        Ok(())
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readlink(&self, _path: &str) -> FsResult<String> {
        Err(FsError::NotSupported)
    }

    fn sync(&self) -> FsResult<()> {
        Ok(())
    }
}

fn split_path(path: &str) -> FsResult<(String, String)> {
    let trimmed = path.trim_matches('/');
    if trimmed.is_empty() {
        return Err(FsError::InvalidArgument);
    }
    if let Some(idx) = trimmed.rfind('/') {
        Ok((
            format!("/{}", &trimmed[..idx]),
            trimmed[idx + 1..].to_string(),
        ))
    } else {
        Ok(("/".to_string(), trimmed.to_string()))
    }
}

/// Convert VM errors for callers that sit above the hugetlb layer.
pub fn vm_error_to_linux(err: VmError) -> LinuxError {
    match err {
        VmError::OutOfMemory => LinuxError::ENOMEM,
        VmError::InvalidAddress | VmError::InvalidSize | VmError::NotAligned => LinuxError::EINVAL,
        VmError::PermissionDenied => LinuxError::EACCES,
        _ => LinuxError::EINVAL,
    }
}
