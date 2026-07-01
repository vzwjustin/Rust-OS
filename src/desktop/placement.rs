//! Window placement policy ported from Mutter.
//!
//! This mirrors the cascade placement logic in
//! `/home/justin/Downloads/mutter-main/src/core/place.c`: start from the
//! work-area origin, skip positions already occupied by nearby window frames,
//! advance by titlebar height, and start a new cascade when the next frame
//! would leave the work area.
//!
//! Geometry ops (rect clamping/intersection) are routed through
//! `crate::mutter_bridge`, which delegates to the ported
//! `mutter_port::mtk::rectangle::Rectangle` algebra so the kernel and the
//! mutter port share one geometry implementation.

use crate::graphics::framebuffer::Rect;
use crate::mutter_bridge;

use super::window_manager::{Window, WindowState};

const CASCADE_FUZZ: usize = 15;
const CASCADE_INTERVAL: usize = 50;

fn abs_diff(a: usize, b: usize) -> usize {
    a.max(b) - a.min(b)
}

fn amount_onscreen_for_axis(length: usize) -> usize {
    (length / 4).clamp(10, 75).min(length)
}

/// Keep enough of a window frame visible that the user can recover it.
///
/// This mirrors Mutter's `constrain_titlebar_visible()` policy from
/// `src/core/constraints.c`: ordinary move/resize operations may leave most of
/// a frame offscreen, but they must keep a usable piece of the frame and the
/// titlebar inside the work area.
pub fn constrain_titlebar_visible(mut rect: Rect, work_area: Rect, titlebar_height: usize) -> Rect {
    if work_area.width == 0 || work_area.height == 0 || rect.width == 0 || rect.height == 0 {
        return rect;
    }

    let horiz_onscreen = amount_onscreen_for_axis(rect.width);
    let titlebar_onscreen = titlebar_height.min(rect.height).max(1);
    let work_right = work_area.x.saturating_add(work_area.width);
    let work_bottom = work_area.y.saturating_add(work_area.height);

    let min_x = work_area
        .x
        .saturating_sub(rect.width.saturating_sub(horiz_onscreen));
    let max_x = work_right.saturating_sub(horiz_onscreen);
    rect.x = rect.x.clamp(min_x, max_x.max(min_x));

    let max_y = work_bottom.saturating_sub(titlebar_onscreen);
    rect.y = rect.y.clamp(work_area.y, max_y.max(work_area.y));

    rect
}

/// Clamp an interactive bottom-right resize to the current work area.
pub fn constrain_resize_rect(
    mut rect: Rect,
    work_area: Rect,
    min_width: usize,
    min_height: usize,
    titlebar_height: usize,
) -> Rect {
    rect = constrain_titlebar_visible(rect, work_area, titlebar_height);

    let work_right = work_area.x.saturating_add(work_area.width);
    let work_bottom = work_area.y.saturating_add(work_area.height);
    let available_width = work_right.saturating_sub(rect.x);
    let available_height = work_bottom.saturating_sub(rect.y);

    rect.width = rect.width.max(min_width);
    if available_width >= min_width {
        rect.width = rect.width.min(available_width);
    }

    rect.height = rect.height.max(min_height);
    if available_height >= min_height {
        rect.height = rect.height.min(available_height);
    }

    rect
}

fn window_blocks_position(
    window: &Window,
    x: usize,
    y: usize,
    width: usize,
    titlebar_height: usize,
) -> bool {
    abs_diff(window.rect.x, x) < CASCADE_FUZZ && abs_diff(window.rect.y, y) < CASCADE_FUZZ
        || abs_diff(
            window.rect.x.saturating_add(window.rect.width),
            x.saturating_add(width),
        ) < CASCADE_FUZZ
            && abs_diff(window.rect.y, y) < CASCADE_FUZZ
        || abs_diff(window.rect.x.saturating_add(titlebar_height), x) < CASCADE_FUZZ
            && abs_diff(window.rect.y.saturating_add(titlebar_height), y) < CASCADE_FUZZ
}

/// Pick a Mutter-style cascaded frame position for a new window.
pub fn cascade_window_rect(
    requested: Rect,
    work_area: Rect,
    windows: &[Window],
    workspace: u8,
    titlebar_height: usize,
) -> Rect {
    if work_area.width == 0 || work_area.height == 0 {
        return requested;
    }

    let mut cascade_origin_x = work_area.x;
    let mut cascade_x = cascade_origin_x;
    let mut cascade_y = work_area.y;
    let mut cascade_stage = 0usize;

    for window in windows {
        if !window.visible || window.state == WindowState::Closed || window.workspace != workspace {
            continue;
        }

        if !window_blocks_position(
            window,
            cascade_x,
            cascade_y,
            requested.width,
            titlebar_height,
        ) {
            continue;
        }

        cascade_x = window.rect.x.saturating_add(titlebar_height);
        cascade_y = window.rect.y.saturating_add(titlebar_height);

        let right = cascade_x.saturating_add(requested.width);
        let bottom = cascade_y.saturating_add(requested.height);
        let work_right = work_area.x.saturating_add(work_area.width);
        let work_bottom = work_area.y.saturating_add(work_area.height);

        if right > work_right || bottom > work_bottom {
            cascade_stage = cascade_stage.saturating_add(1);
            cascade_origin_x = work_area
                .x
                .saturating_add(CASCADE_INTERVAL.saturating_mul(cascade_stage));
            cascade_x = cascade_origin_x;
            cascade_y = work_area.y;

            if cascade_x.saturating_add(requested.width) > work_right {
                cascade_x = work_area.x;
                cascade_y = work_area.y;
                break;
            }
        }
    }

    mutter_bridge::clamp_rect_to_work_area(
        Rect::new(cascade_x, cascade_y, requested.width, requested.height),
        work_area,
    )
}

#[cfg(test)]
mod tests {
    use super::super::window_manager::{Window, WindowId};
    use super::*;

    #[test_case]
    fn cascades_away_from_existing_window() {
        let work_area = Rect::new(0, 30, 1024, 700);
        let existing = Window::new(WindowId(1), "A", 0, 30, 300, 200);
        let rect = cascade_window_rect(Rect::new(0, 30, 300, 200), work_area, &[existing], 0, 28);

        assert_eq!(rect.x, 28);
        assert_eq!(rect.y, 58);
    }

    #[test_case]
    fn clamps_to_work_area() {
        let work_area = Rect::new(0, 30, 320, 200);
        let rect = cascade_window_rect(Rect::new(400, 400, 640, 480), work_area, &[], 0, 28);

        assert_eq!(rect.x, 0);
        assert_eq!(rect.y, 30);
        assert_eq!(rect.width, 320);
        assert_eq!(rect.height, 200);
    }

    #[test_case]
    fn keeps_titlebar_visible_when_dragged_offscreen() {
        let work_area = Rect::new(0, 30, 1024, 700);
        let rect = constrain_titlebar_visible(Rect::new(0, 0, 300, 200), work_area, 28);

        assert_eq!(rect.y, 30);
    }

    #[test_case]
    fn resize_stays_inside_work_area_when_possible() {
        let work_area = Rect::new(0, 30, 320, 200);
        let rect = constrain_resize_rect(Rect::new(100, 80, 400, 400), work_area, 200, 150, 28);

        assert_eq!(rect.x, 100);
        assert_eq!(rect.y, 80);
        assert_eq!(rect.width, 220);
        assert_eq!(rect.height, 150);
    }
}
