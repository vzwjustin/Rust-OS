//! GMemoryMonitorPoll matching `gio/gmemorymonitorpoll.h`.
//! Poll-based memory monitor. In this no_std port we model it with
//! periodic polling of memory pressure.
//! Fully `no_std` compatible using `alloc`.

use crate::gmemorymonitor::MemoryPressureLevel;
use spin::Mutex;

/// A poll-based memory monitor (`GMemoryMonitorPoll`).
pub struct MemoryMonitorPoll {
    level: Mutex<MemoryPressureLevel>,
    poll_count: Mutex<u32>,
}

impl MemoryMonitorPoll {
    pub fn new() -> Self {
        Self {
            level: Mutex::new(MemoryPressureLevel::Low),
            poll_count: Mutex::new(0),
        }
    }

    pub fn get_level(&self) -> MemoryPressureLevel {
        *self.level.lock()
    }
    pub fn set_level(&self, level: MemoryPressureLevel) {
        *self.level.lock() = level;
    }

    pub fn poll(&self) -> MemoryPressureLevel {
        *self.poll_count.lock() += 1;
        *self.level.lock()
    }

    pub fn poll_count(&self) -> u32 {
        *self.poll_count.lock()
    }
}

impl Default for MemoryMonitorPoll {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_poll() {
        let m = MemoryMonitorPoll::new();
        m.set_level(MemoryPressureLevel::Low);
        assert_eq!(m.poll(), MemoryPressureLevel::Low);
        assert_eq!(m.poll_count(), 1);
    }
}
