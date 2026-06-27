//! GPowerProfileMonitor matching `gio/gpowerprofilemonitor.h`.
//!
//! Monitors the system power profile (performance / balanced / power-saver).
//! In this no_std port the profile is settable directly for testing.

use spin::Mutex;

/// Power saving mode (`GPowerProfileMonitorWarningLevel`-equivalent).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerProfile {
    /// Normal (performance) mode — no power saving.
    Performance,
    /// Balanced mode.
    Balanced,
    /// Power-saver mode active.
    PowerSaver,
}

/// Power profile monitor (`GPowerProfileMonitor`).
pub struct PowerProfileMonitor {
    profile: Mutex<PowerProfile>,
}

impl PowerProfileMonitor {
    /// Creates a monitor defaulting to `Performance`.
    pub fn new() -> Self {
        Self {
            profile: Mutex::new(PowerProfile::Performance),
        }
    }

    /// Returns `true` if power-saver mode is active.
    ///
    /// Mirrors `g_power_profile_monitor_get_power_saver_enabled`.
    pub fn get_power_saver_enabled(&self) -> bool {
        *self.profile.lock() == PowerProfile::PowerSaver
    }

    /// Returns the current power profile.
    pub fn get_profile(&self) -> PowerProfile {
        *self.profile.lock()
    }

    /// Sets the current power profile (used by platform layer or tests).
    pub fn set_profile(&self, p: PowerProfile) {
        *self.profile.lock() = p;
    }
}

impl Default for PowerProfileMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_performance() {
        let m = PowerProfileMonitor::new();
        assert_eq!(m.get_profile(), PowerProfile::Performance);
        assert!(!m.get_power_saver_enabled());
    }

    #[test]
    fn test_power_saver() {
        let m = PowerProfileMonitor::new();
        m.set_profile(PowerProfile::PowerSaver);
        assert!(m.get_power_saver_enabled());
    }

    #[test]
    fn test_balanced() {
        let m = PowerProfileMonitor::new();
        m.set_profile(PowerProfile::Balanced);
        assert!(!m.get_power_saver_enabled());
        assert_eq!(m.get_profile(), PowerProfile::Balanced);
    }

    #[test]
    fn test_default_trait() {
        let m = PowerProfileMonitor::default();
        assert_eq!(m.get_profile(), PowerProfile::Performance);
    }

    #[test]
    fn test_switch_back() {
        let m = PowerProfileMonitor::new();
        m.set_profile(PowerProfile::PowerSaver);
        m.set_profile(PowerProfile::Performance);
        assert!(!m.get_power_saver_enabled());
    }
}
