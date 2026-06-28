//! RustOS platform drivers for the native GLib/GIO abstraction layer.
//!
//! Binds `glib-native` platform traits to the kernel syscall VFS (`crate::vfs`).

use crate::glib_spawn;
use crate::vfs::{self, InodeType, OpenFlags as VfsOpenFlags, Stat, VfsError};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use core::any::Any;
use core::sync::atomic::{AtomicBool, Ordering};
use glib_native::bytes::Bytes;
use glib_native::dir::{DirError, DirPlatform, DIR_NO_DOT_AND_DOTDOT};
use glib_native::error::Error;
use glib_native::gcancellable::GCancellable;
use glib_native::gdbusaddress::DBusAddressPlatform;
use glib_native::gfile::{FileCreateFlags, FileInfo, FilePlatform, FileQueryInfoFlags, FileType};
use glib_native::ginputstream::{InputStream, MemoryInputStream};
use glib_native::gioerror::{io_error_quark, IOErrorEnum};
use glib_native::giomodule::{IoModuleHandle, IoModulePlatform};
use glib_native::gmodule::{ModuleHandle, ModulePlatform};
use glib_native::goutputstream::{OutputStream, OutputStreamImpl};
use glib_native::mappedfile::{MappedFile, MappedFileError, MappedFilePlatform};
use glib_native::poll::{PollFD, PollPlatform, TimerPollPlatform};
use glib_native::spawn::{SpawnChildSetupFunc, SpawnError, SpawnFlags, SpawnPlatform, SpawnResult};
use glib_native::stdio::{OpenFlags as GOpenFlags, StatBuf, StdioPlatform, F_OK, R_OK, W_OK, X_OK};
use glib_native::thread::{ThreadError, ThreadHandle, ThreadPlatform};
use spin::Mutex;

static RUSTOS_FILE_PLATFORM: RustOsFilePlatform = RustOsFilePlatform;
static RUSTOS_DIR_PLATFORM: RustOsDirPlatform = RustOsDirPlatform;
static RUSTOS_MAPPED_FILE_PLATFORM: RustOsMappedFilePlatform = RustOsMappedFilePlatform;
static RUSTOS_DBUS_ADDRESS_PLATFORM: RustOsDBusAddressPlatform = RustOsDBusAddressPlatform;
static RUSTOS_STDIO_PLATFORM: RustOsStdioPlatform = RustOsStdioPlatform;
static RUSTOS_SPAWN_PLATFORM: RustOsSpawnPlatform = RustOsSpawnPlatform;
static RUSTOS_POLL_PLATFORM: RustOsPollPlatform = RustOsPollPlatform;
static RUSTOS_THREAD_PLATFORM: RustOsThreadPlatform = RustOsThreadPlatform;

/// Register every GLib platform hook with the RustOS VFS-backed drivers.
pub fn register_all() {
    glib_native::register_file_platform(&RUSTOS_FILE_PLATFORM);
    glib_native::register_dir_platform(&RUSTOS_DIR_PLATFORM);
    glib_native::register_mapped_file_platform(&RUSTOS_MAPPED_FILE_PLATFORM);
    glib_native::register_dbus_address_platform(&RUSTOS_DBUS_ADDRESS_PLATFORM);
    glib_native::register_stdio_platform(&RUSTOS_STDIO_PLATFORM);
    glib_native::register_spawn_platform(&RUSTOS_SPAWN_PLATFORM);
    glib_native::register_poll_platform(&RUSTOS_POLL_PLATFORM);
    glib_native::register_thread_platform(&RUSTOS_THREAD_PLATFORM);
}

/// Dynamic module platform for RustOS (no runtime loader; paths use `/lib`).
pub struct RustOsModulePlatform;

impl ModulePlatform for RustOsModulePlatform {
    fn supported() -> bool {
        false
    }

    fn open(_file_name: &str, _bind_lazy: bool, _bind_local: bool) -> Result<ModuleHandle, String> {
        Err(String::from("dynamic modules are not supported on RustOS"))
    }

    fn self_handle() -> Result<ModuleHandle, String> {
        Err(String::from("dynamic modules are not supported on RustOS"))
    }

    fn symbol(_handle: ModuleHandle, _symbol_name: &str) -> Result<*mut core::ffi::c_void, String> {
        Err(String::from("dynamic modules are not supported on RustOS"))
    }

    fn close(_handle: ModuleHandle) {}

    fn build_path(directory: Option<&str>, module_name: &str) -> String {
        let has_lib_prefix = module_name.starts_with("lib");
        let base = directory.unwrap_or("/lib");
        if has_lib_prefix {
            format!("{base}/{module_name}.so")
        } else {
            format!("{base}/lib{module_name}.so")
        }
    }
}

/// GIO module platform for RustOS.
pub struct RustOsIoModulePlatform;

impl IoModulePlatform for RustOsIoModulePlatform {
    fn supported() -> bool {
        false
    }

    fn open(_path: &str) -> Result<IoModuleHandle, glib_native::gioerror::IOErrorEnum> {
        Err(glib_native::gioerror::IOErrorEnum::NotSupported)
    }

    fn symbol(
        _handle: IoModuleHandle,
        _symbol_name: &str,
    ) -> Result<*mut core::ffi::c_void, glib_native::gioerror::IOErrorEnum> {
        Err(glib_native::gioerror::IOErrorEnum::NotSupported)
    }

    fn close(_handle: IoModuleHandle) {}

    fn build_path(directory: Option<&str>, module_name: &str) -> String {
        RustOsModulePlatform::build_path(directory, module_name)
    }
}

pub struct RustOsFilePlatform;

impl FilePlatform for RustOsFilePlatform {
    fn read(&self, path: &str) -> Result<InputStream, Error> {
        let data = vfs_read_all(path).map_err(vfs_to_io_error)?;
        Ok(InputStream::from(MemoryInputStream::new_from_bytes(
            Bytes::new(&data),
        )))
    }

    fn create(&self, path: &str, _flags: FileCreateFlags) -> Result<OutputStream, Error> {
        let fd = vfs::vfs_open(
            path,
            VfsOpenFlags::WRONLY | VfsOpenFlags::CREAT | VfsOpenFlags::TRUNC,
            0o644,
        )
        .map_err(vfs_to_io_error)?;
        Ok(OutputStream::new(VfsFdOutputStream::new(fd)))
    }

    fn replace(
        &self,
        path: &str,
        _etag: Option<&str>,
        _make_backup: bool,
        _flags: FileCreateFlags,
    ) -> Result<OutputStream, Error> {
        self.create(path, FileCreateFlags::None)
    }

    fn query_exists(&self, path: &str) -> bool {
        vfs::vfs_stat(path).is_ok()
    }

    fn query_info(
        &self,
        path: &str,
        _attributes: &str,
        _flags: FileQueryInfoFlags,
    ) -> Result<FileInfo, Error> {
        let stat = vfs::vfs_stat(path).map_err(vfs_to_io_error)?;
        Ok(stat_to_file_info(&stat, path))
    }

    fn delete(&self, path: &str) -> Result<(), Error> {
        vfs_delete_path(path).map_err(vfs_to_io_error)
    }

    fn trash(&self, path: &str) -> Result<(), Error> {
        vfs_delete_path(path).map_err(vfs_to_io_error)
    }
}

pub struct RustOsDirPlatform;

impl DirPlatform for RustOsDirPlatform {
    fn open(&self, path: &str, flags: u32) -> Result<Vec<String>, DirError> {
        let entries = vfs::vfs_readdir(path).map_err(vfs_to_dir_error)?;
        let mut names: Vec<String> = entries.into_iter().map(|e| e.name).collect();
        if flags & DIR_NO_DOT_AND_DOTDOT != 0 {
            names.retain(|name| name != "." && name != "..");
        }
        names.sort();
        Ok(names)
    }
}

pub struct RustOsMappedFilePlatform;

impl MappedFilePlatform for RustOsMappedFilePlatform {
    fn open(&self, path: &str, writable: bool) -> Result<MappedFile, MappedFileError> {
        let data = vfs_read_all(path).map_err(vfs_to_mapped_file_error)?;
        Ok(MappedFile::from_contents(data, writable))
    }

    fn open_from_fd(&self, fd: i32, writable: bool) -> Result<MappedFile, MappedFileError> {
        if fd < 0 {
            return Err(MappedFileError::InvalidFd);
        }
        let stat = vfs::vfs_fstat(fd).map_err(|_| MappedFileError::InvalidFd)?;
        if stat.inode_type != InodeType::File {
            return Err(MappedFileError::Other);
        }
        let mut data = alloc::vec![0u8; stat.size as usize];
        let read = vfs::vfs_pread(fd, &mut data, 0).map_err(|_| MappedFileError::Other)?;
        data.truncate(read);
        Ok(MappedFile::from_contents(data, writable))
    }
}

pub struct RustOsDBusAddressPlatform;

impl DBusAddressPlatform for RustOsDBusAddressPlatform {
    fn get_session_bus_address(&self) -> Option<String> {
        if crate::gnome_overlay::is_ready() {
            Some(String::from(crate::gnome_overlay::DBUS_SESSION_ADDRESS))
        } else {
            Some(String::from("loopback:"))
        }
    }

    fn get_system_bus_address(&self) -> Option<String> {
        Some(String::from("unix:path=/run/dbus/system_bus_socket"))
    }
}

pub struct RustOsStdioPlatform;

impl StdioPlatform for RustOsStdioPlatform {
    fn access(&self, path: &str, mode: i32) -> i32 {
        match vfs::vfs_stat(path) {
            Ok(stat) => {
                if mode == F_OK {
                    return 0;
                }
                let perm = stat.mode & 0o777;
                if mode & R_OK != 0 && perm & 0o400 == 0 {
                    return -1;
                }
                if mode & W_OK != 0 && perm & 0o200 == 0 {
                    return -1;
                }
                if mode & X_OK != 0 && perm & 0o111 == 0 {
                    return -1;
                }
                0
            }
            Err(_) => -1,
        }
    }

    fn chdir(&self, _path: &str) -> i32 {
        -1
    }

    fn mkdir(&self, path: &str, mode: u32) -> i32 {
        if vfs::vfs_mkdir(path, mode).is_ok() {
            0
        } else {
            -1
        }
    }

    fn rmdir(&self, path: &str) -> i32 {
        if vfs::vfs_rmdir(path).is_ok() {
            0
        } else {
            -1
        }
    }

    fn unlink(&self, path: &str) -> i32 {
        if vfs::vfs_unlink(path).is_ok() {
            0
        } else {
            -1
        }
    }

    fn rename(&self, oldpath: &str, newpath: &str) -> i32 {
        if vfs::vfs_rename(oldpath, newpath).is_ok() {
            0
        } else {
            -1
        }
    }

    fn chmod(&self, path: &str, mode: u32) -> i32 {
        if vfs::vfs_chmod(path, mode).is_ok() {
            0
        } else {
            -1
        }
    }

    fn open(&self, path: &str, flags: GOpenFlags, mode: u32) -> i32 {
        vfs::vfs_open(path, flags.0 as u32, mode).unwrap_or(-1)
    }

    fn creat(&self, path: &str, mode: u32) -> i32 {
        vfs::vfs_open(
            path,
            VfsOpenFlags::WRONLY | VfsOpenFlags::CREAT | VfsOpenFlags::TRUNC,
            mode,
        )
        .unwrap_or(-1)
    }

    fn read(&self, fd: i32, buf: &mut [u8]) -> isize {
        vfs::vfs_read(fd, buf).map(|n| n as isize).unwrap_or(-1)
    }

    fn close(&self, fd: i32) -> i32 {
        if vfs::vfs_close(fd).is_ok() {
            0
        } else {
            -1
        }
    }

    fn stat(&self, path: &str) -> Option<StatBuf> {
        vfs::vfs_stat(path).ok().map(stat_to_stat_buf)
    }

    fn lstat(&self, path: &str) -> Option<StatBuf> {
        self.stat(path)
    }

    fn remove(&self, path: &str) -> i32 {
        match vfs::vfs_stat(path) {
            Ok(stat) => match stat.inode_type {
                InodeType::Directory => self.rmdir(path),
                _ => self.unlink(path),
            },
            Err(_) => -1,
        }
    }

    fn fsync(&self, fd: i32) -> i32 {
        if vfs::vfs_fsync(fd).is_ok() {
            0
        } else {
            -1
        }
    }

    fn list_dir(&self, path: &str) -> Vec<String> {
        vfs::vfs_list_dir(path)
            .map(|entries| {
                entries
                    .into_iter()
                    .map(|entry| entry.name)
                    .filter(|name| name != "." && name != "..")
                    .collect()
            })
            .unwrap_or_else(|_| Vec::new())
    }
}

pub struct RustOsSpawnPlatform;

/// Poll platform for RustOS: timer-based wait (no kernel `poll` syscall yet).
pub struct RustOsPollPlatform;

impl PollPlatform for RustOsPollPlatform {
    fn poll(&self, fds: &mut [PollFD], timeout_ms: i32) -> i32 {
        TimerPollPlatform.poll(fds, timeout_ms)
    }
}

/// Thread platform for RustOS: threads are not available on bare metal.
pub struct RustOsThreadPlatform;

impl ThreadPlatform for RustOsThreadPlatform {
    fn spawn(&self, _name: &str, _func: fn()) -> Result<ThreadHandle, ThreadError> {
        Err(ThreadError::Again)
    }

    fn join(&self, _handle: ThreadHandle) -> Result<(), ThreadError> {
        Err(ThreadError::Again)
    }
}

impl SpawnPlatform for RustOsSpawnPlatform {
    fn spawn_async(
        &self,
        working_directory: Option<&str>,
        argv: &[&str],
        envp: Option<&[&str]>,
        flags: SpawnFlags,
        child_setup: Option<SpawnChildSetupFunc>,
    ) -> Result<i32, SpawnError> {
        glib_spawn::spawn_child_async(working_directory, argv, envp, flags, child_setup)
    }

    fn spawn_sync(
        &self,
        working_directory: Option<&str>,
        argv: &[&str],
        envp: Option<&[&str]>,
        flags: SpawnFlags,
        child_setup: Option<SpawnChildSetupFunc>,
    ) -> Result<SpawnResult, SpawnError> {
        glib_spawn::spawn_child_sync(working_directory, argv, envp, flags, child_setup)
    }

    fn check_wait_status(&self, wait_status: i32) -> Result<(), SpawnError> {
        if wait_status == 0 {
            Ok(())
        } else {
            Err(SpawnError::Failed)
        }
    }
}

struct VfsFdOutputStream {
    fd: Mutex<Option<i32>>,
    closed: AtomicBool,
    pending: AtomicBool,
}

impl VfsFdOutputStream {
    fn new(fd: i32) -> Self {
        Self {
            fd: Mutex::new(Some(fd)),
            closed: AtomicBool::new(false),
            pending: AtomicBool::new(false),
        }
    }
}

impl OutputStreamImpl for VfsFdOutputStream {
    fn as_any(&self) -> &dyn Any {
        self
    }

    fn write(&self, buffer: &[u8], _cancellable: Option<&GCancellable>) -> Result<usize, Error> {
        if self.is_closed() {
            return Err(io_error(IOErrorEnum::Closed, "stream closed"));
        }
        let fd = *self.fd.lock();
        let fd = fd.ok_or_else(|| io_error(IOErrorEnum::Closed, "stream closed"))?;
        vfs::vfs_write(fd, buffer).map_err(|e| vfs_to_io_error(e))
    }

    fn flush(&self, _cancellable: Option<&GCancellable>) -> Result<(), Error> {
        if self.is_closed() {
            return Err(io_error(IOErrorEnum::Closed, "stream closed"));
        }
        Ok(())
    }

    fn close(&self, _cancellable: Option<&GCancellable>) -> Result<(), Error> {
        if self.closed.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        if let Some(fd) = self.fd.lock().take() {
            let _ = vfs::vfs_close(fd);
        }
        Ok(())
    }

    fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    fn has_pending(&self) -> bool {
        self.pending.load(Ordering::SeqCst)
    }

    fn set_pending(&self) -> Result<(), Error> {
        if self
            .pending
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err(io_error(IOErrorEnum::Pending, "operation pending"));
        }
        Ok(())
    }

    fn clear_pending(&self) {
        self.pending.store(false, Ordering::SeqCst);
    }
}

fn vfs_read_all(path: &str) -> Result<Vec<u8>, VfsError> {
    let stat = vfs::vfs_stat(path)?;
    if stat.inode_type != InodeType::File {
        return Err(VfsError::IsDirectory);
    }
    let fd = vfs::vfs_open(path, VfsOpenFlags::RDONLY, 0)?;
    let mut data = alloc::vec![0u8; stat.size as usize];
    let mut offset = 0usize;
    while offset < data.len() {
        let n = vfs::vfs_read(fd, &mut data[offset..])?;
        if n == 0 {
            break;
        }
        offset += n;
    }
    let _ = vfs::vfs_close(fd);
    data.truncate(offset);
    Ok(data)
}

fn vfs_delete_path(path: &str) -> Result<(), VfsError> {
    match vfs::vfs_stat(path)?.inode_type {
        InodeType::Directory => vfs::vfs_rmdir(path),
        _ => vfs::vfs_unlink(path),
    }
}

fn stat_to_file_info(stat: &Stat, path: &str) -> FileInfo {
    let mut info = FileInfo::new();
    if let Some(name) = path.rsplit('/').next() {
        if !name.is_empty() {
            info.set_name(name);
            info.set_attribute_string("standard::display-name", name);
        }
    }
    info.set_size(stat.size);
    info.set_file_type(match stat.inode_type {
        InodeType::File => FileType::Regular,
        InodeType::Directory => FileType::Directory,
        InodeType::Symlink => FileType::SymbolicLink,
        _ => FileType::Special,
    });
    info
}

fn stat_to_stat_buf(stat: Stat) -> StatBuf {
    let file_type_bits = match stat.inode_type {
        InodeType::File => 0o100000,
        InodeType::Directory => 0o040000,
        InodeType::Symlink => 0o120000,
        InodeType::CharDevice => 0o020000,
        InodeType::BlockDevice => 0o060000,
        InodeType::Fifo => 0o010000,
        InodeType::Socket => 0o140000,
    };
    StatBuf {
        st_size: stat.size,
        st_mode: file_type_bits | (stat.mode & 0o7777),
        st_uid: stat.uid,
        gid: stat.gid,
        st_atime: stat.atime as i64,
        st_mtime: stat.mtime as i64,
        st_ctime: stat.ctime as i64,
        st_dev: stat.rdev,
        st_ino: stat.ino,
        st_nlink: stat.nlink as u64,
        st_blocks: stat.blocks,
        st_blksize: stat.blksize,
    }
}

fn io_error(code: IOErrorEnum, message: &str) -> Error {
    Error::new(io_error_quark(), code.to_code(), message)
}

fn vfs_to_io_error(err: VfsError) -> Error {
    let code = match err {
        VfsError::NotFound => IOErrorEnum::NotFound,
        VfsError::PermissionDenied => IOErrorEnum::PermissionDenied,
        VfsError::AlreadyExists => IOErrorEnum::Exists,
        VfsError::NotDirectory => IOErrorEnum::NotDirectory,
        VfsError::IsDirectory => IOErrorEnum::IsDirectory,
        VfsError::InvalidArgument => IOErrorEnum::InvalidArgument,
        VfsError::ReadOnly => IOErrorEnum::ReadOnly,
        VfsError::NotSupported => IOErrorEnum::NotSupported,
        _ => IOErrorEnum::Failed,
    };
    io_error(code, "VFS operation failed")
}

fn vfs_to_dir_error(err: VfsError) -> DirError {
    match err {
        VfsError::NotFound => DirError::NotFound,
        VfsError::PermissionDenied => DirError::PermissionDenied,
        VfsError::NotDirectory => DirError::NotDirectory,
        _ => DirError::Other,
    }
}

fn vfs_to_mapped_file_error(err: VfsError) -> MappedFileError {
    match err {
        VfsError::NotFound => MappedFileError::NotFound,
        VfsError::PermissionDenied => MappedFileError::PermissionDenied,
        _ => MappedFileError::Other,
    }
}
