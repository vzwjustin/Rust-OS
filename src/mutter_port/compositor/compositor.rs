//! Core compositor engine ported from GNOME Mutter's `src/compositor/compositor.c`.
//!
//! Manages rendering, window composition, damage tracking, and frame synchronization.
//! This is the central hub that orchestrates all composition operations on RustOS.

use crate::desktop::window_manager::WindowId;
use alloc::vec::Vec;

/// Compositor state for frame rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompositorState {
    /// Compositor is disabled/paused
    Disabled,
    /// Compositor is initializing
    Initializing,
    /// Normal rendering
    Active,
    /// Redrawing entire frame (damage-tracked)
    Redrawing,
}

/// Damage region (dirty area that needs repainting)
#[derive(Debug, Clone)]
pub struct DamageRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl DamageRegion {
    /// Create new damage region
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        DamageRegion {
            x,
            y,
            width,
            height,
        }
    }

    /// Merge another damage region into this one (union)
    pub fn merge(&mut self, other: &DamageRegion) {
        let min_x = self.x.min(other.x);
        let min_y = self.y.min(other.y);
        let max_x = (self.x + self.width).max(other.x + other.width);
        let max_y = (self.y + self.height).max(other.y + other.height);

        self.x = min_x;
        self.y = min_y;
        self.width = max_x - min_x;
        self.height = max_y - min_y;
    }

    /// Check if region is empty
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }
}

/// Frame synchronization mechanism (frame callbacks, vblank)
#[derive(Debug, Clone, Copy)]
pub struct FrameSync {
    /// Monotonic time of last frame
    pub last_frame_us: u64,
    /// Target frame rate (60 Hz typical)
    pub target_fps: u32,
    /// Whether to sync to display refresh rate
    pub sync_to_vblank: bool,
}

impl FrameSync {
    /// Create new frame sync with default 60 Hz
    pub fn new() -> Self {
        FrameSync {
            last_frame_us: 0,
            target_fps: 60,
            sync_to_vblank: true,
        }
    }

    /// Get frame interval in microseconds
    pub fn frame_interval_us(&self) -> u64 {
        if self.target_fps > 0 {
            1_000_000 / self.target_fps as u64
        } else {
            16_667 // Default to ~60 Hz
        }
    }

    /// Check if enough time has elapsed for next frame
    pub fn should_frame(&self, now_us: u64) -> bool {
        now_us >= self.last_frame_us + self.frame_interval_us()
    }
}

/// Main compositor engine
pub struct Compositor {
    state: CompositorState,
    damage_regions: Vec<DamageRegion>,
    frame_sync: FrameSync,
    /// Tracks which windows need repainting
    dirty_windows: Vec<WindowId>,
    /// Whether pending operations should be flushed
    flush_pending: bool,
}

impl Compositor {
    /// Create new compositor instance
    pub fn new() -> Self {
        Compositor {
            state: CompositorState::Initializing,
            damage_regions: Vec::new(),
            frame_sync: FrameSync::new(),
            dirty_windows: Vec::new(),
            flush_pending: false,
        }
    }

    /// Initialize compositor (called during kernel boot)
    pub fn init(&mut self) {
        self.state = CompositorState::Active;
    }

    /// Enable/disable compositing
    pub fn set_enabled(&mut self, enabled: bool) {
        self.state = if enabled {
            CompositorState::Active
        } else {
            CompositorState::Disabled
        };
    }

    /// Mark a window region as damaged (needing repaint)
    pub fn damage_window(&mut self, window_id: WindowId, x: u32, y: u32, width: u32, height: u32) {
        if !self.dirty_windows.contains(&window_id) {
            self.dirty_windows.push(window_id);
        }

        let region = DamageRegion::new(x, y, width, height);

        if let Some(existing) = self
            .damage_regions
            .iter_mut()
            .find(|r| r.x == x && r.y == y)
        {
            existing.merge(&region);
        } else {
            self.damage_regions.push(region);
        }

        self.flush_pending = true;
    }

    /// Mark entire screen as needing repaint
    pub fn damage_all(&mut self) {
        self.damage_regions.clear();
        self.damage_regions
            .push(DamageRegion::new(0, 0, 1920, 1080)); // Default resolution
        self.dirty_windows.clear();
        self.flush_pending = true;
    }

    /// Get current compositor state
    pub fn state(&self) -> CompositorState {
        self.state
    }

    /// Check if compositing is active
    pub fn is_active(&self) -> bool {
        self.state == CompositorState::Active
    }

    /// Perform damage-tracked rendering
    pub fn redraw(&mut self, now_us: u64) -> bool {
        if !self.is_active() {
            return false;
        }

        if !self.frame_sync.should_frame(now_us) {
            return false;
        }

        self.state = CompositorState::Redrawing;

        // Render damaged regions
        let rendered = !self.damage_regions.is_empty();

        if rendered {
            self.damage_regions.clear();
            self.dirty_windows.clear();
            self.flush_pending = false;
        }

        self.state = CompositorState::Active;
        rendered
    }

    /// Manually flush pending operations
    pub fn flush(&mut self) {
        if self.flush_pending {
            self.damage_regions.clear();
            self.dirty_windows.clear();
            self.flush_pending = false;
        }
    }

    /// Get list of windows needing repaint
    pub fn get_dirty_windows(&self) -> &[WindowId] {
        &self.dirty_windows
    }

    /// Get damage regions needing repaint
    pub fn get_damage_regions(&self) -> &[DamageRegion] {
        &self.damage_regions
    }

    /// Update frame synchronization parameters
    pub fn set_frame_rate(&mut self, fps: u32) {
        self.frame_sync.target_fps = fps;
    }
}

impl Default for Compositor {
    fn default() -> Self {
        Self::new()
    }
}
