//! Window placement algorithm ported from GNOME Mutter
//!
//! Source: /home/justin/Downloads/mutter-main/src/core/place.c
//! Implements cascade and overlap-avoidance placement for new windows.

use crate::graphics::framebuffer::Rect;
use alloc::vec::Vec;

const CASCADE_FUZZ: usize = 15;
const CASCADE_INTERVAL: usize = 50;
const TITLEBAR_HEIGHT: usize = 28;

/// Find optimal placement position for a new window
///
/// Attempts placement via:
/// 1. Centering in the screen
/// 2. Cascading down-right by CASCADE_INTERVAL if centered position overlaps
/// 3. Wrapping to origin if cascade exceeds screen bounds
pub fn find_placement(
    existing_windows: &[Rect],
    screen: Rect,
    new_size: (usize, usize),
) -> (i32, i32) {
    let (width, height) = new_size;

    // Calculate center position within screen
    let center_x = (screen.x as i32) + ((screen.width.saturating_sub(width)) / 2) as i32;
    let center_y = (screen.y as i32) + ((screen.height.saturating_sub(height)) / 2) as i32;

    // Try centered position first
    let mut x = center_x.max(0);
    let mut y = center_y.max(0);

    let mut new_window = Rect {
        x: x as usize,
        y: y as usize,
        width,
        height,
    };

    // Check if centered position overlaps with any existing window
    if !any_overlap(&new_window, existing_windows) {
        return (x, y);
    }

    // Apply cascade: shift down-right by CASCADE_INTERVAL until no overlap or bounds exceeded
    let cascade_origin_x = screen.x.max(CASCADE_FUZZ);
    let cascade_origin_y = screen.y.max(CASCADE_FUZZ);

    let mut cascade_x = cascade_origin_x as i32;
    let mut cascade_y = cascade_origin_y as i32;

    const MAX_CASCADE_ATTEMPTS: usize = 100;
    for _ in 0..MAX_CASCADE_ATTEMPTS {
        new_window.x = cascade_x.max(0) as usize;
        new_window.y = cascade_y.max(0) as usize;

        // Ensure window stays within screen bounds
        if new_window.x + width > screen.x + screen.width {
            cascade_x = screen.x as i32;
        }
        if new_window.y + height > screen.y + screen.height {
            cascade_y = screen.y as i32;
        }

        new_window.x = cascade_x.max(0) as usize;
        new_window.y = cascade_y.max(0) as usize;

        // Check if this position is clear
        if !any_overlap(&new_window, existing_windows) {
            return (new_window.x as i32, new_window.y as i32);
        }

        // Cascade further down-right
        cascade_x += CASCADE_INTERVAL as i32;
        cascade_y += (CASCADE_INTERVAL + TITLEBAR_HEIGHT) as i32;
    }

    // Fallback: return cascade origin if no clear position found
    (cascade_origin_x as i32, cascade_origin_y as i32)
}

/// Check if new_window overlaps with any window in the list
fn any_overlap(new_window: &Rect, existing: &[Rect]) -> bool {
    existing.iter().any(|window| new_window.intersects(window))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_center_placement_no_overlap() {
        let screen = Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let existing = vec![];
        let (x, y) = find_placement(&existing, screen, (800, 600));

        // Should be centered
        assert_eq!(x, 560); // (1920 - 800) / 2
        assert_eq!(y, 240); // (1080 - 600) / 2
    }

    #[test]
    fn test_cascade_placement_with_overlap() {
        let screen = Rect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let existing = vec![Rect {
            x: 560,
            y: 240,
            width: 800,
            height: 600,
        }];
        let (x, y) = find_placement(&existing, screen, (800, 600));

        // Should cascade since center overlaps
        assert!(x > 560 || y > 240);
    }

    #[test]
    fn test_respects_screen_bounds() {
        let screen = Rect {
            x: 100,
            y: 100,
            width: 800,
            height: 600,
        };
        let existing = vec![];
        let (x, y) = find_placement(&existing, screen, (400, 300));

        assert!(x >= 100);
        assert!(y >= 100);
    }
}
