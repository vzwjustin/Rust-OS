//! GMemoryMonitor matching `gio/gmemorymonitor.h`.
//!
//! Monitors system memory pressure with a default singleton, platform hook
//! to set pressure, and connect/disconnect callbacks notified on change.
//!
//! Fully `no_std` compatible using `alloc` and `spin`.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU64, Ordering};
use spin::{Mutex, Once};

/// Memory pressure level (`GMemoryPressureLevel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MemoryPressureLevel {
    /// Normal memory pressure.
    Normal,
    /// Low memory — applications should reduce cache usage.
    Low,
    /// Critical memory — applications should free memory aggressively.
    Critical,
}

/// Memory pressure monitor (`GMemoryMonitor`).
pub struct MemoryMonitor {
    pressure: Mutex<MemoryPressureLevel>,
    callbacks: Mutex<Vec<(u64, Arc<dyn Fn(MemoryPressureLevel) + Send + Sync>)>>,
    next_handler_id: AtomicU64,
}

impl MemoryMonitor {
    /// Creates a monitor defaulting to [`MemoryPressureLevel::Normal`].
    pub fn new() -> Self {
        Self {
            pressure: Mutex::new(MemoryPressureLevel::Normal),
            callbacks: Mutex::new(Vec::new()),
            next_handler_id: AtomicU64::new(1),
        }
    }

    /// Returns the current memory pressure level.
    ///
    /// Mirrors `g_memory_monitor_get_memory_pressure`.
    pub fn get_memory_pressure(&self) -> MemoryPressureLevel {
        *self.pressure.lock()
    }

    /// Sets the memory pressure level and notifies connected handlers.
    ///
    /// Platform hook mirroring the OS memory pressure callback.
    pub fn set_memory_pressure(&self, level: MemoryPressureLevel) {
        let changed = {
            let mut pressure = self.pressure.lock();
            if *pressure == level {
                false
            } else {
                *pressure = level;
                true
            }
        };
        if changed {
            self.notify(level);
        }
    }

    /// Connects a callback invoked when memory pressure changes.
    ///
    /// Returns a handler id for [`MemoryMonitor::disconnect`].
    pub fn connect(&self, callback: Arc<dyn Fn(MemoryPressureLevel) + Send + Sync>) -> u64 {
        let id = self.next_handler_id.fetch_add(1, Ordering::SeqCst);
        self.callbacks.lock().push((id, callback));
        id
    }

    /// Disconnects a previously connected handler. Returns `true` on success.
    pub fn disconnect(&self, handler_id: u64) -> bool {
        let mut callbacks = self.callbacks.lock();
        let len_before = callbacks.len();
        callbacks.retain(|(id, _)| *id != handler_id);
        callbacks.len() < len_before
    }

    /// Returns the number of connected handlers.
    pub fn handler_count(&self) -> usize {
        self.callbacks.lock().len()
    }

    /// Emits a memory pressure notification without changing stored pressure.
    pub fn emit_memory_pressure(&self, level: MemoryPressureLevel) {
        self.notify(level);
    }

    fn notify(&self, level: MemoryPressureLevel) {
        let callbacks: Vec<Arc<dyn Fn(MemoryPressureLevel) + Send + Sync>> = self
            .callbacks
            .lock()
            .iter()
            .map(|(_, cb)| Arc::clone(cb))
            .collect();
        for callback in callbacks {
            callback(level);
        }
    }
}

impl Default for MemoryMonitor {
    fn default() -> Self {
        Self::new()
    }
}

// ─────────────────────────── singleton ────────────────────────────────────

static DEFAULT_MONITOR: Once<MemoryMonitor> = Once::new();

/// Returns the default memory monitor singleton.
///
/// Mirrors `g_memory_monitor_get_default`.
pub fn memory_monitor_get_default() -> &'static MemoryMonitor {
    DEFAULT_MONITOR.call_once(MemoryMonitor::new)
}

// ──────────────────────────── Tests ───────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use core::sync::atomic::{AtomicU32, Ordering as AtomicOrdering};

    #[test]
    fn test_default_pressure() {
        let monitor = MemoryMonitor::new();
        assert_eq!(monitor.get_memory_pressure(), MemoryPressureLevel::Normal);
    }

    #[test]
    fn test_set_pressure() {
        let monitor = MemoryMonitor::new();
        monitor.set_memory_pressure(MemoryPressureLevel::Low);
        assert_eq!(monitor.get_memory_pressure(), MemoryPressureLevel::Low);
        monitor.set_memory_pressure(MemoryPressureLevel::Critical);
        assert_eq!(monitor.get_memory_pressure(), MemoryPressureLevel::Critical);
    }

    #[test]
    fn test_connect_and_notify() {
        let monitor = MemoryMonitor::new();
        let seen = Arc::new(AtomicU32::new(0));
        let seen_cb = Arc::clone(&seen);
        monitor.connect(Arc::new(move |level| {
            if level == MemoryPressureLevel::Low {
                seen_cb.fetch_add(1, AtomicOrdering::SeqCst);
            }
        }));
        monitor.set_memory_pressure(MemoryPressureLevel::Low);
        assert_eq!(seen.load(AtomicOrdering::SeqCst), 1);
        // Same level — no duplicate notification.
        monitor.set_memory_pressure(MemoryPressureLevel::Low);
        assert_eq!(seen.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn test_disconnect() {
        let monitor = MemoryMonitor::new();
        let seen = Arc::new(AtomicU32::new(0));
        let id = monitor.connect(Arc::new({
            let seen = Arc::clone(&seen);
            move |_| {
                seen.fetch_add(1, AtomicOrdering::SeqCst);
            }
        }));
        assert!(monitor.disconnect(id));
        monitor.set_memory_pressure(MemoryPressureLevel::Critical);
        assert_eq!(seen.load(AtomicOrdering::SeqCst), 0);
    }

    #[test]
    fn test_singleton() {
        let a = memory_monitor_get_default();
        let b = memory_monitor_get_default();
        assert!(core::ptr::eq(a, b));
    }
}
