//! `gioprivate` matching `gio/gioprivate.h`.
//!
//! Private I/O utilities: async-read/write thread detection, cached remote
//! address, and `G_IOV_MAX` constant.
//!
//! Fully `no_std` compatible.

/// Maximum number of iovecs that can be sent in one call.
///
/// Mirrors `G_IOV_MAX`. POSIX minimum is 16; macOS uses 512; Linux uses 1024.
pub const G_IOV_MAX: usize = 1024;

/// Checks if async read on an input stream is done via threads.
///
/// Mirrors `g_input_stream_async_read_is_via_threads`.
/// In our no_std port, there are no threads, so always false.
pub fn input_stream_async_read_is_via_threads() -> bool {
    false
}

/// Checks if async close on an input stream is done via threads.
///
/// Mirrors `g_input_stream_async_close_is_via_threads`.
pub fn input_stream_async_close_is_via_threads() -> bool {
    false
}

/// Checks if async write on an output stream is done via threads.
///
/// Mirrors `g_output_stream_async_write_is_via_threads`.
pub fn output_stream_async_write_is_via_threads() -> bool {
    false
}

/// Checks if async writev on an output stream is done via threads.
///
/// Mirrors `g_output_stream_async_writev_is_via_threads`.
pub fn output_stream_async_writev_is_via_threads() -> bool {
    false
}

/// Checks if async close on an output stream is done via threads.
///
/// Mirrors `g_output_stream_async_close_is_via_threads`.
pub fn output_stream_async_close_is_via_threads() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_iov_max() {
        assert!(G_IOV_MAX >= 16);
    }

    #[test]
    fn test_no_threads() {
        assert!(!input_stream_async_read_is_via_threads());
        assert!(!input_stream_async_close_is_via_threads());
        assert!(!output_stream_async_write_is_via_threads());
        assert!(!output_stream_async_writev_is_via_threads());
        assert!(!output_stream_async_close_is_via_threads());
    }
}
