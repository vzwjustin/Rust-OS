#![allow(dead_code)]

//! Port of GNOME mutter's clutter/clutter-stage-view.{c,h} and
//! clutter-stage-view-private.h.
//!
//! ClutterStageView represents one monitor output's view of the
//! stage. A stage can span multiple monitors, each with its own
//! framebuffer, refresh rate, color state, and fractional scale. The
//! view holds the layout rectangle (position/size on the stage), the
//! Cogl framebuffer for rendering, and the redraw clip for partial
//! repaints.
//!
//! # What's ported
//!
//! - StageViewId: a u32 newtype identifying a view (used by
//!   stage_window to enumerate views).
//! - StageView struct with layout (MtkRectangle), refresh_rate,
//!   scale, framebuffer placeholder, offscreen framebuffer placeholder,
//!   color_state placeholder, redraw_clip, and dirty flag.
//! - All accessors: get_layout, set_layout, get_refresh_rate,
//!   set_refresh_rate, get_scale, set_scale, get_framebuffer,
//!   get_offscreen_framebuffer, get_color_state.
//! - Redraw clip management: get_redraw_clip, set_redraw_clip,
//!   has_redraw_clip, clear_redraw_clip.
//! - Dirty flag management: is_dirty, set_dirty.
//! - transform_to_screen / transform_from_screen: coordinate
//!   transforms between stage space and screen space, accounting for
//!   the view's layout offset and fractional scale. These are pure
//!   math with no GPU dependency.
//! - paint: modeled as a method taking a PaintContext and a paint
//!   callback (function pointer). The GPU-dependent Cogl/GL draw calls
//!   are documented but not implemented; the callback lets a future
//!   backend plug in the actual rendering.
//! - StageViewCollection: a Vec-based collection managing view ids and
//!   views, with add/remove/at/iter methods, mirroring how the stage
//!   manages its view list.
//!
//! # What's skipped, with rationale
//!
//! - CoglFramebuffer: no Cogl/GL binding exists. Reuses the
//!   paint_context::Framebuffer placeholder.
//! - ClutterColorState: no color-management subsystem. Reuses the
//!   paint_context::ColorState placeholder.
//! - clutter_stage_view_paint full pipeline: the C version calls
//!   clutter_stage_paint_view which traverses the actor tree and
//!   issues Cogl draw calls. Here, paint takes a callback function
//!   pointer that a future backend provides; the view handles only
//!   the redraw-clip and dirty-flag bookkeeping around the callback.
//! - clutter_stage_view_paint_to_framebuffer: same GPU dependency;
//!   the callback approach covers this use case.
//! - ClutterCursor integration: the C view holds a ClutterCursor for
//!   cursor rendering. Not ported (cursor.rs exists but the
//!   integration with the view's paint cycle is GPU-dependent).
//! - Signal emission (paint-signal, cursor-signal): no GObject
//!   signals; modeled as return values and callbacks.
//!
//! As with the rest of mutter_port::clutter, this module uses no
//! unsafe, no external crates, and only core/alloc.

use core::ops::Range;

use super::super::super::mtk::rectangle::Rectangle;
use super::super::paint_context::{ClipFrustum, ColorState, Framebuffer, PaintContext, PaintFlag};

/// Identifier for a stage view. A u32 newtype so the stage-window
/// interface can enumerate views by id without owning the views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct StageViewId(pub u32);

/// Callback type for the paint callback in StageView::paint.
/// The callback receives the paint context and should perform the
/// actual rendering (GPU-dependent). Returns true if painting
/// occurred, false if the view was skipped (e.g. obscured).
pub type PaintCallback = fn(&PaintContext) -> bool;

/// Port of ClutterStageView / ClutterStageViewPrivate.
///
/// Represents one monitor output's view of the stage. Each view has
/// its own layout rectangle (position/size on the stage), framebuffer,
/// refresh rate, color state, and fractional scale.
#[derive(Debug, Clone)]
pub struct StageView {
    id: StageViewId,
    layout: Rectangle,
    refresh_rate: f32,
    scale: f32,
    framebuffer: Framebuffer,
    offscreen_framebuffer: Option<Framebuffer>,
    color_state: ColorState,
    redraw_clip: Option<Rectangle>,
    dirty: bool,
}

impl StageView {
    /// Create a new StageView with the given id, layout, refresh rate,
    /// and scale. The framebuffer and color_state are default
    /// placeholders.
    pub fn new(id: StageViewId, layout: Rectangle, refresh_rate: f32, scale: f32) -> Self {
        StageView {
            id,
            layout,
            refresh_rate,
            scale: scale.max(0.1),
            framebuffer: Framebuffer,
            offscreen_framebuffer: None,
            color_state: ColorState::srgb(),
            redraw_clip: None,
            dirty: true,
        }
    }

    /// clutter_stage_view_get_layout: return the view's layout
    /// rectangle (position/size on the stage).
    pub fn get_layout(&self) -> Rectangle {
        self.layout
    }

    /// Set the layout rectangle.
    pub fn set_layout(&mut self, layout: Rectangle) {
        if layout != self.layout {
            self.layout = layout;
            self.dirty = true;
        }
    }

    /// clutter_stage_view_get_refresh_rate: return the refresh rate
    /// in Hz.
    pub fn get_refresh_rate(&self) -> f32 {
        self.refresh_rate
    }

    /// Set the refresh rate (Hz).
    pub fn set_refresh_rate(&mut self, refresh_rate: f32) {
        self.refresh_rate = refresh_rate;
    }

    /// clutter_stage_view_get_scale: return the fractional scale
    /// factor.
    pub fn get_scale(&self) -> f32 {
        self.scale
    }

    /// Set the fractional scale factor. Clamped to a minimum of 0.1
    /// to avoid division-by-zero in transform_from_screen.
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.max(0.1);
    }

    /// clutter_stage_view_get_framebuffer: return the onscreen
    /// framebuffer for this view.
    pub fn get_framebuffer(&self) -> &Framebuffer {
        &self.framebuffer
    }

    /// Set the onscreen framebuffer.
    pub fn set_framebuffer(&mut self, framebuffer: Framebuffer) {
        self.framebuffer = framebuffer;
    }

    /// clutter_stage_view_get_offscreen_framebuffer: return the
    /// offscreen framebuffer used for intermediate rendering, if any.
    pub fn get_offscreen_framebuffer(&self) -> Option<&Framebuffer> {
        self.offscreen_framebuffer.as_ref()
    }

    /// Set the offscreen framebuffer.
    pub fn set_offscreen_framebuffer(&mut self, framebuffer: Option<Framebuffer>) {
        self.offscreen_framebuffer = framebuffer;
    }

    /// clutter_stage_view_get_color_state: return the color state
    /// for this view.
    pub fn get_color_state(&self) -> &ColorState {
        &self.color_state
    }

    /// Set the color state.
    pub fn set_color_state(&mut self, color_state: ColorState) {
        self.color_state = color_state;
    }

    /// clutter_stage_view_get_redraw_clip: return the redraw clip
    /// rectangle, if set. A None value means the entire view should
    /// be redrawn.
    pub fn get_redraw_clip(&self) -> Option<Rectangle> {
        self.redraw_clip
    }

    /// clutter_stage_view_set_redraw_clip: set the redraw clip
    /// rectangle for partial repaints.
    pub fn set_redraw_clip(&mut self, clip: Rectangle) {
        self.redraw_clip = Some(clip);
    }

    /// Whether a redraw clip is set (partial redraw).
    pub fn has_redraw_clip(&self) -> bool {
        self.redraw_clip.is_some()
    }

    /// Clear the redraw clip, meaning the next paint should redraw
    /// the entire view.
    pub fn clear_redraw_clip(&mut self) {
        self.redraw_clip = None;
    }

    /// Whether the view is dirty (needs repainting).
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Mark the view as dirty (needs repainting).
    pub fn set_dirty(&mut self, dirty: bool) {
        self.dirty = dirty;
    }

    /// Transform a point from stage coordinates to screen coordinates.
    ///
    /// Stage coordinates are relative to the stage origin. Screen
    /// coordinates are relative to the view's top-left corner, scaled
    /// by the fractional scale factor.
    ///
    /// screen_x = (stage_x - layout.x) * scale
    /// screen_y = (stage_y - layout.y) * scale
    pub fn transform_to_screen(&self, stage_x: f32, stage_y: f32) -> (f32, f32) {
        let screen_x = (stage_x - self.layout.x as f32) * self.scale;
        let screen_y = (stage_y - self.layout.y as f32) * self.scale;
        (screen_x, screen_y)
    }

    /// Transform a point from screen coordinates to stage coordinates.
    ///
    /// This is the inverse of transform_to_screen:
    /// stage_x = screen_x / scale + layout.x
    /// stage_y = screen_y / scale + layout.y
    pub fn transform_from_screen(&self, screen_x: f32, screen_y: f32) -> (f32, f32) {
        let stage_x = screen_x / self.scale + self.layout.x as f32;
        let stage_y = screen_y / self.scale + self.layout.y as f32;
        (stage_x, stage_y)
    }

    /// Transform a rectangle from stage coordinates to screen
    /// coordinates.
    pub fn transform_rect_to_screen(&self, rect: &Rectangle) -> (f32, f32, f32, f32) {
        let (x, y) = self.transform_to_screen(rect.x as f32, rect.y as f32);
        let w = rect.width as f32 * self.scale;
        let h = rect.height as f32 * self.scale;
        (x, y, w, h)
    }

    /// Transform a rectangle from screen coordinates to stage
    /// coordinates, returning an MtkRectangle.
    pub fn transform_rect_from_screen(&self, x: f32, y: f32, w: f32, h: f32) -> Rectangle {
        let (sx, sy) = self.transform_from_screen(x, y);
        let sw = w / self.scale;
        let sh = h / self.scale;
        Rectangle::new(sx as i32, sy as i32, sw as i32, sh as i32)
    }

    /// Whether a stage-coordinate point is inside this view's layout.
    pub fn contains_stage_point(&self, stage_x: f32, stage_y: f32) -> bool {
        self.layout.contains_pointf(stage_x, stage_y)
    }

    /// Whether a screen-coordinate point is inside this view's
    /// screen-space bounds (layout size scaled by scale).
    pub fn contains_screen_point(&self, screen_x: f32, screen_y: f32) -> bool {
        let w = self.layout.width as f32 * self.scale;
        let h = self.layout.height as f32 * self.scale;
        screen_x >= 0.0 && screen_x < w && screen_y >= 0.0 && screen_y < h
    }

    /// The view's screen-space width (layout.width * scale).
    pub fn screen_width(&self) -> f32 {
        self.layout.width as f32 * self.scale
    }

    /// The view's screen-space height (layout.height * scale).
    pub fn screen_height(&self) -> f32 {
        self.layout.height as f32 * self.scale
    }

    /// clutter_stage_view_paint: paint the view. The GPU-dependent
    /// rendering is delegated to the paint callback. This method
    /// handles the redraw-clip and dirty-flag bookkeeping around the
    /// callback invocation.
    ///
    /// If the view is not dirty and no redraw clip is set, painting
    /// is skipped (returns false). After a successful paint, the dirty
    /// flag is cleared and the redraw clip is cleared.
    ///
    /// The PaintContext should already be set up for this view
    /// (framebuffer, color state, clip frusta pushed by the caller).
    pub fn paint(&mut self, ctx: &PaintContext, callback: PaintCallback) -> bool {
        if !self.dirty && self.redraw_clip.is_none() {
            return false;
        }

        let painted = callback(ctx);
        if painted {
            self.dirty = false;
            self.redraw_clip = None;
        }
        painted
    }

    /// The view's id.
    pub fn id(&self) -> StageViewId {
        self.id
    }
}

/// A collection of stage views, keyed by StageViewId. Mirrors how
/// the stage manages its view list (the C ClutterStagePrivate::views
/// GList). Provides add/remove/at/iter operations.
#[derive(Debug, Clone, Default)]
pub struct StageViewCollection {
    views: Vec<StageView>,
    next_id: u32,
}

impl StageViewCollection {
    /// Create a new empty collection.
    pub fn new() -> Self {
        StageViewCollection {
            views: Vec::new(),
            next_id: 0,
        }
    }

    /// Number of views.
    pub fn len(&self) -> usize {
        self.views.len()
    }

    /// Whether the collection is empty.
    pub fn is_empty(&self) -> bool {
        self.views.is_empty()
    }

    /// Add a new view with the given layout, refresh rate, and scale.
    /// Returns the assigned StageViewId.
    pub fn add_view(&mut self, layout: Rectangle, refresh_rate: f32, scale: f32) -> StageViewId {
        let id = StageViewId(self.next_id);
        self.next_id += 1;
        self.views
            .push(StageView::new(id, layout, refresh_rate, scale));
        id
    }

    /// Add an existing view to the collection.
    pub fn add(&mut self, view: StageView) -> StageViewId {
        let id = view.id();
        if !self.views.iter().any(|v| v.id() == id) {
            self.views.push(view);
        }
        if id.0 >= self.next_id {
            self.next_id = id.0 + 1;
        }
        id
    }

    /// Remove a view by id. Returns true if the view was found.
    pub fn remove(&mut self, id: StageViewId) -> bool {
        let idx = self.views.iter().position(|v| v.id() == id);
        match idx {
            Some(i) => {
                self.views.remove(i);
                true
            }
            None => false,
        }
    }

    /// Get a view by id.
    pub fn get(&self, id: StageViewId) -> Option<&StageView> {
        self.views.iter().find(|v| v.id() == id)
    }

    /// Get a mutable view by id.
    pub fn get_mut(&mut self, id: StageViewId) -> Option<&mut StageView> {
        self.views.iter_mut().find(|v| v.id() == id)
    }

    /// Get a view by index.
    pub fn at(&self, index: usize) -> Option<&StageView> {
        self.views.get(index)
    }

    /// Get a mutable view by index.
    pub fn at_mut(&mut self, index: usize) -> Option<&mut StageView> {
        self.views.get_mut(index)
    }

    /// Get all view ids.
    pub fn ids(&self) -> Vec<StageViewId> {
        self.views.iter().map(|v| v.id()).collect()
    }

    /// Iterate over all views.
    pub fn iter(&self) -> impl Iterator<Item = &StageView> {
        self.views.iter()
    }

    /// Iterate mutably over all views.
    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut StageView> {
        self.views.iter_mut()
    }

    /// Mark all views as dirty.
    pub fn mark_all_dirty(&mut self) {
        for v in &mut self.views {
            v.set_dirty(true);
        }
    }

    /// Find the view containing the given stage point, if any.
    pub fn view_at_stage_point(&self, x: f32, y: f32) -> Option<StageViewId> {
        self.views
            .iter()
            .find(|v| v.contains_stage_point(x, y))
            .map(|v| v.id())
    }

    /// The bounding rectangle of all views (union of layouts), or
    /// None if empty.
    pub fn bounding_rect(&self) -> Option<Rectangle> {
        self.views
            .iter()
            .map(|v| v.get_layout())
            .reduce(|a, b| a.union(&b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_view(id: u32, x: i32, y: i32, w: i32, h: i32) -> StageView {
        StageView::new(StageViewId(id), Rectangle::new(x, y, w, h), 60.0, 1.0)
    }

    #[test]
    fn new_view_defaults() {
        let v = make_view(0, 0, 0, 1920, 1080);
        assert_eq!(v.id(), StageViewId(0));
        assert_eq!(v.get_layout(), Rectangle::new(0, 0, 1920, 1080));
        assert_eq!(v.get_refresh_rate(), 60.0);
        assert_eq!(v.get_scale(), 1.0);
        assert!(v.is_dirty());
        assert!(!v.has_redraw_clip());
    }

    #[test]
    fn set_layout_marks_dirty() {
        let mut v = make_view(0, 0, 0, 100, 100);
        v.set_dirty(false);
        assert!(!v.is_dirty());

        v.set_layout(Rectangle::new(10, 20, 100, 100));
        assert!(v.is_dirty());
        assert_eq!(v.get_layout(), Rectangle::new(10, 20, 100, 100));

        // Setting the same layout should not mark dirty.
        v.set_dirty(false);
        v.set_layout(Rectangle::new(10, 20, 100, 100));
        assert!(!v.is_dirty());
    }

    #[test]
    fn redraw_clip_management() {
        let mut v = make_view(0, 0, 0, 100, 100);
        assert!(!v.has_redraw_clip());
        v.set_redraw_clip(Rectangle::new(10, 10, 50, 50));
        assert!(v.has_redraw_clip());
        assert_eq!(v.get_redraw_clip(), Some(Rectangle::new(10, 10, 50, 50)));
        v.clear_redraw_clip();
        assert!(!v.has_redraw_clip());
    }

    #[test]
    fn transform_to_screen_identity_at_scale_1() {
        let v = make_view(0, 100, 200, 1920, 1080);
        // Stage point (100, 200) maps to screen (0, 0) since it is
        // the layout origin.
        let (sx, sy) = v.transform_to_screen(100.0, 200.0);
        assert_eq!(sx, 0.0);
        assert_eq!(sy, 0.0);

        // Stage point (200, 300) maps to screen (100, 100).
        let (sx, sy) = v.transform_to_screen(200.0, 300.0);
        assert_eq!(sx, 100.0);
        assert_eq!(sy, 100.0);
    }

    #[test]
    fn transform_from_screen_is_inverse() {
        let v = make_view(0, 100, 200, 1920, 1080);
        let (sx, sy) = v.transform_to_screen(500.0, 600.0);
        let (px, py) = v.transform_from_screen(sx, sy);
        assert!((px - 500.0).abs() < 0.001);
        assert!((py - 600.0).abs() < 0.001);
    }

    #[test]
    fn transform_with_fractional_scale() {
        let mut v = make_view(0, 0, 0, 1920, 1080);
        v.set_scale(1.5);
        // Stage point (100, 100) at scale 1.5 -> screen (150, 150).
        let (sx, sy) = v.transform_to_screen(100.0, 100.0);
        assert_eq!(sx, 150.0);
        assert_eq!(sy, 150.0);

        // Inverse: screen (150, 150) -> stage (100, 100).
        let (px, py) = v.transform_from_screen(150.0, 150.0);
        assert!((px - 100.0).abs() < 0.001);
        assert!((py - 100.0).abs() < 0.001);
    }

    #[test]
    fn contains_stage_point() {
        let v = make_view(0, 100, 100, 200, 200);
        assert!(v.contains_stage_point(150.0, 150.0));
        assert!(v.contains_stage_point(100.0, 100.0));
        assert!(!v.contains_stage_point(300.0, 300.0));
        assert!(!v.contains_stage_point(50.0, 50.0));
    }

    #[test]
    fn contains_screen_point() {
        let mut v = make_view(0, 0, 0, 100, 100);
        v.set_scale(2.0);
        // Screen space is 200x200.
        assert!(v.contains_screen_point(0.0, 0.0));
        assert!(v.contains_screen_point(199.0, 199.0));
        assert!(!v.contains_screen_point(200.0, 200.0));
    }

    #[test]
    fn paint_skips_clean_view() {
        let mut v = make_view(0, 0, 0, 100, 100);
        v.set_dirty(false);
        v.clear_redraw_clip();

        let called = |_: &PaintContext| -> bool { true };
        let ctx = PaintContext::new_for_framebuffer(Framebuffer, None, PaintFlag::NONE, ColorState::srgb());
        assert!(!v.paint(&ctx, called));
    }

    #[test]
    fn paint_clears_dirty_and_clip() {
        let mut v = make_view(0, 0, 0, 100, 100);
        v.set_dirty(true);
        v.set_redraw_clip(Rectangle::new(0, 0, 50, 50));

        let called = |_: &PaintContext| -> bool { true };
        let ctx = PaintContext::new_for_framebuffer(Framebuffer, None, PaintFlag::NONE, ColorState::srgb());
        assert!(v.paint(&ctx, called));
        assert!(!v.is_dirty());
        assert!(!v.has_redraw_clip());
    }

    #[test]
    fn paint_with_callback_returning_false_keeps_dirty() {
        let mut v = make_view(0, 0, 0, 100, 100);
        v.set_dirty(true);

        let skip = |_: &PaintContext| -> bool { false };
        let ctx = PaintContext::new_for_framebuffer(Framebuffer, None, PaintFlag::NONE, ColorState::srgb());
        assert!(!v.paint(&ctx, skip));
        assert!(v.is_dirty());
    }

    #[test]
    fn collection_add_and_get() {
        let mut coll = StageViewCollection::new();
        assert!(coll.is_empty());

        let id1 = coll.add_view(Rectangle::new(0, 0, 1920, 1080), 60.0, 1.0);
        let id2 = coll.add_view(Rectangle::new(1920, 0, 1920, 1080), 120.0, 1.0);

        assert_eq!(coll.len(), 2);
        assert!(coll.get(id1).is_some());
        assert!(coll.get(id2).is_some());
        assert_eq!(
            coll.get(id1).unwrap().get_layout(),
            Rectangle::new(0, 0, 1920, 1080)
        );
        assert_eq!(coll.get(id2).unwrap().get_refresh_rate(), 120.0);
    }

    #[test]
    fn collection_remove() {
        let mut coll = StageViewCollection::new();
        let id = coll.add_view(Rectangle::new(0, 0, 100, 100), 60.0, 1.0);
        assert!(coll.remove(id));
        assert!(coll.is_empty());
        assert!(!coll.remove(id));
    }

    #[test]
    fn collection_ids() {
        let mut coll = StageViewCollection::new();
        coll.add_view(Rectangle::new(0, 0, 100, 100), 60.0, 1.0);
        coll.add_view(Rectangle::new(100, 0, 100, 100), 60.0, 1.0);
        let ids = coll.ids();
        assert_eq!(ids, vec![StageViewId(0), StageViewId(1)]);
    }

    #[test]
    fn collection_mark_all_dirty() {
        let mut coll = StageViewCollection::new();
        let id1 = coll.add_view(Rectangle::new(0, 0, 100, 100), 60.0, 1.0);
        let id2 = coll.add_view(Rectangle::new(100, 0, 100, 100), 60.0, 1.0);

        coll.get_mut(id1).unwrap().set_dirty(false);
        coll.get_mut(id2).unwrap().set_dirty(false);

        coll.mark_all_dirty();
        assert!(coll.get(id1).unwrap().is_dirty());
        assert!(coll.get(id2).unwrap().is_dirty());
    }

    #[test]
    fn collection_view_at_stage_point() {
        let mut coll = StageViewCollection::new();
        let id1 = coll.add_view(Rectangle::new(0, 0, 100, 100), 60.0, 1.0);
        let id2 = coll.add_view(Rectangle::new(100, 0, 100, 100), 60.0, 1.0);

        assert_eq!(coll.view_at_stage_point(50.0, 50.0), Some(id1));
        assert_eq!(coll.view_at_stage_point(150.0, 50.0), Some(id2));
        assert_eq!(coll.view_at_stage_point(250.0, 50.0), None);
    }

    #[test]
    fn collection_bounding_rect() {
        let mut coll = StageViewCollection::new();
        coll.add_view(Rectangle::new(0, 0, 100, 100), 60.0, 1.0);
        coll.add_view(Rectangle::new(100, 0, 100, 100), 60.0, 1.0);
        let bounds = coll.bounding_rect().unwrap();
        assert_eq!(bounds, Rectangle::new(0, 0, 200, 100));

        let empty = StageViewCollection::new();
        assert!(empty.bounding_rect().is_none());
    }

    #[test]
    fn scale_clamped_to_minimum() {
        let mut v = make_view(0, 0, 0, 100, 100);
        v.set_scale(0.0);
        assert_eq!(v.get_scale(), 0.1);
    }
}
