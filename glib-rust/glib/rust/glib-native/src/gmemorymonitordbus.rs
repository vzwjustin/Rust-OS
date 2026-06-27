//! GMemoryMonitorDBus matching `gio/gmemorymonitordbus.h`.
//! D-Bus-based memory monitor. In this no_std port we model it
//! extending the base memory monitor with D-Bus connection state.
//! Fully `no_std` compatible using `alloc`.

use crate::gmemorymonitor::MemoryPressureLevel;
use spin::Mutex;

/// A D-Bus memory monitor (`GMemoryMonitorDBus`).
pub struct MemoryMonitorDBus {
    level: Mutex<MemoryPressureLevel>,
    connected: Mutex<bool>,
}

impl MemoryMonitorDBus {
    pub fn new() -> Self {
        Self {
            level: Mutex::new(MemoryPressureLevel::Low),
            connected: Mutex::new(false),
        }
    }

    pub fn get_level(&self) -> MemoryPressureLevel {
        *self.level.lock()
    }
    pub fn set_level(&self, level: MemoryPressureLevel) {
        *self.level.lock() = level;
    }
    pub fn is_connected(&self) -> bool {
        *self.connected.lock()
    }
    pub fn connect(&self) {
        *self.connected.lock() = true;
    }
    pub fn disconnect(&self) {
        *self.connected.lock() = false;
    }
}

impl Default for MemoryMonitorDBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let m = MemoryMonitorDBus::new();
        assert_eq!(m.get_level(), MemoryPressureLevel::Low);
        assert!(!m.is_connected());
    }

    #[test]
    fn test_connect_set_level() {
        let m = MemoryMonitorDBus::new();
        m.connect();
        m.set_level(MemoryPressureLevel::Critical);
        assert!(m.is_connected());
        assert_eq!(m.get_level(), MemoryPressureLevel::Critical);
    }
}
