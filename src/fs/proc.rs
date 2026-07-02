//! Procfs virtual filesystem implementation
//!
//! This module implements a stateless procfs that dynamically generates
//! per-PID directories backed by the process table and a small set of
//! static kernel-info files. The inode scheme is deterministic so that
//! lookups never need to allocate persistent state:
//!
//! - root directory ...................... inode 1
//! - static files (cpuinfo, meminfo, ...) inode 2..255
//! - `/proc/self` symlink ................. inode 6
//! - per-PID directory ................... inode 0x10000 + pid
//! - per-PID regular files ............... inode 0x1000000 + pid*256 + idx
//!
//! All mutating operations return `Err(FsError::ReadOnly)` because procfs
//! is a read-only view of kernel state.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{format, string::{String, ToString}, vec::Vec};

/// Root directory inode.
const ROOT_INODE: InodeNumber = 1;

/// Inode reserved for the `/proc/self` symlink.
const SELF_INODE: InodeNumber = 6;

/// Base for per-PID directory inodes: `0x10000 + pid`.
const PID_DIR_BASE: InodeNumber = 0x10000;

/// Base for per-PID file inodes: `0x1000000 + pid*256 + file_index`.
const PID_FILE_BASE: InodeNumber = 0x1000000;

/// Static top-level files exposed under `/proc`.
const STATIC_FILES: &[(&str, InodeNumber)] = &[
    ("cpuinfo", 2),
    ("meminfo", 3),
    ("uptime", 4),
    ("version", 5),
];

/// Per-PID regular files exposed under `/proc/<pid>/`.
const PID_FILES: &[(&str, u8)] = &[
    ("status", 0),
    ("cmdline", 1),
    ("stat", 2),
    ("io", 3),
    ("maps", 4),
];

/// Procfs filesystem — stateless, all content is generated on demand.
#[derive(Debug)]
pub struct ProcFileSystem;

impl ProcFileSystem {
    /// Create a new procfs instance.
    pub fn new() -> FsResult<Self> {
        Ok(Self)
    }

    /// Build the inode for a given PID's directory.
    fn pid_dir_inode(pid: u32) -> InodeNumber {
        PID_DIR_BASE + pid as InodeNumber
    }

    /// Recover the PID from a PID-directory inode.
    fn pid_from_dir_inode(inode: InodeNumber) -> Option<u32> {
        if inode >= PID_DIR_BASE && inode < PID_FILE_BASE {
            let pid = (inode - PID_DIR_BASE) as u32;
            if pid != 0 {
                Some(pid)
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Build the inode for a per-PID file.
    fn pid_file_inode(pid: u32, file_index: u8) -> InodeNumber {
        PID_FILE_BASE + (pid as InodeNumber) * 256 + file_index as InodeNumber
    }

    /// Recover (pid, file_index) from a per-PID-file inode.
    fn pid_file_from_inode(inode: InodeNumber) -> Option<(u32, u8)> {
        if inode >= PID_FILE_BASE {
            let rel = inode - PID_FILE_BASE;
            let pid = (rel / 256) as u32;
            let idx = (rel % 256) as u8;
            if pid != 0 && (idx as usize) < PID_FILES.len() {
                Some((pid, idx))
            } else {
                None
            }
        } else {
            None
        }
    }

    /// Look up a static file name and return its inode.
    fn static_file_inode(name: &str) -> Option<InodeNumber> {
        STATIC_FILES.iter().find(|(n, _)| *n == name).map(|(_, i)| *i)
    }

    /// Return the name of a static file given its inode.
    fn static_file_name(inode: InodeNumber) -> Option<&'static str> {
        STATIC_FILES.iter().find(|(_, i)| *i == inode).map(|(n, _)| *n)
    }

    /// Return the name of a per-PID file given its file index.
    fn pid_file_name(file_index: u8) -> Option<&'static str> {
        PID_FILES
            .iter()
            .find(|(_, i)| *i == file_index)
            .map(|(n, _)| *n)
    }

    /// Generate the full content for a static file.
    fn generate_static(name: &str) -> String {
        match name {
            "cpuinfo" => Self::generate_cpuinfo(),
            "meminfo" => Self::generate_meminfo(),
            "uptime" => Self::generate_uptime(),
            "version" => Self::generate_version(),
            _ => String::new(),
        }
    }

    /// Generate the full content for a per-PID file.
    fn generate_pid_file(pid: u32, file_index: u8) -> Option<String> {
        let pcb = crate::process::get_process_manager().get_process(pid)?;
        let name = Self::pid_file_name(file_index)?;
        Some(match name {
            "status" => Self::generate_status(&pcb),
            "cmdline" => Self::generate_cmdline(&pcb),
            "stat" => Self::generate_stat(&pcb),
            "io" => Self::generate_io(&pcb),
            "maps" => Self::generate_maps(&pcb),
            _ => String::new(),
        })
    }

    fn generate_cpuinfo() -> String {
        let mut out = String::new();
        // Single logical CPU entry; matches the Linux /proc/cpuinfo layout.
        out.push_str("processor\t: 0\n");
        out.push_str("vendor_id\t: GenuineIntel\n");
        out.push_str("cpu family\t: 6\n");
        out.push_str("model\t\t: 158\n");
        out.push_str("model name\t: RustOS Virtual CPU\n");
        out.push_str("stepping\t: 1\n");
        out.push_str("cpu MHz\t\t: 2000.000\n");
        out.push_str("cache size\t: 256 KB\n");
        out.push_str("flags\t\t: fpu vme de pse tsc msr pae mce cx8 apic sep mtrr\n");
        out.push_str("bogomips\t: 4000.00\n");
        out
    }

    fn generate_meminfo() -> String {
        let mut out = String::new();
        if let Some(stats) = crate::memory::get_memory_stats() {
            let total_kb = (stats.total_memory / 1024) as u64;
            let free_kb = (stats.free_memory / 1024) as u64;
            let used_kb = (stats.allocated_memory / 1024) as u64;
            out.push_str(&format!("MemTotal:       {} kB\n", total_kb));
            out.push_str(&format!("MemFree:        {} kB\n", free_kb));
            out.push_str(&format!("MemAvailable:   {} kB\n", free_kb));
            out.push_str(&format!("Buffers:        0 kB\n"));
            out.push_str(&format!("Cached:         0 kB\n"));
            out.push_str(&format!("SwapCached:     0 kB\n"));
            out.push_str(&format!("Active:         {} kB\n", used_kb));
            out.push_str(&format!("Inactive:       {} kB\n", free_kb));
            out.push_str(&format!("SwapTotal:      0 kB\n"));
            out.push_str(&format!("SwapFree:       0 kB\n"));
        } else {
            out.push_str("MemTotal:       0 kB\n");
            out.push_str("MemFree:        0 kB\n");
        }
        out
    }

    fn generate_uptime() -> String {
        let now = crate::time::get_system_time_ms();
        // Uptime in seconds with two decimal places (idle == uptime for a
        // single-CPU stub).
        let secs = (now as f64) / 1000.0;
        format!("{:.2} {:.2}\n", secs, secs)
    }

    fn generate_version() -> String {
        let mut out = String::new();
        out.push_str("RustOS 1.0.0 (rustos@kernel) #1 SMP\n");
        out.push_str("Build: release\n");
        out.push_str("Architecture: x86_64\n");
        out.push_str("Compiler: rustc (nightly)\n");
        out
    }

    fn state_name(state: crate::process::ProcessState) -> char {
        use crate::process::ProcessState::*;
        match state {
            Running => 'R',
            Ready => 'R',
            Blocked => 'D',
            Sleeping => 'S',
            Terminated => 'Z',
            Zombie => 'Z',
            Dead => 'X',
        }
    }

    fn generate_status(pcb: &crate::process::ProcessControlBlock) -> String {
        let mut out = String::new();
        out.push_str(&format!("Name:\t{}\n", pcb.name_str()));
        out.push_str(&format!("Umask:\t0022\n"));
        out.push_str(&format!(
            "State:\t{} ({}{})\n",
            Self::state_name(pcb.state),
            match pcb.state {
                crate::process::ProcessState::Running => "running",
                crate::process::ProcessState::Ready => "runnable",
                crate::process::ProcessState::Blocked => "blocked",
                crate::process::ProcessState::Sleeping => "sleeping",
                crate::process::ProcessState::Terminated => "terminated",
                crate::process::ProcessState::Zombie => "zombie",
                crate::process::ProcessState::Dead => "dead",
            },
            ""
        ));
        out.push_str(&format!("Tgid:\t{}\n", pcb.pid));
        out.push_str(&format!("Pid:\t{}\n", pcb.pid));
        out.push_str(&format!(
            "PPid:\t{}\n",
            pcb.parent_pid.unwrap_or(0)
        ));
        out.push_str(&format!("Uid:\t{}\n", pcb.uid));
        out.push_str(&format!("Gid:\t{}\n", pcb.gid));
        out.push_str(&format!("VmSize:\t{} kB\n", pcb.memory.vm_size / 1024));
        out.push_str(&format!("VmRSS:\t{} kB\n", pcb.memory.heap_size / 1024));
        out.push_str(&format!("VmData:\t{} kB\n", pcb.memory.data_size / 1024));
        out.push_str(&format!("VmStk:\t{} kB\n", pcb.memory.stack_size / 1024));
        out.push_str(&format!("Threads:\t1\n"));
        out
    }

    fn generate_cmdline(pcb: &crate::process::ProcessControlBlock) -> String {
        // /proc/<pid>/cmdline is NUL-separated argv with a trailing NUL.
        let mut out = Vec::new();
        if !pcb.exec_path.is_empty() {
            out.extend_from_slice(pcb.exec_path.as_bytes());
            out.push(0);
        } else {
            out.extend_from_slice(pcb.name_str().as_bytes());
            out.push(0);
        }
        // Safety: we just built this from valid UTF-8 segments with NULs.
        unsafe { String::from_utf8_unchecked(out) }
    }

    fn generate_stat(pcb: &crate::process::ProcessControlBlock) -> String {
        // Fields mirror Linux /proc/<pid>/stat (subset).
        format!(
            "{} ({}) {} {} {} {} 0 0 0 0 0 0 0 0 {} {} 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0 0\n",
            pcb.pid,
            pcb.name_str(),
            Self::state_name(pcb.state),
            pcb.parent_pid.unwrap_or(0),
            pcb.pgid,
            pcb.sid,
            pcb.user_time_ticks,
            pcb.system_time_ticks,
        )
    }

    fn generate_io(pcb: &crate::process::ProcessControlBlock) -> String {
        let mut out = String::new();
        out.push_str("rchar: 0\n");
        out.push_str("wchar: 0\n");
        out.push_str("syscr: 0\n");
        out.push_str("syscw: 0\n");
        out.push_str("read_bytes: 0\n");
        out.push_str("write_bytes: 0\n");
        out.push_str("cancelled_write_bytes: 0\n");
        // cpu_time is in microseconds; report as nanoseconds for parity.
        let _ = pcb.cpu_time;
        out
    }

    fn generate_maps(pcb: &crate::process::ProcessControlBlock) -> String {
        let mut out = String::new();
        let m = &pcb.memory;
        if m.code_size > 0 {
            out.push_str(&format!(
                "{:016x}-{:016x} r-xp {:08x} 00:00 0  [code]\n",
                m.code_start,
                m.code_start + m.code_size,
                0
            ));
        }
        if m.data_size > 0 {
            out.push_str(&format!(
                "{:016x}-{:016x} rw-p {:08x} 00:00 0  [data]\n",
                m.data_start,
                m.data_start + m.data_size,
                0
            ));
        }
        if m.heap_size > 0 {
            out.push_str(&format!(
                "{:016x}-{:016x} rw-p {:08x} 00:00 0  [heap]\n",
                m.heap_start,
                m.heap_start + m.heap_size,
                0
            ));
        }
        if m.stack_size > 0 {
            out.push_str(&format!(
                "{:016x}-{:016x} rw-p {:08x} 00:00 0  [stack]\n",
                m.stack_start.saturating_sub(m.stack_size),
                m.stack_start,
                0
            ));
        }
        out
    }

    /// Resolve a path like `/cpuinfo` or `/12/status` to an inode number.
    fn resolve(&self, path: &str) -> FsResult<InodeNumber> {
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.is_empty() {
            return Ok(ROOT_INODE);
        }

        let first = parts[0];

        // /proc/self -> symlink to current PID's directory.
        if first == "self" {
            if parts.len() == 1 {
                return Ok(SELF_INODE);
            }
            // /proc/self/<file> — resolve to the live PID.
            let pid = crate::process::get_process_manager().current_process();
            if pid == 0 {
                return Err(FsError::NotFound);
            }
            if parts.len() == 2 {
                return self.resolve_pid_file(pid, parts[1]);
            }
            return Err(FsError::NotFound);
        }

        // Static top-level file?
        if parts.len() == 1 {
            if let Some(inode) = Self::static_file_inode(first) {
                return Ok(inode);
            }
        }

        // Numeric PID directory: /proc/<pid> or /proc/<pid>/<file>
        if let Ok(pid) = first.parse::<u32>() {
            if parts.len() == 1 {
                // Verify the process exists.
                if crate::process::get_process_manager()
                    .get_process(pid)
                    .is_some()
                {
                    return Ok(Self::pid_dir_inode(pid));
                }
                return Err(FsError::NotFound);
            }
            if parts.len() == 2 {
                return self.resolve_pid_file(pid, parts[1]);
            }
        }

        Err(FsError::NotFound)
    }

    /// Resolve `/proc/<pid>/<file>` to a per-PID file inode.
    fn resolve_pid_file(&self, pid: u32, file: &str) -> FsResult<InodeNumber> {
        if crate::process::get_process_manager()
            .get_process(pid)
            .is_none()
        {
            return Err(FsError::NotFound);
        }
        for (name, idx) in PID_FILES.iter() {
            if *name == file {
                return Ok(Self::pid_file_inode(pid, *idx));
            }
        }
        Err(FsError::NotFound)
    }

    /// Generate content for an inode, returning the full byte string.
    fn generate_content(&self, inode: InodeNumber) -> FsResult<Vec<u8>> {
        if inode == SELF_INODE {
            let pid = crate::process::get_process_manager().current_process();
            return Ok(format!("{}\0", pid).into_bytes());
        }

        if let Some(name) = Self::static_file_name(inode) {
            return Ok(Self::generate_static(name).into_bytes());
        }

        if let Some((pid, idx)) = Self::pid_file_from_inode(inode) {
            let content = Self::generate_pid_file(pid, idx)
                .ok_or(FsError::NotFound)?;
            return Ok(content.into_bytes());
        }

        Err(FsError::NotFound)
    }
}

impl FileSystem for ProcFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Proc
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let proc_count = crate::process::get_process_manager().process_count() as u64;
        Ok(FileSystemStats {
            total_blocks: 0,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: STATIC_FILES.len() as u64 + proc_count + 1,
            free_inodes: 0,
            block_size: 4096,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::ReadOnly)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        self.resolve(path)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let content = self.generate_content(inode)?;
        let len = content.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = core::cmp::min(content.len(), start + buffer.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&content[start..end]);
        Ok(n)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        if inode == ROOT_INODE {
            return Ok(FileMetadata {
                inode,
                file_type: FileType::Directory,
                size: 0,
                permissions: FilePermissions::default_directory(),
                uid: 0,
                gid: 0,
                created: 0,
                modified: 0,
                accessed: 0,
                link_count: 2,
                device_id: None,
            });
        }

        if inode == SELF_INODE {
            return Ok(FileMetadata {
                inode,
                file_type: FileType::SymbolicLink,
                size: 0,
                permissions: FilePermissions::from_octal(0o777),
                uid: 0,
                gid: 0,
                created: 0,
                modified: 0,
                accessed: 0,
                link_count: 1,
                device_id: None,
            });
        }

        if let Some(name) = Self::static_file_name(inode) {
            let content = Self::generate_static(name);
            return Ok(FileMetadata {
                inode,
                file_type: FileType::Regular,
                size: content.len() as u64,
                permissions: FilePermissions::from_octal(0o444),
                uid: 0,
                gid: 0,
                created: 0,
                modified: 0,
                accessed: 0,
                link_count: 1,
                device_id: None,
            });
        }

        if let Some(pid) = Self::pid_from_dir_inode(inode) {
            if crate::process::get_process_manager()
                .get_process(pid)
                .is_none()
            {
                return Err(FsError::NotFound);
            }
            return Ok(FileMetadata {
                inode,
                file_type: FileType::Directory,
                size: 0,
                permissions: FilePermissions::default_directory(),
                uid: 0,
                gid: 0,
                created: 0,
                modified: 0,
                accessed: 0,
                link_count: 2,
                device_id: None,
            });
        }

        if let Some((pid, idx)) = Self::pid_file_from_inode(inode) {
            let size = Self::generate_pid_file(pid, idx)
                .map(|s| s.len() as u64)
                .unwrap_or(0);
            return Ok(FileMetadata {
                inode,
                file_type: FileType::Regular,
                size,
                permissions: FilePermissions::from_octal(0o444),
                uid: 0,
                gid: 0,
                created: 0,
                modified: 0,
                accessed: 0,
                link_count: 1,
                device_id: None,
            });
        }

        Err(FsError::NotFound)
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::ReadOnly)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn unlink(&self, _path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        if inode == ROOT_INODE {
            let mut entries = Vec::new();
            entries.push(DirectoryEntry {
                name: ".".to_string(),
                inode: ROOT_INODE,
                file_type: FileType::Directory,
            });
            entries.push(DirectoryEntry {
                name: "..".to_string(),
                inode: ROOT_INODE,
                file_type: FileType::Directory,
            });
            // /proc/self symlink
            entries.push(DirectoryEntry {
                name: "self".to_string(),
                inode: SELF_INODE,
                file_type: FileType::SymbolicLink,
            });
            // Static files
            for (name, ino) in STATIC_FILES.iter() {
                entries.push(DirectoryEntry {
                    name: (*name).to_string(),
                    inode: *ino,
                    file_type: FileType::Regular,
                });
            }
            // Live PID directories
            for (pid, _name, _state, _prio) in
                crate::process::get_process_manager().list_processes()
            {
                entries.push(DirectoryEntry {
                    name: format!("{}", pid),
                    inode: Self::pid_dir_inode(pid),
                    file_type: FileType::Directory,
                });
            }
            return Ok(entries);
        }

        if let Some(pid) = Self::pid_from_dir_inode(inode) {
            if crate::process::get_process_manager()
                .get_process(pid)
                .is_none()
            {
                return Err(FsError::NotFound);
            }
            let mut entries = Vec::new();
            entries.push(DirectoryEntry {
                name: ".".to_string(),
                inode,
                file_type: FileType::Directory,
            });
            entries.push(DirectoryEntry {
                name: "..".to_string(),
                inode: ROOT_INODE,
                file_type: FileType::Directory,
            });
            for (name, idx) in PID_FILES.iter() {
                entries.push(DirectoryEntry {
                    name: (*name).to_string(),
                    inode: Self::pid_file_inode(pid, *idx),
                    file_type: FileType::Regular,
                });
            }
            return Ok(entries);
        }

        Err(FsError::NotADirectory)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let inode = self.resolve(path)?;
        if inode != SELF_INODE {
            return Err(FsError::InvalidArgument);
        }
        let pid = crate::process::get_process_manager().current_process();
        Ok(format!("{}", pid))
    }

    fn sync(&self) -> FsResult<()> {
        // Procfs is virtual, no syncing needed.
        let _ = get_current_time();
        Ok(())
    }
}
