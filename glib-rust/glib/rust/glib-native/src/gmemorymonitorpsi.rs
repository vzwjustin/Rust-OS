//! GMemoryMonitorPsi matching `gio/gmemorymonitorpsi.h`.
//! PSI (Pressure Stall Information) based memory monitor. In this
//! no_std port we model it with PSI line tracking.
//! Fully `no_std` compatible using `alloc`.

use crate::gmemorymonitor::MemoryPressureLevel;
use spin::Mutex;

/// A PSI-based memory monitor (`GMemoryMonitorPsi`).
pub struct MemoryMonitorPsi {
    level: Mutex<MemoryPressureLevel>,
    some_psi: Mutex<f64>,
    full_psi: Mutex<f64>,
}

impl MemoryMonitorPsi {
    pub fn new() -> Self {
        Self {
            level: Mutex::new(MemoryPressureLevel::Low),
            some_psi: Mutex::new(0.0),
            full_psi: Mutex::new(0.0),
        }
    }

    pub fn get_level(&self) -> MemoryPressureLevel {
        *self.level.lock()
    }
    pub fn set_level(&self, level: MemoryPressureLevel) {
        *self.level.lock() = level;
    }

    pub fn get_some_psi(&self) -> f64 {
        *self.some_psi.lock()
    }
    pub fn set_some_psi(&self, psi: f64) {
        *self.some_psi.lock() = psi;
    }

    pub fn get_full_psi(&self) -> f64 {
        *self.full_psi.lock()
    }
    pub fn set_full_psi(&self, psi: f64) {
        *self.full_psi.lock() = psi;
    }

    pub fn update_from_psi(&self, some: f64, full: f64) {
        self.set_some_psi(some);
        self.set_full_psi(full);
        if full > 50.0 {
            self.set_level(MemoryPressureLevel::Critical);
        } else if some > 10.0 {
            self.set_level(MemoryPressureLevel::Low);
        } else {
            self.set_level(MemoryPressureLevel::Normal);
        }
    }
}

impl Default for MemoryMonitorPsi {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_psi_update() {
        let m = MemoryMonitorPsi::new();
        m.update_from_psi(5.0, 0.0);
        assert_eq!(m.get_level(), MemoryPressureLevel::Normal);
        m.update_from_psi(60.0, 0.0);
        assert_eq!(m.get_level(), MemoryPressureLevel::Low);
        m.update_from_psi(60.0, 55.0);
        assert_eq!(m.get_level(), MemoryPressureLevel::Critical);
    }
}
