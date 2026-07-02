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
/// and pending composition. A full implementation would emit
/// zwp_text_input_v3 events to the input method and relay commit
/// strings back to the client.
#[derive(Debug)]
pub struct MetaWaylandTextInput {
    pub seat: Option<*mut core::ffi::c_void>, // MetaWaylandSeat pointer
    pub focus_surface: Option<*mut core::ffi::c_void>, // MetaWaylandSurface pointer
    pub state: TextInputState,
    pub content_hint: u32,
    pub content_purpose: TextInputPurpose,
    pub surrounding_text: String,
    pub cursor_position: u32,
    /// Selection anchor position within the surrounding text.
    /// Equal to cursor_position when there is no selection.
    pub anchor_position: u32,
}

impl MetaWaylandTextInput {
    pub fn new(seat: *mut core::ffi::c_void) -> Self {
        MetaWaylandTextInput {
            seat: if seat.is_null() { None } else { Some(seat) },
            focus_surface: None,
            state: TextInputState::UNFOCUSED,
            content_hint: TEXT_INPUT_HINT_NONE,
            content_purpose: TextInputPurpose::DEFAULT,
            surrounding_text: String::new(),
            cursor_position: 0,
            anchor_position: 0,
        }
    }

    /// Set the focused surface for text input.
    /// A full implementation would emit enter/leave events to the
    /// client and notify the input method of the focus change.
    pub fn set_focus(&mut self, surface: Option<*mut core::ffi::c_void>) {
        self.focus_surface = surface.filter(|&p| !p.is_null());
        if self.focus_surface.is_some() {
            self.state = TextInputState::FOCUSED;
        } else {
            self.state = TextInputState::UNFOCUSED;
            // Clear surrounding text when focus is lost.
            self.surrounding_text.clear();
            self.cursor_position = 0;
            self.anchor_position = 0;
        }
    }

    /// Get the focused surface pointer, if any.
    pub fn get_focus_surface(&self) -> Option<*mut core::ffi::c_void> {
        self.focus_surface
    }

    /// Check whether text input is currently focused on a surface.
    pub fn is_focused(&self) -> bool {
        self.focus_surface.is_some()
    }

    /// Get the current text input state.
    pub fn get_state(&self) -> TextInputState {
        self.state
    }

    /// Set the surrounding text for the input method.
    /// A full implementation would relay this to the input method via
    /// the zwp_text_input_v3.set_surrounding_text request.
    pub fn set_surrounding_text(&mut self, text: String, cursor: u32, anchor: u32) {
        self.surrounding_text = text;
        self.cursor_position = cursor;
        self.anchor_position = anchor;
    }

    /// Get the surrounding text.
    pub fn get_surrounding_text(&self) -> &str {
        &self.surrounding_text
    }

    /// Get the cursor position within the surrounding text.
    pub fn get_cursor_position(&self) -> u32 {
        self.cursor_position
    }

    /// Get the selection anchor position within the surrounding text.
    pub fn get_anchor_position(&self) -> u32 {
        self.anchor_position
    }

    /// Check whether there is an active text selection.
    pub fn has_selection(&self) -> bool {
        self.cursor_position != self.anchor_position
    }

    /// Set the content hint bitmask.
    pub fn set_content_hint(&mut self, hint: u32) {
        self.content_hint = hint;
    }

    /// Get the content hint bitmask.
    pub fn get_content_hint(&self) -> u32 {
        self.content_hint
    }

    /// Set the content purpose.
    pub fn set_content_purpose(&mut self, purpose: TextInputPurpose) {
        self.content_purpose = purpose;
    }

    /// Get the content purpose.
    pub fn get_content_purpose(&self) -> TextInputPurpose {
        self.content_purpose
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
            anchor_position: 0,
        }
    }
}
