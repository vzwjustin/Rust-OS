//! Stage Impl Private — ported from GNOME Mutter
//!
//! Private implementation details for stage frame and view management.
//! Contains methods for frame info and view rebuild operations.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stage-impl-private.h

/// Private stage implementation state for frame and view management.
pub struct MetaStageImplPrivate {
    /// Whether the stage views need rebuilding.
    pub views_dirty: bool,
    /// Current frame counter.
    pub frame_counter: u64,
    /// Whether the stage is currently painting.
    pub painting: bool,
}

impl MetaStageImplPrivate {
    /// Create a new stage impl private state.
    pub fn new() -> Self {
        Self {
            views_dirty: false,
            frame_counter: 0,
            painting: false,
        }
    }

    /// Mark the stage views as needing rebuild.
    pub fn mark_views_dirty(&mut self) {
        self.views_dirty = true;
    }

    /// Check if views need rebuilding.
    pub fn needs_view_rebuild(&self) -> bool {
        self.views_dirty
    }

    /// Clear the views dirty flag after rebuild.
    pub fn clear_views_dirty(&mut self) {
        self.views_dirty = false;
    }

    /// Increment the frame counter (called at start of each frame).
    pub fn begin_frame(&mut self) {
        self.frame_counter += 1;
        self.painting = true;
    }

    /// Mark the end of a frame.
    pub fn end_frame(&mut self) {
        self.painting = false;
    }

    /// Whether the stage is currently painting.
    pub fn is_painting(&self) -> bool {
        self.painting
    }

    /// Get the current frame counter.
    pub fn get_frame_counter(&self) -> u64 {
        self.frame_counter
    }
}

impl Default for MetaStageImplPrivate {
    fn default() -> Self {
        Self::new()
    }
}
