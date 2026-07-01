//! Wayland Text Input protocol implementation.
//!
//! Ported from: meta-wayland-text-input.c/h
//!
//! Implements the zwp_text_input_v3 protocol for input method communication.
//! Allows clients to request input method services (predictive text, composition, etc.)
//! for text editing.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-text-input.h

use alloc::string::String;

/// Text input focus state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum TextInputState {
    UNFOCUSED = 0,
    FOCUSED = 1,
    PENDING_COMMIT = 2,
}

/// Text input hint bitmask.
pub const TEXT_INPUT_HINT_NONE: u32 = 0;
pub const TEXT_INPUT_HINT_COMPLETION: u32 = 1 << 0;
pub const TEXT_INPUT_HINT_SPELLCHECK: u32 = 1 << 1;
pub const TEXT_INPUT_HINT_AUTO_CAPITALIZATION: u32 = 1 << 2;
pub const TEXT_INPUT_HINT_LOWERCASE: u32 = 1 << 3;
pub const TEXT_INPUT_HINT_UPPERCASE: u32 = 1 << 4;
pub const TEXT_INPUT_HINT_TITLECASE: u32 = 1 << 5;
pub const TEXT_INPUT_HINT_HIDDEN_TEXT: u32 = 1 << 6;
pub const TEXT_INPUT_HINT_SENSITIVE_DATA: u32 = 1 << 7;
pub const TEXT_INPUT_HINT_LATIN: u32 = 1 << 8;
pub const TEXT_INPUT_HINT_MULTILINE: u32 = 1 << 9;

/// Text input purpose enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum TextInputPurpose {
    DEFAULT = 0,
    ALPHA = 1,
    DIGITS = 2,
    NUMBER = 3,
    PHONE = 4,
    URL = 5,
    EMAIL = 6,
    NAME = 7,
    PASSWORD = 8,
    DATE = 9,
    TIME = 10,
    DATETIME = 11,
    TERMINAL = 12,
}

/// Represents text input state for an input method client.
///
/// Tracks the focused surface, input hints/purpose, surrounding text,
/// and pending composition. Protocol I/O is TODO.
#[derive(Debug)]
pub struct MetaWaylandTextInput {
    pub seat: Option<*mut core::ffi::c_void>,        // MetaWaylandSeat pointer
    pub focus_surface: Option<*mut core::ffi::c_void>, // MetaWaylandSurface pointer
    pub state: TextInputState,
    pub content_hint: u32,
    pub content_purpose: TextInputPurpose,
    pub surrounding_text: String,
    pub cursor_position: u32,
}

impl MetaWaylandTextInput {
    pub fn new(seat: *mut core::ffi::c_void) -> Self {
        MetaWaylandTextInput {
            seat: Some(seat),
            focus_surface: None,
            state: TextInputState::UNFOCUSED,
            content_hint: TEXT_INPUT_HINT_NONE,
            content_purpose: TextInputPurpose::DEFAULT,
            surrounding_text: String::new(),
            cursor_position: 0,
        }
    }

    pub fn set_focus(&mut self, surface: Option<*mut core::ffi::c_void>) {
        self.focus_surface = surface;
        if surface.is_some() {
            self.state = TextInputState::FOCUSED;
        } else {
            self.state = TextInputState::UNFOCUSED;
        }
    }

    pub fn get_state(&self) -> TextInputState {
        self.state
    }

    pub fn set_content_hint(&mut self, hint: u32) {
        self.content_hint = hint;
    }

    pub fn set_content_purpose(&mut self, purpose: TextInputPurpose) {
        self.content_purpose = purpose;
    }
}

impl Default for MetaWaylandTextInput {
    fn default() -> Self {
        MetaWaylandTextInput {
            seat: None,
            focus_surface: None,
            state: TextInputState::UNFOCUSED,
            content_hint: TEXT_INPUT_HINT_NONE,
            content_purpose: TextInputPurpose::DEFAULT,
            surrounding_text: String::new(),
            cursor_position: 0,
        }
    }
}
