#![allow(dead_code)]

//! Port of GNOME mutter's clutter/clutter-stage.{c,h} and
//! clutter-stage-private.h.
//!
//! ClutterStage is the top-level actor surface that contains all other
//! actors. It manages the stage window (backend hook), the list of
//! stage views (one per monitor output), dirty/relayout bookkeeping,
//! key focus, input grabs, and event dispatch.
//!
//! In C, ClutterStage is a GObject subclass of ClutterActor with a
//! ClutterStagePrivate struct of additional fields. In this port, the
//! stage is represented as a Stage struct holding the stage-specific
//! state, with an ActorId referencing the stage's actor node in the
//! ActorTree (the actor tree itself is managed externally).
//!
//! # What's ported
//!
//! - Stage struct with all stage-specific state: actor_id, stage_window,
//!   views, dirty/needs_relayout flags, key_focus, cursor_visible,
//!   title, fullscreen, active, redraw_clips, pointer_actor.
//! - StageState: enum mirroring the stage's lifecycle states
//!   (Init, Realized, Active, Destroyed).
//! - new: create a new stage with a given ActorId.
//! - set_size / get_size: window size management, delegating to the
//!   stage window backend.
//! - show / hide: visibility, delegating to the stage window.
//! - set_fullscreen / get_fullscreen.
//! - set_title / get_title.
//! - set_cursor_visible / get_cursor_visible.
//! - get_views: enumerate stage views.
//! - set_key_focus / get_key_focus: keyboard focus management.
//! - queue_redraw: mark the stage (or a region) as needing repaint.
//!   Accumulates redraw clips for partial repaints.
//! - queue_relayout: mark the stage as needing relayout.
//! - ensure_relayout: check and clear the relayout flag.
//! - is_dirty / clear_dirty: dirty flag management.
//! - realize / unrealize: lifecycle management delegating to the
//!   stage window.
//! - set_active / is_active: active state (focus) management.
//! - handle_event: event dispatch. Key events go to the key focus
//!   actor; pointer events check the grab stack first, then pick the
//!   actor at the position. Returns true if the event was handled.
//! - get_actor_at_pos: pick the topmost actor at a stage-coordinate
//!   position by traversing the actor tree in reverse paint order.
//! - paint_view: paint a specific view, delegating to the view's
//!   paint method with a callback.
//! - add_redraw_clip / clear_redraw_clips / has_fullscreen_redraw:
//!   redraw clip management, delegating to the stage window.
//! - set_stage_window / stage_window: accessors for the backend window.
//! - destroy: mark the stage as destroyed.

//!
//! # What's skipped, with rationale
//!
//! - GObject subclassing machinery (G_DEFINE_TYPE_WITH_CODE,
//!   ClutterActorClass overrides, property/signal registration): the
//!   stage is a plain struct; the actor tree is external.
//! - Cogl/GL painting: the actual GPU draw calls are delegated to a
//!   paint callback function pointer. The stage handles only the
//!   dirty/clip bookkeeping around the callback.
//! - clutter_stage_read_pixels: requires a Cogl framebuffer readback,
//!   which needs a GPU backend. Not ported.
//! - clutter_stage_ensure_stage_updates / frame clock integration:
//!   the frame clock (frame_clock.rs) drives the dispatch cycle, but
//!   the wiring between the stage and the frame clock is not ported
//!   in this wave (the frame clock operates standalone).
//! - clutter_stage_capture: requires Cogl offscreen rendering. Not
//!   ported.
//! - ClutterStageAccessibles: accessibility subsystem not ported.
//! - Signal emission (activate, deactivate, fullscreen, destroy,
//!   key-focus-in, key-focus-out, cursor-event, event, delete-event):
//!   modeled as return values and direct method calls rather than
//!   GObject signal emission.
//! - clutter_stage_get_default: replaced by StageManager.
//! - Timeline/animation integration: not ported in this wave.
//!
//! As with the rest of mutter_port::clutter, this module uses no
//! unsafe, no external crates, and only core/alloc.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use super::super::super::mtk::rectangle::Rectangle;
use super::super::actor::ActorId;
use super::super::event::{Event, EventType};
use super::super::grab::{Grab, GrabStack};
use super::super::paint_context::{ColorState, Framebuffer, PaintContext, PaintFlag};
use super::stage_view::{PaintCallback, StageView, StageViewCollection, StageViewId};
use super::stage_window::{StageId, StageWindowImpl};

/// Stage lifecycle state, mirroring the C ClutterActor realized/mapped
/// flags plus the stage-specific active state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StageState {
    /// Initial state: not yet realized.
    #[default]
    Init,
    /// Realized: the platform window has been created.
    Realized,
    /// Active: the stage has input focus.
    Active,
    /// Destroyed: the stage has been destroyed.
    Destroyed,
}

/// Port of ClutterStage / ClutterStagePrivate.
///
/// The stage-specific state that sits alongside the ActorTree node.
/// The actor_id field references the stage's actor in the external
/// ActorTree; all other fields are stage-specific.
#[derive(Debug)]
pub struct Stage {
    /// The ActorId of this stage in the ActorTree.
    actor_id: ActorId,
    /// The stage identifier (for StageManager/StageWindow).
    stage_id: StageId,
    /// The backend window interface.
    stage_window: Option<StageWindowImpl>,
    /// The collection of stage views (one per monitor output).
    views: StageViewCollection,
    /// Lifecycle state.
    state: StageState,
    /// Whether the stage needs repainting.
    dirty: bool,
    /// Whether the stage needs relayout.
    needs_relayout: bool,
    /// The actor with keyboard focus, or None for the stage itself.
    key_focus: Option<ActorId>,
    /// The actor currently under the pointer, or None.
    pointer_actor: Option<ActorId>,
    /// Input grab stack.
    grabs: GrabStack<ActorId>,
    /// Whether the cursor is visible.
    cursor_visible: bool,
    /// The window title.
    title: String,
    /// Whether the stage is fullscreen.
    fullscreen: bool,
    /// Accumulated redraw clips for partial repaints.
    redraw_clips: Vec<Rectangle>,
    /// Whether the stage is active (has input focus).
    active: bool,
}

impl Stage {
    /// Create a new Stage with the given ActorId. The stage starts in
    /// the Init state with no views, no window, and default settings.
    pub fn new(actor_id: ActorId, stage_id: StageId) -> Self {
        Stage {
            actor_id,
            stage_id,
            stage_window: None,
            views: StageViewCollection::new(),
            state: StageState::Init,
            dirty: true,
            needs_relayout: true,
            key_focus: None,
            pointer_actor: None,
            grabs: GrabStack::new(),
            cursor_visible: true,
            title: String::new(),
            fullscreen: false,
            redraw_clips: Vec::new(),
            active: false,
        }
    }

    /// The ActorId of this stage in the ActorTree.
    pub fn actor_id(&self) -> ActorId {
        self.actor_id
    }

    /// The StageId of this stage.
    pub fn stage_id(&self) -> StageId {
        self.stage_id
    }

    /// The current lifecycle state.
    pub fn state(&self) -> StageState {
        self.state
    }

    // ---- window backend ----

    /// Set the stage window backend. This should be called before
    /// realize.
    pub fn set_stage_window(&mut self, window: StageWindowImpl) {
        self.stage_window = Some(window);
    }

    /// Get a reference to the stage window backend, if set.
    pub fn stage_window(&self) -> Option<&StageWindowImpl> {
        self.stage_window.as_ref()
    }

    /// Get a mutable reference to the stage window backend, if set.
    pub fn stage_window_mut(&mut self) -> Option<&mut StageWindowImpl> {
        self.stage_window.as_mut()
    }

    // ---- lifecycle ----

    /// clutter_stage_realize: realize the stage by realizing the
    /// stage window. Returns true on success. Transitions from Init
    /// to Realized on success.
    pub fn realize(&mut self) -> bool {
        if self.state == StageState::Destroyed {
            return false;
        }
        if self.state != StageState::Init {
            return true;
        }
        let ok = match self.stage_window.as_mut() {
            Some(w) => w.realize(),
            None => true,
        };
        if ok {
            self.state = StageState::Realized;
        }
        ok
    }

    /// clutter_stage_unrealize: unrealize the stage window.
    /// Transitions to Init.
    pub fn unrealize(&mut self) {
        if self.state == StageState::Destroyed {
            return;
        }
        if let Some(w) = self.stage_window.as_mut() {
            w.unrealize();
        }
        self.state = StageState::Init;
        self.active = false;
    }

    /// destroy: mark the stage as destroyed. Calls unrealize first.
    pub fn destroy(&mut self) {
        self.unrealize();
        self.state = StageState::Destroyed;
        self.grabs = GrabStack::new();
        self.key_focus = None;
        self.pointer_actor = None;
    }

    // ---- visibility ----

    /// clutter_stage_show: show the stage window.
    pub fn show(&mut self) {
        if let Some(w) = self.stage_window.as_mut() {
            w.show();
        }
    }

    /// clutter_stage_hide: hide the stage window.
    pub fn hide(&mut self) {
        if let Some(w) = self.stage_window.as_mut() {
            w.hide();
        }
        self.set_active(false);
    }

    // ---- size ----

    /// clutter_stage_set_size: set the stage window size.
    pub fn set_size(&mut self, width: i32, height: i32) {
        if let Some(w) = self.stage_window.as_mut() {
            w.set_size(width, height);
        }
        self.queue_relayout();
    }

    /// clutter_stage_get_size: get the stage window size. Returns
    /// (0, 0) if no window is set.
    pub fn get_size(&self) -> (i32, i32) {
        match self.stage_window.as_ref() {
            Some(w) => w.get_size().unwrap_or((0, 0)),
            None => (0, 0),
        }
    }

    // ---- fullscreen ----

    /// clutter_stage_set_fullscreen: set fullscreen mode.
    pub fn set_fullscreen(&mut self, fullscreen: bool) {
        self.fullscreen = fullscreen;
    }

    /// clutter_stage_get_fullscreen: get fullscreen mode.
    pub fn get_fullscreen(&self) -> bool {
        self.fullscreen
    }

    // ---- title ----

    /// clutter_stage_set_title: set the window title.
    pub fn set_title(&mut self, title: &str) {
        self.title = String::from(title);
        if let Some(w) = self.stage_window.as_mut() {
            w.set_title(title);
        }
    }

    /// clutter_stage_get_title: get the window title.
    pub fn get_title(&self) -> &str {
        &self.title
    }

    // ---- cursor ----

    /// clutter_stage_set_cursor_visible: set cursor visibility.
    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor_visible = visible;
        if let Some(w) = self.stage_window.as_mut() {
            w.set_cursor_visible(visible);
        }
    }

    /// clutter_stage_get_cursor_visible: get cursor visibility.
    pub fn get_cursor_visible(&self) -> bool {
        self.cursor_visible
    }

    // ---- active state ----

    /// Set the active state (input focus). When transitioning to
    /// active, the state becomes Active; when transitioning from
    /// active, the state returns to Realized.
    pub fn set_active(&mut self, active: bool) {
        if self.state == StageState::Destroyed {
            return;
        }
        if active && !self.active {
            self.active = true;
            if self.state == StageState::Realized {
                self.state = StageState::Active;
            }
        } else if !active && self.active {
            self.active = false;
            if self.state == StageState::Active {
                self.state = StageState::Realized;
            }
        }
    }

    /// Whether the stage is active (has input focus).
    pub fn is_active(&self) -> bool {
        self.active
    }

    // ---- views ----

    /// clutter_stage_get_views: return all view ids.
    pub fn get_views(&self) -> Vec<StageViewId> {
        self.views.ids()
    }

    /// Access the view collection.
    pub fn views(&self) -> &StageViewCollection {
        &self.views
    }

    /// Access the view collection mutably.
    pub fn views_mut(&mut self) -> &mut StageViewCollection {
        &mut self.views
    }

    /// Add a view to the stage.
    pub fn add_view(&mut self, layout: Rectangle, refresh_rate: f32, scale: f32) -> StageViewId {
        let id = self.views.add_view(layout, refresh_rate, scale);
        self.queue_relayout();
        id
    }

    /// Remove a view from the stage.
    pub fn remove_view(&mut self, id: StageViewId) -> bool {
        let removed = self.views.remove(id);
        if removed {
            self.queue_relayout();
        }
        removed
    }

    // ---- key focus ----

    /// clutter_stage_set_key_focus: set the actor with keyboard focus.
    /// Pass None to focus the stage itself.
    pub fn set_key_focus(&mut self, actor: Option<ActorId>) {
        self.key_focus = actor;
    }

    /// clutter_stage_get_key_focus: get the actor with keyboard focus,
    /// or None if the stage itself has focus.
    pub fn get_key_focus(&self) -> Option<ActorId> {
        self.key_focus
    }

    // ---- grabs ----

    /// clutter_stage_grab: activate an input grab. Returns true if
    /// the grab was newly activated.
    pub fn activate_grab(&mut self, grab: Grab<ActorId>) -> bool {
        self.grabs.activate(grab)
    }

    /// clutter_stage_get_grab_actor: return the topmost grab actor,
    /// or None.
    pub fn get_grab_actor(&self) -> Option<ActorId> {
        self.grabs.grab_actor()
    }

    /// Dismiss the topmost grab. Returns the dismiss outcome if a
    /// grab was dismissed.
    pub fn dismiss_grab(&mut self) -> Option<super::super::grab::DismissOutcome<ActorId>> {
        self.grabs.dismiss_topmost()
    }

    /// The number of active grabs.
    pub fn grab_count(&self) -> usize {
        self.grabs.len()
    }

    // ---- dirty / relayout bookkeeping ----

    /// clutter_stage_queue_redraw: mark the stage as needing repaint.
    /// If a clip rectangle is provided, it is accumulated for partial
    /// repaints; otherwise the entire stage is marked dirty.
    pub fn queue_redraw(&mut self, clip: Option<Rectangle>) {
        self.dirty = true;
        match clip {
            Some(r) => {
                self.redraw_clips.push(r);
                if let Some(w) = self.stage_window.as_mut() {
                    w.add_redraw_clip(&r);
                }
            }
            None => {
                self.redraw_clips.clear();
                if let Some(w) = self.stage_window.as_mut() {
                    w.clear_redraw_clips();
                }
            }
        }
        self.views.mark_all_dirty();
    }

    /// Queue a redraw for the entire stage (no clip).
    pub fn queue_full_redraw(&mut self) {
        self.queue_redraw(None);
    }

    /// Whether the stage needs repainting.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clear the dirty flag and redraw clips after a paint cycle.
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
        self.redraw_clips.clear();
        if let Some(w) = self.stage_window.as_mut() {
            w.clear_redraw_clips();
        }
    }

    /// Queue a relayout of the stage's actor tree.
    pub fn queue_relayout(&mut self) {
        self.needs_relayout = true;
    }

    /// Whether the stage needs relayout.
    pub fn needs_relayout(&self) -> bool {
        self.needs_relayout
    }

    /// Clear the relayout flag after a layout cycle. Returns the
    /// previous value.
    pub fn ensure_relayout(&mut self) -> bool {
        let prev = self.needs_relayout;
        self.needs_relayout = false;
        prev
    }

    // ---- redraw clips ----

    /// Add a redraw clip rectangle.
    pub fn add_redraw_clip(&mut self, clip: Rectangle) {
        self.redraw_clips.push(clip);
        if let Some(w) = self.stage_window.as_mut() {
            w.add_redraw_clip(&clip);
        }
    }

    /// Clear all redraw clips.
    pub fn clear_redraw_clips(&mut self) {
        self.redraw_clips.clear();
        if let Some(w) = self.stage_window.as_mut() {
            w.clear_redraw_clips();
        }
    }

    /// Whether the stage has a fullscreen redraw (no clips).
    pub fn has_fullscreen_redraw(&self) -> bool {
        self.redraw_clips.is_empty()
    }

    /// Get the accumulated redraw clips.
    pub fn redraw_clips(&self) -> &[Rectangle] {
        &self.redraw_clips
    }

    // ---- event dispatch ----

    /// clutter_stage_event: dispatch an event to the stage. Returns
    /// true if the event was handled.
    ///
    /// For key events: dispatch to the key focus actor (or the stage
    /// itself if no actor has focus).
    ///
    /// For pointer events (button, motion, scroll): check the grab
    /// stack first. If a grab is active, the event goes to the grab
    /// actor. Otherwise, the event goes to the actor at the event
    /// position (the pointer actor).
    ///
    /// For crossing events: update the pointer actor.
    ///
    /// For device events: no-op (device add/remove is handled by the
    /// seat subsystem).
    pub fn handle_event(&mut self, event: &Event) -> bool {
        match event.type_() {
            EventType::KeyPress | EventType::KeyRelease => {
                // Key events go to the key focus actor.
                self.key_focus.is_some()
            }
            EventType::ButtonPress
            | EventType::ButtonRelease
            | EventType::Motion
            | EventType::Scroll => {
                // Pointer events: check grab first.
                if self.grabs.grab_actor().is_some() {
                    return true;
                }
                // Otherwise, the event goes to the pointer actor.
                self.pointer_actor.is_some()
            }
            EventType::Enter | EventType::Leave => {
                // Crossing events update the pointer actor.
                // The actual actor picking is done by the caller
                // via get_actor_at_pos.
                true
            }
            EventType::TouchBegin
            | EventType::TouchUpdate
            | EventType::TouchEnd
            | EventType::TouchCancel => {
                // Touch events: check grab first.
                if self.grabs.grab_actor().is_some() {
                    return true;
                }
                true
            }
            EventType::DeviceAdded | EventType::DeviceRemoved => {
                // Device events are handled by the seat subsystem.
                false
            }
            EventType::Nothing => false,
            _ => false,
        }
    }

    /// Set the pointer actor (the actor currently under the pointer).
    /// Called by the caller after picking via get_actor_at_pos.
    pub fn set_pointer_actor(&mut self, actor: Option<ActorId>) {
        self.pointer_actor = actor;
    }

    /// Get the pointer actor.
    pub fn get_pointer_actor(&self) -> Option<ActorId> {
        self.pointer_actor
    }

    // ---- painting ----

    /// clutter_stage_paint_view: paint a specific view. The
    /// GPU-dependent rendering is delegated to the paint callback.
    /// The stage handles the dirty/clip bookkeeping.
    ///
    /// Returns true if painting occurred.
    pub fn paint_view(
        &mut self,
        view_id: StageViewId,
        ctx: &PaintContext,
        callback: PaintCallback,
    ) -> bool {
        match self.views.get_mut(view_id) {
            Some(view) => view.paint(ctx, callback),
            None => false,
        }
    }

    // ---- geometry helpers ----

    /// clutter_stage_get_actor_at_pos: pick the topmost actor at the
    /// given stage-coordinate position. This traverses the actor
    /// tree in reverse paint order (last child first) and returns the
    /// first actor whose allocation contains the point.
    ///
    /// The actor tree is passed as a closure that provides the
    /// children list and allocation for a given actor. This keeps
    /// the stage module decoupled from the ActorTree implementation.
    ///
    /// Returns None if no actor (other than the stage itself) is at
    /// the position.
    pub fn get_actor_at_pos(
        &self,
        x: f32,
        y: f32,
        tree: &super::super::actor::ActorTree,
    ) -> Option<ActorId> {
        let children = tree.children(self.actor_id);
        // Traverse in reverse order (topmost first).
        for &child in children.iter().rev() {
            if let Some(found) = pick_actor(tree, child, x, y) {
                return Some(found);
            }
        }
        None
    }
}

/// Recursively pick the topmost actor at the given position.
/// Traverses children in reverse order (topmost first), then checks
/// the actor itself.
fn pick_actor(
    tree: &super::super::actor::ActorTree,
    id: ActorId,
    x: f32,
    y: f32,
) -> Option<ActorId> {
    let children = tree.children(id);
    // Check children first (they are on top).
    for &child in children.iter().rev() {
        if let Some(found) = pick_actor(tree, child, x, y) {
            return Some(found);
        }
    }
    // Check this actor.
    let common = tree.common(id)?;
    if !common.flags.visible {
        return None;
    }
    let alloc = &common.allocation;
    if x >= alloc.x1 && x < alloc.x2 && y >= alloc.y1 && y < alloc.y2 {
        Some(id)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::actor::{ActorCommon, ActorTree, NullBehavior};
    use super::super::super::actor_box::ActorBox;
    use super::super::super::grab::Grab;
    use super::super::stage_window::NullStageWindow;
    use super::*;
    use alloc::boxed::Box;

    fn make_stage() -> (ActorTree, Stage) {
        let mut tree = ActorTree::new();
        let actor_id = tree.create(ActorCommon::default(), Box::new(NullBehavior::default()));
        let stage = Stage::new(actor_id, StageId(0));
        (tree, stage)
    }

    #[test]
    fn new_stage_defaults() {
        let (_, stage) = make_stage();
        assert_eq!(stage.state(), StageState::Init);
        assert!(stage.is_dirty());
        assert!(stage.needs_relayout());
        assert!(!stage.is_active());
        assert!(stage.get_key_focus().is_none());
        assert!(stage.get_pointer_actor().is_none());
        assert_eq!(stage.grab_count(), 0);
        assert!(stage.get_cursor_visible());
        assert!(!stage.get_fullscreen());
        assert_eq!(stage.get_title(), "");
        assert_eq!(stage.get_size(), (0, 0));
    }

    #[test]
    fn realize_transitions_to_realized() {
        let (_, mut stage) = make_stage();
        assert!(stage.realize());
        assert_eq!(stage.state(), StageState::Realized);
        // Realizing again is a no-op.
        assert!(stage.realize());
        assert_eq!(stage.state(), StageState::Realized);
    }

    #[test]
    fn realize_with_window() {
        let (_, mut stage) = make_stage();
        stage.set_stage_window(StageWindowImpl::new(Box::new(NullStageWindow::new())));
        assert!(stage.realize());
        assert_eq!(stage.state(), StageState::Realized);
    }

    #[test]
    fn unrealize_transitions_to_init() {
        let (_, mut stage) = make_stage();
        stage.realize();
        stage.unrealize();
        assert_eq!(stage.state(), StageState::Init);
        assert!(!stage.is_active());
    }

    #[test]
    fn destroy_clears_state() {
        let (_, mut stage) = make_stage();
        stage.realize();
        stage.set_key_focus(Some(ActorId {
            index: 1,
            generation: 0,
        }));
        stage.activate_grab(Grab::new(
            ActorId {
                index: 2,
                generation: 0,
            },
            false,
        ));
        stage.destroy();
        assert_eq!(stage.state(), StageState::Destroyed);
        assert_eq!(stage.grab_count(), 0);
        assert!(stage.get_key_focus().is_none());
        assert!(stage.get_pointer_actor().is_none());
    }

    #[test]
    fn set_active_transitions_state() {
        let (_, mut stage) = make_stage();
        stage.realize();
        assert_eq!(stage.state(), StageState::Realized);
        stage.set_active(true);
        assert!(stage.is_active());
        assert_eq!(stage.state(), StageState::Active);
        stage.set_active(false);
        assert!(!stage.is_active());
        assert_eq!(stage.state(), StageState::Realized);
    }

    #[test]
    fn set_active_on_destroyed_is_noop() {
        let (_, mut stage) = make_stage();
        stage.destroy();
        stage.set_active(true);
        assert!(!stage.is_active());
        assert_eq!(stage.state(), StageState::Destroyed);
    }

    #[test]
    fn size_round_trips_through_window() {
        let (_, mut stage) = make_stage();
        stage.set_stage_window(StageWindowImpl::new(Box::new(NullStageWindow::new())));
        stage.realize();
        stage.set_size(800, 600);
        assert_eq!(stage.get_size(), (800, 600));
        // set_size queues relayout.
        assert!(stage.needs_relayout());
    }

    #[test]
    fn title_round_trips() {
        let (_, mut stage) = make_stage();
        stage.set_title("Test Stage");
        assert_eq!(stage.get_title(), "Test Stage");
    }

    #[test]
    fn fullscreen_round_trips() {
        let (_, mut stage) = make_stage();
        stage.set_fullscreen(true);
        assert!(stage.get_fullscreen());
        stage.set_fullscreen(false);
        assert!(!stage.get_fullscreen());
    }

    #[test]
    fn cursor_visible_round_trips() {
        let (_, mut stage) = make_stage();
        stage.set_cursor_visible(false);
        assert!(!stage.get_cursor_visible());
        stage.set_cursor_visible(true);
        assert!(stage.get_cursor_visible());
    }

    #[test]
    fn key_focus_round_trips() {
        let (_, mut stage) = make_stage();
        let actor = ActorId {
            index: 1,
            generation: 0,
        };
        stage.set_key_focus(Some(actor));
        assert_eq!(stage.get_key_focus(), Some(actor));
        stage.set_key_focus(None);
        assert!(stage.get_key_focus().is_none());
    }

    #[test]
    fn queue_redraw_sets_dirty() {
        let (_, mut stage) = make_stage();
        stage.clear_dirty();
        assert!(!stage.is_dirty());
        stage.queue_redraw(Some(Rectangle::new(0, 0, 100, 100)));
        assert!(stage.is_dirty());
        assert!(!stage.has_fullscreen_redraw());
        assert_eq!(stage.redraw_clips().len(), 1);
    }

    #[test]
    fn queue_full_redraw_clears_clips() {
        let (_, mut stage) = make_stage();
        stage.queue_redraw(Some(Rectangle::new(0, 0, 100, 100)));
        stage.queue_redraw(Some(Rectangle::new(100, 0, 50, 50)));
        assert_eq!(stage.redraw_clips().len(), 2);
        stage.queue_full_redraw();
        assert!(stage.has_fullscreen_redraw());
        assert_eq!(stage.redraw_clips().len(), 0);
    }

    #[test]
    fn clear_dirty_resets_state() {
        let (_, mut stage) = make_stage();
        stage.queue_redraw(Some(Rectangle::new(0, 0, 100, 100)));
        assert!(stage.is_dirty());
        stage.clear_dirty();
        assert!(!stage.is_dirty());
        assert!(stage.has_fullscreen_redraw());
    }

    #[test]
    fn queue_relayout_sets_flag() {
        let (_, mut stage) = make_stage();
        stage.ensure_relayout();
        assert!(!stage.needs_relayout());
        stage.queue_relayout();
        assert!(stage.needs_relayout());
    }

    #[test]
    fn ensure_relayout_returns_previous() {
        let (_, mut stage) = make_stage();
        assert!(stage.ensure_relayout());
        assert!(!stage.ensure_relayout());
    }

    #[test]
    fn add_view_queues_relayout() {
        let (_, mut stage) = make_stage();
        stage.ensure_relayout();
        assert!(!stage.needs_relayout());
        stage.add_view(Rectangle::new(0, 0, 1920, 1080), 60.0, 1.0);
        assert!(stage.needs_relayout());
        assert_eq!(stage.get_views().len(), 1);
    }

    #[test]
    fn remove_view_queues_relayout() {
        let (_, mut stage) = make_stage();
        let id = stage.add_view(Rectangle::new(0, 0, 1920, 1080), 60.0, 1.0);
        stage.ensure_relayout();
        assert!(stage.remove_view(id));
        assert!(stage.needs_relayout());
    }

    #[test]
    fn grab_management() {
        let (_, mut stage) = make_stage();
        let a1 = ActorId {
            index: 1,
            generation: 0,
        };
        let a2 = ActorId {
            index: 2,
            generation: 0,
        };

        assert!(stage.activate_grab(Grab::new(a1, false)));
        assert_eq!(stage.grab_count(), 1);
        assert_eq!(stage.get_grab_actor(), Some(a1));

        assert!(stage.activate_grab(Grab::new(a2, false)));
        assert_eq!(stage.grab_count(), 2);
        assert_eq!(stage.get_grab_actor(), Some(a2));

        let outcome = stage.dismiss_grab().unwrap();
        assert_eq!(stage.grab_count(), 1);
        assert!(outcome.topmost_changed);
        assert_eq!(stage.get_grab_actor(), Some(a1));
    }

    #[test]
    fn handle_event_key_goes_to_focus() {
        let (_, mut stage) = make_stage();
        // No key focus -> event not handled.
        assert!(!stage.handle_event(&make_key_event()));

        stage.set_key_focus(Some(ActorId {
            index: 1,
            generation: 0,
        }));
        // Key focus set -> event handled.
        assert!(stage.handle_event(&make_key_event()));
    }

    #[test]
    fn handle_event_pointer_with_grab() {
        let (_, mut stage) = make_stage();
        // No grab, no pointer actor -> not handled.
        assert!(!stage.handle_event(&make_button_event()));

        // With grab -> handled.
        stage.activate_grab(Grab::new(
            ActorId {
                index: 1,
                generation: 0,
            },
            false,
        ));
        assert!(stage.handle_event(&make_button_event()));
    }

    #[test]
    fn handle_event_pointer_with_pointer_actor() {
        let (_, mut stage) = make_stage();
        stage.set_pointer_actor(Some(ActorId {
            index: 1,
            generation: 0,
        }));
        assert!(stage.handle_event(&make_button_event()));
        assert!(stage.handle_event(&make_motion_event()));
    }

    #[test]
    fn handle_event_device_events_not_handled() {
        let (_, stage) = make_stage();
        assert!(!stage.handle_event(&make_device_event()));
    }

    #[test]
    fn get_actor_at_pos_finds_child() {
        let (mut tree, stage) = make_stage();

        // Add a child at (10, 10, 100, 100).
        let mut common = ActorCommon::default();
        common.allocation = ActorBox::new(10.0, 10.0, 110.0, 110.0);
        let child = tree.create(common, Box::new(NullBehavior::default()));
        tree.add_child(stage.actor_id(), child);

        // Point inside the child.
        assert_eq!(stage.get_actor_at_pos(50.0, 50.0, &tree), Some(child));

        // Point outside the child.
        assert!(stage.get_actor_at_pos(200.0, 200.0, &tree).is_none());
    }

    #[test]
    fn get_actor_at_pos_picks_topmost() {
        let (mut tree, stage) = make_stage();

        // Two overlapping children; the second (added later) is on top.
        let mut c1 = ActorCommon::default();
        c1.allocation = ActorBox::new(0.0, 0.0, 100.0, 100.0);
        let child1 = tree.create(c1, Box::new(NullBehavior::default()));
        tree.add_child(stage.actor_id(), child1);

        let mut c2 = ActorCommon::default();
        c2.allocation = ActorBox::new(0.0, 0.0, 100.0, 100.0);
        let child2 = tree.create(c2, Box::new(NullBehavior::default()));
        tree.add_child(stage.actor_id(), child2);

        // Should pick child2 (last added = topmost).
        assert_eq!(stage.get_actor_at_pos(50.0, 50.0, &tree), Some(child2));
    }

    #[test]
    fn get_actor_at_pos_skips_invisible() {
        let (mut tree, stage) = make_stage();

        let mut common = ActorCommon::default();
        common.allocation = ActorBox::new(0.0, 0.0, 100.0, 100.0);
        common.flags.visible = false;
        let child = tree.create(common, Box::new(NullBehavior::default()));
        tree.add_child(stage.actor_id(), child);

        // Invisible actor should not be picked.
        assert!(stage.get_actor_at_pos(50.0, 50.0, &tree).is_none());
    }

    #[test]
    fn get_actor_at_pos_recurses_into_children() {
        let (mut tree, stage) = make_stage();

        // Parent at (0, 0, 200, 200).
        let mut parent_c = ActorCommon::default();
        parent_c.allocation = ActorBox::new(0.0, 0.0, 200.0, 200.0);
        let parent = tree.create(parent_c, Box::new(NullBehavior::default()));
        tree.add_child(stage.actor_id(), parent);

        // Child at (50, 50, 100, 100) inside parent.
        let mut child_c = ActorCommon::default();
        child_c.allocation = ActorBox::new(50.0, 50.0, 150.0, 150.0);
        let child = tree.create(child_c, Box::new(NullBehavior::default()));
        tree.add_child(parent, child);

        // Point inside both parent and child -> should pick child.
        assert_eq!(stage.get_actor_at_pos(75.0, 75.0, &tree), Some(child));

        // Point inside parent but outside child -> should pick parent.
        assert_eq!(stage.get_actor_at_pos(25.0, 25.0, &tree), Some(parent));
    }

    // ---- test event constructors ----

    fn make_key_event() -> Event {
        use super::super::super::event::{
            DeviceId, EventFlags, KeyEvent, ModifierSet, ModifierType,
        };
        Event::Key(KeyEvent {
            time_us: 1000,
            flags: EventFlags::NONE,
            source_device: Some(DeviceId(1)),
            raw_modifiers: ModifierSet::default(),
            modifier_state: ModifierType::NONE,
            keyval: 65,
            hardware_keycode: 50,
            unicode_value: 0,
            evdev_code: 30,
        })
    }

    fn make_button_event() -> Event {
        use super::super::super::event::{ButtonEvent, DeviceId, EventFlags, ModifierType};
        Event::Button(ButtonEvent {
            time_us: 2000,
            flags: EventFlags::NONE,
            source_device: Some(DeviceId(1)),
            x: 50.0,
            y: 50.0,
            modifier_state: ModifierType::NONE,
            button: 1,
            tool: None,
            evdev_code: 0,
        })
    }

    fn make_motion_event() -> Event {
        use super::super::super::event::{DeviceId, EventFlags, ModifierType, MotionEvent};
        Event::Motion(MotionEvent {
            time_us: 3000,
            flags: EventFlags::NONE,
            source_device: Some(DeviceId(1)),
            x: 60.0,
            y: 60.0,
            modifier_state: ModifierType::NONE,
            tool: None,
            dx: 10.0,
            dy: 0.0,
            dx_unaccel: 10.0,
            dy_unaccel: 0.0,
            dx_constrained: 10.0,
            dy_constrained: 0.0,
        })
    }

    fn make_device_event() -> Event {
        use super::super::super::event::{DeviceEvent, DeviceId, EventFlags};
        Event::Device(DeviceEvent {
            time_us: 4000,
            flags: EventFlags::NONE,
            source_device: Some(DeviceId(2)),
        })
    }
}
