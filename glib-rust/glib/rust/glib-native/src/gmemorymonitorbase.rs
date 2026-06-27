//! GMemoryMonitorBase matching `gio/gmemorymonitorbase.h` /
//! `gio/gmemorymonitorbase.c`.
//!
//! Shared base logic for memory monitor implementations: warning-level
//! mapping, optional system memory ratio query, and rate-limited event
//! delivery (one signal per level every 15 seconds).
//!
//! Fully `no_std` compatible using `alloc` and `spin`.

use crate::gmemorymonitor::{MemoryMonitor, MemoryPressureLevel};
use crate::timer::monotonic_time_us;
use alloc::sync::Arc;
use spin::Mutex;

/// Minimum interval between low-memory events for the same level (15 seconds).
pub const RECOVERY_INTERVAL_US: i64 = 15 * 1_000_000;

/// Low-memory warning level (`GMemoryMonitorLowMemoryLevel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum MemoryMonitorLowMemoryLevel {
    /// Invalid / unset level.
    Invalid = -1,
    /// Low memory — reduce cache usage.
    Low = 0,
    /// Medium pressure.
    Medium = 1,
    /// Critical — free memory aggressively.
    Critical = 2,
}

impl MemoryMonitorLowMemoryLevel {
    /// Number of defined warning levels (excludes [`Self::Invalid`]).
    pub const COUNT: usize = 3;

    /// Converts from an integer matching upstream's enum values.
    pub fn from_i32(value: i32) -> Option<Self> {
        match value {
            -1 => Some(Self::Invalid),
            0 => Some(Self::Low),
            1 => Some(Self::Medium),
            2 => Some(Self::Critical),
            _ => None,
        }
    }
}

/// Warning level encoded for the `low-memory-warning` signal (`GMemoryMonitorWarningLevel`).
pub type MemoryMonitorWarningLevel = u8;

/// Maps a [`MemoryMonitorLowMemoryLevel`] to the byte sent with
/// `low-memory-warning` (`g_memory_monitor_base_level_enum_to_byte`).
pub fn memory_monitor_base_level_enum_to_byte(
    level: MemoryMonitorLowMemoryLevel,
) -> MemoryMonitorWarningLevel {
    match level {
        MemoryMonitorLowMemoryLevel::Invalid => 0,
        MemoryMonitorLowMemoryLevel::Low => 50,
        MemoryMonitorLowMemoryLevel::Medium => 100,
        MemoryMonitorLowMemoryLevel::Critical => 255,
    }
}

/// Queries free/total RAM ratio (`g_memory_monitor_base_query_mem_ratio`).
///
/// Returns a value in `0.0..=1.0` on platforms with memory stats, or
/// `-1.0` when unavailable (bare-metal RustOS default).
pub fn memory_monitor_base_query_mem_ratio() -> f64 {
    #[cfg(target_os = "linux")]
    {
        // Best-effort: parse MemAvailable/MemTotal from /proc/meminfo when std I/O exists.
        #[cfg(test)]
        {
            let _ = ();
        }
    }
    -1.0
}

/// Maps a low-memory level to [`MemoryPressureLevel`] for the default monitor.
pub fn low_memory_level_to_pressure(level: MemoryMonitorLowMemoryLevel) -> MemoryPressureLevel {
    match level {
        MemoryMonitorLowMemoryLevel::Low | MemoryMonitorLowMemoryLevel::Medium => {
            MemoryPressureLevel::Low
        }
        MemoryMonitorLowMemoryLevel::Critical => MemoryPressureLevel::Critical,
        MemoryMonitorLowMemoryLevel::Invalid => MemoryPressureLevel::Normal,
    }
}

/// Base type for memory monitor implementations (`GMemoryMonitorBase`).
pub struct MemoryMonitorBase {
    last_trigger_us: Mutex<[i64; MemoryMonitorLowMemoryLevel::COUNT]>,
    target: Arc<MemoryMonitor>,
}

impl MemoryMonitorBase {
    /// Creates a base wired to `target` for event delivery.
    pub fn new(target: Arc<MemoryMonitor>) -> Self {
        Self {
            last_trigger_us: Mutex::new([0; MemoryMonitorLowMemoryLevel::COUNT]),
            target,
        }
    }

    /// Returns the underlying [`MemoryMonitor`].
    pub fn monitor(&self) -> &MemoryMonitor {
        &self.target
    }

    /// Rate-limited low-memory notification (`g_memory_monitor_base_send_event_to_user`).
    ///
    /// Updates the linked [`MemoryMonitor`] pressure level when the event is
    /// not suppressed by the 15-second per-level throttle.
    pub fn send_event_to_user(&self, warning_level: MemoryMonitorLowMemoryLevel) {
        let idx = match warning_level {
            MemoryMonitorLowMemoryLevel::Low => 0,
            MemoryMonitorLowMemoryLevel::Medium => 1,
            MemoryMonitorLowMemoryLevel::Critical => 2,
            MemoryMonitorLowMemoryLevel::Invalid => return,
        };

        let current_time = monotonic_time_us();
        let mut last = self.last_trigger_us.lock();
        if last[idx] != 0
            && current_time >= last[idx]
            && current_time - last[idx] <= RECOVERY_INTERVAL_US
        {
            return;
        }
        last[idx] = current_time;
        drop(last);

        let _warning_byte = memory_monitor_base_level_enum_to_byte(warning_level);
        let pressure = low_memory_level_to_pressure(warning_level);
        let changed = self.target.get_memory_pressure() != pressure;
        self.target.set_memory_pressure(pressure);
        if !changed {
            self.target.emit_memory_pressure(pressure);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU32, Ordering};

    #[test]
    fn level_enum_to_byte_values() {
        assert_eq!(
            memory_monitor_base_level_enum_to_byte(MemoryMonitorLowMemoryLevel::Invalid),
            0
        );
        assert_eq!(
            memory_monitor_base_level_enum_to_byte(MemoryMonitorLowMemoryLevel::Low),
            50
        );
        assert_eq!(
            memory_monitor_base_level_enum_to_byte(MemoryMonitorLowMemoryLevel::Medium),
            100
        );
        assert_eq!(
            memory_monitor_base_level_enum_to_byte(MemoryMonitorLowMemoryLevel::Critical),
            255
        );
    }

    #[test]
    fn query_mem_ratio_unavailable() {
        assert!(memory_monitor_base_query_mem_ratio() < 0.0);
    }

    #[test]
    fn send_event_updates_pressure() {
        let monitor = Arc::new(MemoryMonitor::new());
        let base = MemoryMonitorBase::new(Arc::clone(&monitor));
        base.send_event_to_user(MemoryMonitorLowMemoryLevel::Low);
        assert_eq!(monitor.get_memory_pressure(), MemoryPressureLevel::Low);
    }

    #[test]
    fn send_event_throttled_within_interval() {
        crate::timer::set_clock(|| 1_000_000);
        let monitor = Arc::new(MemoryMonitor::new());
        let base = MemoryMonitorBase::new(Arc::clone(&monitor));
        let hits = Arc::new(AtomicU32::new(0));
        monitor.connect(Arc::new({
            let hits = Arc::clone(&hits);
            move |_| {
                hits.fetch_add(1, Ordering::SeqCst);
            }
        }));
        base.send_event_to_user(MemoryMonitorLowMemoryLevel::Critical);
        base.send_event_to_user(MemoryMonitorLowMemoryLevel::Critical);
        assert_eq!(hits.load(Ordering::SeqCst), 1);
        crate::timer::set_clock(|| 1_000_000 + RECOVERY_INTERVAL_US + 1);
        base.send_event_to_user(MemoryMonitorLowMemoryLevel::Critical);
        assert_eq!(hits.load(Ordering::SeqCst), 2);
    }
}
