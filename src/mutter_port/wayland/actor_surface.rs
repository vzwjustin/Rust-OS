//! Wayland actor surface implementation ported from GNOME Mutter's src/wayland/meta-wayland-actor-surface.c
//!
//! Implements MetaWaylandActorSurface which bridges Wayland surfaces to rendering/display actors.
//! Handles synchronization of actor state with surface state and frame callback emission.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-actor-surface.c

use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::option::Option;

/// Represents a frame callback scheduled on a surface.
#[derive(Debug, Clone, Copy)]
pub struct FrameCallback {
    /// Unique callback ID (serial number).
    callback_id: u32,
    /// Timestamp when callback was registered (milliseconds).
    registered_time_ms: u64,
}

/// Represents a surface actor for rendering and display.
#[derive(Debug)]
pub struct MetaWaylandActorSurface {
    /// Unique surface ID.
    surface_id: u32,

    /// Current actor state synchronized with surface state.
    actor_state: ActorState,

    /// Pending actor state waiting to be applied.
    pending_actor_state: Option<ActorState>,

    /// Pending frame callbacks waiting to be emitted.
    frame_callbacks: VecDeque<FrameCallback>,

    /// Geometry scale (for HiDPI displays).
    geometry_scale: u32,

    /// Whether the actor needs a state sync.
    needs_sync: bool,

    /// Whether the actor is currently visible.
    visible: bool,
}

/// Represents the state of a surface's actor.
#[derive(Debug, Clone, Copy)]
pub struct ActorState {
    /// X coordinate in global space.
    x: i32,
    /// Y coordinate in global space.
    y: i32,
    /// Width in pixels.
    width: u32,
    /// Height in pixels.
    height: u32,
    /// Opacity (0.0 to 1.0).
    opacity: f32,
    /// Rotation in degrees.
    rotation: f32,
}

impl Default for ActorState {
    fn default() -> Self {
        ActorState {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
            opacity: 1.0,
            rotation: 0.0,
        }
    }
}

impl MetaWaylandActorSurface {
    /// Create a new actor surface.
    pub fn new(surface_id: u32) -> Self {
        MetaWaylandActorSurface {
            surface_id,
            actor_state: ActorState::default(),
            pending_actor_state: None,
            frame_callbacks: VecDeque::new(),
            geometry_scale: 1,
            needs_sync: true,
            visible: false,
        }
    }

    /// Get the surface ID.
    pub fn surface_id(&self) -> u32 {
        self.surface_id
    }

    /// Get current actor state.
    pub fn actor_state(&self) -> ActorState {
        self.actor_state
    }

    /// Set pending actor state changes.
    pub fn set_pending_state(&mut self, state: ActorState) {
        self.pending_actor_state = Some(state);
        self.needs_sync = true;
    }

    /// Apply pending actor state changes.
    pub fn apply_pending_state(&mut self) {
        if let Some(state) = self.pending_actor_state.take() {
            self.actor_state = state;
            self.needs_sync = false;
        }
    }

    /// Set geometry scale (for HiDPI support).
    pub fn set_geometry_scale(&mut self, scale: u32) {
        if scale > 0 {
            self.geometry_scale = scale;
        }
    }

    /// Get geometry scale.
    pub fn geometry_scale(&self) -> u32 {
        self.geometry_scale
    }

    /// Check if actor needs synchronization.
    pub fn needs_sync(&self) -> bool {
        self.needs_sync
    }

    /// Mark actor as needing sync.
    pub fn mark_needs_sync(&mut self) {
        self.needs_sync = true;
    }

    /// Set visibility.
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Check if actor is visible.
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Queue a frame callback to be emitted.
    pub fn queue_frame_callback(&mut self, callback_id: u32, registered_time_ms: u64) {
        self.frame_callbacks.push_back(FrameCallback {
            callback_id,
            registered_time_ms,
        });
    }

    /// Get number of pending frame callbacks.
    pub fn frame_callback_count(&self) -> usize {
        self.frame_callbacks.len()
    }

    /// STUB: Synchronize actor state based on surface state changes.
    /// This would typically be called after a surface commit to update the rendering actor.
    pub fn sync_actor_state(&mut self) {
        // STUB: Implementation would:
        // - Call vfunc sync_actor_state on parent class
        // - Update Clutter actor properties (position, size, opacity, rotation)
        // - Handle subsurface actor hierarchies
        // - Trigger redraw if needed
        self.needs_sync = false;
    }

    /// STUB: Emit frame callbacks to clients.
    /// Called when frame is presented to indicate refresh rate completion.
    pub fn emit_frame_callbacks(&mut self, timestamp_ms: u64) -> u32 {
        // STUB: Implementation would:
        // - Iterate pending frame callbacks
        // - Send wl_callback.done event with timestamp
        // - Remove emitted callbacks
        // - Return number of callbacks emitted
        let count = self.frame_callbacks.len() as u32;
        self.frame_callbacks.clear();
        count
    }

    /// STUB: Queue frame callbacks from pending surface state.
    /// Called during surface commit to register callbacks that will fire on next frame.
    pub fn queue_frame_callbacks_from_pending(&mut self) {
        // STUB: Implementation would:
        // - Extract frame callback list from pending surface state
        // - Register each callback with current time
        // - Link to frame callback source for emission
    }

    /// STUB: Reset actor state to initial/unmapped state.
    /// Called when surface is unmapped or destroyed.
    pub fn reset_actor(&mut self) {
        // STUB: Implementation would:
        // - Clear the Clutter actor
        // - Reset all state
        // - Emit unmapped signals
        self.actor_state = ActorState::default();
        self.frame_callbacks.clear();
        self.visible = false;
    }

    /// STUB: Check if actor is on a specific logical monitor.
    /// Used for output tracking and monitor-specific behavior.
    pub fn is_on_logical_monitor(&self, monitor_id: u32) -> bool {
        // STUB: Implementation would:
        // - Check actor allocation against monitor geometry
        // - Handle partial coverage
        // - Return true if surface is visible on this monitor
        false
    }
}

/// Error type for actor surface operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActorSurfaceError {
    /// Invalid surface reference.
    InvalidSurface,
    /// Actor creation failed.
    ActorCreationFailed,
    /// State application failed.
    StateApplicationFailed,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actor_surface_creation() {
        let surface = MetaWaylandActorSurface::new(1);
        assert_eq!(surface.surface_id(), 1);
        assert_eq!(surface.geometry_scale(), 1);
        assert!(!surface.is_visible());
    }

    #[test]
    fn actor_state_management() {
        let mut surface = MetaWaylandActorSurface::new(1);
        let state = ActorState {
            x: 100,
            y: 200,
            width: 800,
            height: 600,
            opacity: 0.8,
            rotation: 0.0,
        };

        surface.set_pending_state(state);
        assert!(surface.needs_sync());

        surface.apply_pending_state();
        assert!(!surface.needs_sync());

        let applied_state = surface.actor_state();
        assert_eq!(applied_state.x, 100);
        assert_eq!(applied_state.y, 200);
    }

    #[test]
    fn frame_callback_queueing() {
        let mut surface = MetaWaylandActorSurface::new(1);
        assert_eq!(surface.frame_callback_count(), 0);

        surface.queue_frame_callback(1, 1000);
        surface.queue_frame_callback(2, 1000);
        assert_eq!(surface.frame_callback_count(), 2);

        let count = surface.emit_frame_callbacks(2000);
        assert_eq!(count, 2);
        assert_eq!(surface.frame_callback_count(), 0);
    }

    #[test]
    fn visibility_tracking() {
        let mut surface = MetaWaylandActorSurface::new(1);
        assert!(!surface.is_visible());

        surface.set_visible(true);
        assert!(surface.is_visible());

        surface.set_visible(false);
        assert!(!surface.is_visible());
    }
}
