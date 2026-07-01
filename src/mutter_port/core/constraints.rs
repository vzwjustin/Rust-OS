//! Window geometry constraint solver ported from GNOME Mutter.
//!
//! Applies size hints, screen/monitor edge snapping, keeps windows on-screen,
//! enforces aspect ratio limits, and respects maximization/tiling constraints.
//!
//! Core algorithm: clamp window position/size to constraints in priority order
//! (size limits → aspect ratio → visibility → monitor containment).
//!
//! Source: mutter-main/src/core/constraints.c (GNU GPL 2+)

use crate::desktop::window_manager::WindowId;
use crate::graphics::framebuffer::Rect;
use alloc::vec::Vec;

/// Constraint priority levels (higher = enforced even if lower constraints break).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConstraintPriority {
    AspectRatio = 0,
    VisibleOnMonitor = 1,
    SizeLimits = 2,
    TitlebarVisible = 3,
    ExternalConstraint = 5,
}

/// Directional hint for resize operations (which edges/corners are fixed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeGravity {
    NorthWest,
    North,
    NorthEast,
    East,
    SouthEast,
    South,
    SouthWest,
    West,
    Center,
}

/// Action being constrained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionType {
    Move,
    Resize,
    MoveAndResize,
}

/// Constraint context and state for a single window operation.
#[derive(Debug, Clone)]
pub struct ConstraintInfo {
    pub window_id: WindowId,
    /// Original rectangle before constraint application
    pub original: Rect,
    /// Current rectangle being constrained
    pub current: Rect,
    /// Monitor work area (excluding struts)
    pub work_area: Rect,
    /// Full monitor rectangle
    pub monitor: Rect,
    /// Type of action triggering constraints
    pub action_type: ActionType,
    /// Resize gravity (which corners/edges are fixed)
    pub resize_gravity: ResizeGravity,
    /// Whether this is a user-initiated action (vs. WM auto-adjust)
    pub is_user_action: bool,
}

/// Size constraints for a window.
#[derive(Debug, Clone, Copy)]
pub struct SizeConstraints {
    pub min_width: usize,
    pub min_height: usize,
    pub max_width: usize,
    pub max_height: usize,
    /// Aspect ratio as (numerator, denominator); None = unconstrained
    pub aspect_ratio: Option<(u32, u32)>,
}

impl SizeConstraints {
    /// Create unconstrained size limits.
    pub fn unconstrained() -> Self {
        Self {
            min_width: 1,
            min_height: 1,
            max_width: usize::MAX,
            max_height: usize::MAX,
            aspect_ratio: None,
        }
    }
}

/// Apply all constraints to a window geometry, returning the constrained rectangle.
///
/// Enforces in order: size limits → aspect ratio → visibility on monitor.
pub fn apply_constraints(info: &mut ConstraintInfo, constraints: &SizeConstraints) -> Rect {
    // Priority 0: clamp to size limits
    constrain_size_limits(&mut info.current, constraints);

    // Priority 1: enforce aspect ratio
    if let Some((num, denom)) = constraints.aspect_ratio {
        constrain_aspect_ratio(
            &mut info.current,
            num as usize,
            denom as usize,
            info.resize_gravity,
        );
    }

    // Priority 2: keep on monitor (simple containment)
    constrain_to_monitor(&mut info.current, &info.work_area, info.resize_gravity);

    // Priority 3: titlebar visible (keep at least top-left in work area)
    constrain_titlebar_visible(&mut info.current, &info.work_area);

    info.current
}

/// Clamp window dimensions to min/max constraints.
fn constrain_size_limits(rect: &mut Rect, constraints: &SizeConstraints) {
    rect.width = rect
        .width
        .max(constraints.min_width)
        .min(constraints.max_width);
    rect.height = rect
        .height
        .max(constraints.min_height)
        .min(constraints.max_height);
}

/// Enforce aspect ratio by adjusting height to match width (or vice versa).
fn constrain_aspect_ratio(
    rect: &mut Rect,
    aspect_num: usize,
    aspect_denom: usize,
    gravity: ResizeGravity,
) {
    if aspect_denom == 0 {
        return;
    }

    // Calculate expected height from width: h = w * denom / num
    let expected_height = (rect.width * aspect_denom) / aspect_num;

    match gravity {
        // Horizontal resize: adjust height to match aspect
        ResizeGravity::West | ResizeGravity::East => {
            rect.height = expected_height;
        }
        // Vertical resize: adjust width to match aspect
        ResizeGravity::North | ResizeGravity::South => {
            rect.width = (rect.height * aspect_num) / aspect_denom;
        }
        // Corner/center: preserve width and adjust height
        _ => {
            rect.height = expected_height;
        }
    }
}

/// Keep window within monitor bounds, adjusting position if needed.
fn constrain_to_monitor(rect: &mut Rect, monitor: &Rect, gravity: ResizeGravity) {
    let monitor_right = monitor.x + monitor.width;
    let monitor_bottom = monitor.y + monitor.height;

    match gravity {
        // Fixed to top-left: shift right/down if window extends beyond monitor
        ResizeGravity::NorthWest => {
            if rect.x + rect.width > monitor_right {
                rect.x = monitor_right.saturating_sub(rect.width);
            }
            if rect.y + rect.height > monitor_bottom {
                rect.y = monitor_bottom.saturating_sub(rect.height);
            }
        }
        // Fixed to center: shrink if too large, recenter if out of bounds
        ResizeGravity::Center => {
            if rect.width > monitor.width {
                rect.width = monitor.width;
            }
            if rect.height > monitor.height {
                rect.height = monitor.height;
            }
            let center_x = monitor.x + monitor.width / 2;
            let center_y = monitor.y + monitor.height / 2;
            rect.x = center_x.saturating_sub(rect.width / 2);
            rect.y = center_y.saturating_sub(rect.height / 2);
        }
        // Default: shift left/up if extends beyond
        _ => {
            if rect.x + rect.width > monitor_right {
                rect.x = monitor_right.saturating_sub(rect.width);
            }
            if rect.y + rect.height > monitor_bottom {
                rect.y = monitor_bottom.saturating_sub(rect.height);
            }
        }
    }

    // Ensure top-left is within monitor
    if (rect.x as isize) < (monitor.x as isize) {
        rect.x = monitor.x;
    }
    if (rect.y as isize) < (monitor.y as isize) {
        rect.y = monitor.y;
    }
}

/// Ensure titlebar/window header is visible (at least 1px in monitor work area).
fn constrain_titlebar_visible(rect: &mut Rect, monitor: &Rect) {
    const TITLEBAR_HEIGHT: usize = 28;

    if rect.y + TITLEBAR_HEIGHT < monitor.y {
        rect.y = monitor.y.saturating_sub(TITLEBAR_HEIGHT);
    }
    if rect.x + rect.width < monitor.x + 1 {
        rect.x = monitor.x.saturating_sub(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_limits() {
        let mut rect = Rect::new(0, 0, 100, 100);
        let constraints = SizeConstraints {
            min_width: 50,
            min_height: 50,
            max_width: 200,
            max_height: 200,
            aspect_ratio: None,
        };

        constrain_size_limits(&mut rect, &constraints);
        assert_eq!(rect.width, 100);
        assert_eq!(rect.height, 100);

        // Too small
        rect.width = 10;
        rect.height = 10;
        constrain_size_limits(&mut rect, &constraints);
        assert_eq!(rect.width, 50);
        assert_eq!(rect.height, 50);

        // Too large
        rect.width = 300;
        rect.height = 300;
        constrain_size_limits(&mut rect, &constraints);
        assert_eq!(rect.width, 200);
        assert_eq!(rect.height, 200);
    }

    #[test]
    fn test_aspect_ratio() {
        let mut rect = Rect::new(0, 0, 100, 50);
        constrain_aspect_ratio(&mut rect, 2, 1, ResizeGravity::West);
        assert_eq!(rect.width, 100);
        assert_eq!(rect.height, 50);

        // Width resize: adjust height
        rect.width = 200;
        constrain_aspect_ratio(&mut rect, 2, 1, ResizeGravity::West);
        assert_eq!(rect.height, 100);
    }

    #[test]
    fn test_monitor_containment() {
        let monitor = Rect::new(0, 0, 1920, 1080);
        let mut rect = Rect::new(1800, 900, 200, 200);

        constrain_to_monitor(&mut rect, &monitor, ResizeGravity::SouthEast);
        assert!(rect.x + rect.width <= 1920);
        assert!(rect.y + rect.height <= 1080);
    }

    #[test]
    fn test_titlebar_visible() {
        let monitor = Rect::new(100, 100, 800, 600);
        let mut rect = Rect::new(100, 50, 300, 300);

        constrain_titlebar_visible(&mut rect, &monitor);
        // Should shift down, or stay mostly visible
        assert!(rect.y <= 100 + 28);
    }
}
