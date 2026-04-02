//! Input Manager - Unified Input Device Abstraction
//!
//! This module provides a unified interface for all input devices (keyboard, mouse, etc.)
//! and manages cursor position, button states, and event routing to the desktop environment.

use spin::Mutex;
use heapless::spsc::Queue;
use crate::keyboard::KeyEvent;
use crate::drivers::ps2_mouse::{MousePacket, MouseButtons};

/// Maximum number of input events in the queue
const INPUT_EVENT_QUEUE_SIZE: usize = 128;

/// Mouse button identifiers
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Right,
    Middle,
    Button4,
    Button5,
}

/// Unified input events
#[derive(Debug, Clone, Copy)]
pub enum InputEvent {
    /// Keyboard key pressed
    KeyPress(KeyEvent),
    /// Keyboard key released
    KeyRelease(KeyEvent),
    /// Mouse moved (absolute position)
    MouseMove { x: usize, y: usize },
    /// Mouse button pressed
    MouseButtonDown { button: MouseButton, x: usize, y: usize },
    /// Mouse button released
    MouseButtonUp { button: MouseButton, x: usize, y: usize },
    /// Mouse wheel scrolled
    MouseScroll { delta: i8, x: usize, y: usize },
}

/// Cursor bounds for screen
#[derive(Debug, Clone, Copy)]
pub struct CursorBounds {
    pub max_x: usize,
    pub max_y: usize,
}

impl CursorBounds {
    pub fn new(max_x: usize, max_y: usize) -> Self {
        Self { max_x, max_y }
    }

    /// Default VGA text mode bounds
    pub fn vga_text() -> Self {
        Self { max_x: 79, max_y: 24 }
    }

    /// Standard VGA graphics bounds (640x480)
    pub fn vga_graphics() -> Self {
        Self { max_x: 639, max_y: 479 }
    }

    /// Constrain a position to within bounds
    pub fn constrain(&self, x: i32, y: i32) -> (usize, usize) {
        let x = x.max(0).min(self.max_x as i32) as usize;
        let y = y.max(0).min(self.max_y as i32) as usize;
        (x, y)
    }
}

/// Input manager state
struct InputManagerState {
    /// Current cursor position
    cursor_x: usize,
    cursor_y: usize,

    /// Cursor bounds
    bounds: CursorBounds,

    /// Current button states
    buttons: MouseButtons,

    /// Input event queue
    event_queue: Queue<InputEvent, INPUT_EVENT_QUEUE_SIZE>,

    /// Statistics
    events_queued: usize,
    events_dropped: usize,

    /// Mouse sensitivity multiplier (fixed-point: 256 = 1.0x)
    mouse_sensitivity: u16,
}

impl InputManagerState {
    fn new() -> Self {
        Self {
            cursor_x: 320,
            cursor_y: 240,
            bounds: CursorBounds::vga_graphics(),
            buttons: MouseButtons::new(),
            event_queue: Queue::new(),
            events_queued: 0,
            events_dropped: 0,
            mouse_sensitivity: 256, // 1.0x default
        }
    }

    /// Queue an input event
    fn queue_event(&mut self, event: InputEvent) -> bool {
        if self.event_queue.enqueue(event).is_ok() {
            self.events_queued += 1;
            true
        } else {
            self.events_dropped += 1;
            false
        }
    }

    /// Update cursor position with delta movement
    fn update_cursor(&mut self, delta_x: i16, delta_y: i16) -> (usize, usize) {
        // Apply sensitivity
        let scaled_x = (delta_x as i32 * self.mouse_sensitivity as i32) / 256;
        let scaled_y = (delta_y as i32 * self.mouse_sensitivity as i32) / 256;

        // Update position
        let new_x = self.cursor_x as i32 + scaled_x;
        let new_y = self.cursor_y as i32 - scaled_y; // Invert Y (PS/2 mouse Y is inverted)

        // Constrain to bounds
        let (new_x, new_y) = self.bounds.constrain(new_x, new_y);

        self.cursor_x = new_x;
        self.cursor_y = new_y;

        (new_x, new_y)
    }

    /// Process a mouse packet and generate events
    fn process_mouse_packet(&mut self, packet: MousePacket) {
        // Update cursor position if there's movement
        let moved = packet.x_movement != 0 || packet.y_movement != 0;
        if moved {
            let (x, y) = self.update_cursor(packet.x_movement, packet.y_movement);
            self.queue_event(InputEvent::MouseMove { x, y });
        }

        // Check for button state changes
        let old_buttons = self.buttons;
        let new_buttons = packet.buttons;

        // Left button
        if new_buttons.left != old_buttons.left {
            let event = if new_buttons.left {
                InputEvent::MouseButtonDown {
                    button: MouseButton::Left,
                    x: self.cursor_x,
                    y: self.cursor_y
                }
            } else {
                InputEvent::MouseButtonUp {
                    button: MouseButton::Left,
                    x: self.cursor_x,
                    y: self.cursor_y
                }
            };
            self.queue_event(event);
        }

        // Right button
        if new_buttons.right != old_buttons.right {
            let event = if new_buttons.right {
                InputEvent::MouseButtonDown {
                    button: MouseButton::Right,
                    x: self.cursor_x,
                    y: self.cursor_y
                }
            } else {
                InputEvent::MouseButtonUp {
                    button: MouseButton::Right,
                    x: self.cursor_x,
                    y: self.cursor_y
                }
            };
            self.queue_event(event);
        }

        // Middle button
        if new_buttons.middle != old_buttons.middle {
            let event = if new_buttons.middle {
                InputEvent::MouseButtonDown {
                    button: MouseButton::Middle,
                    x: self.cursor_x,
                    y: self.cursor_y
                }
            } else {
                InputEvent::MouseButtonUp {
                    button: MouseButton::Middle,
                    x: self.cursor_x,
                    y: self.cursor_y
                }
            };
            self.queue_event(event);
        }

        // Button 4
        if new_buttons.button4 != old_buttons.button4 {
            let event = if new_buttons.button4 {
                InputEvent::MouseButtonDown {
                    button: MouseButton::Button4,
                    x: self.cursor_x,
                    y: self.cursor_y
                }
            } else {
                InputEvent::MouseButtonUp {
                    button: MouseButton::Button4,
                    x: self.cursor_x,
                    y: self.cursor_y
                }
            };
            self.queue_event(event);
        }

        // Button 5
        if new_buttons.button5 != old_buttons.button5 {
            let event = if new_buttons.button5 {
                InputEvent::MouseButtonDown {
                    button: MouseButton::Button5,
                    x: self.cursor_x,
                    y: self.cursor_y
                }
            } else {
                InputEvent::MouseButtonUp {
                    button: MouseButton::Button5,
                    x: self.cursor_x,
                    y: self.cursor_y
                }
            };
            self.queue_event(event);
        }

        // Scroll wheel
        if packet.z_movement != 0 {
            self.queue_event(InputEvent::MouseScroll {
                delta: packet.z_movement,
                x: self.cursor_x,
                y: self.cursor_y,
            });
        }

        // Update button state
        self.buttons = new_buttons;
    }
}

/// Global input manager
static INPUT_MANAGER: Mutex<Option<InputManagerState>> = Mutex::new(None);

/// Initialize the input manager
pub fn init() {
    *INPUT_MANAGER.lock() = Some(InputManagerState::new());
}

/// Check if input manager is initialized
pub fn is_initialized() -> bool {
    INPUT_MANAGER.lock().is_some()
}

/// Set cursor bounds (e.g., when switching video modes)
pub fn set_cursor_bounds(max_x: usize, max_y: usize) {
    if let Some(ref mut manager) = *INPUT_MANAGER.lock() {
        manager.bounds = CursorBounds::new(max_x, max_y);

        // Constrain current cursor position to new bounds
        let (x, y) = manager.bounds.constrain(
            manager.cursor_x as i32,
            manager.cursor_y as i32
        );
        manager.cursor_x = x;
        manager.cursor_y = y;
    }
}

/// Set cursor position (absolute)
pub fn set_cursor_position(x: usize, y: usize) {
    if let Some(ref mut manager) = *INPUT_MANAGER.lock() {
        let (x, y) = manager.bounds.constrain(x as i32, y as i32);
        manager.cursor_x = x;
        manager.cursor_y = y;
    }
}

/// Get current cursor position
pub fn get_cursor_position() -> (usize, usize) {
    if let Some(ref manager) = *INPUT_MANAGER.lock() {
        (manager.cursor_x, manager.cursor_y)
    } else {
        (0, 0)
    }
}

/// Set mouse sensitivity (256 = 1.0x, 128 = 0.5x, 512 = 2.0x)
pub fn set_mouse_sensitivity(sensitivity: u16) {
    if let Some(ref mut manager) = *INPUT_MANAGER.lock() {
        manager.mouse_sensitivity = sensitivity.max(64).min(1024);
    }
}

/// Process a mouse packet from the PS/2 mouse driver (called from IRQ)
pub fn handle_mouse_packet(packet: MousePacket) {
    if let Some(ref mut manager) = *INPUT_MANAGER.lock() {
        manager.process_mouse_packet(packet);
    }
}

/// Process a keyboard event
pub fn handle_keyboard_event(key_event: KeyEvent, pressed: bool) {
    if let Some(ref mut manager) = *INPUT_MANAGER.lock() {
        let event = if pressed {
            InputEvent::KeyPress(key_event)
        } else {
            InputEvent::KeyRelease(key_event)
        };
        manager.queue_event(event);
    }
}

/// Get next input event from queue
pub fn get_event() -> Option<InputEvent> {
    if let Some(ref mut manager) = *INPUT_MANAGER.lock() {
        manager.event_queue.dequeue()
    } else {
        None
    }
}

/// Peek at next event without removing it
pub fn peek_event() -> Option<InputEvent> {
    if let Some(ref manager) = *INPUT_MANAGER.lock() {
        manager.event_queue.peek().copied()
    } else {
        None
    }
}

/// Get number of events in queue
pub fn get_event_count() -> usize {
    if let Some(ref manager) = *INPUT_MANAGER.lock() {
        manager.event_queue.len()
    } else {
        0
    }
}

/// Get input manager statistics
pub fn get_statistics() -> (usize, usize) {
    if let Some(ref manager) = *INPUT_MANAGER.lock() {
        (manager.events_queued, manager.events_dropped)
    } else {
        (0, 0)
    }
}

/// Get current button states
pub fn get_button_states() -> MouseButtons {
    if let Some(ref manager) = *INPUT_MANAGER.lock() {
        manager.buttons
    } else {
        MouseButtons::new()
    }
}

/// Clear all events from queue
pub fn clear_events() {
    if let Some(ref mut manager) = *INPUT_MANAGER.lock() {
        while manager.event_queue.dequeue().is_some() {}
    }
}

/// Simulate a mouse move (useful for keyboard-based cursor control)
pub fn simulate_mouse_move(delta_x: i16, delta_y: i16) {
    if let Some(ref mut manager) = *INPUT_MANAGER.lock() {
        let (x, y) = manager.update_cursor(delta_x, delta_y);
        manager.queue_event(InputEvent::MouseMove { x, y });
    }
}

/// Simulate a mouse button event (useful for keyboard-based clicking)
pub fn simulate_mouse_button(button: MouseButton, pressed: bool) {
    if let Some(ref mut manager) = *INPUT_MANAGER.lock() {
        let event = if pressed {
            InputEvent::MouseButtonDown {
                button,
                x: manager.cursor_x,
                y: manager.cursor_y,
            }
        } else {
            InputEvent::MouseButtonUp {
                button,
                x: manager.cursor_x,
                y: manager.cursor_y,
            }
        };
        manager.queue_event(event);
    }
}
