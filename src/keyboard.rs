//! # RustOS Keyboard Input Handler
//!
//! This module provides comprehensive keyboard input handling for RustOS, including:
//! - PS/2 keyboard interrupt handling
//! - Scancode to ASCII conversion
//! - Key event buffering with circular buffer
//! - Integration with desktop environment
//! - Support for special keys (arrows, function keys, etc.)

use core::fmt;
use heapless::spsc::Queue;
use lazy_static::lazy_static;
use pc_keyboard::{layouts, DecodedKey, HandleControl, KeyCode, Keyboard, ScancodeSet1};
use spin::Mutex;
use x86_64::instructions::port::Port;

/// Maximum number of key events in the buffer
const KEY_BUFFER_SIZE: usize = 64;

// Global key event queue - properly synchronized
lazy_static! {
    static ref KEY_EVENT_QUEUE: Mutex<Queue<KeyEvent, KEY_BUFFER_SIZE>> = Mutex::new(Queue::new());
}

/// Keyboard scan codes for special keys
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SpecialKey {
    Escape = 0x01,
    Backspace = 0x0E,
    Tab = 0x0F,
    Enter = 0x1C,
    LeftCtrl = 0x1D,
    LeftShift = 0x2A,
    RightShift = 0x36,
    LeftAlt = 0x38,
    Space = 0x39,
    CapsLock = 0x3A,
    F1 = 0x3B,
    F2 = 0x3C,
    F3 = 0x3D,
    F4 = 0x3E,
    F5 = 0x3F,
    F6 = 0x40,
    F7 = 0x41,
    F8 = 0x42,
    F9 = 0x43,
    F10 = 0x44,
    F11 = 0x57,
    F12 = 0x58,
    NumLock = 0x45,
    ScrollLock = 0x46,
    Home = 0x47,
    ArrowUp = 0x48,
    PageUp = 0x49,
    ArrowLeft = 0x4B,
    ArrowRight = 0x4D,
    End = 0x4F,
    ArrowDown = 0x50,
    PageDown = 0x51,
    Insert = 0x52,
    Delete = 0x53,
}

impl SpecialKey {
    /// Convert scancode to special key
    pub fn from_scancode(scancode: u8) -> Option<Self> {
        match scancode {
            0x01 => Some(Self::Escape),
            0x0E => Some(Self::Backspace),
            0x0F => Some(Self::Tab),
            0x1C => Some(Self::Enter),
            0x1D => Some(Self::LeftCtrl),
            0x2A => Some(Self::LeftShift),
            0x36 => Some(Self::RightShift),
            0x38 => Some(Self::LeftAlt),
            0x39 => Some(Self::Space),
            0x3A => Some(Self::CapsLock),
            0x3B => Some(Self::F1),
            0x3C => Some(Self::F2),
            0x3D => Some(Self::F3),
            0x3E => Some(Self::F4),
            0x3F => Some(Self::F5),
            0x40 => Some(Self::F6),
            0x41 => Some(Self::F7),
            0x42 => Some(Self::F8),
            0x43 => Some(Self::F9),
            0x44 => Some(Self::F10),
            0x57 => Some(Self::F11),
            0x58 => Some(Self::F12),
            0x45 => Some(Self::NumLock),
            0x46 => Some(Self::ScrollLock),
            0x47 => Some(Self::Home),
            0x48 => Some(Self::ArrowUp),
            0x49 => Some(Self::PageUp),
            0x4B => Some(Self::ArrowLeft),
            0x4D => Some(Self::ArrowRight),
            0x4F => Some(Self::End),
            0x50 => Some(Self::ArrowDown),
            0x51 => Some(Self::PageDown),
            0x52 => Some(Self::Insert),
            0x53 => Some(Self::Delete),
            _ => None,
        }
    }
}

/// Key event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    /// Character key pressed (letters, numbers, symbols)
    CharacterPress(char),
    /// Character key released
    CharacterRelease(char),
    /// Special key pressed (arrows, function keys, etc.)
    SpecialPress(SpecialKey),
    /// Special key released
    SpecialRelease(SpecialKey),
    /// Raw key code for unhandled keys
    RawPress(u8),
    /// Raw key code release for unhandled keys
    RawRelease(u8),
}

impl KeyEvent {
    /// Check if this is a key press event
    pub fn is_press(&self) -> bool {
        matches!(
            self,
            KeyEvent::CharacterPress(_) | KeyEvent::SpecialPress(_) | KeyEvent::RawPress(_)
        )
    }

    /// Check if this is a key release event
    pub fn is_release(&self) -> bool {
        matches!(
            self,
            KeyEvent::CharacterRelease(_) | KeyEvent::SpecialRelease(_) | KeyEvent::RawRelease(_)
        )
    }

    /// Get the character if this is a character event
    pub fn as_char(&self) -> Option<char> {
        match self {
            KeyEvent::CharacterPress(c) | KeyEvent::CharacterRelease(c) => Some(*c),
            _ => None,
        }
    }

    /// Get the special key if this is a special key event
    pub fn as_special(&self) -> Option<SpecialKey> {
        match self {
            KeyEvent::SpecialPress(k) | KeyEvent::SpecialRelease(k) => Some(*k),
            _ => None,
        }
    }
}

impl fmt::Display for KeyEvent {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            KeyEvent::CharacterPress(c) => write!(f, "Char+'{}' ", c),
            KeyEvent::CharacterRelease(c) => write!(f, "Char-'{}' ", c),
            KeyEvent::SpecialPress(k) => write!(f, "Special+{:?} ", k),
            KeyEvent::SpecialRelease(k) => write!(f, "Special-{:?} ", k),
            KeyEvent::RawPress(code) => write!(f, "Raw+{:02X} ", code),
            KeyEvent::RawRelease(code) => write!(f, "Raw-{:02X} ", code),
        }
    }
}

/// Keyboard modifier state
#[derive(Debug, Clone, Copy, Default)]
pub struct ModifierState {
    pub left_shift: bool,
    pub right_shift: bool,
    pub left_ctrl: bool,
    pub right_ctrl: bool,
    pub left_alt: bool,
    pub right_alt: bool,
    pub caps_lock: bool,
    pub num_lock: bool,
    pub scroll_lock: bool,
}

impl ModifierState {
    /// Check if any shift key is pressed
    pub fn shift(&self) -> bool {
        self.left_shift || self.right_shift
    }

    /// Check if any ctrl key is pressed
    pub fn ctrl(&self) -> bool {
        self.left_ctrl || self.right_ctrl
    }

    /// Check if any alt key is pressed
    pub fn alt(&self) -> bool {
        self.left_alt || self.right_alt
    }

    /// Update modifier state based on key event
    pub fn update(&mut self, event: KeyEvent) {
        match event {
            KeyEvent::SpecialPress(SpecialKey::LeftShift) => self.left_shift = true,
            KeyEvent::SpecialRelease(SpecialKey::LeftShift) => self.left_shift = false,
            KeyEvent::SpecialPress(SpecialKey::RightShift) => self.right_shift = true,
            KeyEvent::SpecialRelease(SpecialKey::RightShift) => self.right_shift = false,
            KeyEvent::SpecialPress(SpecialKey::LeftCtrl) => self.left_ctrl = true,
            KeyEvent::SpecialRelease(SpecialKey::LeftCtrl) => self.left_ctrl = false,
            KeyEvent::SpecialPress(SpecialKey::LeftAlt) => self.left_alt = true,
            KeyEvent::SpecialRelease(SpecialKey::LeftAlt) => self.left_alt = false,
            KeyEvent::SpecialPress(SpecialKey::CapsLock) => self.caps_lock = !self.caps_lock,
            KeyEvent::SpecialPress(SpecialKey::NumLock) => self.num_lock = !self.num_lock,
            KeyEvent::SpecialPress(SpecialKey::ScrollLock) => self.scroll_lock = !self.scroll_lock,
            _ => {}
        }
    }
}

/// Keyboard statistics
#[derive(Debug, Clone, Copy, Default)]
pub struct KeyboardStats {
    pub total_keypresses: u64,
    pub total_releases: u64,
    pub character_keys: u64,
    pub special_keys: u64,
    pub raw_keys: u64,
    pub buffer_overflows: u64,
}

/// Main keyboard handler structure
pub struct KeyboardHandler {
    pc_keyboard: Keyboard<layouts::Us104Key, ScancodeSet1>,
    modifiers: ModifierState,
    stats: KeyboardStats,
    last_event: Option<KeyEvent>,
}

impl KeyboardHandler {
    /// Create a new keyboard handler
    fn new() -> Self {
        Self {
            pc_keyboard: Keyboard::new(layouts::Us104Key, ScancodeSet1, HandleControl::Ignore),
            modifiers: ModifierState::default(),
            stats: KeyboardStats::default(),
            last_event: None,
        }
    }

    /// Process a scancode from the keyboard
    pub fn process_scancode(&mut self, scancode: u8) -> Result<(), &'static str> {
        // Check for key release (bit 7 set)
        let is_release = (scancode & 0x80) != 0;
        let base_scancode = scancode & 0x7F;

        // Try to process with pc-keyboard crate first
        if let Ok(Some(key_event)) = self.pc_keyboard.add_byte(scancode) {
            if let Some(key) = self.pc_keyboard.process_keyevent(key_event) {
                let event = match key {
                    DecodedKey::Unicode(character) => {
                        if is_release {
                            KeyEvent::CharacterRelease(character)
                        } else {
                            KeyEvent::CharacterPress(character)
                        }
                    }
                    DecodedKey::RawKey(raw_key) => {
                        // Convert raw key to special key if possible
                        match raw_key {
                            KeyCode::ArrowUp => {
                                if is_release {
                                    KeyEvent::SpecialRelease(SpecialKey::ArrowUp)
                                } else {
                                    KeyEvent::SpecialPress(SpecialKey::ArrowUp)
                                }
                            }
                            KeyCode::ArrowDown => {
                                if is_release {
                                    KeyEvent::SpecialRelease(SpecialKey::ArrowDown)
                                } else {
                                    KeyEvent::SpecialPress(SpecialKey::ArrowDown)
                                }
                            }
                            KeyCode::ArrowLeft => {
                                if is_release {
                                    KeyEvent::SpecialRelease(SpecialKey::ArrowLeft)
                                } else {
                                    KeyEvent::SpecialPress(SpecialKey::ArrowLeft)
                                }
                            }
                            KeyCode::ArrowRight => {
                                if is_release {
                                    KeyEvent::SpecialRelease(SpecialKey::ArrowRight)
                                } else {
                                    KeyEvent::SpecialPress(SpecialKey::ArrowRight)
                                }
                            }
                            KeyCode::Escape => {
                                if is_release {
                                    KeyEvent::SpecialRelease(SpecialKey::Escape)
                                } else {
                                    KeyEvent::SpecialPress(SpecialKey::Escape)
                                }
                            }
                            KeyCode::Enter => {
                                if is_release {
                                    KeyEvent::SpecialRelease(SpecialKey::Enter)
                                } else {
                                    KeyEvent::SpecialPress(SpecialKey::Enter)
                                }
                            }
                            KeyCode::Backspace => {
                                if is_release {
                                    KeyEvent::SpecialRelease(SpecialKey::Backspace)
                                } else {
                                    KeyEvent::SpecialPress(SpecialKey::Backspace)
                                }
                            }
                            KeyCode::Tab => {
                                if is_release {
                                    KeyEvent::SpecialRelease(SpecialKey::Tab)
                                } else {
                                    KeyEvent::SpecialPress(SpecialKey::Tab)
                                }
                            }
                            _ => {
                                // Fall back to raw scancode
                                if is_release {
                                    KeyEvent::RawRelease(base_scancode)
                                } else {
                                    KeyEvent::RawPress(base_scancode)
                                }
                            }
                        }
                    }
                };

                return self.add_event(event);
            }
        }

        // Fall back to direct scancode processing for special keys
        if let Some(special_key) = SpecialKey::from_scancode(base_scancode) {
            let event = if is_release {
                KeyEvent::SpecialRelease(special_key)
            } else {
                KeyEvent::SpecialPress(special_key)
            };
            return self.add_event(event);
        }

        // Unknown key - store as raw
        let event = if is_release {
            KeyEvent::RawRelease(base_scancode)
        } else {
            KeyEvent::RawPress(base_scancode)
        };

        self.add_event(event)
    }

    /// Add a key event to the buffer
    fn add_event(&mut self, event: KeyEvent) -> Result<(), &'static str> {
        // Update statistics
        if event.is_press() {
            self.stats.total_keypresses += 1;
        } else {
            self.stats.total_releases += 1;
        }

        match event {
            KeyEvent::CharacterPress(_) | KeyEvent::CharacterRelease(_) => {
                self.stats.character_keys += 1;
            }
            KeyEvent::SpecialPress(_) | KeyEvent::SpecialRelease(_) => {
                self.stats.special_keys += 1;
            }
            KeyEvent::RawPress(_) | KeyEvent::RawRelease(_) => {
                self.stats.raw_keys += 1;
            }
        }

        // Update modifier state
        self.modifiers.update(event);

        // Track last event for input manager forwarding
        self.last_event = Some(event);

        // Try to add to global buffer
        let mut queue = KEY_EVENT_QUEUE.lock();
        match queue.enqueue(event) {
            Ok(()) => Ok(()),
            Err(_) => {
                self.stats.buffer_overflows += 1;
                Err("Keyboard buffer overflow")
            }
        }
    }

    /// Get the next key event from the buffer
    pub fn get_key_event(&mut self) -> Option<KeyEvent> {
        let mut queue = KEY_EVENT_QUEUE.lock();
        queue.dequeue()
    }

    /// Check if there are pending key events
    pub fn has_key_events(&self) -> bool {
        let queue = KEY_EVENT_QUEUE.lock();
        queue.peek().is_some()
    }

    /// Get the last generated key event (for input manager forwarding)
    pub fn last_event(&self) -> Option<KeyEvent> {
        self.last_event
    }

    /// Get current modifier state
    pub fn modifier_state(&self) -> ModifierState {
        self.modifiers
    }

    /// Get keyboard statistics
    pub fn stats(&self) -> KeyboardStats {
        self.stats
    }

    /// Clear the key event buffer
    pub fn clear_buffer(&mut self) {
        let mut queue = KEY_EVENT_QUEUE.lock();
        while queue.dequeue().is_some() {}
    }

    /// Read a character (blocking)
    pub fn read_char(&mut self) -> Option<char> {
        while let Some(event) = self.get_key_event() {
            if let KeyEvent::CharacterPress(c) = event {
                return Some(c);
            }
        }
        None
    }

    /// Read a line of input (blocking)
    pub fn read_line(&mut self, buffer: &mut [u8]) -> usize {
        let mut pos = 0;

        loop {
            if let Some(event) = self.get_key_event() {
                match event {
                    KeyEvent::CharacterPress(c) => {
                        if c == '\n' || c == '\r' {
                            // Enter pressed - end line
                            break;
                        } else if c.is_ascii() && pos < buffer.len() {
                            buffer[pos] = c as u8;
                            pos += 1;
                        }
                    }
                    KeyEvent::SpecialPress(SpecialKey::Backspace) => {
                        if pos > 0 {
                            pos -= 1;
                        }
                    }
                    KeyEvent::SpecialPress(SpecialKey::Enter) => {
                        break;
                    }
                    _ => {}
                }
            }
        }

        pos
    }
}

// Global keyboard handler instance
lazy_static! {
    static ref KEYBOARD_HANDLER: Mutex<KeyboardHandler> = Mutex::new(KeyboardHandler::new());
}

/// Initialize the keyboard system
pub fn init() {
    // Keyboard is initialized lazily when first accessed
}

/// Process a scancode from the keyboard interrupt handler
pub fn process_scancode(scancode: u8) -> Result<(), &'static str> {
    let mut handler = KEYBOARD_HANDLER.lock();
    handler.process_scancode(scancode)?;

    // Forward the latest event to the input manager so the modern desktop
    // loop (which reads from drivers::get_input_event()) receives keyboard
    // events. The handler.process_scancode call above enqueued the event
    // into KEY_EVENT_QUEUE; we peek at it to determine press/release for
    // the InputEvent variant, then also push it to the input manager.
    // We must not dequeue here — the legacy desktop loop reads from
    // keyboard::get_key_event() which pops from the same queue.
    // Instead, re-derive the event from the scancode for the input manager.
    let is_release = (scancode & 0x80) != 0;
    if let Some(last_event) = handler.last_event() {
        crate::drivers::input_manager::handle_keyboard_event(last_event, !is_release);
    }

    Ok(())
}

/// Get the next key event
pub fn get_key_event() -> Option<KeyEvent> {
    let mut handler = KEYBOARD_HANDLER.lock();
    handler.get_key_event()
}

/// Check if there are pending key events
pub fn has_key_events() -> bool {
    let handler = KEYBOARD_HANDLER.lock();
    handler.has_key_events()
}

/// Get current modifier state
pub fn modifier_state() -> ModifierState {
    let handler = KEYBOARD_HANDLER.lock();
    handler.modifier_state()
}

/// Get keyboard statistics
pub fn get_stats() -> KeyboardStats {
    let handler = KEYBOARD_HANDLER.lock();
    handler.stats()
}

/// Clear the key event buffer
pub fn clear_buffer() {
    let mut handler = KEYBOARD_HANDLER.lock();
    handler.clear_buffer();
}

/// Read a character (non-blocking)
pub fn read_char() -> Option<char> {
    let mut handler = KEYBOARD_HANDLER.lock();
    handler.read_char()
}

/// Read a line of input (blocking)
pub fn read_line(buffer: &mut [u8]) -> usize {
    // IMPORTANT: do NOT hold the KEYBOARD_HANDLER lock across the wait. Input only
    // arrives via the PS/2 IRQ (handle_keyboard_interrupt -> process_scancode ->
    // KEYBOARD_HANDLER.lock()); holding the lock here would prevent the IRQ from
    // ever enqueuing keys, deadlocking forever. Instead, poll one event at a time
    // via get_key_event(), which locks AND unlocks the handler each call, so the
    // IRQ can take the lock between iterations.
    let mut pos = 0;

    loop {
        match get_key_event() {
            Some(KeyEvent::CharacterPress(c)) => {
                if c == '\n' || c == '\r' {
                    break;
                } else if c.is_ascii() && pos < buffer.len() {
                    buffer[pos] = c as u8;
                    pos += 1;
                }
            }
            Some(KeyEvent::SpecialPress(SpecialKey::Backspace)) => {
                if pos > 0 {
                    pos -= 1;
                }
            }
            Some(KeyEvent::SpecialPress(SpecialKey::Enter)) => {
                break;
            }
            Some(_) => {}
            None => {
                // Nothing buffered yet: halt until the next interrupt instead of
                // spinning, leaving the lock free for the IRQ to enqueue input.
                x86_64::instructions::hlt();
            }
        }
    }

    pos
}

/// Wait for a specific key press
pub fn wait_for_key(target_key: SpecialKey) -> bool {
    loop {
        if let Some(event) = get_key_event() {
            if let KeyEvent::SpecialPress(key) = event {
                if key == target_key {
                    return true;
                }
            }
        }
    }
}

/// Wait for any key press
pub fn wait_for_any_key() -> KeyEvent {
    loop {
        if let Some(event) = get_key_event() {
            if event.is_press() {
                return event;
            }
        }
    }
}

/// Check if a specific key is currently pressed (requires tracking)
pub fn is_key_pressed(key: SpecialKey) -> bool {
    let modifiers = modifier_state();
    match key {
        SpecialKey::LeftShift => modifiers.left_shift,
        SpecialKey::RightShift => modifiers.right_shift,
        SpecialKey::LeftCtrl => modifiers.left_ctrl,
        SpecialKey::LeftAlt => modifiers.left_alt,
        SpecialKey::CapsLock => modifiers.caps_lock,
        SpecialKey::NumLock => modifiers.num_lock,
        SpecialKey::ScrollLock => modifiers.scroll_lock,
        _ => false, // For other keys, we'd need a more complex tracking system
    }
}

/// Update the keyboard interrupt handler to use this module
pub fn handle_keyboard_interrupt() {
    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };

    // Process the scancode
    if let Err(_e) = process_scancode(scancode) {
        // Handle error silently in production
    }
}

// Integration with existing interrupt system
// Note: handle_keyboard_interrupt is called directly from interrupts.rs

/// Get scancode from keyboard buffer (non-blocking)
pub fn get_scancode() -> Option<u8> {
    // Return next available scancode if any
    None // Placeholder - would integrate with keyboard interrupt handler
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test_case]
    fn test_special_key_conversion() {
        assert_eq!(SpecialKey::from_scancode(0x01), Some(SpecialKey::Escape));
        assert_eq!(SpecialKey::from_scancode(0x48), Some(SpecialKey::ArrowUp));
        assert_eq!(SpecialKey::from_scancode(0xFF), None);
    }

    #[test_case]
    fn test_key_event_properties() {
        let press = KeyEvent::CharacterPress('a');
        let release = KeyEvent::CharacterRelease('a');

        assert!(press.is_press());
        assert!(!press.is_release());
        assert!(!release.is_press());
        assert!(release.is_release());

        assert_eq!(press.as_char(), Some('a'));
        assert_eq!(release.as_char(), Some('a'));
    }

    #[test_case]
    fn test_modifier_state() {
        let mut modifiers = ModifierState::default();

        modifiers.update(KeyEvent::SpecialPress(SpecialKey::LeftShift));
        assert!(modifiers.shift());

        modifiers.update(KeyEvent::SpecialRelease(SpecialKey::LeftShift));
        assert!(!modifiers.shift());

        modifiers.update(KeyEvent::SpecialPress(SpecialKey::CapsLock));
        assert!(modifiers.caps_lock);

        // CapsLock toggles
        modifiers.update(KeyEvent::SpecialPress(SpecialKey::CapsLock));
        assert!(!modifiers.caps_lock);
    }
}
