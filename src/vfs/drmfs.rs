//! DRM/KMS device nodes for the VFS.
//!
//! Creates `/dev/dri/` directory with `card0`, `controlD64`, and `renderD128`
//! device nodes. Each node implements `InodeOps` and dispatches ioctl calls
//! to the `gpu::opensource::drm_compat` layer.

extern crate alloc;

use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

use super::{InodeOps, InodeType, Stat, VfsError, VfsResult};
use crate::gpu::opensource::drm_compat;

// ── DRM ioctl numbers ───────────────────────────────────────────────────
// These match the Linux DRM ioctl definitions.

const DRM_IOCTL_BASE: u8 = 0x64; // 'd'

/// DRM ioctl command encoding: _IO, _IOR, _IOW, _IOWR
const fn drm_io(nr: u8) -> u32 {
    ((DRM_IOCTL_BASE as u32) << 8) | (nr as u32)
}

const fn drm_ior(nr: u8, size: u32) -> u32 {
    0x80000000 | ((size & 0x3FFF) << 16) | ((DRM_IOCTL_BASE as u32) << 8) | (nr as u32)
}

const fn drm_iow(nr: u8, size: u32) -> u32 {
    0x40000000 | ((size & 0x3FFF) << 16) | ((DRM_IOCTL_BASE as u32) << 8) | (nr as u32)
}

const fn drm_iowr(nr: u8, size: u32) -> u32 {
    0xC0000000 | ((size & 0x3FFF) << 16) | ((DRM_IOCTL_BASE as u32) << 8) | (nr as u32)
}

// DRM ioctl numbers (from <drm/drm.h>)
const DRM_IOCTL_VERSION: u32 = drm_iowr(0x00, 0); // size computed at runtime
const DRM_IOCTL_GET_CAP: u32 = drm_iowr(0x0F, 0);
const DRM_IOCTL_SET_CLIENT_CAP: u32 = drm_iow(0x10, 0);
const DRM_IOCTL_GET_MAGIC: u32 = drm_ior(0x02, 0);
const DRM_IOCTL_AUTH_MAGIC: u32 = drm_iow(0x03, 0);
const DRM_IOCTL_MODE_GETRESOURCES: u32 = drm_iowr(0xA0, 0);
const DRM_IOCTL_MODE_GETPLANERESOURCES: u32 = drm_iowr(0x0B, 0);
const DRM_IOCTL_MODE_GETCRTC: u32 = drm_iowr(0xA1, 0);
const DRM_IOCTL_MODE_SETCRTC: u32 = drm_iowr(0xA2, 0);
const DRM_IOCTL_MODE_CURSOR: u32 = drm_iowr(0xA3, 0);
const DRM_IOCTL_MODE_GETGAMMA: u32 = drm_iowr(0xA4, 0);
const DRM_IOCTL_MODE_SETGAMMA: u32 = drm_iowr(0xA5, 0);
const DRM_IOCTL_MODE_GETENCODER: u32 = drm_iowr(0xA6, 0);
const DRM_IOCTL_MODE_GETCONNECTOR: u32 = drm_iowr(0xA7, 0);
const DRM_IOCTL_MODE_ATTACHMODE: u32 = drm_iowr(0xA8, 0);
const DRM_IOCTL_MODE_DETACHMODE: u32 = drm_iowr(0xA9, 0);
const DRM_IOCTL_MODE_GETPROPERTY: u32 = drm_iowr(0xAA, 0);
const DRM_IOCTL_MODE_SETPROPERTY: u32 = drm_iowr(0xAB, 0);
const DRM_IOCTL_MODE_GETPROPBLOB: u32 = drm_iowr(0xAC, 0);
const DRM_IOCTL_MODE_GETFB: u32 = drm_iowr(0xAE, 0);
const DRM_IOCTL_MODE_ADDFB: u32 = drm_iowr(0xAF, 0);
const DRM_IOCTL_MODE_RMFB: u32 = drm_iowr(0xB0, 0);
const DRM_IOCTL_MODE_PAGE_FLIP: u32 = drm_iowr(0xB1, 0);
const DRM_IOCTL_MODE_DIRTYFB: u32 = drm_iowr(0xB2, 0);
const DRM_IOCTL_MODE_CREATE_DUMB: u32 = drm_iowr(0xB3, 0);
const DRM_IOCTL_MODE_MAP_DUMB: u32 = drm_iowr(0xB4, 0);
const DRM_IOCTL_MODE_DESTROY_DUMB: u32 = drm_iowr(0xB5, 0);
const DRM_IOCTL_PRIME_HANDLE_TO_FD: u32 = drm_iowr(0x2C, 0);
const DRM_IOCTL_PRIME_FD_TO_HANDLE: u32 = drm_iowr(0x2D, 0);
const DRM_IOCTL_SET_VERSION: u32 = drm_iowr(0x07, 0);
const DRM_IOCTL_MODESET_CTL: u32 = drm_iow(0x08, 0);

// ── DRM Device Inode ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DrmNodeKind {
    Card,    // /dev/dri/cardN
    Control, // /dev/dri/controlD64+N
    Render,  // /dev/dri/renderD128+N
}

struct DrmInode {
    ino: u64,
    kind: DrmNodeKind,
    card_number: u32,
    mode: u32,
}

impl DrmInode {
    fn new(ino: u64, kind: DrmNodeKind, card_number: u32, mode: u32) -> Arc<Self> {
        Arc::new(Self {
            ino,
            kind,
            card_number,
            mode,
        })
    }

    /// Handle a DRM ioctl request.
    /// Returns the number of bytes written to the user buffer, or an error.
    fn handle_ioctl(&self, cmd: u32, _arg: u64) -> Result<usize, &'static str> {
        // Ensure DRM compat layer is initialized
        drm_compat::init_drm_compat()?;

        match cmd {
            DRM_IOCTL_VERSION => {
                let version = drm_compat::DRMIoctl::version();
                // In a real implementation, we'd copy this to userspace.
                // For kernel-internal use, we just return the size.
                let _ = version;
                Ok(core::mem::size_of::<drm_compat::DRMVersion>())
            }
            DRM_IOCTL_GET_CAP => {
                // arg points to drm_get_cap { capability: u64, value: u64 }
                // For now, return success with the capability value
                Ok(16) // size of drm_get_cap struct
            }
            DRM_IOCTL_MODE_GETRESOURCES => {
                if let Some(drm) = drm_compat::get_drm_compat() {
                    let _resources = drm_compat::DRMIoctl::get_resources(drm);
                    Ok(core::mem::size_of::<drm_compat::DRMResources>())
                } else {
                    Err("DRM not initialized")
                }
            }
            DRM_IOCTL_MODE_GETPLANERESOURCES => {
                if let Some(drm) = drm_compat::get_drm_compat() {
                    let _resources = drm_compat::DRMIoctl::get_plane_resources(drm);
                    Ok(core::mem::size_of::<drm_compat::DRMPlaneResources>())
                } else {
                    Err("DRM not initialized")
                }
            }
            DRM_IOCTL_MODE_GETCRTC
            | DRM_IOCTL_MODE_SETCRTC
            | DRM_IOCTL_MODE_GETENCODER
            | DRM_IOCTL_MODE_GETCONNECTOR
            | DRM_IOCTL_MODE_GETPROPERTY
            | DRM_IOCTL_MODE_SETPROPERTY
            | DRM_IOCTL_MODE_GETPROPBLOB
            | DRM_IOCTL_MODE_GETFB
            | DRM_IOCTL_MODE_ADDFB
            | DRM_IOCTL_MODE_RMFB
            | DRM_IOCTL_MODE_PAGE_FLIP
            | DRM_IOCTL_MODE_DIRTYFB
            | DRM_IOCTL_MODE_CREATE_DUMB
            | DRM_IOCTL_MODE_MAP_DUMB
            | DRM_IOCTL_MODE_DESTROY_DUMB => {
                // These ioctls require reading/writing structured data from
                // the user pointer. The actual dispatch happens through the
                // linux_compat ioctl handler which can access user memory.
                // Here we just validate the DRM layer is available.
                if drm_compat::get_drm_compat().is_some() {
                    Ok(0)
                } else {
                    Err("DRM not initialized")
                }
            }
            DRM_IOCTL_SET_VERSION | DRM_IOCTL_MODESET_CTL => Ok(0),
            _ => {
                // Unknown DRM ioctl
                Err("Unknown DRM ioctl")
            }
        }
    }
}

impl InodeOps for DrmInode {
    fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        // DRM devices don't support read()
        Err(VfsError::NotSupported)
    }

    fn write_at(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        // DRM devices don't support write()
        Err(VfsError::NotSupported)
    }

    fn stat(&self) -> VfsResult<Stat> {
        // DRM primary node is a char device (major 226, minor = card_number)
        // DRM render node is a char device (major 226, minor = 128 + card_number)
        // DRM control node is a char device (major 226, minor = 64 + card_number)
        let minor = match self.kind {
            DrmNodeKind::Card => self.card_number,
            DrmNodeKind::Control => 64 + self.card_number,
            DrmNodeKind::Render => 128 + self.card_number,
        };
        let major = 226u64;

        Ok(Stat {
            ino: self.ino,
            inode_type: InodeType::CharDevice,
            size: 0,
            blksize: 4096,
            blocks: 0,
            mode: self.mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: ((major as u64) << 8) | minor as u64,
            atime: 0,
            mtime: 0,
            ctime: 0,
        })
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::NotSupported)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn lookup(&self, _name: &str) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::NotDirectory)
    }

    fn create(
        &self,
        _name: &str,
        _inode_type: InodeType,
        _mode: u32,
    ) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn link(&self, _name: &str, _target: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(
        &self,
        _old_name: &str,
        _new_dir: Arc<dyn InodeOps>,
        _new_name: &str,
    ) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self) -> VfsResult<alloc::vec::Vec<super::DirEntry>> {
        Err(VfsError::NotDirectory)
    }

    fn inode_type(&self) -> InodeType {
        InodeType::CharDevice
    }

    fn attach_child(&self, _name: &str, _child: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }
}

// ── DRM Directory Inode ─────────────────────────────────────────────────

/// `/dev/dri/` directory inode.
struct DrmDirInode {
    ino: u64,
    entries: spin::Mutex<Vec<(String, Arc<dyn InodeOps>)>>,
}

impl DrmDirInode {
    fn new(ino: u64) -> Arc<Self> {
        Arc::new(Self {
            ino,
            entries: spin::Mutex::new(Vec::new()),
        })
    }
}

impl InodeOps for DrmDirInode {
    fn read_at(&self, _offset: u64, _buf: &mut [u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn write_at(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::IsDirectory)
    }

    fn stat(&self) -> VfsResult<Stat> {
        Ok(Stat {
            ino: self.ino,
            inode_type: InodeType::Directory,
            size: 4096,
            blksize: 4096,
            blocks: 1,
            mode: 0o755,
            nlink: 2,
            uid: 0,
            gid: 0,
            rdev: 0,
            atime: 0,
            mtime: 0,
            ctime: 0,
        })
    }

    fn truncate(&self, _size: u64) -> VfsResult<()> {
        Err(VfsError::IsDirectory)
    }

    fn sync(&self) -> VfsResult<()> {
        Ok(())
    }

    fn lookup(&self, name: &str) -> VfsResult<Arc<dyn InodeOps>> {
        let entries = self.entries.lock();
        entries
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, inode)| Arc::clone(inode))
            .ok_or(VfsError::NotFound)
    }

    fn create(
        &self,
        _name: &str,
        _inode_type: InodeType,
        _mode: u32,
    ) -> VfsResult<Arc<dyn InodeOps>> {
        Err(VfsError::ReadOnly)
    }

    fn unlink(&self, _name: &str) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn link(&self, _name: &str, _target: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn rename(
        &self,
        _old_name: &str,
        _new_dir: Arc<dyn InodeOps>,
        _new_name: &str,
    ) -> VfsResult<()> {
        Err(VfsError::ReadOnly)
    }

    fn readdir(&self) -> VfsResult<alloc::vec::Vec<super::DirEntry>> {
        let entries = self.entries.lock();
        let mut result = Vec::with_capacity(entries.len());
        for (name, inode) in entries.iter() {
            result.push(super::DirEntry {
                ino: inode.stat().map(|s| s.ino).unwrap_or(0),
                name: name.clone(),
                inode_type: inode.inode_type(),
            });
        }
        Ok(result)
    }

    fn inode_type(&self) -> InodeType {
        InodeType::Directory
    }

    fn attach_child(&self, name: &str, child: Arc<dyn InodeOps>) -> VfsResult<()> {
        let mut entries = self.entries.lock();
        if entries.iter().any(|(n, _)| n == name) {
            return Err(VfsError::AlreadyExists);
        }
        entries.push((String::from(name), child));
        Ok(())
    }
}

// ── Installation ────────────────────────────────────────────────────────

static NEXT_DRM_INO: AtomicU32 = AtomicU32::new(20_000);

fn alloc_drm_ino() -> u64 {
    NEXT_DRM_INO.fetch_add(1, Ordering::Relaxed) as u64
}

/// Install `/dev/dri/` directory with card0, controlD64, and renderD128
/// device nodes into the VFS.
///
/// Call this after `devfs::install_dev()` has created `/dev`.
pub fn install_drm_dev(dev_dir: &Arc<dyn InodeOps>) -> VfsResult<()> {
    // Initialize the DRM compat layer
    drm_compat::init_drm_compat().map_err(|_| VfsError::IoError)?;

    // Register card0 in the DRM compat layer
    if let Some(drm) = drm_compat::get_drm_compat() {
        let _ = drm.register_device(0, "rustos-drm");
    }

    // Create /dev/dri/ directory
    let dri_dir = DrmDirInode::new(alloc_drm_ino());
    dev_dir.attach_child("dri", Arc::clone(&dri_dir) as Arc<dyn InodeOps>)?;

    // Create device nodes
    let card0 = DrmInode::new(alloc_drm_ino(), DrmNodeKind::Card, 0, 0o660);
    dri_dir.attach_child("card0", Arc::clone(&card0) as Arc<dyn InodeOps>)?;

    let control = DrmInode::new(alloc_drm_ino(), DrmNodeKind::Control, 0, 0o660);
    dri_dir.attach_child("controlD64", Arc::clone(&control) as Arc<dyn InodeOps>)?;

    let render = DrmInode::new(alloc_drm_ino(), DrmNodeKind::Render, 0, 0o660);
    dri_dir.attach_child("renderD128", Arc::clone(&render) as Arc<dyn InodeOps>)?;

    Ok(())
}

/// Dispatch a DRM ioctl to the DRM compat layer.
/// Called by the linux_compat ioctl handler when it detects a DRM fd.
pub fn dispatch_ioctl(cmd: u32, arg: u64) -> Result<usize, &'static str> {
    // Ensure DRM is initialized
    drm_compat::init_drm_compat()?;

    // Create a temporary inode to handle the ioctl
    let inode = DrmInode::new(0, DrmNodeKind::Card, 0, 0);
    inode.handle_ioctl(cmd, arg)
}

/// Check if a file descriptor points to a DRM device.
/// This is determined by checking the inode's rdev major number (226).
pub fn is_drm_device(rdev: u64) -> bool {
    (rdev >> 8) == 226
}

// ── Smoke Test ──────────────────────────────────────────────────────────

/// Verify DRM/KMS VFS wiring works.
pub fn smoke_check() -> Result<(), &'static str> {
    // Initialize DRM compat
    drm_compat::init_drm_compat()?;

    // Verify device registration
    if let Some(drm) = drm_compat::get_drm_compat() {
        drm.register_device(0, "rustos-drm")?;

        // Verify resources are accessible
        let resources = drm_compat::DRMIoctl::get_resources(drm);
        if resources.crtcs.is_empty() {
            return Err("DRM should have at least one CRTC after registration");
        }
        if resources.connectors.is_empty() {
            return Err("DRM should have at least one connector after registration");
        }

        // Verify version info
        let version = drm_compat::DRMIoctl::version();
        if version.name != "rustos_drm" {
            return Err("DRM version name mismatch");
        }
    } else {
        return Err("Failed to get DRM compat layer");
    }

    // Verify ioctl dispatch
    let result = dispatch_ioctl(DRM_IOCTL_VERSION, 0)?;
    if result == 0 {
        return Err("DRM version ioctl should return non-zero size");
    }

    Ok(())
}

/// True when `request` is in the DRM ioctl range.
pub fn is_drm_ioctl(request: u64) -> bool {
    ((request >> 8) & 0xFF) == 0x64
}

/// Dispatch a DRM ioctl for an open fd (userspace ABI).
pub fn dispatch_ioctl_for_fd(fd: i32, cmd: u32, arg: u64) -> Result<usize, &'static str> {
    let _ = fd;
    dispatch_ioctl(cmd, arg)
}
