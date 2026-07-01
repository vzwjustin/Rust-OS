//! Window actor for rendering window content ported from `meta-window-actor.c`.
//!
//! Core actor for displaying window surfaces with effects and transformations.

use super::surface_actor::SurfaceActor;
use crate::desktop::window_manager::WindowId;
use alloc::vec::Vec;

/// Window actor states
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowActorState {
    /// Not yet mapped
    Hidden,
    /// First frame is being rendered
    FirstFrame,
    /// Normal rendering
    Mapped,
}

/// Effects state for window
#[derive(Debug)]
pub struct WindowEffects {
    pub opacity: f32,
    pub scale_x: f32,
    pub scale_y: f32,
    pub rotation: f32,
}

impl Default for WindowEffects {
    fn default() -> Self {
        WindowEffects {
            opacity: 1.0,
            scale_x: 1.0,
            scale_y: 1.0,
            rotation: 0.0,
        }
    }
}

/// Main window actor for rendering
#[derive(Debug)]
pub struct WindowActor {
    pub id: u32,
    pub window_id: WindowId,
    pub surface_actors: Vec<SurfaceActor>,
    pub state: WindowActorState,
    pub effects: WindowEffects,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl WindowActor {
    /// Create new window actor
    pub fn new(id: u32, window_id: WindowId) -> Self {
        WindowActor {
            id,
            window_id,
            surface_actors: Vec::new(),
            state: WindowActorState::Hidden,
            effects: WindowEffects::default(),
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }
    }

    /// Add surface actor
    pub fn add_surface_actor(&mut self, surface: SurfaceActor) {
        self.surface_actors.push(surface);
    }

    /// Remove surface actor
    pub fn remove_surface_actor(&mut self, actor_id: u32) {
        self.surface_actors.retain(|sa| sa.id != actor_id);
    }

    /// Set window position
    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    /// Set window size
    pub fn set_size(&mut self, width: u32, height: u32) {
        self.width = width;
        self.height = height;
    }

    /// Map window (make visible)
    pub fn map(&mut self) {
        self.state = WindowActorState::FirstFrame;
    }

    /// Unmap window (hide)
    pub fn unmap(&mut self) {
        self.state = WindowActorState::Hidden;
    }

    /// Check if window is mapped
    pub fn is_mapped(&self) -> bool {
        self.state != WindowActorState::Hidden
    }

    /// Mark first frame complete
    pub fn set_first_frame_complete(&mut self) {
        if self.state == WindowActorState::FirstFrame {
            self.state = WindowActorState::Mapped;
        }
    }

    /// Apply effect (opacity, scale, rotation)
    pub fn set_opacity(&mut self, opacity: f32) {
        self.effects.opacity = opacity.max(0.0).min(1.0);
    }

    /// Paint window
    pub fn paint(&self) {
        if !self.is_mapped() || self.effects.opacity == 0.0 {
            return;
        }

        for surface in &self.surface_actors {
            surface.paint();
        }
    }

    /// Get surface actor count
    pub fn surface_count(&self) -> usize {
        self.surface_actors.len()
    }

    /// Get top surface actor
    pub fn get_top_surface(&self) -> Option<&SurfaceActor> {
        self.surface_actors.last()
    }
}
