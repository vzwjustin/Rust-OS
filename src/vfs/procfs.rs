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
enum DynamicProcKind {
    CpuInfo,
    MemInfo,
    Hostname,
    SelfStatus,
}

struct DynamicProcInode {
    ino: u64,
    kind: DynamicProcKind,
    mode: u32,
}

impl DynamicProcInode {
    fn new(ino: u64, kind: DynamicProcKind, mode: u32) -> Arc<Self> {
        Arc::new(Self { ino, kind, mode })
    }

    fn read_content(&self) -> String {
        match self.kind {
            DynamicProcKind::CpuInfo => cpuinfo_content(),
            DynamicProcKind::MemInfo => meminfo_content(),
            DynamicProcKind::Hostname => {
                format!("{}\n", crate::linux_compat::sysinfo_ops::kernel_hostname())
            }
            DynamicProcKind::SelfStatus => self_status_content(),
        }
    }

    fn write_content(&self, buf: &[u8]) -> VfsResult<usize> {
        match self.kind {
            DynamicProcKind::Hostname => {
                let text = core::str::from_utf8(buf).map_err(|_| VfsError::InvalidArgument)?;
                let trimmed = text.trim_end_matches('\0').trim();
                crate::linux_compat::sysinfo_ops::set_kernel_hostname(trimmed)
                    .map_err(|_| VfsError::InvalidArgument)?;
                Ok(buf.len())
            }
            _ => Err(VfsError::ReadOnly),
        }
    }
}

impl InodeOps for DynamicProcInode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        copy_proc_bytes(self.read_content().as_bytes(), offset, buf)
    }

    fn write_at(&self, offset: u64, buf: &[u8]) -> VfsResult<usize> {
        if offset != 0 {
            return Err(VfsError::InvalidArgument);
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

struct SelfExeProcInode {
    ino: u64,
}

impl SelfExeProcInode {
    fn new(ino: u64) -> Arc<Self> {
        Arc::new(Self { ino })
    }

    fn target(&self) -> String {
        let pid = crate::process::current_pid();
        if pid == 0 {
            return String::from("/proc/self/exe");
        }
        if let Some(pcb) = crate::process::get_process_manager().get_process(pid) {
            if !pcb.exec_path.is_empty() {
                return pcb.exec_path;
            }
            if !pcb.name_str().is_empty() {
                return format!("/{}", pcb.name_str());
            }
        }
        String::from("/proc/self/exe")
    }
}

impl InodeOps for SelfExeProcInode {
    fn read_at(&self, offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
        copy_proc_bytes(self.target().as_bytes(), offset, buf)
    }

    fn write_at(&self, _offset: u64, _buf: &[u8]) -> VfsResult<usize> {
        Err(VfsError::ReadOnly)
    }

    fn stat(&self) -> VfsResult<Stat> {
        let size = self.target().len() as u64;
        Ok(Stat {
            ino: self.ino,
            inode_type: InodeType::Symlink,
            size,
            blksize: 4096,
            blocks: (size + 511) / 512,
            mode: 0o777,
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
        InodeType::Symlink
    }

    fn attach_child(&self, _name: &str, _child: Arc<dyn InodeOps>) -> VfsResult<()> {
        Err(VfsError::NotDirectory)
    }
}

fn copy_proc_bytes(bytes: &[u8], offset: u64, buf: &mut [u8]) -> VfsResult<usize> {
    let start = offset as usize;
    if start >= bytes.len() {
        return Ok(0);
    }
    let end = core::cmp::min(start + buf.len(), bytes.len());
    let n = end - start;
    buf[..n].copy_from_slice(&bytes[start..end]);
    Ok(n)
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

fn init_installer_state() {
    *INSTALLER_STATUS.lock() = String::from("idle");
    INSTALLER_PROGRESS.store(0, Ordering::Relaxed);
    *INSTALLER_MODE.lock() = String::from("normal");
    INSTALLER_PLAN.lock().clear();
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
    if let Some(stats) = crate::memory::get_memory_stats() {
        let total_kb = stats.total_memory / 1024;
        let free_kb = stats.free_memory / 1024;
        let avail_kb = stats.free_memory / 1024;
        let active_kb = stats.allocated_memory / 1024;
        let swap_total_kb = stats.swap_stats.total_slots as u64 * 4096 / 1024;
        let swap_free_kb = stats.swap_stats.free_slots as u64 * 4096 / 1024;
        format!(
            "MemTotal:       {total_kb} kB\n\
             MemFree:        {free_kb} kB\n\
             MemAvailable:   {avail_kb} kB\n\
             Active:         {active_kb} kB\n\
             Inactive:       0 kB\n\
             SwapTotal:      {swap_total_kb} kB\n\
             SwapFree:       {swap_free_kb} kB\n"
        )
    } else if let Ok(stats) = crate::memory_basic::get_memory_stats() {
        let total = stats.usable_memory / 1024;
        let free = stats.usable_memory.saturating_sub(KERNEL_HEAP_SIZE) / 1024;
        format!(
            "MemTotal:       {total} kB\nMemFree:        {free} kB\nMemAvailable:   {free} kB\n"
        )
    } else {
        String::from("MemTotal:       0 kB\nMemFree:        0 kB\nMemAvailable:   0 kB\n")
    }
}

fn cpuinfo_content() -> String {
    let info = crate::arch::cpu_info();
    let vendor_map = match info.vendor.as_str() {
        "GenuineIntel" => "GenuineIntel",
        "AuthenticAMD" => "AuthenticAMD",
        other => other,
    };
    let cpu_count = crate::smp::cpu_count().max(1);
    let online = crate::smp::online_cpus().max(1);
    let mut out = String::new();

    for processor in 0..cpu_count {
        let online_flag = if processor < online { "yes" } else { "no" };
        out.push_str(&format!(
            "processor\t: {processor}\n\
             vendor_id\t: {vendor_map}\n\
             cpu family\t: {}\n\
             model\t\t: {}\n\
             model name\t: {}\n\
             stepping\t: {}\n\
             cpu MHz\t\t: 2400.000\n\
             cache size\t: 4096 KB\n\
             physical id\t: 0\n\
             siblings\t: {cpu_count}\n\
             core id\t\t: {processor}\n\
             cpu cores\t: {cpu_count}\n\
             apicid\t\t: {}\n\
             initial apicid\t: {}\n\
             fpu\t\t: yes\n\
             fpu_exception\t: yes\n\
             cpuid level\t: {}\n\
             wp\t\t: yes\n\
             flags\t\t: fpu vme de pse tsc msr pae mce cx8 apic sep mtrr pge mca cmov pat pse36 clflush mmx fxsr sse sse2\n\
             bugs\t\t:\n\
             bogomips\t: 4800.00\n\
             clflush size\t: 64\n\
             cache_alignment\t: 64\n\
             address sizes\t: 39 bits physical, 48 bits virtual\n\
             power management:\n",
            info.family,
            info.model,
            info.brand,
            info.stepping,
            crate::smp::get_cpu_data(processor)
                .map(|d| d.apic_id)
                .unwrap_or(processor),
            crate::smp::get_cpu_data(processor)
                .map(|d| d.apic_id)
                .unwrap_or(processor),
            info.max_cpuid,
        ));
        out.push_str(&format!("online\t\t: {online_flag}\n\n"));
    }

    out
}

fn process_state_letter(state: crate::process::ProcessState) -> char {
    use crate::process::ProcessState;
    match state {
        ProcessState::Running => 'R',
        ProcessState::Ready => 'R',
        ProcessState::Blocked => 'D',
        ProcessState::Sleeping => 'S',
        ProcessState::Zombie => 'Z',
        ProcessState::Terminated | ProcessState::Dead => 'X',
    }
}

fn self_status_content() -> String {
    let pid = crate::process::current_pid();
    if pid == 0 {
        return String::from(
            "Name:\tkernel\nState:\tR (running)\nTgid:\t0\nPid:\t0\nPPid:\t0\nUid:\t0\t0\t0\t0\nGid:\t0\t0\t0\t0\n",
        );
    }

    let Some(pcb) = crate::process::get_process_manager().get_process(pid) else {
        return format!("Name:\tunknown\nPid:\t{pid}\n");
    };

    let ppid = pcb.parent_pid.unwrap_or(0);
    format!(
        "Name:\t{}\n\
         State:\t{} ({})\n\
         Tgid:\t{}\n\
         Ngid:\t0\n\
         Pid:\t{}\n\
         PPid:\t{}\n\
         TracerPid:\t0\n\
         Uid:\t{}\t{}\t{}\t{}\n\
         Gid:\t{}\t{}\t{}\t{}\n\
         FDSize:\t{}\n\
         Threads:\t1\n\
         VmPeak:\t0 kB\n\
         VmSize:\t0 kB\n\
         VmRSS:\t0 kB\n\
         RssAnon:\t0 kB\n\
         RssFile:\t0 kB\n\
         RssShmem:\t0 kB\n\
         voluntary_ctxt_switches:\t0\n\
         nonvoluntary_ctxt_switches:\t0\n",
        pcb.name_str(),
        process_state_letter(pcb.state),
        match pcb.state {
            crate::process::ProcessState::Running => "running",
            crate::process::ProcessState::Ready => "running",
            crate::process::ProcessState::Blocked => "disk sleep",
            crate::process::ProcessState::Sleeping => "sleeping",
            crate::process::ProcessState::Zombie => "zombie",
            crate::process::ProcessState::Terminated | crate::process::ProcessState::Dead => {
                "dead"
            }
        },
        pid,
        pid,
        ppid,
        pcb.uid,
        pcb.euid,
        pcb.uid,
        pcb.uid,
        pcb.gid,
        pcb.egid,
        pcb.gid,
        pcb.gid,
        pcb.fd_table.len(),
    )
}

fn attach_dynamic(
    dir: &Arc<dyn InodeOps>,
    name: &str,
    kind: DynamicProcKind,
    mode: u32,
    ino: u64,
) -> VfsResult<()> {
    dir.attach_child(name, DynamicProcInode::new(ino, kind, mode))
}

const KERNEL_HEAP_SIZE: usize = crate::memory_basic::KERNEL_HEAP_SIZE;

/// Create /proc with basic files expected by glibc and GTK.
pub fn install_proc(root: Arc<dyn InodeOps>) -> VfsResult<()> {
    root.create("proc", InodeType::Directory, 0o555)?;
    let proc = root.lookup("proc")?;

    let mut ino = 10_000u64;

    write_file(
        &proc,
        "version",
        "Linux version 6.1.0-rustos (rustos@local) (rustc) #1 SMP PREEMPT\n",
    )?;

    ino += 1;
    attach_dynamic(&proc, "meminfo", DynamicProcKind::MemInfo, 0o444, ino)?;
    ino += 1;
    attach_dynamic(&proc, "cpuinfo", DynamicProcKind::CpuInfo, 0o444, ino)?;

    ino += 1;
    proc.attach_child("mounts", MountsProcInode::new(ino, 0o444))?;

    proc.create("self", InodeType::Directory, 0o555)?;
    let self_dir = proc.lookup("self")?;
    ino += 1;
    attach_dynamic(&self_dir, "status", DynamicProcKind::SelfStatus, 0o444, ino)?;
    ino += 1;
    self_dir.attach_child("exe", SelfExeProcInode::new(ino))?;

    proc.create("sys", InodeType::Directory, 0o555)?;
    let sys = proc.lookup("sys")?;
    sys.create("kernel", InodeType::Directory, 0o555)?;
    let kernel = sys.lookup("kernel")?;
    ino += 1;
    attach_dynamic(&kernel, "hostname", DynamicProcKind::Hostname, 0o644, ino)?;
    crate::audit::install_proc(&kernel, &mut ino)?;

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
