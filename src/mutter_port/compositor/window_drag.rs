//! Window drag operations ported from `meta-window-drag.c`.
//!
//! Handles window movement via mouse/pointer dragging.

use crate::desktop::window_manager::WindowId;

/// Window drag state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragState {
    /// Not dragging
    Idle,
    /// Drag in progress
    Active,
    /// Drag completed
    Completed,
}

/// Window drag operation
#[derive(Debug)]
pub struct WindowDrag {
    pub id: u32,
    pub window_id: WindowId,
    pub state: DragState,
    pub start_x: i32,
    pub start_y: i32,
    pub current_x: i32,
    pub current_y: i32,
    pub offset_x: i32,
    pub offset_y: i32,
}

impl WindowDrag {
    /// Create new window drag operation
    pub fn new(id: u32, window_id: WindowId) -> Self {
        WindowDrag {
            id,
            window_id,
            state: DragState::Idle,
            start_x: 0,
            start_y: 0,
            current_x: 0,
            current_y: 0,
            offset_x: 0,
            offset_y: 0,
        }
    }

    /// Begin drag operation
    pub fn begin(&mut self, x: i32, y: i32) {
        self.state = DragState::Active;
        self.start_x = x;
        self.start_y = y;
        self.current_x = x;
        self.current_y = y;
        self.offset_x = 0;
        self.offset_y = 0;
    }

    /// Update drag position
    pub fn update(&mut self, x: i32, y: i32) {
        if self.state == DragState::Active {
            self.current_x = x;
            self.current_y = y;
            self.offset_x = x - self.start_x;
            self.offset_y = y - self.start_y;
        }
    }

    /// Complete drag operation
    pub fn complete(&mut self) {
        self.state = DragState::Completed;
    }

    /// Cancel drag operation
    pub fn cancel(&mut self) {
        self.state = DragState::Idle;
        self.offset_x = 0;
        self.offset_y = 0;
    }

    /// Check if drag is active
    pub fn is_active(&self) -> bool {
        self.state == DragState::Active
    }

    /// Get current offset from start
    pub fn get_offset(&self) -> (i32, i32) {
        (self.offset_x, self.offset_y)
    }

    /// Get current position
    pub fn get_current_position(&self) -> (i32, i32) {
        (self.current_x, self.current_y)
    }
}
