//! GPowerProfileMonitorPortal matching `gio/gpowerprofilemonitorportal.h`.
//! Portal-based power profile monitor. In this no_std port we model it
//! with power saver state and portal availability.
//! Fully `no_std` compatible using `alloc`.

use spin::Mutex;

/// A portal-based power profile monitor (`GPowerProfileMonitorPortal`).
pub struct PowerProfileMonitorPortal {
    power_saver_enabled: Mutex<bool>,
    available: Mutex<bool>,
}

impl PowerProfileMonitorPortal {
    pub fn new() -> Self {
        Self {
            power_saver_enabled: Mutex::new(false),
            available: Mutex::new(false),
        }
    }

    pub fn is_power_saver_enabled(&self) -> bool {
        *self.power_saver_enabled.lock()
    }
    pub fn set_power_saver_enabled(&self, enabled: bool) {
        *self.power_saver_enabled.lock() = enabled;
    }
    pub fn is_available(&self) -> bool {
        *self.available.lock()
    }
    pub fn set_available(&self, available: bool) {
        *self.available.lock() = available;
    }
}

impl Default for PowerProfileMonitorPortal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_portal() {
        let m = PowerProfileMonitorPortal::new();
        m.set_available(true);
        m.set_power_saver_enabled(true);
        assert!(m.is_available());
        assert!(m.is_power_saver_enabled());
    }
}
