//! memfd_secret syscall support.
//!
//! Linux secretmem creates a file descriptor whose storage is accessed through
//! mmap. RustOS does not yet have an uncached/direct-map-isolated page class, so
//! this subsystem provides the correct fd lifetime, flag validation, and private
//! anonymous mmap behavior without exposing the memory through VFS read/write.

use alloc::collections::BTreeMap;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::RwLock;

use crate::linux_compat::{LinuxError, LinuxResult};
use crate::memory_manager::{api::vm_mmap, MmapFlags, ProtectionFlags};
use crate::vfs;

const FD_CLOEXEC: u32 = 0x1;
const VALID_FLAGS: u32 = FD_CLOEXEC;

static NEXT_ID: AtomicU32 = AtomicU32::new(1);
static MEMFD_SECRETS: RwLock<BTreeMap<u32, MemfdSecretState>> = RwLock::new(BTreeMap::new());

#[derive(Clone, Copy)]
struct MemfdSecretState {
    flags: u32,
    mappings: usize,
}

pub fn memfd_secret(flags: u32) -> i32 {
    if flags & !VALID_FLAGS != 0 {
        return -(LinuxError::EINVAL as i32);
    }

    let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
    MEMFD_SECRETS
        .write()
        .insert(id, MemfdSecretState { flags, mappings: 0 });

    let mut fd_flags = vfs::OpenFlags::RDWR;
    if flags & FD_CLOEXEC != 0 {
        fd_flags |= vfs::OpenFlags::CLOEXEC;
    }

    crate::linux_compat::special_fd::register_memfd_secret(id, fd_flags)
}

pub fn close_memfd_secret(id: u32) {
    MEMFD_SECRETS.write().remove(&id);
}

pub fn init() {
    MEMFD_SECRETS.write().clear();
    NEXT_ID.store(1, Ordering::SeqCst);
}

pub fn mmap(
    id: u32,
    addr: usize,
    length: usize,
    protection: ProtectionFlags,
    mut flags: MmapFlags,
    offset: usize,
) -> LinuxResult<*mut u8> {
    if offset != 0 {
        return Err(LinuxError::EINVAL);
    }
    if length == 0 {
        return Err(LinuxError::EINVAL);
    }

    {
        let table = MEMFD_SECRETS.read();
        if !table.contains_key(&id) {
            return Err(LinuxError::EBADF);
        }
    }

    flags.shared = false;
    flags.anonymous = true;
    let mapped = vm_mmap(addr, length, protection, flags).map_err(|_| LinuxError::ENOMEM)?;

    if let Some(state) = MEMFD_SECRETS.write().get_mut(&id) {
        state.mappings = state.mappings.saturating_add(1);
    }

    Ok(mapped)
}
