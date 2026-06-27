//! Shared helpers for GIO CLI tools matching `gio/gio-tool.c`.
//!
//! Provides error printing, type/name helpers, help text, and an in-memory
//! virtual filesystem used by the ported `gio` subcommands.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::bytes::Bytes;
use crate::error::Error;
use crate::gfile::{File, FileCreateFlags, FileInfo, FilePlatform, FileQueryInfoFlags, FileType};
use crate::gfileattribute::FileAttributeType;
use crate::ginputstream::{InputStream, MemoryInputStream};
use crate::gioerror::IOErrorEnum;
use crate::goutputstream::{MemoryOutputStream, OutputStream};
use crate::prelude::*;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::string::ToString;
use spin::Mutex;

static STDERR: Mutex<Vec<u8>> = Mutex::new(Vec::new());
static STDOUT: Mutex<Vec<u8>> = Mutex::new(Vec::new());

/// In-memory filesystem backing the tool `FilePlatform`.
pub struct ToolVfs {
    pub files: BTreeMap<String, Vec<u8>>,
    pub dirs: BTreeSet<String>,
}

impl ToolVfs {
    pub fn new() -> Self {
        let mut vfs = Self {
            files: BTreeMap::new(),
            dirs: BTreeSet::new(),
        };
        vfs.dirs.insert("/".to_string());
        vfs
    }

    pub fn reset(&mut self) {
        self.files.clear();
        self.dirs.clear();
        self.dirs.insert("/".to_string());
    }

    pub fn add_dir(&mut self, path: &str) {
        self.dirs.insert(path.to_string());
    }

    pub fn add_file(&mut self, path: &str, data: &[u8]) {
        if let Some(parent) = parent_path(path) {
            self.dirs.insert(parent);
        }
        self.files.insert(path.to_string(), data.to_vec());
    }

    pub fn is_dir(&self, path: &str) -> bool {
        self.dirs.contains(path)
    }

    pub fn list_children(&self, dir: &str) -> Vec<FileInfo> {
        let prefix = if dir.ends_with('/') {
            dir.to_string()
        } else {
            format!("{}/", dir)
        };
        let mut names = BTreeSet::new();
        for path in self.files.keys() {
            if let Some(rest) = path.strip_prefix(&prefix) {
                if let Some(name) = rest.split('/').next() {
                    if !name.is_empty() {
                        names.insert(name.to_string());
                    }
                }
            }
        }
        for d in &self.dirs {
            if let Some(rest) = d.strip_prefix(&prefix) {
                if let Some(name) = rest.split('/').next() {
                    if !name.is_empty() {
                        names.insert(name.to_string());
                    }
                }
            }
        }
        names
            .into_iter()
            .map(|name| {
                let child = format!("{}{}", prefix, name);
                let mut info = FileInfo::new();
                info.set_name(&name);
                if self.is_dir(&child) {
                    info.set_file_type(FileType::Directory);
                } else if self.files.contains_key(&child) {
                    info.set_file_type(FileType::Regular);
                    info.set_size(self.files[&child].len() as u64);
                } else {
                    info.set_file_type(FileType::Unknown);
                }
                info
            })
            .collect()
    }
}

fn parent_path(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    trimmed.rfind('/').map(|idx| {
        if idx == 0 {
            "/".to_string()
        } else {
            trimmed[..idx].to_string()
        }
    })
}

static TOOL_VFS: Mutex<ToolVfs> = Mutex::new(ToolVfs {
    files: BTreeMap::new(),
    dirs: BTreeSet::new(),
});

#[cfg(test)]
mod test_io {
    use std::cell::RefCell;
    std::thread_local! {
        pub(super) static STDERR: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
        pub(super) static STDOUT: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }
}

#[cfg(test)]
fn clear_io_buffers() {
    test_io::STDERR.with(|b| b.borrow_mut().clear());
    test_io::STDOUT.with(|b| b.borrow_mut().clear());
}

#[cfg(test)]
fn take_stdout_buffer() -> Vec<u8> {
    test_io::STDOUT.with(|b| core::mem::take(&mut *b.borrow_mut()))
}

#[cfg(test)]
fn take_stderr_buffer() -> Vec<u8> {
    test_io::STDERR.with(|b| core::mem::take(&mut *b.borrow_mut()))
}

#[cfg(test)]
fn append_stdout_buffer(msg: &str) {
    test_io::STDOUT.with(|b| b.borrow_mut().extend_from_slice(msg.as_bytes()));
}

#[cfg(test)]
fn append_stderr_buffer(msg: &str) {
    test_io::STDERR.with(|b| b.borrow_mut().extend_from_slice(msg.as_bytes()));
}

/// Reset tool stdout/stderr capture buffers.
pub fn reset_tool_state() {
    #[cfg(test)]
    clear_io_buffers();
    #[cfg(not(test))]
    {
        STDERR.lock().clear();
        STDOUT.lock().clear();
    }
}

/// Reset the in-memory VFS (typically in tests).
pub fn reset_tool_vfs() {
    TOOL_VFS.lock().reset();
}

/// Returns captured stdout bytes.
pub fn take_stdout() -> Vec<u8> {
    #[cfg(test)]
    return take_stdout_buffer();
    #[cfg(not(test))]
    core::mem::take(&mut *STDOUT.lock())
}

/// Returns captured stderr bytes.
pub fn take_stderr() -> Vec<u8> {
    #[cfg(test)]
    return take_stderr_buffer();
    #[cfg(not(test))]
    core::mem::take(&mut *STDERR.lock())
}

fn write_stderr(msg: &str) {
    #[cfg(test)]
    append_stderr_buffer(msg);
    #[cfg(not(test))]
    STDERR.lock().extend_from_slice(msg.as_bytes());
}

fn write_stdout(msg: &str) {
    #[cfg(test)]
    append_stdout_buffer(msg);
    #[cfg(not(test))]
    STDOUT.lock().extend_from_slice(msg.as_bytes());
}

/// Print a `gio:` error line to the tool stderr buffer.
pub fn print_error(msg: &str) {
    write_stderr(&format!("gio: {msg}\n"));
}

/// Print a file URI error line to the tool stderr buffer.
pub fn print_file_error(file: &File, msg: &str) {
    print_error(&format!("{}: {msg}", file.get_uri()));
}

/// Print a line to the tool stdout buffer.
pub fn print_line(msg: &str) {
    write_stdout(msg);
    if !msg.ends_with('\n') {
        write_stdout("\n");
    }
}

/// Append raw bytes to the tool stdout buffer.
pub fn append_stdout(data: &[u8]) {
    #[cfg(test)]
    test_io::STDOUT.with(|b| b.borrow_mut().extend_from_slice(data));
    #[cfg(not(test))]
    STDOUT.lock().extend_from_slice(data);
}

/// Human-readable name for [`FileType`].
pub fn file_type_to_string(ty: FileType) -> &'static str {
    match ty {
        FileType::Unknown => "unknown",
        FileType::Regular => "regular",
        FileType::Directory => "directory",
        FileType::SymbolicLink => "symlink",
        FileType::Special => "special",
        FileType::ShortCut => "shortcut",
        FileType::Mountable => "mountable",
    }
}

/// Human-readable name for [`FileAttributeType`].
pub fn attribute_type_to_string(ty: FileAttributeType) -> &'static str {
    ty.as_str()
}

/// Print help for a subcommand, optionally prefixed with an error message.
pub fn show_help(command: &str, summary: &str, usage: &str, message: Option<&str>) {
    if let Some(msg) = message {
        print_error(msg);
        write_stderr("\n");
    }
    write_stderr(&format!("Usage: gio {command} {usage}\n\n{summary}\n"));
}

/// Returns whether `file` refers to a directory in the tool VFS / platform.
pub fn file_is_dir(file: &File) -> bool {
    if let Some(path) = file.get_path() {
        if TOOL_VFS.lock().is_dir(&path) {
            return true;
        }
    }
    file.query_info("standard::type", FileQueryInfoFlags::None, None)
        .map(|info| info.get_file_type() == FileType::Directory)
        .unwrap_or(false)
}

/// Build a child [`File`] under `parent`.
pub fn file_get_child(parent: &File, name: &str) -> File {
    let base = parent.get_path().unwrap_or_else(|| "/".to_string());
    let child = if base.ends_with('/') {
        format!("{base}{name}")
    } else {
        format!("{base}/{name}")
    };
    File::new_for_path(&child)
}

/// Store written stream bytes into the tool VFS.
pub fn vfs_store_from_stream(path: &str, stream: &OutputStream) {
    if let Some(mem) = stream.downcast_ref::<MemoryOutputStream>() {
        TOOL_VFS.lock().add_file(path, &mem.get_data());
    }
}

/// Access the tool VFS for tests.
pub fn with_tool_vfs<F, R>(f: F) -> R
where
    F: FnOnce(&mut ToolVfs) -> R,
{
    f(&mut *TOOL_VFS.lock())
}

/// `FilePlatform` implementation backed by [`TOOL_VFS`].
pub struct ToolFilePlatform;

impl FilePlatform for ToolFilePlatform {
    fn read(&self, path: &str) -> Result<InputStream, Error> {
        let vfs = TOOL_VFS.lock();
        let data = vfs.files.get(path).ok_or_else(|| {
            Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::NotFound.to_code(),
                "No such file",
            )
        })?;
        Ok(InputStream::from(MemoryInputStream::new_from_bytes(
            Bytes::from_vec(data.clone()),
        )))
    }

    fn create(&self, path: &str, _flags: FileCreateFlags) -> Result<OutputStream, Error> {
        if TOOL_VFS.lock().files.contains_key(path) {
            return Err(Error::new(
                crate::gioerror::io_error_quark(),
                IOErrorEnum::Exists.to_code(),
                "File exists",
            ));
        }
        Ok(OutputStream::from(MemoryOutputStream::new_resizable()))
    }

    fn replace(
        &self,
        _path: &str,
        _etag: Option<&str>,
        _make_backup: bool,
        _flags: FileCreateFlags,
    ) -> Result<OutputStream, Error> {
        Ok(OutputStream::from(MemoryOutputStream::new_resizable()))
    }

    fn query_exists(&self, path: &str) -> bool {
        let vfs = TOOL_VFS.lock();
        vfs.files.contains_key(path) || vfs.dirs.contains(path)
    }

    fn query_info(
        &self,
        path: &str,
        _attributes: &str,
        _flags: FileQueryInfoFlags,
    ) -> Result<FileInfo, Error> {
        let vfs = TOOL_VFS.lock();
        let mut info = FileInfo::new();
        if let Some(data) = vfs.files.get(path) {
            info.set_file_type(FileType::Regular);
            info.set_size(data.len() as u64);
            if let Some(name) = path.rsplit('/').next() {
                info.set_name(name);
            }
            return Ok(info);
        }
        if vfs.dirs.contains(path) {
            info.set_file_type(FileType::Directory);
            if let Some(name) = path.trim_end_matches('/').rsplit('/').next() {
                info.set_name(if name.is_empty() { "/" } else { name });
            }
            return Ok(info);
        }
        Err(Error::new(
            crate::gioerror::io_error_quark(),
            IOErrorEnum::NotFound.to_code(),
            "Not found",
        ))
    }

    fn delete(&self, path: &str) -> Result<(), Error> {
        let mut vfs = TOOL_VFS.lock();
        if vfs.files.remove(path).is_some() {
            return Ok(());
        }
        if vfs.dirs.remove(path) {
            return Ok(());
        }
        Err(Error::new(
            crate::gioerror::io_error_quark(),
            IOErrorEnum::NotFound.to_code(),
            "Not found",
        ))
    }

    fn trash(&self, path: &str) -> Result<(), Error> {
        self.delete(path)
    }
}

static TOOL_PLATFORM: ToolFilePlatform = ToolFilePlatform;

/// Register the in-memory tool filesystem as the active [`FilePlatform`].
pub fn register_tool_file_platform() {
    crate::gfile::register_file_platform(&TOOL_PLATFORM);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_print_error() {
        reset_tool_state();
        reset_tool_vfs();
        print_error("something failed");
        assert_eq!(take_stderr(), b"gio: something failed\n".to_vec());
    }

    #[test]
    fn test_file_type_to_string() {
        assert_eq!(file_type_to_string(FileType::Directory), "directory");
    }

    #[test]
    fn test_tool_vfs_list_children() {
        reset_tool_state();
        with_tool_vfs(|vfs| {
            vfs.add_dir("/tmp");
            vfs.add_file("/tmp/a.txt", b"aaa");
            vfs.add_file("/tmp/b.txt", b"bbb");
        });
        let kids = TOOL_VFS.lock().list_children("/tmp");
        assert_eq!(kids.len(), 2);
    }

    #[test]
    fn test_show_help() {
        reset_tool_state();
        show_help(
            "cat",
            "Concatenate files",
            "LOCATION…",
            Some("missing file"),
        );
        let err = take_stderr();
        assert!(core::str::from_utf8(&err).unwrap().contains("gio cat"));
    }
}
