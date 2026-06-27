//! Poll types matching `gpoll.h`.
//!
//! Defines `PollFD` and `IOCondition` flags. The actual `g_poll` function
//! requires OS syscall support and is deferred.
//! Fully `no_std` compatible.

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
