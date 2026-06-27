//! Poll types matching `gpoll.h`.
//!
//! Defines `PollFD`, `IOCondition` flags, and a platform abstraction for
//! `g_poll`. Fully `no_std` compatible.

use crate::timer::monotonic_time_us;
use alloc::collections::BTreeSet;
use spin::RwLock;

/// I/O condition flags (`GIOCondition`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[repr(u16)]
pub enum IOCondition {
    In = 1,
    Out = 4,
    Pri = 2,
    Err = 8,
    Hup = 16,
    Nval = 32,
}

impl IOCondition {
    /// Convert to bitfield.
    pub fn bits(self) -> u16 {
        self as u16
    }

    /// Check if a bitfield contains this condition.
    pub fn contains(bits: u16, cond: IOCondition) -> bool {
        bits & cond.bits() != 0
    }
}

/// A poll file descriptor (`GPollFD`).
#[derive(Clone, Debug)]
pub struct PollFD {
    pub fd: i32,
    pub events: u16,
    pub revents: u16,
}

impl PollFD {
    /// Create a new `PollFD`.
    pub fn new(fd: i32, events: u16) -> Self {
        Self {
            fd,
            events,
            revents: 0,
        }
    }
}

/// Poll function type (`GPollFunc`).
pub type PollFunc = fn(&mut [PollFD], i32) -> i32;

/// Platform trait for polling file descriptors.
pub trait PollPlatform: Sync {
    /// Poll the given file descriptors, waiting up to `timeout_ms` milliseconds.
    ///
    /// Returns the number of descriptors with non-zero `revents`, or `0` on
    /// timeout / no readiness.
    fn poll(&self, fds: &mut [PollFD], timeout_ms: i32) -> i32;
}

/// A no-op poll platform: returns immediately without setting `revents`.
pub struct NoPollPlatform;

impl PollPlatform for NoPollPlatform {
    fn poll(&self, fds: &mut [PollFD], _timeout_ms: i32) -> i32 {
        for pfd in fds.iter_mut() {
            pfd.revents = 0;
        }
        0
    }
}

/// Timer-based poll platform for environments without OS `poll`/`epoll`.
///
/// Waits until the monotonic deadline derived from `timeout_ms` using
/// [`monotonic_time_us`], sleeping in small busy-wait slices. Does not mark
/// any file descriptors ready.
pub struct TimerPollPlatform;

impl PollPlatform for TimerPollPlatform {
    fn poll(&self, fds: &mut [PollFD], timeout_ms: i32) -> i32 {
        for pfd in fds.iter_mut() {
            pfd.revents = 0;
        }

        if timeout_ms == 0 {
            return 0;
        }

        if timeout_ms < 0 {
            return 0;
        }

        let deadline_us = monotonic_time_us() + (timeout_ms as i64) * 1000;
        const SLICE_US: i64 = 500;

        loop {
            let now = monotonic_time_us();
            if now >= deadline_us {
                break;
            }
            let slice_end = (now + SLICE_US).min(deadline_us);
            while monotonic_time_us() < slice_end {
                core::hint::spin_loop();
            }
        }
        0
    }
}

/// Test poll platform: marks descriptors ready when their `fd` is registered.
pub struct TestPollPlatform;

static TEST_READY_FDS: RwLock<BTreeSet<i32>> = RwLock::new(BTreeSet::new());

/// Register a file descriptor as ready for the next [`TestPollPlatform`] poll.
pub fn test_poll_register_fd(fd: i32) {
    TEST_READY_FDS.write().insert(fd);
}

/// Clear all registered ready file descriptors for [`TestPollPlatform`].
pub fn test_poll_clear_fds() {
    TEST_READY_FDS.write().clear();
}

impl PollPlatform for TestPollPlatform {
    fn poll(&self, fds: &mut [PollFD], timeout_ms: i32) -> i32 {
        let registered = TEST_READY_FDS.read();
        let mut ready = 0;
        for pfd in fds.iter_mut() {
            pfd.revents = 0;
            if registered.contains(&pfd.fd) {
                pfd.revents = pfd.events;
                ready += 1;
            }
        }
        if ready > 0 || timeout_ms == 0 {
            return ready;
        }
        TimerPollPlatform.poll(fds, timeout_ms)
    }
}

/// Host `poll(2)` platform for unit tests on Linux, macOS, and Android.
///
/// Only compiled for `cargo test` on Unix hosts with a real `poll` syscall.
#[cfg(all(
    test,
    any(target_os = "linux", target_os = "macos", target_os = "android")
))]
pub struct HostPollPlatform;

#[cfg(all(
    test,
    any(target_os = "linux", target_os = "macos", target_os = "android")
))]
mod host_poll {
    use super::{HostPollPlatform, PollFD, PollPlatform};
    use alloc::vec::Vec;

    const POLLIN: i16 = 0x0001;
    const POLLOUT: i16 = 0x0004;
    const POLLPRI: i16 = 0x0002;
    const POLLERR: i16 = 0x0008;
    const POLLHUP: i16 = 0x0010;
    const POLLNVAL: i16 = 0x0020;

    #[repr(C)]
    struct NativePollFD {
        fd: i32,
        events: i16,
        revents: i16,
    }

    extern "C" {
        fn poll(fds: *mut NativePollFD, nfds: u32, timeout: i32) -> i32;
    }

    /// Map `G_IO_*` / [`IOCondition`] bits to POSIX `poll` event flags.
    fn gio_events_to_poll(events: u16) -> i16 {
        let mut poll_events: i16 = 0;
        if events & 1 != 0 {
            poll_events |= POLLIN;
        }
        if events & 2 != 0 {
            poll_events |= POLLPRI;
        }
        if events & 4 != 0 {
            poll_events |= POLLOUT;
        }
        if events & 8 != 0 {
            poll_events |= POLLERR;
        }
        if events & 16 != 0 {
            poll_events |= POLLHUP;
        }
        if events & 32 != 0 {
            poll_events |= POLLNVAL;
        }
        poll_events
    }

    /// Map POSIX `poll` revents back to `G_IO_*` / [`IOCondition`] bits.
    fn poll_revents_to_gio(revents: i16) -> u16 {
        let mut gio_revents: u16 = 0;
        if revents & POLLIN != 0 {
            gio_revents |= 1;
        }
        if revents & POLLPRI != 0 {
            gio_revents |= 2;
        }
        if revents & POLLOUT != 0 {
            gio_revents |= 4;
        }
        if revents & POLLERR != 0 {
            gio_revents |= 8;
        }
        if revents & POLLHUP != 0 {
            gio_revents |= 16;
        }
        if revents & POLLNVAL != 0 {
            gio_revents |= 32;
        }
        gio_revents
    }

    impl PollPlatform for HostPollPlatform {
        fn poll(&self, fds: &mut [PollFD], timeout_ms: i32) -> i32 {
            for pfd in fds.iter_mut() {
                pfd.revents = 0;
            }

            let mut native: Vec<NativePollFD> = fds
                .iter()
                .map(|pfd| NativePollFD {
                    fd: pfd.fd,
                    events: gio_events_to_poll(pfd.events),
                    revents: 0,
                })
                .collect();

            // SAFETY: `native` is contiguous and lives for the duration of the call.
            let result = unsafe { poll(native.as_mut_ptr(), native.len() as u32, timeout_ms) };

            if result < 0 {
                return 0;
            }

            for (pfd, native_pfd) in fds.iter_mut().zip(native.iter()) {
                pfd.revents = poll_revents_to_gio(native_pfd.revents);
            }

            result
        }
    }
}

/// Install [`HostPollPlatform`] for host unit tests.
#[cfg(all(
    test,
    any(target_os = "linux", target_os = "macos", target_os = "android")
))]
pub fn register_host_poll_platform_for_tests() {
    register_poll_platform(&HostPollPlatform);
    test_poll_clear_fds();
}

static POLL_PLATFORM: RwLock<&'static dyn PollPlatform> = RwLock::new(&NoPollPlatform);

/// Installs the platform poll implementation.
pub fn register_poll_platform(platform: &'static dyn PollPlatform) {
    *POLL_PLATFORM.write() = platform;
}

/// Poll file descriptors (`g_poll`).
pub fn g_poll(fds: &mut [PollFD], timeout_ms: i32) -> i32 {
    POLL_PLATFORM.read().poll(fds, timeout_ms)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timer::set_clock;
    use core::sync::atomic::{AtomicI64, Ordering};

    fn reset_poll_platform() {
        register_poll_platform(&NoPollPlatform);
        test_poll_clear_fds();
    }

    #[test]
    fn io_condition_bits() {
        assert_eq!(IOCondition::In.bits(), 1);
        assert_eq!(IOCondition::Out.bits(), 4);
        assert_eq!(IOCondition::Err.bits(), 8);
    }

    #[test]
    fn io_condition_contains() {
        let bits = IOCondition::In.bits() | IOCondition::Hup.bits();
        assert!(IOCondition::contains(bits, IOCondition::In));
        assert!(IOCondition::contains(bits, IOCondition::Hup));
        assert!(!IOCondition::contains(bits, IOCondition::Out));
    }

    #[test]
    fn poll_fd_new() {
        let pfd = PollFD::new(3, IOCondition::In.bits() | IOCondition::Err.bits());
        assert_eq!(pfd.fd, 3);
        assert_eq!(pfd.events, 9);
        assert_eq!(pfd.revents, 0);
    }

    #[test]
    fn no_poll_platform_returns_zero() {
        reset_poll_platform();
        register_poll_platform(&NoPollPlatform);
        let mut fds = [PollFD::new(1, IOCondition::In.bits())];
        assert_eq!(g_poll(&mut fds, 100), 0);
        assert_eq!(fds[0].revents, 0);
    }

    #[test]
    fn timer_poll_platform_waits_for_timeout() {
        static NOW_US: AtomicI64 = AtomicI64::new(0);
        fn mock_clock() -> i64 {
            NOW_US.fetch_add(200, Ordering::Relaxed)
        }
        NOW_US.store(0, Ordering::Relaxed);
        set_clock(mock_clock);
        reset_poll_platform();
        register_poll_platform(&TimerPollPlatform);

        let mut fds: [PollFD; 0] = [];
        assert_eq!(g_poll(&mut fds, 10), 0);
        assert!(NOW_US.load(Ordering::Relaxed) >= 10_000);
    }

    #[test]
    fn timer_poll_platform_zero_timeout_returns_immediately() {
        static NOW_US: AtomicI64 = AtomicI64::new(0);
        fn mock_clock() -> i64 {
            NOW_US.load(Ordering::Relaxed)
        }
        NOW_US.store(0, Ordering::Relaxed);
        set_clock(mock_clock);
        reset_poll_platform();
        register_poll_platform(&TimerPollPlatform);

        let mut fds = [PollFD::new(5, IOCondition::In.bits())];
        assert_eq!(g_poll(&mut fds, 0), 0);
        assert_eq!(fds[0].revents, 0);
    }

    #[test]
    fn test_poll_platform_marks_registered_fd() {
        reset_poll_platform();
        register_poll_platform(&TestPollPlatform);
        test_poll_register_fd(42);

        let mut fds = [PollFD::new(42, IOCondition::In.bits())];
        assert_eq!(g_poll(&mut fds, 0), 1);
        assert_eq!(fds[0].revents, IOCondition::In.bits());
    }

    #[test]
    fn test_poll_platform_ignores_unregistered_fd() {
        reset_poll_platform();
        register_poll_platform(&TestPollPlatform);

        let mut fds = [PollFD::new(99, IOCondition::Out.bits())];
        assert_eq!(g_poll(&mut fds, 0), 0);
        assert_eq!(fds[0].revents, 0);
    }

    #[test]
    fn test_poll_platform_multiple_fds() {
        reset_poll_platform();
        register_poll_platform(&TestPollPlatform);
        test_poll_register_fd(1);
        test_poll_register_fd(3);

        let mut fds = [
            PollFD::new(1, IOCondition::In.bits()),
            PollFD::new(2, IOCondition::In.bits()),
            PollFD::new(3, IOCondition::Out.bits()),
        ];
        assert_eq!(g_poll(&mut fds, 0), 2);
        assert_eq!(fds[0].revents, IOCondition::In.bits());
        assert_eq!(fds[1].revents, 0);
        assert_eq!(fds[2].revents, IOCondition::Out.bits());
    }

    #[test]
    fn register_poll_platform_switches_implementation() {
        reset_poll_platform();
        register_poll_platform(&NoPollPlatform);
        let mut fds = [PollFD::new(7, IOCondition::In.bits())];
        assert_eq!(g_poll(&mut fds, 0), 0);

        register_poll_platform(&TestPollPlatform);
        test_poll_register_fd(7);
        assert_eq!(g_poll(&mut fds, 0), 1);
    }

    #[cfg(all(
        test,
        any(target_os = "linux", target_os = "macos", target_os = "android")
    ))]
    mod host_poll_tests {
        use super::*;
        use std::io::Write;
        use std::os::unix::io::AsRawFd;

        #[test]
        fn host_poll_platform_pipe_readable() {
            reset_poll_platform();
            register_host_poll_platform_for_tests();

            let (reader, mut writer) = std::io::pipe().unwrap();
            writer.write_all(b"x").unwrap();

            let mut fds = [PollFD::new(reader.as_raw_fd(), IOCondition::In.bits())];
            assert_eq!(g_poll(&mut fds, 100), 1);
            assert!(IOCondition::contains(fds[0].revents, IOCondition::In));
        }
    }
}
