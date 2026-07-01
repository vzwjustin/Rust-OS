//! fd_source - GSource wrapper that polls a single file descriptor.
//!
//! Ported from GNOME Mutter's src/backends/meta-fd-source.c. The GLib GSource
//! main-loop integration (g_source_new, g_source_add_poll, prepare/check/dispatch
//! callbacks) is not available in the kernel, so the main-loop wiring is stubbed;
//! the polled-fd state and callback pointers are preserved.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-fd-source.c

use alloc::string::String;

/// I/O event flag equivalent to G_IO_IN (data available for reading).
pub const IO_IN: u32 = 0x01;

/// Callback invoked from the source. Returns whether the source should stay
/// active (equivalent to a GSourceFunc returning gboolean).
pub type FdSourceFunc = fn(user_data: usize) -> bool;

/// A source that watches a single file descriptor for readability.
///
/// Corresponds to the C `MetaFdSource` struct plus the polled GPollFD.
#[derive(Clone)]
pub struct FdSource {
    /// Name of the source (g_source_set_name).
    pub name: String,
    /// Watched file descriptor.
    pub fd: i32,
    /// Events we care about (G_IO_IN).
    pub events: u32,
    /// Events returned by the last poll.
    pub revents: u32,
    /// Prepare callback (fd_source->prepare).
    pub prepare: Option<FdSourceFunc>,
    /// Dispatch callback (fd_source->dispatch).
    pub dispatch: Option<FdSourceFunc>,
    /// Opaque user data passed to the callbacks.
    pub user_data: usize,
}

impl FdSource {
    /// Create a new fd source. Mirrors `meta_create_fd_source`.
    pub fn new(
        fd: i32,
        name: String,
        prepare: Option<FdSourceFunc>,
        dispatch: Option<FdSourceFunc>,
        user_data: usize,
    ) -> Self {
        FdSource {
            name,
            fd,
            events: IO_IN,
            revents: 0,
            prepare,
            dispatch,
            user_data,
        }
    }

    /// Mirrors `meta_fd_source_prepare`: sets timeout to infinite (-1) and
    /// runs the prepare callback. Returns (timeout_ms, ready).
    pub fn prepare(&self) -> (i32, bool) {
        let ready = match self.prepare {
            Some(f) => f(self.user_data),
            None => false,
        };
        (-1, ready)
    }

    /// Mirrors `meta_fd_source_check`: source is ready if G_IO_IN is set.
    pub fn check(&self) -> bool {
        (self.revents & IO_IN) != 0
    }

    /// Mirrors `meta_fd_source_dispatch`: run the dispatch callback.
    pub fn dispatch(&self) -> bool {
        match self.dispatch {
            Some(f) => f(self.user_data),
            None => false,
        }
    }

    /// Record the events returned from a poll of the fd.
    pub fn set_revents(&mut self, revents: u32) {
        self.revents = revents;
    }

    /// Mirrors `meta_fd_source_finalize`: closes the watched fd.
    /// Stub: no real fd table in the kernel port, so this just clears state.
    pub fn finalize(&mut self) {
        // Would call close(self.fd) via the host runtime.
        self.fd = -1;
    }
}
