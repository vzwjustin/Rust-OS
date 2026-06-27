//! GUnixFDList matching `gio/gunixfdlist.h` / `gio/gunixfdlist.c`.
//!
//! Holds a list of Unix file descriptors for passing over domain sockets
//! (`SCM_RIGHTS`). On bare-metal `no_std` targets fds are simulated as `i32`
//! values without syscalls.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::error::Error;
use crate::gioerror::{io_error_quark, IOErrorEnum};
use alloc::vec::Vec;
use spin::Mutex;

/// A list of Unix file descriptors (`GUnixFDList`).
pub struct UnixFDList {
    fds: Mutex<Vec<i32>>,
}

impl UnixFDList {
    /// Creates an empty fd list.
    ///
    /// Mirrors `g_unix_fd_list_new`.
    pub fn new() -> Self {
        Self {
            fds: Mutex::new(Vec::new()),
        }
    }

    /// Creates an fd list initialized with `fds`.
    pub fn new_from_array(fds: &[i32]) -> Self {
        Self {
            fds: Mutex::new(fds.to_vec()),
        }
    }

    /// Returns the number of file descriptors in the list.
    ///
    /// Mirrors `g_unix_fd_list_get_length`.
    pub fn get_length(&self) -> usize {
        self.fds.lock().len()
    }

    /// Returns the fd at `index`, or an error if out of range.
    ///
    /// Mirrors `g_unix_fd_list_get`.
    pub fn get(&self, index: usize) -> Result<i32, Error> {
        let fds = self.fds.lock();
        fds.get(index).copied().ok_or_else(|| {
            Error::new(
                io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                "FD index out of range",
            )
        })
    }

    /// Appends `fd` to the list and returns its index.
    ///
    /// Mirrors `g_unix_fd_list_append`.
    pub fn add(&self, fd: i32) -> Result<usize, Error> {
        if fd < 0 {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                "Invalid file descriptor",
            ));
        }
        let mut fds = self.fds.lock();
        fds.push(fd);
        Ok(fds.len() - 1)
    }

    /// Removes the fd at `index`.
    ///
    /// Mirrors `g_unix_fd_list_remove`.
    pub fn remove(&self, index: usize) -> Result<(), Error> {
        let mut fds = self.fds.lock();
        if index >= fds.len() {
            return Err(Error::new(
                io_error_quark(),
                IOErrorEnum::InvalidArgument.to_code(),
                "FD index out of range",
            ));
        }
        fds.remove(index);
        Ok(())
    }

    /// Steals all fds from the list, leaving it empty.
    ///
    /// Mirrors `g_unix_fd_list_steal_fds`.
    pub fn steal_fds(&self) -> Vec<i32> {
        core::mem::take(&mut *self.fds.lock())
    }

    /// Returns a copy of all fds without modifying the list.
    pub fn to_vec(&self) -> Vec<i32> {
        self.fds.lock().clone()
    }
}

impl Default for UnixFDList {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for UnixFDList {
    fn clone(&self) -> Self {
        Self {
            fds: Mutex::new(self.fds.lock().clone()),
        }
    }
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_empty() {
        let list = UnixFDList::new();
        assert_eq!(list.get_length(), 0);
    }

    #[test]
    fn test_add_and_get() {
        let list = UnixFDList::new();
        assert_eq!(list.add(3).unwrap(), 0);
        assert_eq!(list.add(7).unwrap(), 1);
        assert_eq!(list.get_length(), 2);
        assert_eq!(list.get(0).unwrap(), 3);
        assert_eq!(list.get(1).unwrap(), 7);
    }

    #[test]
    fn test_add_invalid_fd() {
        let list = UnixFDList::new();
        assert!(list.add(-1).is_err());
    }

    #[test]
    fn test_remove() {
        let list = UnixFDList::new_from_array(&[1, 2, 3]);
        list.remove(1).unwrap();
        assert_eq!(list.to_vec(), vec![1, 3]);
    }

    #[test]
    fn test_remove_out_of_range() {
        let list = UnixFDList::new();
        assert!(list.remove(0).is_err());
    }

    #[test]
    fn test_steal_fds() {
        let list = UnixFDList::new_from_array(&[10, 20]);
        let stolen = list.steal_fds();
        assert_eq!(stolen, vec![10, 20]);
        assert_eq!(list.get_length(), 0);
    }

    #[test]
    fn test_get_out_of_range() {
        let list = UnixFDList::new();
        assert!(list.get(0).is_err());
    }
}
