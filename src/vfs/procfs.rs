//! Minimal /proc files for userspace compatibility.

extern crate alloc;

use super::{InodeOps, InodeType, Stat, VfsError, VfsResult};
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;

use core::sync::atomic::{AtomicU32, Ordering};
use lazy_static::lazy_static;
use spin::Mutex;

lazy_static! {
    static ref INSTALLER_STATUS: Mutex<String> = Mutex::new(String::from("idle"));
    static ref INSTALLER_PROGRESS: AtomicU32 = AtomicU32::new(0);
    static ref INSTALLER_MODE: Mutex<String> = Mutex::new(String::from("normal"));
    static ref INSTALLER_PLAN: Mutex<String> = Mutex::new(String::new());
}

fn init_installer_state() {
    *INSTALLER_STATUS.lock() = String::from("idle");
    INSTALLER_PROGRESS.store(0, Ordering::Relaxed);
    *INSTALLER_MODE.lock() = String::from("normal");
    INSTALLER_PLAN.lock().clear();
}


struct MountsProcInode {
    ino: u64,
    mode: u32,
}

impl MountsProcInode {
    fn new(ino: u64, mode: u32) -> Arc<Self> {
        Arc::new(Self { ino, mode })
    }
}

impl InodeOps for MountsProcInode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = crate::linux_compat::fs_ops::mounts_proc_content();
        let bytes = content.as_bytes();
        let start = offset as usize;
        if start >= bytes.len() {
            return Ok(0);
        }
        let end = core::cmp::min(start + buf.len(), bytes.len());
        let n = end - start;
        buf[..n].copy_from_slice(&bytes[start..end]);
        Ok(n)
    }

    fn write_at(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let size = crate::linux_compat::fs_ops::mounts_proc_content().len() as u64;
        Ok(Stat {
            ino: self.ino,
            inode_type: InodeType::File,
            size,
            blksize: 4096,
            blocks: (size + 511) / 512,
            mode: self.mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
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
        InodeType::File
    }

    fn attach_child(&self, _name: &str, _child: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallerProcKind {
    Status,
    Progress,
    Mode,
    Plan,
    Apply,
}

struct InstallerProcInode {
    ino: u64,
    kind: InstallerProcKind,
    mode: u32,
}

impl InstallerProcInode {
    fn new(ino: u64, kind: InstallerProcKind, mode: u32) -> Arc<Self> {
        Arc::new(Self { ino, kind, mode })
    }

    fn read_content(&self) -> String {
        match self.kind {
            InstallerProcKind::Status => INSTALLER_STATUS.lock().clone(),
            InstallerProcKind::Progress => {
                format!("{}\n", INSTALLER_PROGRESS.load(Ordering::Relaxed))
            }
            InstallerProcKind::Mode => format!("{}\n", INSTALLER_MODE.lock().clone()),
            InstallerProcKind::Plan => INSTALLER_PLAN.lock().clone(),
            InstallerProcKind::Apply => {
                String::from("write 'apply' to run queued installer plan\n")
            }
        }
    }

    fn write_content(&self, buf: &[u8]) -> VfsResult<usize> {
        let text = core::str::from_utf8(buf).map_err(|_| VfsError::InvalidArgument)?;
        let trimmed = text.trim();

        match self.kind {
            InstallerProcKind::Mode => {
                if trimmed != "install" && trimmed != "live" && trimmed != "normal" {
                    return Err(VfsError::InvalidArgument);
                }
                *INSTALLER_MODE.lock() = String::from(trimmed);
                Ok(buf.len())
            }
            InstallerProcKind::Plan => {
                *INSTALLER_PLAN.lock() = text.to_string();
                Ok(buf.len())
            }
            InstallerProcKind::Apply => {
                if trimmed != "apply" && trimmed != "1" {
                    return Err(VfsError::InvalidArgument);
                }

                let plan_text = INSTALLER_PLAN.lock().clone();
                if plan_text.trim().is_empty() {
                    return Err(VfsError::InvalidArgument);
                }

                let plan = crate::installer::InstallPlan::deserialize(&plan_text)
                    .map_err(|_| VfsError::InvalidArgument)?;
                crate::installer::apply_plan(&plan).map_err(|_| VfsError::IoError)?;
                Ok(buf.len())
            }
            InstallerProcKind::Status | InstallerProcKind::Progress => Err(VfsError::ReadOnly),
        }
    }
}

impl InodeOps for InstallerProcInode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        let content = self.read_content();
        let bytes = content.as_bytes();
        let start = offset as usize;
        if start >= bytes.len() {
            return Ok(0);
        }
        let end = core::cmp::min(start + buf.len(), bytes.len());
        let n = end - start;
        buf[..n].copy_from_slice(&bytes[start..end]);
        Ok(n)
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        if offset != 0 {
            let existing = self.read_content();
            let mut merged = existing.into_bytes();
            let start = offset as usize;
            if start > merged.len() {
                merged.resize(start, 0);
            }
            let end = start + buf.len();
            if end > merged.len() {
                merged.resize(end, 0);
            }
            merged[start..end].copy_from_slice(buf);
            return self.write_content(&merged);
        }
        self.write_content(buf)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let size = self.read_content().len() as u64;
        Ok(Stat {
            ino: self.ino,
            inode_type: InodeType::File,
            size,
            blksize: 4096,
            blocks: (size + 511) / 512,
            mode: self.mode,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
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
        InodeType::File
    }

    fn attach_child(&self, _name: &str, _child: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }
}

fn install_installer(rustos: &Arc<dyn InodeOps>) -> VfsResult<()> {
    init_installer_state();

    rustos.create("installer", InodeType::Directory, 0o555)?;
    let installer = rustos.lookup("installer")?;

    let mut ino = 20_000u64;
    let files = [
        (InstallerProcKind::Status, "status", 0o444),
        (InstallerProcKind::Progress, "progress", 0o444),
        (InstallerProcKind::Mode, "mode", 0o644),
        (InstallerProcKind::Plan, "plan", 0o644),
        (InstallerProcKind::Apply, "apply", 0o200),
    ];

    for (kind, name, mode) in files {
        ino += 1;
        installer.attach_child(name, InstallerProcInode::new(ino, kind, mode))?;
    }

    Ok(())
}

/// Update installer status and progress exposed under `/proc/rustos/installer`.
pub fn update_installer_status(status: &str, progress: u32) -> VfsResult<()> {
    *INSTALLER_STATUS.lock() = String::from(status);
    INSTALLER_PROGRESS.store(progress.min(100), Ordering::Relaxed);
    Ok(())
}

/// Get the current installer mode (`install`, `live`, or `normal`).
pub fn installer_mode() -> String {
    INSTALLER_MODE.lock().clone()
}

/// Get the current installer plan text.
pub fn installer_plan() -> String {
    INSTALLER_PLAN.lock().clone()
}

fn write_file(dir: &Arc<dyn InodeOps>, name: &str, content: &str) -> VfsResult<()> {
    dir.create(name, InodeType::File, 0o444)?;
    let file = dir.lookup(name)?;
    file.write_at(0, content.as_bytes())?;
    Ok(())
}

fn meminfo_content() -> String {
    if let Ok(stats) = crate::memory_basic::get_memory_stats() {
        let total = stats.usable_memory / 1024;
        let free = stats.usable_memory.saturating_sub(KERNEL_HEAP_SIZE) / 1024;
        format!(
            "MemTotal:       {total} kB\nMemFree:        {free} kB\nMemAvailable:   {free} kB\n"
        )
    } else {
        String::from("MemTotal:       67108864 kB\nMemFree:        64000000 kB\nMemAvailable:   64000000 kB\n")
    }
}

const KERNEL_HEAP_SIZE: usize = crate::memory_basic::KERNEL_HEAP_SIZE;

/// Create /proc with basic files expected by glibc and GTK.
pub fn install_proc(root: Arc<dyn InodeOps>) -> VfsResult<()> {
    root.create("proc", InodeType::Directory, 0o555)?;
    let proc = root.lookup("proc")?;

    write_file(
        &proc,
        "version",
        "Linux version 6.1.0-rustos (rustos@local) (rustc) #1 SMP PREEMPT\n",
    )?;
    write_file(&proc, "meminfo", &meminfo_content())?;
    write_file(
        &proc,
        "cpuinfo",
        "processor\t: 0\nvendor_id\t: GenuineIntel\nmodel name\t: RustOS Virtual CPU\n\
         cpu MHz\t\t: 2400.000\ncpu cores\t: 1\n",
    )?;
    write_file(
        &proc,
        "mounts",
        "rootfs / rootfs rw 0 0\nramfs / ramfs rw 0 0\n",
    )?;

    proc.create("self", InodeType::Directory, 0o555)?;
    let self_dir = proc.lookup("self")?;
    write_file(&self_dir, "exe", "/bin/init")?;

    install_rustos(&proc)?;

    Ok(())
}

fn gnome_status_content() -> String {
    let readiness = crate::gnome::probe();
    let mut out = String::new();

    out.push_str("component=gnome\n");
    out.push_str(&format!(
        "overlay={}\n",
        if crate::gnome_overlay::is_ready() {
            "ready"
        } else {
            "blocked"
        }
    ));
    out.push_str(&format!(
        "dbus={}\n",
        if crate::dbus::is_ready() {
            "ready"
        } else {
            "blocked"
        }
    ));
    out.push_str(&format!(
        "wayland={}\n",
        if crate::wayland::is_ready() {
            "ready"
        } else {
            "blocked"
        }
    ));
    out.push_str(&format!(
        "mutter={}\n",
        if crate::mutter::is_ready() {
            "ready"
        } else {
            "blocked"
        }
    ));
    out.push_str(&format!(
        "foundation_ready={}\n",
        if readiness.foundation_ready() {
            "yes"
        } else {
            "no"
        }
    ));
    out.push_str(&format!(
        "shell_ready={}\n",
        if readiness.gnome_shell_ready() {
            "yes"
        } else {
            "no"
        }
    ));

    if crate::dbus::is_ready() {
        let bus = crate::dbus::bus();
        out.push_str(&format!(
            "name_has_owner_org_freedesktop_DBus={}\n",
            if bus.name_has_owner(crate::dbus::BUS_NAME) {
                "yes"
            } else {
                "no"
            }
        ));
        out.push_str(&format!(
            "name_has_owner_org_gnome_Shell={}\n",
            if bus.name_has_owner(crate::dbus::GNOME_SHELL_NAME) {
                "yes"
            } else {
                "no"
            }
        ));
        out.push_str(&format!(
            "name_has_owner_org_rustos_GnomeReadiness={}\n",
            if bus.name_has_owner(crate::dbus::GNOME_READINESS_NAME) {
                "yes"
            } else {
                "no"
            }
        ));
    }

    out
}

fn install_rustos(proc: &Arc<dyn InodeOps>) -> VfsResult<()> {
    proc.create("rustos", InodeType::Directory, 0o555)?;
    let rustos = proc.lookup("rustos")?;
    write_file(&rustos, "gnome", &gnome_status_content())?;
    install_installer(&rustos)?;
    Ok(())
}

/// Refresh `/proc/rustos/gnome` after subsystem init.
pub fn update_gnome_status() -> VfsResult<()> {
    let root = crate::vfs::get_vfs().lookup("/")?;
    let proc = root.lookup("proc")?;
    let rustos = proc.lookup("rustos")?;
    let file = rustos.lookup("gnome")?;
    file.write_at(0, gnome_status_content().as_bytes())?;
    Ok(())
}
