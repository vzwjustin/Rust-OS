//! GDebugController matching `gio/gdebugcontroller.h`.
//! A debug controller for D-Bus-based debug control. In this no_std port
//! we model it with a debug-enabled flag.
//! Fully `no_std` compatible using `alloc`.

use spin::Mutex;

/// A debug controller (`GDebugController`).
pub struct DebugController {
    debug_enabled: Mutex<bool>,
}

impl DebugController {
    pub fn new() -> Self {
        Self {
            debug_enabled: Mutex::new(false),
        }
    }

    pub fn get_debug_enabled(&self) -> bool {
        *self.debug_enabled.lock()
    }

    pub fn set_debug_enabled(&self, enabled: bool) {
        *self.debug_enabled.lock() = enabled;
    }
}

impl Default for DebugController {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_disabled() {
        let c = DebugController::new();
        assert!(!c.get_debug_enabled());
    }

    #[test]
    fn test_enable_disable() {
        let c = DebugController::new();
        c.set_debug_enabled(true);
        assert!(c.get_debug_enabled());
        c.set_debug_enabled(false);
        assert!(!c.get_debug_enabled());
    }
}
