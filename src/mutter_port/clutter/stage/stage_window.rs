#![allow(dead_code)]

//! Port of GNOME mutter's `clutter/clutter-stage-window.{c,h}`.
//!
//! `ClutterStageWindow` is a GObject interface (GInterface) implemented by
//! backend-specific stage window types. It defines the contract between
//! the stage and the platform window that hosts it: geometry, visibility,
//! focus, redraw clipping, and view enumeration.
//!
//! # What's ported
//!
//! - The `ClutterStageWindowInterface` vtable as a `StageWindow` trait
//!   with default implementations matching the C null-vtable guards.
//! - `StageWindowGeometry` struct mirroring the C out-parameters.
//! - `StageWindowImpl` wrapper providing the `clutter_stage_window_*` API.
//! - All interface methods: realize, unrealize, show, hide, raise, lower,
//!   focus, set_title, set_cursor_visible, get_geometry, get_size,
//!   set_size, resize, get_views, get_stage_view_at, get_default_view,
//!   add_redraw_clip, clear_redraw_clips, has_fullscreen_redraw.
//! - `NullStageWindow`: a concrete implementation for testing.
//!
//! # What's skipped, with rationale
//!
//! - GObject GInterface registration: the trait system replaces it.
//! - `ClutterStage *stage` back-pointer: stored as `Option<StageId>` on
//!   `StageWindowImpl` rather than a virtual method.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

use super::super::super::mtk::rectangle::Rectangle;
use super::stage_view::StageViewId;

/// Identifier for a stage, used by the stage-window interface to
/// reference its owning stage without a direct dependency on the
/// `stage` module (avoiding a circular import).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct StageId(pub u32);

/// Geometry returned by `clutter_stage_window_get_geometry`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StageWindowGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl StageWindowGeometry {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        StageWindowGeometry {
            x,
            y,
            width,
            height,
        }
    }

    pub fn to_rectangle(&self) -> Rectangle {
        Rectangle::new(self.x, self.y, self.width, self.height)
    }

    pub fn from_rectangle(rect: &Rectangle) -> Self {
        StageWindowGeometry {
            x: rect.x,
            y: rect.y,
            width: rect.width,
            height: rect.height,
        }
    }
}

/// Port of `ClutterStageWindowInterface` — the vtable that backend
/// stage window implementations provide.
pub trait StageWindow {
    fn realize(&mut self) -> bool {
        false
    }
    fn unrealize(&mut self) {}
    fn show(&mut self) {}
    fn hide(&mut self) {}
    fn raise(&mut self) {}
    fn lower(&mut self) {}
    fn focus(&mut self, _time_ms: u32) -> bool {
        false
    }
    fn set_title(&mut self, _title: &str) {}
    fn set_cursor_visible(&mut self, _visible: bool) {}
    fn get_geometry(&self) -> Option<StageWindowGeometry> {
        None
    }
    fn get_size(&self) -> Option<(i32, i32)> {
        None
    }
    fn set_size(&mut self, _width: i32, _height: i32) {}
    fn resize(&mut self, _width: i32, _height: i32) {}
    fn get_views(&self) -> Vec<StageViewId> {
        Vec::new()
    }
    fn get_stage_view_at(&self, _index: usize) -> Option<StageViewId> {
        None
    }
    fn get_default_view(&self) -> Option<StageViewId> {
        None
    }
    fn add_redraw_clip(&mut self, _clip: &Rectangle) {}
    fn clear_redraw_clips(&mut self) {}
    fn has_fullscreen_redraw(&self) -> bool {
        true
    }
}

/// Port of the `clutter_stage_window_*` wrapper-function API.
#[derive(Debug)]
pub struct StageWindowImpl {
    window: Box<dyn StageWindow>,
    stage: Option<StageId>,
}

impl StageWindowImpl {
    pub fn new(window: Box<dyn StageWindow>) -> Self {
        StageWindowImpl {
            window,
            stage: None,
        }
    }

    pub fn stage(&self) -> Option<StageId> {
        self.stage
    }

    pub fn set_stage(&mut self, stage: Option<StageId>) {
        self.stage = stage;
    }

    pub fn realize(&mut self) -> bool {
        self.window.realize()
    }

    pub fn unrealize(&mut self) {
        self.window.unrealize();
    }

    pub fn show(&mut self) {
        self.window.show();
    }

    pub fn hide(&mut self) {
        self.window.hide();
    }

    pub fn raise(&mut self) {
        self.window.raise();
    }

    pub fn lower(&mut self) {
        self.window.lower();
    }

    pub fn focus(&mut self, time_ms: u32) -> bool {
        self.window.focus(time_ms)
    }

    pub fn set_title(&mut self, title: &str) {
        self.window.set_title(title);
    }

    pub fn set_cursor_visible(&mut self, visible: bool) {
        self.window.set_cursor_visible(visible);
    }

    pub fn get_geometry(&self) -> Option<StageWindowGeometry> {
        self.window.get_geometry()
    }

    pub fn get_size(&self) -> Option<(i32, i32)> {
        self.window.get_size()
    }

    pub fn set_size(&mut self, width: i32, height: i32) {
        self.window.set_size(width, height);
    }

    pub fn resize(&mut self, width: i32, height: i32) {
        self.window.resize(width, height);
    }

    pub fn get_views(&self) -> Vec<StageViewId> {
        self.window.get_views()
    }

    pub fn get_stage_view_at(&self, index: usize) -> Option<StageViewId> {
        self.window.get_stage_view_at(index)
    }

    pub fn get_default_view(&self) -> Option<StageViewId> {
        self.window.get_default_view()
    }

    pub fn add_redraw_clip(&mut self, clip: &Rectangle) {
        self.window.add_redraw_clip(clip);
    }

    pub fn clear_redraw_clips(&mut self) {
        self.window.clear_redraw_clips();
    }

    pub fn has_fullscreen_redraw(&self) -> bool {
        self.window.has_fullscreen_redraw()
    }
}

/// A null/default `StageWindow` implementation for testing.
#[derive(Debug, Default)]
pub struct NullStageWindow {
    geometry: StageWindowGeometry,
    size: (i32, i32),
    visible: bool,
    title: String,
    cursor_visible: bool,
    clips: Vec<Rectangle>,
    realized: bool,
}

impl NullStageWindow {
    pub fn new() -> Self {
        NullStageWindow {
            geometry: StageWindowGeometry::default(),
            size: (0, 0),
            visible: false,
            title: String::new(),
            cursor_visible: true,
            clips: Vec::new(),
            realized: false,
        }
    }
}

impl StageWindow for NullStageWindow {
    fn realize(&mut self) -> bool {
        self.realized = true;
        true
    }

    fn unrealize(&mut self) {
        self.realized = false;
    }

    fn show(&mut self) {
        self.visible = true;
    }

    fn hide(&mut self) {
        self.visible = false;
    }

    fn set_title(&mut self, title: &str) {
        self.title = String::from(title);
    }

    fn set_cursor_visible(&mut self, visible: bool) {
        self.cursor_visible = visible;
    }

    fn get_geometry(&self) -> Option<StageWindowGeometry> {
        Some(self.geometry)
    }

    fn get_size(&self) -> Option<(i32, i32)> {
        Some(self.size)
    }

    fn set_size(&mut self, width: i32, height: i32) {
        self.size = (width, height);
        self.geometry.width = width;
        self.geometry.height = height;
    }

    fn resize(&mut self, width: i32, height: i32) {
        self.set_size(width, height);
    }

    fn add_redraw_clip(&mut self, clip: &Rectangle) {
        self.clips.push(*clip);
    }

    fn clear_redraw_clips(&mut self) {
        self.clips.clear();
    }

    fn has_fullscreen_redraw(&self) -> bool {
        self.clips.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_round_trips_to_rectangle() {
        let g = StageWindowGeometry::new(10, 20, 100, 200);
        let r = g.to_rectangle();
        assert_eq!(r, Rectangle::new(10, 20, 100, 200));
        let g2 = StageWindowGeometry::from_rectangle(&r);
        assert_eq!(g, g2);
    }

    #[test]
    fn null_window_realize_and_show() {
        let mut w = NullStageWindow::new();
        assert!(w.realize());
        assert!(w.realized);
        w.show();
        assert!(w.visible);
        w.hide();
        assert!(!w.visible);
    }

    #[test]
    fn null_window_size_round_trips() {
        let mut w = NullStageWindow::new();
        w.set_size(800, 600);
        assert_eq!(w.get_size(), Some((800, 600)));
        let g = w.get_geometry().unwrap();
        assert_eq!(g.width, 800);
        assert_eq!(g.height, 600);
    }

    #[test]
    fn null_window_clip_management() {
        let mut w = NullStageWindow::new();
        assert!(w.has_fullscreen_redraw());
        w.add_redraw_clip(&Rectangle::new(0, 0, 100, 100));
        assert!(!w.has_fullscreen_redraw());
        w.add_redraw_clip(&Rectangle::new(100, 100, 50, 50));
        assert_eq!(w.clips.len(), 2);
        w.clear_redraw_clips();
        assert!(w.has_fullscreen_redraw());
    }

    #[test]
    fn stage_window_impl_dispatches_to_inner() {
        let mut imp = StageWindowImpl::new(Box::new(NullStageWindow::new()));
        assert!(imp.stage().is_none());
        imp.set_stage(Some(StageId(1)));
        assert_eq!(imp.stage(), Some(StageId(1)));
        assert!(imp.realize());
        imp.set_size(640, 480);
        assert_eq!(imp.get_size(), Some((640, 480)));
        imp.show();
        imp.set_title("Hello");
        imp.add_redraw_clip(&Rectangle::new(0, 0, 10, 10));
        assert!(!imp.has_fullscreen_redraw());
        imp.clear_redraw_clips();
        assert!(imp.has_fullscreen_redraw());
    }

    #[test]
    fn default_trait_methods_are_noops() {
        struct Bare;
        impl StageWindow for Bare {}
        let mut w = Bare;
        w.realize();
        w.unrealize();
        w.show();
        w.hide();
        w.raise();
        w.lower();
        w.focus(0);
        w.set_title("test");
        w.set_cursor_visible(true);
        assert!(w.get_geometry().is_none());
        assert!(w.get_size().is_none());
        w.set_size(100, 100);
        w.resize(200, 200);
        assert!(w.get_views().is_empty());
        assert!(w.get_stage_view_at(0).is_none());
        assert!(w.get_default_view().is_none());
        w.add_redraw_clip(&Rectangle::new(0, 0, 10, 10));
        w.clear_redraw_clips();
        assert!(w.has_fullscreen_redraw());
    }
}
