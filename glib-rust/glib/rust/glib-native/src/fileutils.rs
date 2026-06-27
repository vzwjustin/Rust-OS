//! File utility functions matching `gfileutils.h` / `gfileutils.c`.
//!
//! Phase 6 covers path manipulation helpers (`g_build_filename`,
//! `g_path_is_absolute`, `g_path_get_basename`, `g_path_get_dirname`,
//! `g_canonicalize_filename`) and the `GFileError` / `GFileTest` types.
//!
//! Actual file I/O (`g_file_get_contents`, `g_file_set_contents`, `g_file_test`)
//! requires OS-level syscalls and is deferred to a platform abstraction layer.

use crate::prelude::*;
use crate::quark::{quark_from_static_string, Quark};

/// File error codes matching `GFileError` (`gfileutils.h`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum FileError {
    /// Operation would have resulted in an existing file (`EEXIST`).
    Exist = 0,
    /// Operation was on a directory (`EISDIR`).
    IsDir,
    /// Permission denied (`EACCES`).
    Acces,
    /// Filename too long (`ENAMETOOLONG`).
    NameTooLong,
    /// No such file or directory (`ENOENT`).
    NoEnt,
    /// Not a directory (`ENOTDIR`).
    NotDir,
    /// No such device or address (`ENXIO`).
    Nxio,
    /// No such device (`ENODEV`).
    NoDev,
    /// Read-only filesystem (`EROFS`).
    RoFs,
    /// Text file busy (`ETXTBSY`).
    TxtBsy,
    /// Bad address (`EFAULT`).
    Fault,
    /// Too many symbolic links encountered (`ELOOP`).
    Loop,
    /// No space left on device (`ENOSPC`).
    NoSpc,
    /// Out of memory / not enough space (`ENOMEM`).
    NoMem,
    /// Too many open files (`EMFILE`).
    MFile,
    /// Too many open files in system (`ENFILE`).
    NFile,
    /// Bad file descriptor (`EBADF`).
    BadF,
    /// Invalid argument (`EINVAL`).
    Inval,
    /// Broken pipe (`EPIPE`).
    Pipe,
    /// Resource temporarily unavailable (`EAGAIN`).
    Again,
    /// Interrupted system call (`EINTR`).
    Intr,
    /// I/O error (`EIO`).
    Io,
    /// Operation not permitted (`EPERM`).
    Perm,
    /// Function not implemented (`ENOSYS`).
    NoSys,
    /// Generic failure.
    Failed,
}

/// Returns the quark for `G_FILE_ERROR`.
pub fn file_error_quark() -> Quark {
    quark_from_static_string(Some("g-file-error-quark"))
}

/// Convert an `errno` value into a `FileError` (`g_file_error_from_errno`).
///
/// Mirrors the upstream `switch` over `E*` macros. Errno constants
/// are taken from `core::ffi` / Linux headers; on bare metal the
/// well-known values still match (Linux/glibc numbering). Unknown
/// errnos return `FileError::Failed`, matching the upstream default.
pub fn file_error_from_errno(err_no: i32) -> FileError {
    // Errno values from <errno.h> (Linux/glibc). Bare-metal RustOS
    // uses the same numbering for the well-known codes.
    const EEXIST: i32 = 17;
    const EISDIR: i32 = 21;
    const EACCES: i32 = 13;
    const ENAMETOOLONG: i32 = 36;
    const ENOENT: i32 = 2;
    const ENOTDIR: i32 = 20;
    const ENXIO: i32 = 6;
    const ENODEV: i32 = 19;
    const EROFS: i32 = 30;
    const ETXTBSY: i32 = 26;
    const EFAULT: i32 = 14;
    const ELOOP: i32 = 40;
    const ENOSPC: i32 = 28;
    const ENOMEM: i32 = 12;
    const EMFILE: i32 = 24;
    const ENFILE: i32 = 23;
    const EBADF: i32 = 9;
    const EINVAL: i32 = 22;
    const EPIPE: i32 = 32;
    const EAGAIN: i32 = 11;
    const EINTR: i32 = 4;
    const EIO: i32 = 5;
    const EPERM: i32 = 1;
    const ENOSYS: i32 = 38;
    match err_no {
        EEXIST => FileError::Exist,
        EISDIR => FileError::IsDir,
        EACCES => FileError::Acces,
        ENAMETOOLONG => FileError::NameTooLong,
        ENOENT => FileError::NoEnt,
        ENOTDIR => FileError::NotDir,
        ENXIO => FileError::Nxio,
        ENODEV => FileError::NoDev,
        EROFS => FileError::RoFs,
        ETXTBSY => FileError::TxtBsy,
        EFAULT => FileError::Fault,
        ELOOP => FileError::Loop,
        ENOSPC => FileError::NoSpc,
        ENOMEM => FileError::NoMem,
        EMFILE => FileError::MFile,
        ENFILE => FileError::NFile,
        EBADF => FileError::BadF,
        EINVAL => FileError::Inval,
        EPIPE => FileError::Pipe,
        EAGAIN => FileError::Again,
        EINTR => FileError::Intr,
        EIO => FileError::Io,
        EPERM => FileError::Perm,
        ENOSYS => FileError::NoSys,
        _ => FileError::Failed,
    }
}

/// Flags for `g_file_test` (`GFileTest`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum FileTest {
    /// `G_FILE_TEST_IS_REGULAR`
    IsRegular = 1 << 0,
    /// `G_FILE_TEST_IS_SYMLINK`
    IsSymlink = 1 << 1,
    /// `G_FILE_TEST_IS_DIR`
    IsDir = 1 << 2,
    /// `G_FILE_TEST_IS_EXECUTABLE`
    IsExecutable = 1 << 3,
    /// `G_FILE_TEST_EXISTS`
    Exists = 1 << 4,
}

/// Returns `true` if `file_name` is an absolute path.
///
/// On Unix, an absolute path starts with `/`. On Windows, it may also start
/// with a drive letter followed by `:\` or `:/`.
pub fn path_is_absolute(file_name: &str) -> bool {
    if file_name.is_empty() {
        return false;
    }
    file_name.starts_with('/')
}

/// Returns the portion of `file_name` after the root component.
///
/// Returns `None` if `file_name` is not absolute or has no root component.
pub fn path_skip_root(file_name: &str) -> Option<&str> {
    if file_name.is_empty() {
        return None;
    }
    if file_name.starts_with('/') {
        let mut i = 1;
        while i < file_name.len() && file_name.as_bytes()[i] == b'/' {
            i += 1;
        }
        return Some(&file_name[i..]);
    }
    None
}

/// Returns the last component of `file_name` (`g_path_get_basename`).
///
/// Returns an empty string if `file_name` is empty.
/// Returns `/` if `file_name` is all slashes.
pub fn path_get_basename(file_name: &str) -> String {
    if file_name.is_empty() {
        return String::new();
    }

    let bytes = file_name.as_bytes();
    let mut end = bytes.len();

    // Strip trailing slashes
    while end > 0 && bytes[end - 1] == b'/' {
        end -= 1;
    }

    if end == 0 {
        // String consisted only of slashes → the root directory.
        return "/".to_owned();
    }

    // Find the last slash before end
    let start = bytes[..end].iter().rposition(|&b| b == b'/');
    match start {
        Some(i) => file_name[i + 1..end].to_owned(),
        None => file_name[..end].to_owned(),
    }
}

/// Returns the directory components of `file_name` (`g_path_get_dirname`).
///
/// If `file_name` has no directory components, returns `.`.
/// If `file_name` is all slashes, returns `/`.
pub fn path_get_dirname(file_name: &str) -> String {
    if file_name.is_empty() {
        return ".".to_owned();
    }

    let bytes = file_name.as_bytes();
    let mut end = bytes.len();

    // Strip trailing slashes (but keep at least one if the whole path is slashes)
    while end > 1 && bytes[end - 1] == b'/' {
        end -= 1;
    }

    if end == 0 {
        return ".".to_owned();
    }

    // Find the last slash
    let last_slash = bytes[..end].iter().rposition(|&b| b == b'/');

    match last_slash {
        None => ".".to_owned(),
        Some(0) => "/".to_owned(),
        Some(i) => {
            // Strip trailing slashes from the dirname portion
            let mut dir_end = i;
            while dir_end > 1 && bytes[dir_end - 1] == b'/' {
                dir_end -= 1;
            }
            file_name[..dir_end].to_owned()
        }
    }
}

/// Checks if `c` is a directory separator.
pub fn is_dir_separator(c: char) -> bool {
    c == '/'
}

/// Builds a filename from components, inserting `/` separators (`g_build_filename`).
///
/// Trailing slashes on components (except the last) are stripped, and leading
/// slashes on components (except the first) are stripped.
pub fn build_filename(parts: &[&str]) -> String {
    if parts.is_empty() {
        return String::new();
    }

    let mut result = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            result.push_str(part);
        } else {
            // Ensure exactly one separator between parts
            if !result.is_empty() && !result.ends_with('/') {
                result.push('/');
            }
            // Strip leading slashes from subsequent parts
            let trimmed = part.trim_start_matches('/');
            result.push_str(trimmed);
        }
        // Strip trailing slashes (except for the last part)
        if i < parts.len() - 1 {
            while result.ends_with('/') {
                result.pop();
            }
        }
    }
    result
}

/// Builds a path from components using `separator` (`g_build_pathv`).
///
/// Similar to [`build_filename`] but with a custom separator.
pub fn build_pathv(separator: &str, parts: &[&str]) -> String {
    if parts.is_empty() {
        return String::new();
    }

    let mut result = String::new();
    for (i, part) in parts.iter().enumerate() {
        if i == 0 {
            result.push_str(part);
        } else {
            // Ensure exactly one separator between parts
            if !result.is_empty() && !result.ends_with(separator) {
                result.push_str(separator);
            }
            // Strip leading separators from subsequent parts
            let trimmed = part.trim_start_matches(separator);
            result.push_str(trimmed);
        }
        // Strip trailing separators (except for the last part)
        if i < parts.len() - 1 {
            while result.ends_with(separator) {
                for _ in 0..separator.len() {
                    result.pop();
                }
            }
        }
    }
    result
}

/// Canonicalizes `filename` relative to `relative_to` (`g_canonicalize_filename`).
///
/// If `filename` is absolute, it is normalized (`. ` and `..` components resolved).
/// If `filename` is relative, it is joined with `relative_to` (or the current
/// directory if `relative_to` is `None`).
pub fn canonicalize_filename(filename: &str, relative_to: Option<&str>) -> String {
    let base = if path_is_absolute(filename) {
        filename.to_owned()
    } else {
        let base = relative_to.unwrap_or(".");
        build_filename(&[base, filename])
    };

    normalize_path(&base)
}

/// Resolves `.` and `..` components in a path.
fn normalize_path(path: &str) -> String {
    let mut components: Vec<&str> = Vec::new();
    let is_absolute = path.starts_with('/');

    for component in path.split('/') {
        match component {
            "" | "." => {}
            ".." => {
                if let Some(last) = components.last() {
                    if *last != ".." {
                        components.pop();
                        continue;
                    }
                }
                if !is_absolute {
                    components.push("..");
                }
            }
            other => components.push(other),
        }
    }

    let mut result = String::new();
    if is_absolute {
        result.push('/');
    }
    for (i, component) in components.iter().enumerate() {
        if i > 0 {
            result.push('/');
        }
        result.push_str(component);
    }

    if result.is_empty() {
        if is_absolute {
            return "/".to_owned();
        }
        return ".".to_owned();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_is_absolute_unix() {
        assert!(path_is_absolute("/usr/bin"));
        assert!(path_is_absolute("/"));
        assert!(!path_is_absolute("usr/bin"));
        assert!(!path_is_absolute(""));
    }

    #[test]
    fn basename_simple() {
        assert_eq!(path_get_basename("/usr/bin/test"), "test");
        assert_eq!(path_get_basename("test"), "test");
        assert_eq!(path_get_basename("/usr/bin/"), "bin");
        assert_eq!(path_get_basename("/usr/bin///"), "bin");
        assert_eq!(path_get_basename("/"), "/");
        assert_eq!(path_get_basename(""), "");
    }

    #[test]
    fn dirname_simple() {
        assert_eq!(path_get_dirname("/usr/bin/test"), "/usr/bin");
        assert_eq!(path_get_dirname("test"), ".");
        assert_eq!(path_get_dirname("/usr/bin/"), "/usr");
        assert_eq!(path_get_dirname("/"), "/");
        assert_eq!(path_get_dirname(""), ".");
    }

    #[test]
    fn build_filename_simple() {
        assert_eq!(build_filename(&["usr", "bin", "test"]), "usr/bin/test");
        assert_eq!(build_filename(&["/usr", "bin", "test"]), "/usr/bin/test");
        assert_eq!(build_filename(&["usr/", "/bin/", "test"]), "usr/bin/test");
        assert_eq!(build_filename(&["usr"]), "usr");
        assert_eq!(build_filename(&[]), "");
    }

    #[test]
    fn canonicalize_absolute() {
        assert_eq!(canonicalize_filename("/usr/../bin", None), "/bin");
        assert_eq!(canonicalize_filename("/usr/./bin", None), "/usr/bin");
        assert_eq!(canonicalize_filename("/usr/bin", None), "/usr/bin");
        assert_eq!(canonicalize_filename("/../bin", None), "/bin");
    }

    #[test]
    fn canonicalize_relative() {
        assert_eq!(canonicalize_filename("bin", Some("/usr")), "/usr/bin");
        assert_eq!(canonicalize_filename("../bin", Some("/usr/lib")), "/usr/bin");
        assert_eq!(canonicalize_filename("./test", Some("/usr")), "/usr/test");
    }

    #[test]
    fn file_error_quark_is_nonzero() {
        assert_ne!(file_error_quark(), 0);
    }
}
