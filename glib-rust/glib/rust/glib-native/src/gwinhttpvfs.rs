//! `gwinhttpvfs` matching `gio/win32/gwinhttpvfs.h`.
//!
//! WinHTTP VFS: HTTP/HTTPS virtual filesystem backed by WinHTTP.
//! Stubbed in no_std since WinHTTP DLL is not available.
//!
//! Fully `no_std` compatible using `alloc`.

use crate::prelude::*;
use crate::winhttp::{self, HInternet, UrlComponents};
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// WinHTTP DLL function table (mirrors `GWinHttpDllFuncs`).
/// In our port, these are all stubs.
#[derive(Debug, Default)]
pub struct WinHttpDllFuncs;

/// WinHTTP VFS (mirrors `GWinHttpVfs`).
pub struct WinHttpVfs {
    session: Mutex<HInternet>,
    funcs: WinHttpDllFuncs,
}

impl WinHttpVfs {
    /// Creates a new WinHTTP VFS (mirrors `_g_winhttp_vfs_new`).
    pub fn new() -> Self {
        Self {
            session: Mutex::new(0),
            funcs: WinHttpDllFuncs,
        }
    }

    /// Returns the session handle.
    pub fn session(&self) -> HInternet {
        *self.session.lock()
    }

    /// Sets an error from a WinHTTP error code
    /// (mirrors `_g_winhttp_set_error`).
    pub fn set_error(error_code: u32, what: &str) -> String {
        format!("{}: {}", what, winhttp::error_message(error_code))
    }

    /// Checks the WinHTTP response (mirrors `_g_winhttp_response`).
    /// In our port, always returns `Ok(())`.
    pub fn response(&self, _request: HInternet) -> Result<(), String> {
        Ok(())
    }

    /// Queries a header from the response (mirrors `_g_winhttp_query_header`).
    /// In our port, always returns `None`.
    pub fn query_header(
        &self,
        _request: HInternet,
        _request_description: &str,
        _which_header: u32,
    ) -> Option<String> {
        None
    }
}

impl Default for WinHttpVfs {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let vfs = WinHttpVfs::new();
        assert_eq!(vfs.session(), 0);
    }

    #[test]
    fn test_set_error() {
        let msg = WinHttpVfs::set_error(winhttp::ERROR_WINHTTP_TIMEOUT, "Download");
        assert!(msg.contains("Download"));
        assert!(msg.contains("timed out"));
    }

    #[test]
    fn test_response_ok() {
        let vfs = WinHttpVfs::new();
        assert!(vfs.response(0).is_ok());
    }

    #[test]
    fn test_query_header_none() {
        let vfs = WinHttpVfs::new();
        assert!(vfs.query_header(0, "test", 1).is_none());
    }
}
