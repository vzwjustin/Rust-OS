//! GMemoryMonitorPortal matching `gio/gmemorymonitorportal.h`.
//! Portal-based memory monitor. In this no_std port we model it with
//! portal availability and pressure level.
//! Fully `no_std` compatible using `alloc`.

use crate::gmemorymonitor::MemoryPressureLevel;
use spin::Mutex;

/// A portal-based memory monitor (`GMemoryMonitorPortal`).
pub struct MemoryMonitorPortal {
    level: Mutex<MemoryPressureLevel>,
    available: Mutex<bool>,
}

impl MemoryMonitorPortal {
    pub fn new() -> Self {
        Self {
            level: Mutex::new(MemoryPressureLevel::Low),
            available: Mutex::new(false),
        }
    }

    pub fn get_level(&self) -> MemoryPressureLevel {
        *self.level.lock()
    }
    pub fn set_level(&self, level: MemoryPressureLevel) {
        *self.level.lock() = level;
    }
    pub fn is_available(&self) -> bool {
        *self.available.lock()
    }
    pub fn set_available(&self, available: bool) {
        *self.available.lock() = available;
    }
}

impl Default for MemoryMonitorPortal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_portal() {
        let m = MemoryMonitorPortal::new();
        m.set_available(true);
        m.set_level(MemoryPressureLevel::Critical);
        assert!(m.is_available());
        assert_eq!(m.get_level(), MemoryPressureLevel::Critical);
    }
}
