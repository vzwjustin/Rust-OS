//! GPowerProfileMonitorDBus matching `gio/gpowerprofilemonitordbus.h`.
//! D-Bus-based power profile monitor. In this no_std port we model it
//! with power saver state and D-Bus connection.
//! Fully `no_std` compatible using `alloc`.

use spin::Mutex;

/// Power saver mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerProfile {
    Performance,
    Balanced,
    PowerSaver,
}

/// A D-Bus power profile monitor (`GPowerProfileMonitorDBus`).
pub struct PowerProfileMonitorDBus {
    power_saver_enabled: Mutex<bool>,
    profile: Mutex<PowerProfile>,
    connected: Mutex<bool>,
}

impl PowerProfileMonitorDBus {
    pub fn new() -> Self {
        Self {
            power_saver_enabled: Mutex::new(false),
            profile: Mutex::new(PowerProfile::Balanced),
            connected: Mutex::new(false),
        }
    }

    pub fn is_power_saver_enabled(&self) -> bool {
        *self.power_saver_enabled.lock()
    }
    pub fn set_power_saver_enabled(&self, enabled: bool) {
        *self.power_saver_enabled.lock() = enabled;
        *self.profile.lock() = if enabled {
            PowerProfile::PowerSaver
        } else {
            PowerProfile::Balanced
        };
    }
    pub fn get_profile(&self) -> PowerProfile {
        *self.profile.lock()
    }
    pub fn set_profile(&self, profile: PowerProfile) {
        *self.profile.lock() = profile;
        *self.power_saver_enabled.lock() = profile == PowerProfile::PowerSaver;
    }
    pub fn is_connected(&self) -> bool {
        *self.connected.lock()
    }
    pub fn connect(&self) {
        *self.connected.lock() = true;
    }
}

impl Default for PowerProfileMonitorDBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let m = PowerProfileMonitorDBus::new();
        assert!(!m.is_power_saver_enabled());
        assert_eq!(m.get_profile(), PowerProfile::Balanced);
    }

    #[test]
    fn test_set_power_saver() {
        let m = PowerProfileMonitorDBus::new();
        m.set_power_saver_enabled(true);
        assert!(m.is_power_saver_enabled());
        assert_eq!(m.get_profile(), PowerProfile::PowerSaver);
    }
}
