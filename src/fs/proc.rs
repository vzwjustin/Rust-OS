//! Procfs (process introspection) virtual filesystem
//!
//! Provides a read-only view of the running process table, mirroring the
//! layout of a Linux `/proc` mount: a root directory containing one
//! subdirectory per PID plus a handful of pseudo-files (`cpuinfo`, `meminfo`,
//! `uptime`, `version`) and the `self` symlink. Each per-PID directory exposes
//! `status`, `cmdline` and `maps` files whose contents are generated on demand
//! from an in-memory process registry.
//!
//! The registry is self-contained so the filesystem can be exercised without a
//! live scheduler; `register_process`/`unregister_process` keep it in sync with
//! the rest of the kernel.

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use lazy_static::lazy_static;
use spin::RwLock;

// ---------------------------------------------------------------------------
// Inode numbering
// ---------------------------------------------------------------------------
//
// Inode numbers are derived deterministically from the kind of entry so that
// lookups never need to allocate or mutate state for the common read path.
//
//   1                 root (/proc)
//   2                 /proc/self        (symbolic link)
//   3                 /proc/cpuinfo
//   4                 /proc/meminfo
//   5                 /proc/uptime
//   6                 /proc/version
//   1000 + pid*16     /proc/<pid>       (directory)
//   1000 + pid*16 + 1 /proc/<pid>/status
//   1000 + pid*16 + 2 /proc/<pid>/cmdline
//   1000 + pid*16 + 3 /proc/<pid>/maps

const ROOT_INODE: InodeNumber = 1;
const SELF_INODE: InodeNumber = 2;
const CPUINFO_INODE: InodeNumber = 3;
const MEMINFO_INODE: InodeNumber = 4;
const UPTIME_INODE: InodeNumber = 5;
const VERSION_INODE: InodeNumber = 6;
const PID_BASE: InodeNumber = 1000;
const PID_STRIDE: InodeNumber = 16;

/// Kind of procfs entry, recoverable from an inode number.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcKind {
    Root,
    SelfLink,
    CpuInfo,
    MemInfo,
    Uptime,
    Version,
    PidDir(u32),
    Status(u32),
    Cmdline(u32),
    Maps(u32),
}

impl ProcKind {
    /// Compute the inode number for this kind.
    fn to_inode(self) -> InodeNumber {
        match self {
            ProcKind::Root => ROOT_INODE,
            ProcKind::SelfLink => SELF_INODE,
            ProcKind::CpuInfo => CPUINFO_INODE,
            ProcKind::MemInfo => MEMINFO_INODE,
            ProcKind::Uptime => UPTIME_INODE,
            ProcKind::Version => VERSION_INODE,
            ProcKind::PidDir(pid) => PID_BASE + (pid as InodeNumber) * PID_STRIDE,
            ProcKind::Status(pid) => PID_BASE + (pid as InodeNumber) * PID_STRIDE + 1,
            ProcKind::Cmdline(pid) => PID_BASE + (pid as InodeNumber) * PID_STRIDE + 2,
            ProcKind::Maps(pid) => PID_BASE + (pid as InodeNumber) * PID_STRIDE + 3,
        }
    }

    /// Recover the kind from an inode number, or `None` if it is not a valid
    /// procfs inode.
    fn from_inode(inode: InodeNumber) -> Option<ProcKind> {
        match inode {
            ROOT_INODE => Some(ProcKind::Root),
            SELF_INODE => Some(ProcKind::SelfLink),
            CPUINFO_INODE => Some(ProcKind::CpuInfo),
            MEMINFO_INODE => Some(ProcKind::MemInfo),
            UPTIME_INODE => Some(ProcKind::Uptime),
            VERSION_INODE => Some(ProcKind::Version),
            n if n >= PID_BASE => {
                let offset = n - PID_BASE;
                let pid = (offset / PID_STRIDE) as u32;
                let rem = offset % PID_STRIDE;
                match rem {
                    0 => Some(ProcKind::PidDir(pid)),
                    1 => Some(ProcKind::Status(pid)),
                    2 => Some(ProcKind::Cmdline(pid)),
                    3 => Some(ProcKind::Maps(pid)),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn is_dir(self) -> bool {
        matches!(self, ProcKind::Root | ProcKind::PidDir(_))
    }

    fn file_type(self) -> FileType {
        match self {
            ProcKind::SelfLink => FileType::SymbolicLink,
            ProcKind::Root | ProcKind::PidDir(_) => FileType::Directory,
            _ => FileType::Regular,
        }
    }
}

// ---------------------------------------------------------------------------
// In-memory process registry
// ---------------------------------------------------------------------------

/// A snapshot of a process tracked by procfs.
#[derive(Debug, Clone)]
pub struct ProcEntry {
    /// Process ID.
    pub pid: u32,
    /// Human-readable process name.
    pub name: String,
    /// Single-character Linux-style state code (e.g. "R", "S", "Z").
    pub state: String,
    /// Command line as a single string (arguments joined by spaces).
    pub cmdline: String,
    /// Owner user ID.
    pub uid: u32,
    /// Owner group ID.
    pub gid: u32,
    /// Parent PID (0 for the init/kernel process).
    pub parent_pid: u32,
    /// Number of threads in the process.
    pub threads: u32,
    /// Virtual memory size in bytes.
    pub vm_size: u64,
    /// Resident set size in bytes.
    pub vm_rss: u64,
    /// Pre-formatted `/proc/<pid>/maps` body.
    pub maps: String,
}

lazy_static! {
    /// Global registry of processes visible through procfs.
    static ref PROC_REGISTRY: RwLock<Vec<ProcEntry>> = RwLock::new(Vec::new());
}

/// Register or replace a process entry in the procfs registry.
pub fn register_process(entry: ProcEntry) {
    let mut registry = PROC_REGISTRY.write();
    if let Some(existing) = registry.iter_mut().find(|e| e.pid == entry.pid) {
        *existing = entry;
    } else {
        registry.push(entry);
        registry.sort_by_key(|e| e.pid);
    }
}

/// Remove a process entry from the procfs registry.
pub fn unregister_process(pid: u32) {
    let mut registry = PROC_REGISTRY.write();
    registry.retain(|e| e.pid != pid);
}

/// Look up a process entry by PID.
fn lookup_process(pid: u32) -> Option<ProcEntry> {
    PROC_REGISTRY.read().iter().find(|e| e.pid == pid).cloned()
}

/// Snapshot of all registered processes, sorted by PID.
fn all_processes() -> Vec<ProcEntry> {
    PROC_REGISTRY.read().clone()
}

// ---------------------------------------------------------------------------
// Filesystem
// ---------------------------------------------------------------------------

/// Procfs virtual filesystem.
#[derive(Debug)]
pub struct ProcFileSystem {
    /// Cached metadata for the small set of static root inodes. Per-PID
    /// inodes are derived from `ProcKind` and need not be stored.
    inodes: RwLock<BTreeMap<InodeNumber, FileMetadata>>,
}

impl ProcFileSystem {
    /// Create a new procfs instance.
    pub fn new() -> FsResult<Self> {
        let mut inodes = BTreeMap::new();
        inodes.insert(
            ROOT_INODE,
            Self::base_metadata(ROOT_INODE, FileType::Directory, 0),
        );
        inodes.insert(
            SELF_INODE,
            Self::base_metadata(SELF_INODE, FileType::SymbolicLink, 0),
        );
        inodes.insert(
            CPUINFO_INODE,
            Self::base_metadata(CPUINFO_INODE, FileType::Regular, 0),
        );
        inodes.insert(
            MEMINFO_INODE,
            Self::base_metadata(MEMINFO_INODE, FileType::Regular, 0),
        );
        inodes.insert(
            UPTIME_INODE,
            Self::base_metadata(UPTIME_INODE, FileType::Regular, 0),
        );
        inodes.insert(
            VERSION_INODE,
            Self::base_metadata(VERSION_INODE, FileType::Regular, 0),
        );
        Ok(Self {
            inodes: RwLock::new(inodes),
        })
    }

    fn base_metadata(inode: InodeNumber, file_type: FileType, size: u64) -> FileMetadata {
        let permissions = match file_type {
            FileType::Directory => FilePermissions::default_directory(),
            FileType::SymbolicLink => FilePermissions::from_octal(0o777),
            _ => FilePermissions::from_octal(0o444),
        };
        FileMetadata {
            inode,
            file_type,
            size,
            permissions,
            uid: 0,
            gid: 0,
            created: 0,
            modified: 0,
            accessed: 0,
            link_count: 1,
            device_id: None,
        }
    }

    /// Resolve a path relative to the procfs root into a `ProcKind`.
    fn resolve(&self, path: &str) -> FsResult<ProcKind> {
        let components: Vec<&str> = path.split('/').filter(|c| !c.is_empty()).collect();
        if components.is_empty() {
            return Ok(ProcKind::Root);
        }

        match components[0] {
            "self" => {
                if components.len() == 1 {
                    Ok(ProcKind::SelfLink)
                } else {
                    // /proc/self/<file> resolves through the current PID.
                    let pid = current_pid();
                    resolve_pid_file(pid, &components[1..])
                }
            }
            "cpuinfo" if components.len() == 1 => Ok(ProcKind::CpuInfo),
            "meminfo" if components.len() == 1 => Ok(ProcKind::MemInfo),
            "uptime" if components.len() == 1 => Ok(ProcKind::Uptime),
            "version" if components.len() == 1 => Ok(ProcKind::Version),
            name => {
                let pid = name.parse::<u32>().map_err(|_| FsError::NotFound)?;
                let entry = lookup_process(pid).ok_or(FsError::NotFound)?;
                // Validate the PID is known, then resolve the remainder.
                resolve_pid_file(entry.pid, &components[1..])
            }
        }
    }

    /// Generate the byte content for a non-directory inode.
    fn generate_content(&self, kind: ProcKind) -> FsResult<Vec<u8>> {
        let text = match kind {
            ProcKind::Root | ProcKind::PidDir(_) => {
                return Err(FsError::IsADirectory);
            }
            ProcKind::SelfLink => {
                // Symlink target: the current PID directory.
                return Ok(format!("{}", current_pid()).into_bytes());
            }
            ProcKind::CpuInfo => generate_cpuinfo(),
            ProcKind::MemInfo => generate_meminfo(),
            ProcKind::Uptime => generate_uptime(),
            ProcKind::Version => generate_version(),
            ProcKind::Status(pid) => {
                let entry = lookup_process(pid).ok_or(FsError::NotFound)?;
                generate_status(&entry)
            }
            ProcKind::Cmdline(pid) => {
                let entry = lookup_process(pid).ok_or(FsError::NotFound)?;
                // Linux stores cmdline with NUL separators; mirror that.
                let mut bytes = Vec::new();
                for arg in entry.cmdline.split_whitespace() {
                    bytes.extend_from_slice(arg.as_bytes());
                    bytes.push(0);
                }
                String::from_utf8(bytes).unwrap_or_default()
            }
            ProcKind::Maps(pid) => {
                let entry = lookup_process(pid).ok_or(FsError::NotFound)?;
                entry.maps.clone()
            }
        };
        Ok(text.into_bytes())
    }

    /// Build the directory listing for a directory inode.
    fn list_dir(&self, kind: ProcKind) -> FsResult<Vec<DirectoryEntry>> {
        match kind {
            ProcKind::Root => {
                let mut entries = Vec::new();
                entries.push(DirectoryEntry {
                    name: "self".to_string(),
                    inode: SELF_INODE,
                    file_type: FileType::SymbolicLink,
                });
                entries.push(DirectoryEntry {
                    name: "cpuinfo".to_string(),
                    inode: CPUINFO_INODE,
                    file_type: FileType::Regular,
                });
                entries.push(DirectoryEntry {
                    name: "meminfo".to_string(),
                    inode: MEMINFO_INODE,
                    file_type: FileType::Regular,
                });
                entries.push(DirectoryEntry {
                    name: "uptime".to_string(),
                    inode: UPTIME_INODE,
                    file_type: FileType::Regular,
                });
                entries.push(DirectoryEntry {
                    name: "version".to_string(),
                    inode: VERSION_INODE,
                    file_type: FileType::Regular,
                });
                for entry in all_processes() {
                    entries.push(DirectoryEntry {
                        name: format!("{}", entry.pid),
                        inode: ProcKind::PidDir(entry.pid).to_inode(),
                        file_type: FileType::Directory,
                    });
                }
                Ok(entries)
            }
            ProcKind::PidDir(pid) => {
                // Confirm the PID exists.
                let _ = lookup_process(pid).ok_or(FsError::NotFound)?;
                Ok(alloc::vec![
                    DirectoryEntry {
                        name: "status".to_string(),
                        inode: ProcKind::Status(pid).to_inode(),
                        file_type: FileType::Regular,
                    },
                    DirectoryEntry {
                        name: "cmdline".to_string(),
                        inode: ProcKind::Cmdline(pid).to_inode(),
                        file_type: FileType::Regular,
                    },
                    DirectoryEntry {
                        name: "maps".to_string(),
                        inode: ProcKind::Maps(pid).to_inode(),
                        file_type: FileType::Regular,
                    },
                ])
            }
            _ => Err(FsError::NotADirectory),
        }
    }
}

/// Resolve `<pid>/<file>` (already split into components) to a kind.
fn resolve_pid_file(pid: u32, rest: &[&str]) -> FsResult<ProcKind> {
    if rest.is_empty() {
        return Ok(ProcKind::PidDir(pid));
    }
    match rest[0] {
        "status" if rest.len() == 1 => Ok(ProcKind::Status(pid)),
        "cmdline" if rest.len() == 1 => Ok(ProcKind::Cmdline(pid)),
        "maps" if rest.len() == 1 => Ok(ProcKind::Maps(pid)),
        _ => Err(FsError::NotFound),
    }
}

/// Best-effort current PID: pulls from the process subsystem when available,
/// otherwise falls back to PID 1 so `self` always resolves.
fn current_pid() -> u32 {
    // Avoid a hard dependency on the scheduler; use the global accessor if the
    // process subsystem is wired up, else default to 1.
    crate::process::current_pid()
}

// ---------------------------------------------------------------------------
// Content generators
// ---------------------------------------------------------------------------

fn generate_status(entry: &ProcEntry) -> String {
    let now = get_current_time();
    format!(
        "Name:\t{name}\n\
         Umask:\t0022\n\
         State:\t{state}\n\
         Tgid:\t{pid}\n\
         Ngid:\t0\n\
         Pid:\t{pid}\n\
         PPid:\t{ppid}\n\
         TracerPid:\t0\n\
         Uid:\t{uid}\t{uid}\t{uid}\t{uid}\n\
         Gid:\t{gid}\t{gid}\t{gid}\t{gid}\n\
         FDSize:\t64\n\
         NStgid:\t0\n\
         NSpid:\t0\n\
         NSpgid:\t0\n\
         NSsid:\t0\n\
         Threads:\t{threads}\n\
         VmSize:\t{vm_size} kB\n\
         VmRSS:\t{vm_rss} kB\n\
         VmPeak:\t{vm_size} kB\n\
         State2:\t{state}\n\
         starttime:\t{now}\n",
        name = entry.name,
        state = entry.state,
        pid = entry.pid,
        ppid = entry.parent_pid,
        uid = entry.uid,
        gid = entry.gid,
        threads = entry.threads,
        vm_size = entry.vm_size / 1024,
        vm_rss = entry.vm_rss / 1024,
        now = now,
    )
}

fn generate_cpuinfo() -> String {
    let mut out = String::new();
    // Report a single generic CPU; a real kernel would enumerate cores.
    out.push_str("processor\t: 0\n");
    out.push_str("vendor_id\t: GenuineRustOS\n");
    out.push_str("cpu family\t: 6\n");
    out.push_str("model\t\t: 0\n");
    out.push_str("model name\t: RustOS Virtual CPU\n");
    out.push_str("stepping\t: 1\n");
    out.push_str("cpu MHz\t\t: 2000.000\n");
    out.push_str("cache size\t: 256 KB\n");
    out.push_str("flags\t\t: fpu vme de pse tsc msr pae mce cx8 apic\n");
    out.push_str("bogomips\t: 4000.00\n");
    out
}

fn generate_meminfo() -> String {
    let procs = all_processes();
    let used: u64 = procs.iter().map(|e| e.vm_rss).sum();
    let total: u64 = 256 * 1024 * 1024; // 256 MiB hypothetical physical memory
    let free = total.saturating_sub(used);
    format!(
        "MemTotal:\t{total} kB\n\
         MemFree:\t{free} kB\n\
         MemAvailable:\t{free} kB\n\
         Buffers:\t0 kB\n\
         Cached:\t0 kB\n\
         SwapTotal:\t0 kB\n\
         SwapFree:\t0 kB\n",
        total = total / 1024,
        free = free / 1024,
    )
}

fn generate_uptime() -> String {
    let now = get_current_time();
    // `now` is in milliseconds; uptime is reported in seconds with two
    // decimals followed by the idle time.
    let seconds = now as f64 / 1000.0;
    format!("{seconds:.2} {seconds:.2}\n", seconds = seconds)
}

fn generate_version() -> String {
    "RustOS 0.1.0 (procfs) #1 SMP\n\
     Compiled with rustc nightly\n\
     \n"
    .to_string()
}

// ---------------------------------------------------------------------------
// FileSystem trait impl
// ---------------------------------------------------------------------------

impl FileSystem for ProcFileSystem {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::Proc
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let count = all_processes().len() as u64;
        Ok(FileSystemStats {
            total_blocks: 0,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: 4096,
            free_inodes: 4096u64.saturating_sub(count + 6),
            block_size: 4096,
            max_filename_length: 255,
        })
    }

    fn create(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        // procfs is read-only.
        Err(FsError::ReadOnly)
    }

    fn open(&self, path: &str, _flags: OpenFlags) -> FsResult<InodeNumber> {
        let kind = self.resolve(path)?;
        // For per-PID entries, confirm existence so `open` fails on stale PIDs.
        match kind {
            ProcKind::PidDir(pid)
            | ProcKind::Status(pid)
            | ProcKind::Cmdline(pid)
            | ProcKind::Maps(pid) => {
                let _ = lookup_process(pid).ok_or(FsError::NotFound)?;
            }
            _ => {}
        }
        Ok(kind.to_inode())
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        let kind = ProcKind::from_inode(inode).ok_or(FsError::NotFound)?;
        if kind.is_dir() {
            return Err(FsError::IsADirectory);
        }
        let content = self.generate_content(kind)?;
        let len = content.len() as u64;
        if offset >= len {
            return Ok(0);
        }
        let start = offset as usize;
        let end = core::cmp::min(start + buffer.len(), content.len());
        let n = end - start;
        buffer[..n].copy_from_slice(&content[start..end]);
        Ok(n)
    }

    fn write(&self, _inode: InodeNumber, _offset: u64, _buffer: &[u8]) -> FsResult<usize> {
        Err(FsError::ReadOnly)
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        let kind = ProcKind::from_inode(inode).ok_or(FsError::NotFound)?;
        let file_type = kind.file_type();

        // Compute size: directories report 0; symlinks report target length;
        // regular files report their generated content length.
        let size = match kind {
            ProcKind::Root | ProcKind::PidDir(_) => 0,
            ProcKind::SelfLink => format!("{}", current_pid()).len() as u64,
            ProcKind::CpuInfo | ProcKind::MemInfo | ProcKind::Uptime | ProcKind::Version => {
                self.generate_content(kind)?.len() as u64
            }
            ProcKind::Status(pid) | ProcKind::Cmdline(pid) | ProcKind::Maps(pid) => {
                let _ = lookup_process(pid).ok_or(FsError::NotFound)?;
                self.generate_content(kind)?.len() as u64
            }
        };

        // Use cached metadata for the static root inodes when available so
        // timestamps stay stable, otherwise synthesize fresh metadata.
        if let Some(cached) = self.inodes.read().get(&inode).cloned() {
            if size == cached.size {
                return Ok(cached);
            }
            let mut md = cached;
            md.size = size;
            return Ok(md);
        }

        Ok(Self::base_metadata(inode, file_type, size))
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
        let kind = ProcKind::from_inode(inode).ok_or(FsError::NotFound)?;
        self.list_dir(kind)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn symlink(&self, _target: &str, _link_path: &str) -> FsResult<()> {
        Err(FsError::ReadOnly)
    }

    fn readlink(&self, path: &str) -> FsResult<String> {
        let kind = self.resolve(path)?;
        match kind {
            ProcKind::SelfLink => Ok(format!("{}", current_pid())),
            _ => Err(FsError::InvalidArgument),
        }
    }

    fn sync(&self) -> FsResult<()> {
        // procfs is virtual; nothing to sync.
        Ok(())
    }
}
