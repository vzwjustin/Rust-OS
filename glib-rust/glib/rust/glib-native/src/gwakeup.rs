//! Wakeup primitive compatibility (`gwakeup.c`).

use core::sync::atomic::{AtomicBool, Ordering};

/// A no_std wakeup flag.
#[derive(Debug, Default)]
pub struct Wakeup {
    signaled: AtomicBool,
}

impl Wakeup {
    /// Create a cleared wakeup flag.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            signaled: AtomicBool::new(false),
        }
    }

    /// Signal the wakeup.
    pub fn signal(&self) {
        self.signaled.store(true, Ordering::Release);
    }

    /// Clear and return whether the wakeup was signaled.
    #[must_use]
    pub fn acknowledge(&self) -> bool {
        self.signaled.swap(false, Ordering::AcqRel)
    }

    /// Return whether the wakeup is currently signaled.
    #[must_use]
    pub fn is_signaled(&self) -> bool {
        self.signaled.load(Ordering::Acquire)
    }
}

#[cfg(test)]
mod tests {
    use super::Wakeup;

    #[test]
    fn signals_and_acknowledges() {
        let wakeup = Wakeup::new();
        assert!(!wakeup.is_signaled());
        wakeup.signal();
        assert!(wakeup.is_signaled());
        assert!(wakeup.acknowledge());
        assert!(!wakeup.acknowledge());
    }
}
