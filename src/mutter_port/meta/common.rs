//! Common Mutter structures and constants
//! Ported from meta/common.h

use crate::mutter_port::meta::enums::MetaButtonFunction;
use crate::mutter_port::mtk::MtkRectangle;

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

/// Complete frame geometry for a decorated window.
///
/// Combines the three border measurements used by the window manager:
/// - `border`: the visible decoration border widths (title bar + side bars).
/// - `visible`: the rectangle of the visible frame area in screen space.
/// - `total`: the full frame rectangle including invisible (extended)
///   borders used for input region and shadow hit-testing.
///
/// Mirrors the upstream `MetaFrameGeometry` struct from `meta/common.h`.
#[derive(Debug, Clone, Copy, Default)]
pub struct MetaFrameGeometry {
    /// Visible decoration border widths.
    pub border: MetaFrameBorder,
    /// Visible frame rectangle in screen coordinates.
    pub visible: MtkRectangle,
    /// Total frame rectangle (visible + invisible extended borders).
    pub total: MtkRectangle,
}

impl MetaFrameGeometry {
    /// Compute the total frame rectangle from the visible rectangle and
    /// the invisible border widths. The total rect expands outward from
    /// the visible rect by the invisible borders on each side.
    pub fn from_visible(
        visible: MtkRectangle,
        visible_border: MetaFrameBorder,
        invisible: MetaFrameBorder,
    ) -> Self {
        let total = MtkRectangle {
            x: visible.x - invisible.left as i32,
            y: visible.y - invisible.top as i32,
            width: visible.width + (invisible.left + invisible.right) as i32,
            height: visible.height + (invisible.top + invisible.bottom) as i32,
        };
        Self {
            border: visible_border,
            visible,
            total,
        }
    }

    /// The invisible border widths, derived as the difference between the
    /// total and visible rectangles.
    pub fn invisible_border(&self) -> MetaFrameBorder {
        MetaFrameBorder {
            left: (self.total.x - self.visible.x) as i16,
            top: (self.total.y - self.visible.y) as i16,
            right: ((self.total.width - self.visible.width) as i16)
                - (self.total.x - self.visible.x) as i16,
            bottom: ((self.total.height - self.visible.height) as i16)
                - (self.total.y - self.visible.y) as i16,
        }
    }
}
