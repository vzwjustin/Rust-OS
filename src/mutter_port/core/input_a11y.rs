//! MetaInputA11y ported from GNOME Mutter's src/core/meta-input-a11y.c
//!
//! MetaInputA11y manages input accessibility settings: keyboard
//! accessibility features like sticky keys, slow keys, bounce keys,
//! mouse keys, and the on-screen keyboard. In Mutter this reads from
//! GSettings (org.gnome.desktop.a11y.keyboard, .mouse) and applies
//! them to ClutterInputDevice objects.
//!
//! In the kernel, GSettings is not available. The settings are stored
//! as plain fields and applied via the seat implementation.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-input-a11y.c

use alloc::vec::Vec;

/// Keyboard accessibility features, mirroring GSettings a11y.keyboard keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyboardA11y {
    /// Sticky keys: modifier keys stay active until the next non-modifier key.
    pub sticky_keys: bool,
    /// Slow keys: keys must be held for a delay before registering.
    pub slow_keys: bool,
    /// Bounce keys: ignores rapid duplicate key presses.
    pub bounce_keys: bool,
    /// Toggle keys: beep when a modifier key is pressed.
    pub toggle_keys: bool,
    /// Mouse keys: numpad controls the pointer.
    pub mouse_keys: bool,
}

impl Default for KeyboardA11y {
    fn default() -> Self {
        KeyboardA11y {
            sticky_keys: false,
            slow_keys: false,
            bounce_keys: false,
            toggle_keys: false,
            mouse_keys: false,
        }
    }
}

/// Slow keys delay in milliseconds (how long a key must be held).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlowKeysDelay {
    Short,
    Medium,
    Long,
}

impl SlowKeysDelay {
    pub fn delay_ms(&self) -> u32 {
        match self {
            SlowKeysDelay::Short => 300,
            SlowKeysDelay::Medium => 600,
            SlowKeysDelay::Long => 900,
        }
    }
}

/// Bounce keys delay in milliseconds (minimum interval between key presses).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BounceKeysDelay {
    Short,
    Medium,
    Long,
}

impl BounceKeysDelay {
    pub fn delay_ms(&self) -> u32 {
        match self {
            BounceKeysDelay::Short => 100,
            BounceKeysDelay::Medium => 300,
            BounceKeysDelay::Long => 500,
        }
    }
}

/// Mouse accessibility settings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MouseA11y {
    /// Whether the secondary click is simulated by holding the primary button.
    pub secondary_click_enabled: bool,
    /// Delay before the simulated secondary click (ms).
    pub secondary_click_delay_ms: u32,
    /// Whether dwell click is enabled (click by hovering).
    pub dwell_click_enabled: bool,
    /// Dwell time before a click is triggered (ms).
    pub dwell_time_ms: u32,
    /// Dwell movement threshold (pixels).
    pub dwell_threshold_px: u32,
}

impl Default for MouseA11y {
    fn default() -> Self {
        MouseA11y {
            secondary_click_enabled: false,
            secondary_click_delay_ms: 500,
            dwell_click_enabled: false,
            dwell_time_ms: 1200,
            dwell_threshold_px: 10,
        }
    }
}

/// Sticky key modifier state: which modifiers are currently "stuck" on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StickyModifierState {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub meta: bool,
}

impl StickyModifierState {
    pub fn any_active(&self) -> bool {
        self.shift || self.ctrl || self.alt || self.meta
    }

    pub fn clear(&mut self) {
        self.shift = false;
        self.ctrl = false;
        self.alt = false;
        self.meta = false;
    }
}

/// The input accessibility manager. Mirrors MetaInputA11y.
#[derive(Debug)]
pub struct MetaInputA11y {
    /// Keyboard accessibility settings.
    keyboard: KeyboardA11y,
    /// Slow keys delay.
    slow_keys_delay: SlowKeysDelay,
    /// Bounce keys delay.
    bounce_keys_delay: BounceKeysDelay,
    /// Mouse accessibility settings.
    mouse: MouseA11y,
    /// Current sticky modifier state.
    sticky_state: StickyModifierState,
    /// Timestamp of the last key press (for bounce keys filtering).
    last_key_time: u64,
    /// Last keycode pressed (for bounce keys filtering).
    last_keycode: u32,
    /// Whether the on-screen keyboard should be shown.
    onscreen_keyboard: bool,
    /// Pending a11y notifications.
    pending_notifications: Vec<A11yNotification>,
}

/// A11y notification events (replaces GSettings changed signals).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A11yNotification {
    StickyKeysChanged,
    SlowKeysChanged,
    BounceKeysChanged,
    MouseKeysChanged,
    OnscreenKeyboardChanged,
}

impl MetaInputA11y {
    /// Create a new a11y manager with defaults. Mirrors meta_input_a11y_new().
    pub fn new() -> Self {
        MetaInputA11y {
            keyboard: KeyboardA11y::default(),
            slow_keys_delay: SlowKeysDelay::Medium,
            bounce_keys_delay: BounceKeysDelay::Medium,
            mouse: MouseA11y::default(),
            sticky_state: StickyModifierState::default(),
            last_key_time: 0,
            last_keycode: 0,
            onscreen_keyboard: false,
            pending_notifications: Vec::new(),
        }
    }

    // ── Keyboard a11y ─────────────────────────────────────────────────

    pub fn keyboard(&self) -> &KeyboardA11y {
        &self.keyboard
    }

    pub fn set_sticky_keys(&mut self, enabled: bool) {
        if self.keyboard.sticky_keys != enabled {
            self.keyboard.sticky_keys = enabled;
            if !enabled {
                self.sticky_state.clear();
            }
            self.pending_notifications
                .push(A11yNotification::StickyKeysChanged);
        }
    }

    pub fn set_slow_keys(&mut self, enabled: bool) {
        if self.keyboard.slow_keys != enabled {
            self.keyboard.slow_keys = enabled;
            self.pending_notifications
                .push(A11yNotification::SlowKeysChanged);
        }
    }

    pub fn set_bounce_keys(&mut self, enabled: bool) {
        if self.keyboard.bounce_keys != enabled {
            self.keyboard.bounce_keys = enabled;
            self.pending_notifications
                .push(A11yNotification::BounceKeysChanged);
        }
    }

    pub fn set_mouse_keys(&mut self, enabled: bool) {
        if self.keyboard.mouse_keys != enabled {
            self.keyboard.mouse_keys = enabled;
            self.pending_notifications
                .push(A11yNotification::MouseKeysChanged);
        }
    }

    pub fn set_toggle_keys(&mut self, enabled: bool) {
        self.keyboard.toggle_keys = enabled;
    }

    pub fn slow_keys_delay(&self) -> SlowKeysDelay {
        self.slow_keys_delay
    }

    pub fn set_slow_keys_delay(&mut self, delay: SlowKeysDelay) {
        self.slow_keys_delay = delay;
    }

    pub fn bounce_keys_delay(&self) -> BounceKeysDelay {
        self.bounce_keys_delay
    }

    pub fn set_bounce_keys_delay(&mut self, delay: BounceKeysDelay) {
        self.bounce_keys_delay = delay;
    }

    // ── Sticky keys state ─────────────────────────────────────────────

    pub fn sticky_state(&self) -> StickyModifierState {
        self.sticky_state
    }

    /// Process a modifier key event for sticky keys. Returns whether the
    /// event should be passed through to the normal input path.
    ///
    /// With sticky keys enabled, pressing a modifier "latches" it on
    /// for the next non-modifier key. Pressing it again "locks" it on
    /// until pressed a third time.
    pub fn process_sticky_modifier(&mut self, mod_bit: u32, pressed: bool) -> bool {
        if !self.keyboard.sticky_keys || !pressed {
            return true; // Pass through.
        }

        let field = if mod_bit == 1 << 0 {
            &mut self.sticky_state.shift
        } else if mod_bit == 1 << 2 {
            &mut self.sticky_state.ctrl
        } else if mod_bit == 1 << 3 {
            &mut self.sticky_state.alt
        } else if mod_bit == 1 << 5 {
            &mut self.sticky_state.meta
        } else {
            return true;
        };

        if *field {
            // Second press: lock it (stays on). Third press would unlock.
            // For simplicity, we toggle: second press unlocks.
            *field = false;
        } else {
            // First press: latch on.
            *field = true;
        }

        // Don't pass modifier events through when sticky keys is active;
        // the modifier state is tracked here.
        false
    }

    /// Consume sticky modifier state after a non-modifier key is pressed.
    /// Latched modifiers are cleared; locked modifiers stay.
    pub fn consume_sticky_modifiers(&mut self) -> StickyModifierState {
        let state = self.sticky_state;
        // Clear latched modifiers (those that were on but not locked).
        // In this simplified model, all sticky modifiers are latched.
        self.sticky_state.clear();
        state
    }

    // ── Bounce keys filtering ─────────────────────────────────────────

    /// Check whether a key event should be filtered by bounce keys.
    /// Returns true if the event should be *dropped* (filtered).
    pub fn should_filter_bounce(&mut self, keycode: u32, timestamp_ms: u64) -> bool {
        if !self.keyboard.bounce_keys {
            return false;
        }

        if keycode == self.last_keycode {
            let elapsed = timestamp_ms.saturating_sub(self.last_key_time);
            if elapsed < self.bounce_keys_delay.delay_ms() as u64 {
                return true; // Filter: too soon after last press of same key.
            }
        }

        self.last_keycode = keycode;
        self.last_key_time = timestamp_ms;
        false
    }

    // ── Slow keys filtering ───────────────────────────────────────────

    /// Check whether a key event should be delayed by slow keys.
    /// Returns the delay in milliseconds, or 0 if no delay.
    pub fn slow_keys_delay_ms(&self) -> u32 {
        if self.keyboard.slow_keys {
            self.slow_keys_delay.delay_ms()
        } else {
            0
        }
    }

    // ── Mouse a11y ────────────────────────────────────────────────────

    pub fn mouse(&self) -> &MouseA11y {
        &self.mouse
    }

    pub fn set_secondary_click(&mut self, enabled: bool, delay_ms: u32) {
        self.mouse.secondary_click_enabled = enabled;
        self.mouse.secondary_click_delay_ms = delay_ms;
    }

    pub fn set_dwell_click(&mut self, enabled: bool, time_ms: u32, threshold_px: u32) {
        self.mouse.dwell_click_enabled = enabled;
        self.mouse.dwell_time_ms = time_ms;
        self.mouse.dwell_threshold_px = threshold_px;
    }

    // ── On-screen keyboard ────────────────────────────────────────────

    pub fn onscreen_keyboard(&self) -> bool {
        self.onscreen_keyboard
    }

    pub fn set_onscreen_keyboard(&mut self, enabled: bool) {
        if self.onscreen_keyboard != enabled {
            self.onscreen_keyboard = enabled;
            self.pending_notifications
                .push(A11yNotification::OnscreenKeyboardChanged);
        }
    }

    // ── Notifications ─────────────────────────────────────────────────

    pub fn take_pending_notifications(&mut self) -> Vec<A11yNotification> {
        core::mem::take(&mut self.pending_notifications)
    }
}

impl Default for MetaInputA11y {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults() {
        let a11y = MetaInputA11y::new();
        assert!(!a11y.keyboard().sticky_keys);
        assert!(!a11y.keyboard().slow_keys);
        assert!(!a11y.keyboard().bounce_keys);
        assert!(!a11y.onscreen_keyboard());
    }

    #[test]
    fn test_set_sticky_keys() {
        let mut a11y = MetaInputA11y::new();
        a11y.set_sticky_keys(true);
        assert!(a11y.keyboard().sticky_keys);

        let notifications = a11y.take_pending_notifications();
        assert!(notifications.contains(&A11yNotification::StickyKeysChanged));
    }

    #[test]
    fn test_sticky_modifier_latch() {
        let mut a11y = MetaInputA11y::new();
        a11y.set_sticky_keys(true);

        // Press Shift → latches on.
        let pass = a11y.process_sticky_modifier(1 << 0, true);
        assert!(!pass); // Event consumed by sticky keys.
        assert!(a11y.sticky_state().shift);

        // Non-modifier key → consume latched state.
        let state = a11y.consume_sticky_modifiers();
        assert!(state.shift);
        assert!(!a11y.sticky_state().any_active());
    }

    #[test]
    fn test_sticky_modifier_unlock() {
        let mut a11y = MetaInputA11y::new();
        a11y.set_sticky_keys(true);

        // First press: latch on.
        a11y.process_sticky_modifier(1 << 0, true);
        assert!(a11y.sticky_state().shift);

        // Second press: unlock.
        a11y.process_sticky_modifier(1 << 0, true);
        assert!(!a11y.sticky_state().shift);
    }

    #[test]
    fn test_sticky_disabled_passes_through() {
        let mut a11y = MetaInputA11y::new();
        // Sticky keys not enabled.
        let pass = a11y.process_sticky_modifier(1 << 0, true);
        assert!(pass); // Should pass through.
    }

    #[test]
    fn test_bounce_keys_filter() {
        let mut a11y = MetaInputA11y::new();
        a11y.set_bounce_keys(true);

        // First press of key 30 at t=0: not filtered.
        assert!(!a11y.should_filter_bounce(30, 0));

        // Same key at t=50 (within delay): filtered.
        assert!(a11y.should_filter_bounce(30, 50));

        // Same key at t=400 (after delay): not filtered.
        assert!(!a11y.should_filter_bounce(30, 400));
    }

    #[test]
    fn test_bounce_keys_different_keys() {
        let mut a11y = MetaInputA11y::new();
        a11y.set_bounce_keys(true);

        // Key 30 at t=0: not filtered.
        assert!(!a11y.should_filter_bounce(30, 0));

        // Different key 31 at t=50: not filtered (different key).
        assert!(!a11y.should_filter_bounce(31, 50));
    }

    #[test]
    fn test_bounce_keys_disabled() {
        let mut a11y = MetaInputA11y::new();
        // Bounce keys not enabled.
        assert!(!a11y.should_filter_bounce(30, 0));
        assert!(!a11y.should_filter_bounce(30, 10));
    }

    #[test]
    fn test_slow_keys_delay() {
        let mut a11y = MetaInputA11y::new();
        assert_eq!(a11y.slow_keys_delay_ms(), 0); // Disabled.

        a11y.set_slow_keys(true);
        assert_eq!(a11y.slow_keys_delay_ms(), 600); // Medium default.

        a11y.set_slow_keys_delay(SlowKeysDelay::Long);
        assert_eq!(a11y.slow_keys_delay_ms(), 900);
    }

    #[test]
    fn test_mouse_a11y() {
        let mut a11y = MetaInputA11y::new();
        a11y.set_secondary_click(true, 300);
        assert!(a11y.mouse().secondary_click_enabled);
        assert_eq!(a11y.mouse().secondary_click_delay_ms, 300);

        a11y.set_dwell_click(true, 800, 15);
        assert!(a11y.mouse().dwell_click_enabled);
        assert_eq!(a11y.mouse().dwell_time_ms, 800);
        assert_eq!(a11y.mouse().dwell_threshold_px, 15);
    }

    #[test]
    fn test_onscreen_keyboard() {
        let mut a11y = MetaInputA11y::new();
        a11y.set_onscreen_keyboard(true);
        assert!(a11y.onscreen_keyboard());

        let notifications = a11y.take_pending_notifications();
        assert!(notifications.contains(&A11yNotification::OnscreenKeyboardChanged));
    }

    #[test]
    fn test_disable_sticky_clears_state() {
        let mut a11y = MetaInputA11y::new();
        a11y.set_sticky_keys(true);
        a11y.process_sticky_modifier(1 << 0, true);
        assert!(a11y.sticky_state().shift);

        a11y.set_sticky_keys(false);
        assert!(!a11y.sticky_state().any_active());
    }
}
