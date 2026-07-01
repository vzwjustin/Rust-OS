//! Idle monitor — ported from gnome-idle-monitor.c
//!
//! Tracks user idle time based on input events (mouse movement, keyboard).
//! Supports adding idle watches (fire after N ms of inactivity) and
//! user-active watches (fire when user becomes active again).
//!
//! The upstream uses DBus to communicate with Mutter's IdleMonitor.  We
//! implement the idle tracking directly in the kernel using uptime timestamps.

use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, AtomicU64, Ordering};

static NEXT_WATCH_ID: AtomicU32 = AtomicU32::new(1);

/// Last time (in uptime_ms) that user input was received.
static LAST_INPUT_TIME: AtomicU64 = AtomicU64::new(0);

/// Record that user input occurred.  Called from mouse/keyboard event handlers.
pub fn notify_user_input() {
    LAST_INPUT_TIME.store(crate::time::uptime_ms(), Ordering::Relaxed);
}

/// Get the current idle time in milliseconds (time since last input).
pub fn get_idletime() -> u64 {
    let last = LAST_INPUT_TIME.load(Ordering::Relaxed);
    let now = crate::time::uptime_ms();
    if last == 0 {
        // No input yet — treat boot time as last input
        now
    } else {
        now.saturating_sub(last)
    }
}

/// Type of watch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WatchKind {
    /// Fires after `timeout_msec` of idle time.
    Idle,
    /// Fires when user becomes active (one-shot).
    UserActive,
}

/// Callback function type for idle/active watches.
pub type IdleCallback = fn(u32);

/// A single idle/active watch entry.
struct Watch {
    id: u32,
    kind: WatchKind,
    timeout_msec: u64,
    fired: bool,
    callback: IdleCallback,
}

/// Idle monitor — manages idle and user-active watches.
pub struct IdleMonitor {
    watches: Vec<Watch>,
}

impl IdleMonitor {
    /// Create a new idle monitor.
    pub fn new() -> Self {
        Self {
            watches: Vec::new(),
        }
    }

    /// Add an idle watch — fires `callback(watch_id)` when the user has been
    /// idle for `interval_msec` milliseconds.  Returns a watch ID.
    pub fn add_idle_watch(&mut self, interval_msec: u64, callback: IdleCallback) -> u32 {
        assert!(interval_msec > 0, "idle watch interval must be > 0");
        let id = NEXT_WATCH_ID.fetch_add(1, Ordering::Relaxed);
        self.watches.push(Watch {
            id,
            kind: WatchKind::Idle,
            timeout_msec: interval_msec,
            fired: false,
            callback,
        });
        id
    }

    /// Add a user-active watch — fires `callback(watch_id)` once when the user
    /// becomes active after being idle.  Returns a watch ID.
    pub fn add_user_active_watch(&mut self, callback: IdleCallback) -> u32 {
        let id = NEXT_WATCH_ID.fetch_add(1, Ordering::Relaxed);
        self.watches.push(Watch {
            id,
            kind: WatchKind::UserActive,
            timeout_msec: 0,
            fired: false,
            callback,
        });
        id
    }

    /// Remove a watch by ID.
    pub fn remove_watch(&mut self, id: u32) {
        self.watches.retain(|w| w.id != id);
    }

    /// Get the current idle time in milliseconds.
    pub fn get_idletime(&self) -> u64 {
        get_idletime()
    }

    /// Poll all watches and fire callbacks for triggered conditions.
    /// Should be called periodically from the desktop tick.
    pub fn tick(&mut self) {
        let idle = get_idletime();
        let now = crate::time::uptime_ms();

        // We need to be careful: callbacks may modify the watch list.
        // Collect fired watch IDs first, then invoke callbacks.
        let mut to_fire: Vec<(u32, IdleCallback)> = Vec::new();

        for w in &self.watches {
            if w.fired {
                continue;
            }
            match w.kind {
                WatchKind::Idle => {
                    if idle >= w.timeout_msec {
                        to_fire.push((w.id, w.callback));
                    }
                }
                WatchKind::UserActive => {
                    let last = LAST_INPUT_TIME.load(Ordering::Relaxed);
                    if last > 0 && now.saturating_sub(last) < 500 {
                        to_fire.push((w.id, w.callback));
                    }
                }
            }
        }

        for (id, _) in &to_fire {
            for w in &mut self.watches {
                if w.id == *id {
                    w.fired = true;
                    break;
                }
            }
        }

        for (id, cb) in &to_fire {
            cb(*id);
        }

        // Remove one-shot user-active watches that have fired
        self.watches
            .retain(|w| !(w.kind == WatchKind::UserActive && w.fired));

        // Reset idle watches when user becomes active again
        let last = LAST_INPUT_TIME.load(Ordering::Relaxed);
        if last > 0 && now.saturating_sub(last) < 500 {
            for w in &mut self.watches {
                if w.kind == WatchKind::Idle {
                    w.fired = false;
                }
            }
        }
    }

    /// Number of active watches.
    pub fn watch_count(&self) -> usize {
        self.watches.len()
    }
}

impl Default for IdleMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_idletime_initial() {
        let _idle = get_idletime();
        // Just verify it doesn't panic
    }

    fn test_notify_input() {
        notify_user_input();
        let idle = get_idletime();
        // Right after input, idle should be very small
        assert!(idle < 100);
    }

    fn dummy_cb(_id: u32) {}

    fn test_add_remove_watch() {
        let mut monitor = IdleMonitor::new();
        let id = monitor.add_idle_watch(5000, dummy_cb);
        assert_eq!(monitor.watch_count(), 1);
        monitor.remove_watch(id);
        assert_eq!(monitor.watch_count(), 0);
    }
}
