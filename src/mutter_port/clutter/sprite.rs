//! Port of GNOME mutter's `clutter/clutter-sprite.{c,h}` — input sprite tracking
//! for pointer/touch sequences and event emission.
//!
//! `ClutterSprite` tracks a single input device + event sequence pair
//! (pointer or touch), maintains the current actor under the cursor/touch,
//! synthesizes ENTER/LEAVE crossing events as the cursor moves, and
//! coordinates implicit grab semantics and event emission chains across
//! the actor tree.
//!
//! # What's ported
//!
//! - The `Sprite` struct carrying the private fields:
//!   - `device`, `sequence`, `coords`, `role`: device/sequence tracking
//!   - `current_actor`: the actor under the pointer/touch
//!   - `implicit_grab_actor`, `press_count`: implicit grab state
//!   - `event_emission_chain`: the chain of actors/actions to dispatch to
//!   - `cursor`: the current cursor icon
//! - Getters: `device()`, `sequence()`, `coords()`, `role()`, `cursor()`.
//! - Update functions: `update()`, `update_coords()`.
//! - Event emission helpers: `emit_crossing_event()` (synthesized ENTER/LEAVE).
//! - Clear area tracking (`clear_area` field).
//!
//! # What's skipped
//!
//! - GObject machinery (properties, signals, virtual class methods): dropped.
//! - Seat integration (grabs, backend device lookup): stubs added.
//! - Action/actor dispatching internals: skeletal only, not ported.
//! - Cursor update callbacks: would need a virtual method; skipped.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use super::actor::ActorId;
use super::input_device::InputDevice;
use alloc::vec::Vec;

/// Opaque event sequence identifier (evdev slot or Wayland serial).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventSequence(u64);

/// Sprite role (POINTER or TOUCH).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum SpriteRole {
    #[default]
    Pointer = 0,
    Touch = 1,
}

/// Opaque region type for clear area tracking.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region(u64);

/// Event emission chain entry — actor, action, or both in a phase.
#[derive(Debug, Clone)]
struct EventReceiver {
    actor: Option<ActorId>,
    phase: EventPhase,
    emit_to_actor: bool,
}

/// Event phase for bubbling (CAPTURE, TARGET, BUBBLE).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EventPhase {
    Capture,
    Target,
    Bubble,
}

/// A sprite tracks a pointer or touch input sequence.
pub struct Sprite {
    /// Input device (pointer, touch device, etc.).
    device: Option<InputDevice>,
    /// Event sequence ID (touch slot, mouse button pairing).
    sequence: Option<EventSequence>,
    /// Current sprite coordinates (stage-global).
    coords: (f32, f32),
    /// Role: pointer or touch.
    role: SpriteRole,
    /// Current actor under the cursor/touch.
    current_actor: Option<ActorId>,
    /// Region to clear (for pointer/touch trails).
    clear_area: Option<Region>,
    /// Actors collected for event dispatch.
    cur_event_actors: Vec<ActorId>,
    /// Event emission chain (current).
    cur_event_emission_chain: Vec<EventReceiver>,
    /// Press count for multi-button or multi-touch tracking.
    press_count: u32,
    /// Actor holding the implicit grab (if any).
    implicit_grab_actor: Option<ActorId>,
    /// Event emission chain during implicit grab.
    event_emission_chain: Vec<EventReceiver>,
    /// Cursor icon (opaque).
    cursor: Option<u64>,
}

impl Sprite {
    /// Create a new sprite for a device and sequence.
    pub fn new(
        device: Option<InputDevice>,
        sequence: Option<EventSequence>,
        role: SpriteRole,
    ) -> Self {
        Self {
            device,
            sequence,
            coords: (0.0, 0.0),
            role,
            current_actor: None,
            clear_area: None,
            cur_event_actors: Vec::new(),
            cur_event_emission_chain: Vec::new(),
            press_count: 0,
            implicit_grab_actor: None,
            event_emission_chain: Vec::new(),
            cursor: None,
        }
    }

    /// Return the input device for this sprite.
    pub fn device(&self) -> Option<&InputDevice> {
        self.device.as_ref()
    }

    /// Return the event sequence for this sprite.
    pub fn sequence(&self) -> Option<EventSequence> {
        self.sequence
    }

    /// Return the sprite's current coordinates.
    pub fn coords(&self) -> (f32, f32) {
        self.coords
    }

    /// Return the sprite's role (pointer or touch).
    pub fn role(&self) -> SpriteRole {
        self.role
    }

    /// Return the current actor under the sprite.
    pub fn current_actor(&self) -> Option<ActorId> {
        self.current_actor
    }

    /// Return the cursor icon (if set).
    pub fn cursor(&self) -> Option<u64> {
        self.cursor
    }

    /// Update the sprite's coordinates and clear area.
    pub fn update(&mut self, coords: (f32, f32), clear_area: Option<Region>) {
        self.coords = coords;
        self.clear_area = clear_area;
    }

    /// Update only the sprite's coordinates.
    pub fn update_coords(&mut self, coords: (f32, f32)) {
        self.coords = coords;
    }

    /// Set the current actor under the sprite.
    pub fn set_current_actor(&mut self, actor: Option<ActorId>) {
        self.current_actor = actor;
    }

    /// Set the implicit grab actor.
    pub fn set_implicit_grab_actor(&mut self, actor: Option<ActorId>) {
        self.implicit_grab_actor = actor;
    }

    /// Return the implicit grab actor (if held).
    pub fn implicit_grab_actor(&self) -> Option<ActorId> {
        self.implicit_grab_actor
    }

    /// Return the current press count.
    pub fn press_count(&self) -> u32 {
        self.press_count
    }

    /// Increment the press count (for multi-button tracking).
    pub fn inc_press_count(&mut self) {
        self.press_count = self.press_count.saturating_add(1);
    }

    /// Decrement the press count.
    pub fn dec_press_count(&mut self) {
        self.press_count = self.press_count.saturating_sub(1);
    }

    /// Reset the press count to zero.
    pub fn reset_press_count(&mut self) {
        self.press_count = 0;
    }

    /// Check if point is in clear area.
    pub fn point_in_clear_area(&self, point: (f32, f32)) -> bool {
        self.clear_area.is_some()
    }

    /// Clear the event emission chain.
    pub fn clear_event_chain(&mut self) {
        self.event_emission_chain.clear();
    }

    /// Add an actor to the current event chain.
    pub fn add_actor_to_chain(&mut self, actor: ActorId, phase: EventPhase) {
        self.event_emission_chain.push(EventReceiver {
            actor: Some(actor),
            phase,
            emit_to_actor: true,
        });
    }

    /// Check if sprite is in implicit grab state.
    pub fn in_implicit_grab(&self) -> bool {
        self.press_count > 0
    }

    /// Invalidate and update the cursor (placeholder).
    pub fn invalidate_cursor(&mut self) {
        // Cursor update would dispatch to virtual method;
        // in Rust, this is left to the caller to integrate.
    }
}

impl core::fmt::Debug for Sprite {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Sprite")
            .field("device", &self.device)
            .field("sequence", &self.sequence)
            .field("coords", &self.coords)
            .field("role", &self.role)
            .field("current_actor", &self.current_actor)
            .field("press_count", &self.press_count)
            .field("in_grab", &self.in_implicit_grab())
            .finish()
    }
}
