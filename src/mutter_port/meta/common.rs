//! Common Mutter structures and constants
//! Ported from meta/common.h

use crate::mutter_port::meta::enums::MetaButtonFunction;

pub const MAX_BUTTONS_PER_CORNER: usize = 4;

/// Represents button layout for window decoration
#[derive(Debug, Clone, Copy)]
pub struct MetaButtonLayout {
    pub left_buttons: [MetaButtonFunction; MAX_BUTTONS_PER_CORNER],
    pub left_buttons_has_spacer: [bool; MAX_BUTTONS_PER_CORNER],
    pub right_buttons: [MetaButtonFunction; MAX_BUTTONS_PER_CORNER],
    pub right_buttons_has_spacer: [bool; MAX_BUTTONS_PER_CORNER],
}

impl Default for MetaButtonLayout {
    fn default() -> Self {
        Self {
            left_buttons: [MetaButtonFunction::Last; MAX_BUTTONS_PER_CORNER],
            left_buttons_has_spacer: [false; MAX_BUTTONS_PER_CORNER],
            right_buttons: [MetaButtonFunction::Last; MAX_BUTTONS_PER_CORNER],
            right_buttons_has_spacer: [false; MAX_BUTTONS_PER_CORNER],
        }
    }
}

/// Frame border dimensions
#[derive(Debug, Clone, Copy, Default)]
pub struct MetaFrameBorder {
    pub left: i16,
    pub right: i16,
    pub top: i16,
    pub bottom: i16,
}

/// Complete frame border information including visible and invisible portions
#[derive(Debug, Clone, Copy, Default)]
pub struct MetaFrameBorders {
    pub visible: MetaFrameBorder,
    pub invisible: MetaFrameBorder,
    pub total: MetaFrameBorder,
}

impl MetaFrameBorders {
    /// Clear all border dimensions to zero
    pub fn clear(&mut self) {
        self.visible = MetaFrameBorder::default();
        self.invisible = MetaFrameBorder::default();
        self.total = MetaFrameBorder::default();
    }
}

/// Main loop priorities for event handling
pub const META_PRIORITY_RESIZE: i32 = -75; // G_PRIORITY_HIGH_IDLE + 15
pub const META_PRIORITY_BEFORE_REDRAW: i32 = -60; // G_PRIORITY_HIGH_IDLE + 40
pub const META_PRIORITY_REDRAW: i32 = -50; // G_PRIORITY_HIGH_IDLE + 50
pub const META_PRIORITY_PREFS_NOTIFY: i32 = -10; // G_PRIORITY_DEFAULT_IDLE + 10

// TODO: port additional common types as needed
