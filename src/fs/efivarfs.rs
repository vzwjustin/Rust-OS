//! EFIVARFS (EFI Variable File System) implementation.
//!
//! Efivarfs is a pseudo-filesystem that exposes UEFI firmware variables as
//! regular files. Each file represents one EFI variable, with the file name
//! encoding the variable's GUID and name, and the file contents holding the
//! variable's data payload.
//!
//! In a real UEFI environment, reads and writes are translated to EFI runtime
//! services calls (`GetVariable`, `SetVariable`, `GetNextVariableName`). This
//! implementation uses an in-memory `BTreeMap` as the variable store, which
//! can be populated at boot time from the EFI runtime table if available.
//!
//! File naming convention (matching Linux efivarfs):
//!   `variable-name-<GUID>`
//! where GUID is formatted as `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`.
//!
//! See: linux-master/fs/efivarfs/

use super::{
    get_current_time, DirectoryEntry, FileMetadata, FilePermissions, FileSystem, FileSystemStats,
    FileSystemType, FileType, FsError, FsResult, InodeNumber, OpenFlags,
};
use alloc::{
    collections::BTreeMap,
    string::{String, ToString},
    vec::Vec,
};
use spin::RwLock;

/// Inode number for the root directory.
const ROOT_INODE: InodeNumber = 1;
/// First inode number available for EFI variable files.
const FIRST_VAR_INODE: InodeNumber = 2;

/// EFI variable attributes (from UEFI specification).
#[allow(dead_code)]
const EFI_VARIABLE_NON_VOLATILE: u32 = 0x00000001;
#[allow(dead_code)]
const EFI_VARIABLE_BOOTSERVICE_ACCESS: u32 = 0x00000002;
#[allow(dead_code)]
const EFI_VARIABLE_RUNTIME_ACCESS: u32 = 0x00000004;
#[allow(dead_code)]
const EFI_VARIABLE_HARDWARE_ERROR_RECORD: u32 = 0x00000008;
#[allow(dead_code)]
const EFI_VARIABLE_AUTHENTICATED_WRITE_ACCESS: u32 = 0x00000010;
#[allow(dead_code)]
const EFI_VARIABLE_TIME_BASED_AUTHENTICATED_WRITE_ACCESS: u32 = 0x00000020;
#[allow(dead_code)]
const EFI_VARIABLE_APPEND_WRITE: u32 = 0x00000040;

/// Default attributes for new EFI variables (non-volatile + bootservice + runtime).
const DEFAULT_VAR_ATTRIBUTES: u32 = 0x00000001 | 0x00000002 | 0x00000004;

/// An EFI variable entry stored in the variable map.
#[derive(Debug, Clone)]
struct EfiVariable {
    /// Variable data payload.
    data: Vec<u8>,
    /// EFI variable attributes.
    #[allow(dead_code)]
    attributes: u32,
}

/// EFIVARFS virtual filesystem exposing UEFI variables as files.
///
/// The variable store is an in-memory `BTreeMap` keyed by the variable file
/// name (e.g. `BootOrder-8be4df61-93ca-11d2-aa0d-00e09803246c`). Inode numbers
/// are assigned sequentially starting from `FIRST_VAR_INODE` and are kept
/// stable via a secondary name→inode map.
#[derive(Debug)]
pub struct EfivarFs {
    /// Variable store: file name → variable data + attributes.
    variables: RwLock<BTreeMap<String, EfiVariable>>,
    /// Name → inode number mapping (for stable inode assignment).
    name_to_inode: RwLock<BTreeMap<String, InodeNumber>>,
    /// Inode number → variable name mapping (reverse lookup).
    inode_to_name: RwLock<BTreeMap<InodeNumber, String>>,
    /// Next inode number to assign.
    next_inode: RwLock<InodeNumber>,
}

impl EfivarFs {
    /// Create a new efivarfs instance with an empty variable store.
    ///
    /// In a real UEFI boot, this would query `GetNextVariableName` to populate
    /// the initial variable list. Here we start empty; variables can be added
    /// via `create` and `write`.
    pub fn new() -> FsResult<Self> {
        Ok(Self {
            variables: RwLock::new(BTreeMap::new()),
            name_to_inode: RwLock::new(BTreeMap::new()),
            inode_to_name: RwLock::new(BTreeMap::new()),
            next_inode: RwLock::new(FIRST_VAR_INODE),
        })
    }

    /// Create a new efivarfs instance pre-populated with a set of variables.
    /// Each tuple is (variable_name, data, attributes).
    pub fn with_variables(vars: &[(String, Vec<u8>, u32)]) -> FsResult<Self> {
        let fs = Self::new()?;
        {
            let mut variables = fs.variables.write();
            let mut name_to_inode = fs.name_to_inode.write();
            let mut inode_to_name = fs.inode_to_name.write();
            let mut next_inode = fs.next_inode.write();
            for (name, data, attrs) in vars {
                let ino = *next_inode;
                *next_inode += 1;
                variables.insert(
                    name.clone(),
                    EfiVariable {
                        data: data.clone(),
                        attributes: *attrs,
                    },
                );
                name_to_inode.insert(name.clone(), ino);
                inode_to_name.insert(ino, name.clone());
            }
        }
        Ok(fs)
    }

    /// Assign (or retrieve) an inode number for a variable name.
    fn assign_inode(&self, name: &str) -> InodeNumber {
        {
            let map = self.name_to_inode.read();
            if let Some(&ino) = map.get(name) {
                return ino;
            }
        }
        let mut next = self.next_inode.write();
        let ino = *next;
        *next += 1;
        self.name_to_inode.write().insert(name.to_string(), ino);
        self.inode_to_name.write().insert(ino, name.to_string());
        ino
    }

    /// Look up the variable name for a given inode number.
    fn name_for_inode(&self, inode: InodeNumber) -> Option<String> {
        let map = self.inode_to_name.read();
        map.get(&inode).cloned()
    }

    /// Parse a path into a variable name. Strips leading slashes.
    fn parse_path(path: &str) -> String {
        path.trim_start_matches('/').to_string()
    }

    /// Build metadata for a variable file.
    fn metadata_for_var(&self, _name: &str, ino: InodeNumber, data_len: usize) -> FileMetadata {
        let now = get_current_time();
        FileMetadata {
            inode: ino,
            file_type: FileType::Regular,
            size: data_len as u64,
            permissions: FilePermissions::from_octal(0o644),
            uid: 0,
            gid: 0,
            created: now,
            modified: now,
            accessed: now,
            link_count: 1,
            device_id: None,
        }
    }
}

impl FileSystem for EfivarFs {
    fn fs_type(&self) -> FileSystemType {
        FileSystemType::SysFs
    }

    fn statfs(&self) -> FsResult<FileSystemStats> {
        let variables = self.variables.read();
        let count = variables.len() as u64;
        Ok(FileSystemStats {
            total_blocks: 0,
            free_blocks: 0,
            available_blocks: 0,
            total_inodes: count + 1,
            free_inodes: u64::MAX - count,
            block_size: 4096,
            max_filename_length: 255,
        })
    }

    fn create(&self, path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        let name = Self::parse_path(path);
        if name.is_empty() || name == "/" {
            return Err(FsError::InvalidArgument);
        }

        let mut variables = self.variables.write();
        if variables.contains_key(&name) {
            return Err(FsError::AlreadyExists);
        }

        let ino = self.assign_inode(&name);
        variables.insert(
            name.clone(),
            EfiVariable {
                data: Vec::new(),
                attributes: DEFAULT_VAR_ATTRIBUTES,
            },
        );
        Ok(ino)
    }

    fn open(&self, path: &str, flags: OpenFlags) -> FsResult<InodeNumber> {
        let name = Self::parse_path(path);

        if name.is_empty() {
            return Ok(ROOT_INODE);
        }

        let variables = self.variables.read();
        if variables.get(&name).is_some() {
            let ino = self.assign_inode(&name);
            if flags.truncate && flags.write {
                drop(variables);
                let mut vars = self.variables.write();
                if let Some(v) = vars.get_mut(&name) {
                    v.data.clear();
                }
            }
            return Ok(ino);
        }

        if flags.create {
            drop(variables);
            return self.create(path, FilePermissions::default_file());
        }

        Err(FsError::NotFound)
    }

    fn read(&self, inode: InodeNumber, offset: u64, buffer: &mut [u8]) -> FsResult<usize> {
        if inode == ROOT_INODE {
            return Err(FsError::IsADirectory);
        }

        let name = self.name_for_inode(inode).ok_or(FsError::NotFound)?;
        let variables = self.variables.read();
        let var = variables.get(&name).ok_or(FsError::NotFound)?;

        let data_len = var.data.len() as u64;
        if offset >= data_len {
            return Ok(0);
        }

        let remaining = data_len - offset;
        let to_read = core::cmp::min(buffer.len() as u64, remaining) as usize;
        let start = offset as usize;
        let end = start + to_read;
        buffer[..to_read].copy_from_slice(&var.data[start..end]);
        Ok(to_read)
    }

    fn write(&self, inode: InodeNumber, offset: u64, buffer: &[u8]) -> FsResult<usize> {
        if inode == ROOT_INODE {
            return Err(FsError::IsADirectory);
        }

        let name = self.name_for_inode(inode).ok_or(FsError::NotFound)?;
        let mut variables = self.variables.write();
        let var = variables.get_mut(&name).ok_or(FsError::NotFound)?;

        let required_len = (offset + buffer.len() as u64) as usize;
        if var.data.len() < required_len {
            var.data.resize(required_len, 0);
        }

        let start = offset as usize;
        let end = start + buffer.len();
        var.data[start..end].copy_from_slice(buffer);

        Ok(buffer.len())
    }

    fn metadata(&self, inode: InodeNumber) -> FsResult<FileMetadata> {
        if inode == ROOT_INODE {
            return Ok(FileMetadata {
                inode: ROOT_INODE,
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

        let name = self.name_for_inode(inode).ok_or(FsError::NotFound)?;
        let variables = self.variables.read();
        let var = variables.get(&name).ok_or(FsError::NotFound)?;
        Ok(self.metadata_for_var(&name, inode, var.data.len()))
    }

    fn set_metadata(&self, _inode: InodeNumber, _metadata: &FileMetadata) -> FsResult<()> {
        Ok(())
    }

    fn mkdir(&self, _path: &str, _permissions: FilePermissions) -> FsResult<InodeNumber> {
        Err(FsError::NotSupported)
    }

    fn rmdir(&self, _path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
    }

    fn unlink(&self, path: &str) -> FsResult<()> {
        let name = Self::parse_path(path);
        if name.is_empty() {
            return Err(FsError::InvalidArgument);
        }

        let mut variables = self.variables.write();
        if variables.remove(&name).is_none() {
            return Err(FsError::NotFound);
        }

        if let Some(ino) = self.name_to_inode.write().remove(&name) {
            self.inode_to_name.write().remove(&ino);
        }

        Ok(())
    }

    fn readdir(&self, inode: InodeNumber) -> FsResult<Vec<DirectoryEntry>> {
        if inode != ROOT_INODE {
            return Err(FsError::NotADirectory);
        }

        let variables = self.variables.read();
        let name_to_inode = self.name_to_inode.read();
        let mut entries = Vec::new();
        for (name, _) in variables.iter() {
            let ino = name_to_inode.get(name).copied().unwrap_or(FIRST_VAR_INODE);
            entries.push(DirectoryEntry {
                name: name.clone(),
                inode: ino,
                file_type: FileType::Regular,
            });
        }
        Ok(entries)
    }

    fn rename(&self, _old_path: &str, _new_path: &str) -> FsResult<()> {
        Err(FsError::NotSupported)
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
