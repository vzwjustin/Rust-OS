//! GNOME Mutter's src/backends/meta-sprite.c
//!
//! MetaSprite: Mutter's subclass of ClutterSprite. A sprite ties an input
//! focus (pointer / tablet tool) to a cursor and, when its cursor changes,
//! pushes that cursor into the appropriate MetaCursorRenderer (and, for the
//! pointer role, the MetaCursorTracker).
//!
//! Stubbed: ClutterSprite, MetaBackend, MetaCursorRenderer and
//! MetaCursorTracker are Clutter/backend objects not present in the kernel.
//! The GObject "backend" property becomes a plain field, and the cursor-sync
//! logic is expressed against small trait/struct stand-ins so the control flow
//! of meta_sprite_update_cursor() stays faithful.
//!
//! Reference:
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-sprite.c

use core::any::Any;

/// ClutterSpriteRole — whether this sprite drives the visible pointer or some
/// other input (e.g. a tablet tool). Only POINTER interacts with the tracker.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpriteRole {
    Pointer,
    Other,
}

/// Opaque cursor handle (ClutterCursor). Real cursor content lives in the
/// backend; here it is just an identifier passed through the renderer.
pub type CursorId = u64;

/// Backend hooks needed by sprite cursor syncing. Stubs out
/// meta_backend_get_cursor_renderer_for_sprite / get_cursor_tracker and the
/// renderer/tracker setters, keeping the same call sequence.
pub trait SpriteBackend {
    /// meta_backend_get_cursor_renderer_for_sprite(): may be absent.
    fn has_cursor_renderer(&self) -> bool;
    /// meta_cursor_renderer_set_cursor()
    fn cursor_renderer_set_cursor(&mut self, cursor: Option<CursorId>);
    /// meta_cursor_tracker_get_pointer_visible()
    fn pointer_visible(&self) -> bool;
    /// meta_cursor_tracker_set_current_cursor()
    fn cursor_tracker_set_current_cursor(&mut self, cursor: Option<CursorId>);

    fn as_any(&self) -> &dyn Any;
}

/// MetaSprite. In C, MetaSpritePrivate only holds the backend pointer; the
/// focused actor, role, and current cursor live in the ClutterSprite base.
/// They are folded together here.
#[derive(Debug)]
pub struct Sprite {
    /// The sprite's input role (pointer vs. other).
    role: SpriteRole,
    /// Whether a focus actor is currently set (clutter_focus_get_current_actor).
    has_focus: bool,
    /// The cursor currently assigned to this sprite (clutter_sprite_get_cursor).
    cursor: Option<CursorId>,
}

impl Sprite {
    /// meta_sprite_init() — a fresh sprite has no focus and no cursor.
    pub fn new(role: SpriteRole) -> Self {
        Sprite {
            role,
            has_focus: false,
            cursor: None,
        }
    }

    pub fn role(&self) -> SpriteRole {
        self.role
    }

    pub fn has_focus(&self) -> bool {
        self.has_focus
    }

    pub fn set_focus(&mut self, has_focus: bool) {
        self.has_focus = has_focus;
    }

    /// clutter_sprite_get_cursor()
    pub fn cursor(&self) -> Option<CursorId> {
        self.cursor
    }

    pub fn set_cursor(&mut self, cursor: Option<CursorId>) {
        self.cursor = cursor;
    }

    /// meta_sprite_update_cursor()
    ///
    /// Drives the given cursor into the backend's cursor renderer. For the
    /// pointer role, respects pointer visibility and updates the cursor
    /// tracker; other roles just set the cursor.
    pub fn update_cursor(&self, backend: &mut dyn SpriteBackend, cursor: Option<CursorId>) {
        // if (!clutter_focus_get_current_actor (...)) return;
        if !self.has_focus {
            return;
        }
        // cursor_renderer = meta_backend_get_cursor_renderer_for_sprite(...)
        if !backend.has_cursor_renderer() {
            return;
        }

        if self.role == SpriteRole::Pointer {
            let pointer_visible = backend.pointer_visible();
            backend.cursor_renderer_set_cursor(if pointer_visible { cursor } else { None });
            backend.cursor_tracker_set_current_cursor(cursor);
        } else {
            backend.cursor_renderer_set_cursor(cursor);
        }
    }

    /// meta_sprite_sync_cursor()
    pub fn sync_cursor(&self, backend: &mut dyn SpriteBackend) {
        let cursor = self.cursor();
        self.update_cursor(backend, cursor);
    }
}
