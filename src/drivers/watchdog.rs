//! Watchdog timer driver for RustOS
//!
//! Implements Linux-style watchdog core semantics for the built-in software
//! watchdog: one registered device, explicit start/stop, keepalive ping,
//! timeout validation, nowayout handling, and per-second expiry accounting.

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use spin::Mutex;

const DEFAULT_TIMEOUT_SECS: u32 = 10;
const DEFAULT_MIN_TIMEOUT_SECS: u32 = 1;
const DEFAULT_MAX_TIMEOUT_SECS: u32 = 3600;

static WATCHDOG_TIMEOUT: AtomicU32 = AtomicU32::new(DEFAULT_TIMEOUT_SECS);
static WATCHDOG_REMAINING: AtomicU32 = AtomicU32::new(DEFAULT_TIMEOUT_SECS);
static WATCHDOG_ENABLED: AtomicBool = AtomicBool::new(false);
static WATCHDOG_OPEN: AtomicBool = AtomicBool::new(false);
static WATCHDOG_NOWAYOUT: AtomicBool = AtomicBool::new(false);
static WATCHDOG_TICKS: AtomicU64 = AtomicU64::new(0);
static WATCHDOG_LAST_PING_TICK: AtomicU64 = AtomicU64::new(0);

static WATCHDOG_REGISTRATION: Mutex<WatchdogRegistration> = Mutex::new(WatchdogRegistration {
    name: "rustos-softdog",
    identity: "RustOS software watchdog",
    min_timeout_secs: DEFAULT_MIN_TIMEOUT_SECS,
    max_timeout_secs: DEFAULT_MAX_TIMEOUT_SECS,
    boot_status: 0,
    registered: true,
});

/// Linux-compatible watchdog option flags for this device.
pub mod options {
    /// Driver can report boot status.
    pub const WDIOF_CARDRESET: u32 = 0x0020;
    /// Driver supports keepalive pings.
    pub const WDIOF_KEEPALIVEPING: u32 = 0x8000;
    /// Driver supports setting timeouts.
    pub const WDIOF_SETTIMEOUT: u32 = 0x0080;
    /// Magic close/nowayout semantics are supported.
    pub const WDIOF_MAGICCLOSE: u32 = 0x0100;
}

/// Runtime status bits modelled after Linux watchdog core state.
pub mod status {
    pub const REGISTERED: u32 = 1 << 0;
    pub const OPEN: u32 = 1 << 1;
    pub const ACTIVE: u32 = 1 << 2;
    pub const NOWAYOUT: u32 = 1 << 3;
}

/// Watchdog operation errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WatchdogError {
    AlreadyRegistered,
    NotRegistered,
    AlreadyOpen,
    NotActive,
    InvalidTimeout,
    Nowayout,
}

/// Static registration data for a watchdog device.
#[derive(Debug, Clone, Copy)]
pub struct WatchdogRegistration {
    pub name: &'static str,
    pub identity: &'static str,
    pub min_timeout_secs: u32,
    pub max_timeout_secs: u32,
    pub boot_status: u32,
    registered: bool,
}

impl WatchdogRegistration {
    pub const fn new(
        name: &'static str,
        identity: &'static str,
        min_timeout_secs: u32,
        max_timeout_secs: u32,
        boot_status: u32,
    ) -> Self {
        Self {
            name,
            identity,
            min_timeout_secs,
            max_timeout_secs,
            boot_status,
            registered: false,
        }
    }

    pub const fn software_default() -> Self {
        Self {
            name: "rustos-softdog",
            identity: "RustOS software watchdog",
            min_timeout_secs: DEFAULT_MIN_TIMEOUT_SECS,
            max_timeout_secs: DEFAULT_MAX_TIMEOUT_SECS,
            boot_status: 0,
            registered: true,
        }
    }
}

/// Userspace-visible watchdog information.
#[derive(Debug, Clone, Copy)]
pub struct WatchdogInfo {
    pub options: u32,
    pub firmware_version: u32,
    pub identity: &'static str,
}

fn registration() -> WatchdogRegistration {
    *WATCHDOG_REGISTRATION.lock()
}

fn validate_timeout(seconds: u32, reg: WatchdogRegistration) -> Result<(), WatchdogError> {
    if !reg.registered {
        return Err(WatchdogError::NotRegistered);
    }
    if seconds < reg.min_timeout_secs || seconds > reg.max_timeout_secs {
        return Err(WatchdogError::InvalidTimeout);
    }
    Ok(())
}

/// Register a watchdog device. RustOS currently exposes one watchdog slot; this
/// mirrors Linux's core registration gate and rejects a second different device.
pub fn register_watchdog(mut device: WatchdogRegistration) -> Result<(), WatchdogError> {
    if WATCHDOG_ENABLED.load(Ordering::Acquire) {
        return Err(WatchdogError::AlreadyOpen);
    }
    if device.min_timeout_secs == 0 || device.min_timeout_secs > device.max_timeout_secs {
        return Err(WatchdogError::InvalidTimeout);
    }

    let mut reg = WATCHDOG_REGISTRATION.lock();
    if reg.registered && reg.name != device.name {
        return Err(WatchdogError::AlreadyRegistered);
    }

    device.registered = true;
    *reg = device;
    if WATCHDOG_TIMEOUT.load(Ordering::Acquire) < device.min_timeout_secs
        || WATCHDOG_TIMEOUT.load(Ordering::Acquire) > device.max_timeout_secs
    {
        WATCHDOG_TIMEOUT.store(device.min_timeout_secs, Ordering::Release);
        WATCHDOG_REMAINING.store(device.min_timeout_secs, Ordering::Release);
    }
    Ok(())
}

/// Unregister the watchdog device when it is inactive.
pub fn unregister_watchdog() -> Result<(), WatchdogError> {
    if WATCHDOG_ENABLED.load(Ordering::Acquire) || WATCHDOG_OPEN.load(Ordering::Acquire) {
        return Err(WatchdogError::AlreadyOpen);
    }
    WATCHDOG_REGISTRATION.lock().registered = false;
    Ok(())
}

/// Incrementally count down the watchdog timer.
///
/// This should be called once per second (e.g., from the timer interrupt).
pub fn watchdog_tick() {
    let tick = WATCHDOG_TICKS.fetch_add(1, Ordering::AcqRel) + 1;
    if !WATCHDOG_ENABLED.load(Ordering::Acquire) {
        return;
    }

    loop {
        let remaining = WATCHDOG_REMAINING.load(Ordering::Acquire);
        if remaining == 0 {
            panic!("WATCHDOG TIMER EXPIRED: System lockup detected!");
        }
        if WATCHDOG_REMAINING
            .compare_exchange(remaining, remaining - 1, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
        {
            if remaining == 1 {
                WATCHDOG_LAST_PING_TICK.store(tick, Ordering::Release);
                panic!("WATCHDOG TIMER EXPIRED: System lockup detected!");
            }
            break;
        }
    }
}

/// Ping/keepalive the watchdog timer back to its configured timeout.
pub fn try_ping() -> Result<(), WatchdogError> {
    if !registration().registered {
        return Err(WatchdogError::NotRegistered);
    }
    if !WATCHDOG_ENABLED.load(Ordering::Acquire) {
        return Err(WatchdogError::NotActive);
    }

    let timeout = WATCHDOG_TIMEOUT.load(Ordering::Acquire);
    WATCHDOG_REMAINING.store(timeout, Ordering::Release);
    WATCHDOG_LAST_PING_TICK.store(WATCHDOG_TICKS.load(Ordering::Acquire), Ordering::Release);
    Ok(())
}

/// Reset (kick) the watchdog timer back to its configured timeout.
pub fn kick() {
    let _ = try_ping();
}

/// Set the watchdog timeout in seconds with validation.
pub fn try_set_timeout(seconds: u32) -> Result<(), WatchdogError> {
    let reg = registration();
    validate_timeout(seconds, reg)?;
    WATCHDOG_TIMEOUT.store(seconds, Ordering::Release);
    if WATCHDOG_ENABLED.load(Ordering::Acquire) {
        WATCHDOG_REMAINING.store(seconds, Ordering::Release);
        WATCHDOG_LAST_PING_TICK.store(WATCHDOG_TICKS.load(Ordering::Acquire), Ordering::Release);
    }
    Ok(())
}

/// Set the watchdog timeout in seconds.
///
/// This preserves the historical infallible API by clamping to the registered
/// device range; callers that need Linux-style `EINVAL` should use
/// `try_set_timeout`.
pub fn set_timeout(seconds: u32) {
    let reg = registration();
    if !reg.registered {
        return;
    }
    let clamped = seconds.clamp(reg.min_timeout_secs, reg.max_timeout_secs);
    let _ = try_set_timeout(clamped);
}

/// Get the current watchdog timeout in seconds.
pub fn get_timeout() -> u32 {
    WATCHDOG_TIMEOUT.load(Ordering::Acquire)
}

/// Get the remaining time before expiration in seconds.
pub fn get_timeleft() -> u32 {
    if WATCHDOG_ENABLED.load(Ordering::Acquire) {
        WATCHDOG_REMAINING.load(Ordering::Acquire)
    } else {
        0
    }
}

/// Enable/open the watchdog timer.
pub fn try_enable() -> Result<(), WatchdogError> {
    let reg = registration();
    if !reg.registered {
        return Err(WatchdogError::NotRegistered);
    }
    if WATCHDOG_OPEN.swap(true, Ordering::AcqRel) {
        return Err(WatchdogError::AlreadyOpen);
    }
    let timeout = WATCHDOG_TIMEOUT.load(Ordering::Acquire).clamp(
        reg.min_timeout_secs,
        reg.max_timeout_secs,
    );
    WATCHDOG_TIMEOUT.store(timeout, Ordering::Release);
    WATCHDOG_REMAINING.store(timeout, Ordering::Release);
    WATCHDOG_LAST_PING_TICK.store(WATCHDOG_TICKS.load(Ordering::Acquire), Ordering::Release);
    WATCHDOG_ENABLED.store(true, Ordering::Release);
    Ok(())
}

/// Enable the watchdog timer.
pub fn enable() {
    let _ = try_enable();
}

/// Disable/close the watchdog timer.
pub fn try_disable() -> Result<(), WatchdogError> {
    if WATCHDOG_NOWAYOUT.load(Ordering::Acquire) {
        return Err(WatchdogError::Nowayout);
    }
    WATCHDOG_ENABLED.store(false, Ordering::Release);
    WATCHDOG_OPEN.store(false, Ordering::Release);
    Ok(())
}

/// Disable the watchdog timer.
pub fn disable() {
    let _ = try_disable();
}

/// Enable or disable nowayout semantics.  Once enabled while active, disabling
/// is rejected to preserve the safety contract expected by watchdog users.
pub fn set_nowayout(enabled: bool) -> Result<(), WatchdogError> {
    if !enabled
        && WATCHDOG_NOWAYOUT.load(Ordering::Acquire)
        && WATCHDOG_ENABLED.load(Ordering::Acquire)
    {
        return Err(WatchdogError::Nowayout);
    }
    WATCHDOG_NOWAYOUT.store(enabled, Ordering::Release);
    Ok(())
}

/// Check if the watchdog timer is enabled.
pub fn is_enabled() -> bool {
    WATCHDOG_ENABLED.load(Ordering::Acquire)
}

/// Return Linux-style status bits for the watchdog core.
pub fn get_status() -> u32 {
    let mut bits = 0;
    if registration().registered {
        bits |= status::REGISTERED;
    }
    if WATCHDOG_OPEN.load(Ordering::Acquire) {
        bits |= status::OPEN;
    }
    if WATCHDOG_ENABLED.load(Ordering::Acquire) {
        bits |= status::ACTIVE;
    }
    if WATCHDOG_NOWAYOUT.load(Ordering::Acquire) {
        bits |= status::NOWAYOUT;
    }
    bits
}

/// Return static watchdog identity and capability flags.
pub fn get_info() -> WatchdogInfo {
    let reg = registration();
    WatchdogInfo {
        options: options::WDIOF_CARDRESET
            | options::WDIOF_KEEPALIVEPING
            | options::WDIOF_SETTIMEOUT
            | options::WDIOF_MAGICCLOSE,
        firmware_version: 1,
        identity: reg.identity,
    }
}

/// Boot status reported by the registered watchdog device.
pub fn get_boot_status() -> u32 {
    registration().boot_status
}

/// Last tick at which the watchdog was pinged.
pub fn last_ping_tick() -> u64 {
    WATCHDOG_LAST_PING_TICK.load(Ordering::Acquire)
}
