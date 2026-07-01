//! GNOME Mutter's src/backends/meta-idle-monitor.c
//!
//! Idle counter (similar to X's IDLETIME): tracks how long the user has been
//! idle and fires callbacks (watches) after configurable idle intervals, or
//! when the user becomes active again.
//!
//! Stubbed: the GSource-based timeout scheduling and the D-Bus session-manager
//! proxy that reports idle inhibitors are not available in the kernel. Watches
//! keep their full state; firing is driven by `tick()`/`reset_idletime()`
//! instead of a main-loop GSource, and inhibition is set via `set_inhibited()`.
//!
//! Reference:
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-idle-monitor.c

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

/// GSM_INHIBITOR_FLAG_IDLE (1 << 3), used against the session manager's
/// `InhibitedActions` property to decide whether idle tracking is inhibited.
pub const GSM_INHIBITOR_FLAG_IDLE: u32 = 1 << 3;

/// Flags for an idle watch, mirroring MetaIdleMonitorWatchFlags.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WatchFlags {
    /// Start counting from "now" rather than from the last event time.
    pub start_now: bool,
    /// Watch is NOT affected by idle inhibitors.
    pub uninhibitable: bool,
}

impl WatchFlags {
    pub const NONE: WatchFlags = WatchFlags {
        start_now: false,
        uninhibitable: false,
    };
}

/// A single idle watch. Mirrors MetaIdleMonitorWatch.
///
/// A watch with `timeout_msec == 0` is a "user active" watch: it is one-shot
/// and fires when the user becomes active again (i.e. on reset).
#[derive(Debug, Clone)]
pub struct IdleWatch {
    pub id: u32,
    /// Idle interval before firing, in milliseconds. 0 == user-active watch.
    pub timeout_msec: u64,
    /// Whether this watch is affected by idle inhibition.
    pub inhibitable: bool,
    /// Whether the watch started counting from "now" instead of last event.
    pub start_now: bool,
    /// Absolute monotonic time (microseconds) at which the watch should fire,
    /// or `None` when disarmed (e.g. while inhibited). Mirrors the GSource
    /// "ready time". Only meaningful for timeout watches.
    pub ready_time: Option<i64>,
    /// Set true once the watch has fired (so `tick()` won't refire it).
    pub fired: bool,
}

static WATCH_SERIAL: AtomicU32 = AtomicU32::new(0);

/// get_next_watch_serial()
fn get_next_watch_serial() -> u32 {
    WATCH_SERIAL.fetch_add(1, Ordering::SeqCst) + 1
}

/// Mutter idle counter. Mirrors struct _MetaIdleMonitor.
#[derive(Debug)]
pub struct IdleMonitor {
    inhibited: bool,
    watches: BTreeMap<u32, IdleWatch>,
    /// Monotonic time (microseconds) of the last user input event.
    last_event_time: i64,
}

impl IdleMonitor {
    /// meta_idle_monitor_new() / meta_idle_monitor_init()
    ///
    /// The D-Bus session proxy that watches `InhibitedActions` is stubbed out;
    /// call `set_inhibited()` to feed inhibition state in.
    pub fn new(now_usec: i64) -> Self {
        IdleMonitor {
            inhibited: false,
            watches: BTreeMap::new(),
            last_event_time: now_usec,
        }
    }

    /// make_watch() — internal helper that builds and inserts a watch.
    fn make_watch(&mut self, timeout_msec: u64, flags: WatchFlags, now_usec: i64) -> u32 {
        let id = get_next_watch_serial();
        let inhibitable = !flags.uninhibitable;
        let mut ready_time = None;

        if timeout_msec != 0 && (!inhibitable || !self.inhibited) {
            let start_time = if flags.start_now {
                now_usec
            } else {
                self.last_event_time
            };
            ready_time = Some(start_time + (timeout_msec as i64) * 1000);
        }

        let watch = IdleWatch {
            id,
            timeout_msec,
            inhibitable,
            start_now: flags.start_now,
            ready_time,
            fired: false,
        };
        self.watches.insert(id, watch);
        id
    }

    /// meta_idle_monitor_add_idle_watch()
    pub fn add_idle_watch(&mut self, interval_msec: u64, now_usec: i64) -> u32 {
        self.add_idle_watch_full(interval_msec, WatchFlags::NONE, now_usec)
    }

    /// meta_idle_monitor_add_idle_watch_full()
    pub fn add_idle_watch_full(
        &mut self,
        interval_msec: u64,
        flags: WatchFlags,
        now_usec: i64,
    ) -> u32 {
        if interval_msec == 0 {
            return 0;
        }
        self.make_watch(interval_msec, flags, now_usec)
    }

    /// meta_idle_monitor_add_user_active_watch()
    ///
    /// One-time watch that fires when the user becomes active again.
    pub fn add_user_active_watch(&mut self, now_usec: i64) -> u32 {
        self.make_watch(0, WatchFlags::NONE, now_usec)
    }

    /// meta_idle_monitor_remove_watch()
    pub fn remove_watch(&mut self, id: u32) {
        self.watches.remove(&id);
    }

    /// meta_idle_monitor_get_idletime() — current idle time in milliseconds.
    pub fn get_idletime(&self, now_usec: i64) -> i64 {
        (now_usec - self.last_event_time) / 1000
    }

    /// update_inhibited() + update_inhibited_watch(): re-arm / disarm the
    /// timeout watches when the inhibition state changes.
    pub fn set_inhibited(&mut self, inhibited: bool, now_usec: i64) {
        if inhibited == self.inhibited {
            return;
        }
        // When leaving inhibition, treat it like fresh activity.
        if !inhibited {
            self.last_event_time = now_usec;
        }
        self.inhibited = inhibited;

        let last = self.last_event_time;
        for watch in self.watches.values_mut() {
            if watch.timeout_msec == 0 || !watch.inhibitable {
                continue;
            }
            if inhibited {
                watch.ready_time = None;
            } else {
                watch.ready_time = Some(last + (watch.timeout_msec as i64) * 1000);
            }
        }
    }

    pub fn is_inhibited(&self) -> bool {
        self.inhibited
    }

    /// meta_idle_monitor_reset_idletime()
    ///
    /// Marks fresh user activity. User-active (timeout 0) watches fire and are
    /// removed; timeout watches are re-armed from the new event time (unless
    /// inhibited). Returns the ids of watches that fired.
    pub fn reset_idletime(&mut self, now_usec: i64) -> Vec<u32> {
        self.last_event_time = now_usec;
        let mut fired = Vec::new();

        let ids: Vec<u32> = self.watches.keys().copied().collect();
        for id in ids {
            let (is_active_watch, timeout, inhibitable) = match self.watches.get(&id) {
                Some(w) => (w.timeout_msec == 0, w.timeout_msec, w.inhibitable),
                None => continue,
            };

            if is_active_watch {
                fired.push(id);
                // User-active watches are one-shot: remove after firing.
                self.watches.remove(&id);
            } else if let Some(w) = self.watches.get_mut(&id) {
                w.fired = false;
                if inhibitable && self.inhibited {
                    w.ready_time = None;
                } else {
                    w.ready_time = Some(now_usec + (timeout as i64) * 1000);
                }
            }
        }
        fired
    }

    /// idle_monitor_dispatch_timeout() equivalent, driven manually.
    ///
    /// Fires any armed timeout watch whose ready_time has been reached at
    /// `now_usec`. Replaces the GSource dispatch loop. Returns fired watch ids.
    pub fn tick(&mut self, now_usec: i64) -> Vec<u32> {
        let mut fired = Vec::new();
        for watch in self.watches.values_mut() {
            if watch.timeout_msec == 0 || watch.fired {
                continue;
            }
            if let Some(ready) = watch.ready_time {
                if ready <= now_usec {
                    watch.fired = true;
                    watch.ready_time = None;
                    fired.push(watch.id);
                }
            }
        }
        fired
    }

    pub fn get_watch(&self, id: u32) -> Option<&IdleWatch> {
        self.watches.get(&id)
    }

    pub fn watch_count(&self) -> usize {
        self.watches.len()
    }
}
