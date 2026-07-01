//! Feedback actor for UI animations ported from `meta-feedback-actor.c`.
//!
//! Provides visual feedback actors for user interactions (click ripples, etc).

/// Animation feedback actor
#[derive(Debug)]
pub struct FeedbackActor {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub radius: u32,
    pub opacity: f32,
    pub animation_progress: f32,
    pub visible: bool,
}

impl FeedbackActor {
    /// Create new feedback actor
    pub fn new(id: u32) -> Self {
        FeedbackActor {
            id,
            x: 0,
            y: 0,
            radius: 0,
            opacity: 1.0,
            animation_progress: 0.0,
            visible: false,
        }
    }

    /// Start feedback animation
    pub fn start_animation(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
        self.opacity = 1.0;
        self.animation_progress = 0.0;
        self.visible = true;
        self.radius = 0;
    }

    /// Update animation progress
    pub fn update(&mut self, delta: f32) {
        if !self.visible {
            return;
        }

        self.animation_progress += delta;

        // Grow radius over time
        self.radius = (self.animation_progress * 100.0) as u32;

        // Fade out
        self.opacity = (1.0 - self.animation_progress).max(0.0);

        if self.animation_progress >= 1.0 {
            self.visible = false;
        }
    }

    /// Paint feedback effect
    pub fn paint(&self) {
        if !self.visible || self.opacity == 0.0 {
            return;
        }
        // Render ripple/feedback circle at (x, y) with radius
    }

    /// Check if animation is complete
    pub fn is_complete(&self) -> bool {
        !self.visible
    }
}
