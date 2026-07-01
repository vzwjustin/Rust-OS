//! Window configuration ported from GNOME Mutter's src/core/meta-window-config.c
//!
//! Implements window configuration properties and constraints that determine how
//! a window is laid out and constrained on the desktop.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-window-config.c

use super::window::Rect;

/// Window positioning gravity (for window placement rules).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementGravity {
    /// No gravity preference.
    None,
    /// Top edge.
    Top,
    /// Bottom edge.
    Bottom,
    /// Left edge.
    Left,
    /// Right edge.
    Right,
}

/// Anchor point for popup/popover positioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlacementAnchor {
    /// No anchor.
    None,
    /// Anchor to top.
    Top,
    /// Anchor to bottom.
    Bottom,
    /// Anchor to left.
    Left,
    /// Anchor to right.
    Right,
}

/// Constraint adjustment flags for popup positioning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstraintAdjustment {
    /// No adjustment.
    None = 0,
    /// Slide along X axis.
    SlideX = 1,
    /// Slide along Y axis.
    SlideY = 2,
    /// Flip along X axis.
    FlipX = 4,
    /// Flip along Y axis.
    FlipY = 8,
    /// Resize along X axis.
    ResizeX = 16,
    /// Resize along Y axis.
    ResizeY = 32,
}

/// Sizing hints from the application.
#[derive(Debug, Clone, Copy)]
pub struct SizeHints {
    /// User-specified position present.
    pub user_position: bool,
    /// User-specified size present.
    pub user_size: bool,
    /// Program-specified position present.
    pub program_position: bool,
    /// Program-specified size present.
    pub program_size: bool,
    /// Minimum size hints present.
    pub min_size: bool,
    /// Maximum size hints present.
    pub max_size: bool,
    /// Resize increment hints present.
    pub resize_increment: bool,
    /// Aspect ratio hints present.
    pub aspect_ratio: bool,
    /// Base size hints present.
    pub base_size: bool,
    /// Win gravity hints present.
    pub win_gravity_hint: bool,

    /// Minimum width.
    pub min_width: u32,
    /// Minimum height.
    pub min_height: u32,
    /// Maximum width.
    pub max_width: u32,
    /// Maximum height.
    pub max_height: u32,
    /// Width resize increment.
    pub width_inc: u32,
    /// Height resize increment.
    pub height_inc: u32,
    /// Base width.
    pub base_width: u32,
    /// Base height.
    pub base_height: u32,

    /// Aspect ratio numerator.
    pub aspect_x: u32,
    /// Aspect ratio denominator.
    pub aspect_y: u32,

    /// Window gravity.
    pub win_gravity: i32,
}

impl Default for SizeHints {
    fn default() -> Self {
        SizeHints {
            user_position: false,
            user_size: false,
            program_position: false,
            program_size: false,
            min_size: false,
            max_size: false,
            resize_increment: false,
            aspect_ratio: false,
            base_size: false,
            win_gravity_hint: false,
            min_width: 0,
            min_height: 0,
            max_width: u32::MAX,
            max_height: u32::MAX,
            width_inc: 1,
            height_inc: 1,
            base_width: 0,
            base_height: 0,
            aspect_x: 1,
            aspect_y: 1,
            win_gravity: 0,
        }
    }
}

/// Placement rule for popups (xdg_positioner).
#[derive(Debug, Clone)]
pub struct PlacementRule {
    /// Anchor rectangle (parent coordinates).
    pub anchor_rect: Rect,
    /// Gravity for placement.
    pub gravity: PlacementGravity,
    /// Anchor point.
    pub anchor: PlacementAnchor,
    /// Constraint adjustments to apply.
    pub constraint_adjustment: ConstraintAdjustment,
    /// Offset from anchor point.
    pub offset_x: i32,
    pub offset_y: i32,
    /// Reactive: window should move with parent if constraint violated.
    pub reactive: bool,
    /// Parent window ID (u64 encoded).
    pub parent_id: Option<u64>,
}

impl Default for PlacementRule {
    fn default() -> Self {
        PlacementRule {
            anchor_rect: Rect::new(0, 0, 1, 1),
            gravity: PlacementGravity::None,
            anchor: PlacementAnchor::None,
            constraint_adjustment: ConstraintAdjustment::None,
            offset_x: 0,
            offset_y: 0,
            reactive: false,
            parent_id: None,
        }
    }
}

/// Window configuration capturing all layout properties.
#[derive(Debug, Clone)]
pub struct MetaWindowConfig {
    /// Geometry rectangle.
    pub rect: Rect,
    /// Border width.
    pub border_width: u32,
    /// Size hints from application.
    pub size_hints: SizeHints,
    /// Placement rule (for popups).
    pub placement_rule: Option<PlacementRule>,
    /// Whether geometry has been configured.
    pub geometry_configured: bool,
}

impl Default for MetaWindowConfig {
    fn default() -> Self {
        MetaWindowConfig {
            rect: Rect::new(0, 0, 800, 600),
            border_width: 0,
            size_hints: SizeHints::default(),
            placement_rule: None,
            geometry_configured: false,
        }
    }
}

impl MetaWindowConfig {
    /// Create a new window configuration.
    pub fn new(rect: Rect) -> Self {
        MetaWindowConfig {
            rect,
            ..Default::default()
        }
    }

    /// Apply constraints to the configuration.
    pub fn constrain(&mut self, workarea: Rect, placement_rule: Option<PlacementRule>) {
        if let Some(rule) = placement_rule {
            self.apply_placement_rule(&rule, workarea);
            self.placement_rule = Some(rule);
        }

        // Apply size hints constraints.
        if self.size_hints.min_width > 0 {
            self.rect.width = self.rect.width.max(self.size_hints.min_width);
        }
        if self.size_hints.max_width < u32::MAX {
            self.rect.width = self.rect.width.min(self.size_hints.max_width);
        }
        if self.size_hints.min_height > 0 {
            self.rect.height = self.rect.height.max(self.size_hints.min_height);
        }
        if self.size_hints.max_height < u32::MAX {
            self.rect.height = self.rect.height.min(self.size_hints.max_height);
        }

        // Keep window within workarea.
        if self.rect.x + self.rect.width as i32 > workarea.right() {
            self.rect.x = (workarea.right() - self.rect.width as i32).max(workarea.x);
        }
        if self.rect.y + self.rect.height as i32 > workarea.bottom() {
            self.rect.y = (workarea.bottom() - self.rect.height as i32).max(workarea.y);
        }
        if self.rect.x < workarea.x {
            self.rect.x = workarea.x;
        }
        if self.rect.y < workarea.y {
            self.rect.y = workarea.y;
        }

        self.geometry_configured = true;
    }

    /// Apply a placement rule to position the window.
    fn apply_placement_rule(&mut self, rule: &PlacementRule, workarea: Rect) {
        // Position relative to anchor rectangle.
        let mut x = rule.anchor_rect.x + rule.offset_x;
        let mut y = rule.anchor_rect.y + rule.offset_y;

        // Apply gravity.
        match rule.gravity {
            PlacementGravity::Top => y = rule.anchor_rect.y - self.rect.height as i32,
            PlacementGravity::Bottom => y = rule.anchor_rect.bottom(),
            PlacementGravity::Left => x = rule.anchor_rect.x - self.rect.width as i32,
            PlacementGravity::Right => x = rule.anchor_rect.right(),
            PlacementGravity::None => {}
        }

        self.rect.x = x;
        self.rect.y = y;

        // Apply constraint adjustments if needed.
        if self.rect.right() > workarea.right() {
            match rule.constraint_adjustment {
                ConstraintAdjustment::SlideX => {
                    self.rect.x -= self.rect.right() - workarea.right();
                }
                ConstraintAdjustment::FlipX => {
                    self.rect.x = rule.anchor_rect.x - self.rect.width as i32;
                }
                _ => {}
            }
        }

        if self.rect.bottom() > workarea.bottom() {
            match rule.constraint_adjustment {
                ConstraintAdjustment::SlideY => {
                    self.rect.y -= self.rect.bottom() - workarea.bottom();
                }
                ConstraintAdjustment::FlipY => {
                    self.rect.y = rule.anchor_rect.y - self.rect.height as i32;
                }
                _ => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_size_hints_default() {
        let hints = SizeHints::default();
        assert!(!hints.user_position);
        assert!(!hints.user_size);
        assert_eq!(hints.min_width, 0);
        assert_eq!(hints.max_width, u32::MAX);
    }

    #[test]
    fn test_window_config_creation() {
        let config = MetaWindowConfig::new(Rect::new(100, 100, 400, 300));
        assert_eq!(config.rect.x, 100);
        assert_eq!(config.rect.width, 400);
        assert!(!config.geometry_configured);
    }

    #[test]
    fn test_size_hints_constraint() {
        let mut config = MetaWindowConfig::new(Rect::new(0, 0, 800, 600));
        config.size_hints.min_width = 200;
        config.size_hints.max_width = 400;

        let workarea = Rect::new(0, 0, 1024, 768);
        config.constrain(workarea, None);

        assert!(config.rect.width >= 200);
        assert!(config.rect.width <= 400);
    }

    #[test]
    fn test_placement_rule() {
        let mut config = MetaWindowConfig::new(Rect::new(0, 0, 100, 100));
        let rule = PlacementRule {
            anchor_rect: Rect::new(500, 500, 50, 50),
            gravity: PlacementGravity::Top,
            offset_x: 0,
            offset_y: 0,
            ..Default::default()
        };

        let workarea = Rect::new(0, 0, 1024, 768);
        config.constrain(workarea, Some(rule));

        // Window should be positioned above anchor rect.
        assert!(config.rect.y < 500);
    }
}
