//! Drag and drop implementation ported from `meta-dnd.c`.
//!
//! Manages drag-and-drop operations between windows and applications.

use alloc::vec::Vec;

/// Drag and drop event types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DndEvent {
    /// Drag operation started
    Enter,
    /// Drag pointer moved
    PositionChange { x: i32, y: i32 },
    /// Drag operation ended
    Leave,
    /// Drop operation completed
    Drop { x: i32, y: i32 },
}

/// Drag and drop state manager
#[derive(Debug)]
pub struct Dnd {
    pub id: u32,
    pub active: bool,
    pub during_modal: bool,
    pub current_x: i32,
    pub current_y: i32,
    pub listeners: Vec<u32>, // Listener IDs
}

impl Dnd {
    /// Create new DnD manager
    pub fn new(id: u32) -> Self {
        Dnd {
            id,
            active: false,
            during_modal: false,
            current_x: 0,
            current_y: 0,
            listeners: Vec::new(),
        }
    }

    /// Start drag operation
    pub fn start_drag(&mut self, x: i32, y: i32) {
        self.active = true;
        self.current_x = x;
        self.current_y = y;
    }

    /// Update drag position
    pub fn update_position(&mut self, x: i32, y: i32) {
        if self.active {
            self.current_x = x;
            self.current_y = y;
        }
    }

    /// End drag operation
    pub fn end_drag(&mut self) {
        self.active = false;
    }

    /// Handle modal mode start
    pub fn begin_modal(&mut self) {
        self.during_modal = true;
    }

    /// Handle modal mode end
    pub fn end_modal(&mut self) {
        self.during_modal = false;
    }

    /// Check if drag is active
    pub fn is_active(&self) -> bool {
        self.active
    }

    /// Get current drag position
    pub fn get_position(&self) -> (i32, i32) {
        (self.current_x, self.current_y)
    }

    /// Add event listener
    pub fn add_listener(&mut self, listener_id: u32) {
        if !self.listeners.contains(&listener_id) {
            self.listeners.push(listener_id);
        }
    }

    /// Remove event listener
    pub fn remove_listener(&mut self, listener_id: u32) {
        self.listeners.retain(|&id| id != listener_id);
    }
}
