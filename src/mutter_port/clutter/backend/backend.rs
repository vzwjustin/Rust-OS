//! Port of GNOME mutter's `clutter/clutter-backend.{c,h}`.
//!
//! `ClutterBackend` is the abstract base class for the platform/graphics
//! backend in Clutter.
//!
//! # What's ported
//!
//! - `ClutterBackend` struct fields and accessor methods.
//! - `ClutterBackendClass` vtable as a `BackendImpl` trait with defaults.
//! - `ClutterFeatureFlags`, `ClutterUnitType`, `ClutterScalingFilter`.
//! - Stage tracking via `Vec<StageId>`.
//!
//! # What's skipped
//!
//! - GObject machinery, CoglContext/Framebuffer (opaque placeholders).
//! - ClutterStageWindow, ClutterInputMethod, ClutterStageManager (opaque).
//! - Signal emission, deprecated fog API.

#![allow(dead_code)]

use alloc::boxed::Box;
use alloc::vec::Vec;

use super::super::context::TextDirection;
use super::super::keymap::Keymap;
use super::super::seat::Seat;
use super::super::settings::Settings;

/// `ClutterFeatureFlags` — bitfield of optional backend features.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FeatureFlags(pub u32);

impl FeatureFlags {
    pub const STAGE_STATIC: Self = Self(1 << 0);
    pub const STAGE_USER_RESIZE: Self = Self(1 << 1);
    pub const STAGE_CURSOR: Self = Self(1 << 2);
    pub const SHADERS_GLSL: Self = Self(1 << 3);
    pub const TEXTURE_YUV: Self = Self(1 << 4);
    pub const TEXTURE_READ_PIXELS: Self = Self(1 << 5);
    pub const STAGE_DYNAMIC_RESOLUTION: Self = Self(1 << 6);
    pub const NONE: Self = Self(0);

    pub fn union(self, other: Self) -> Self { FeatureFlags(self.0 | other.0) }
    pub fn contains(self, flag: Self) -> bool { (self.0 & flag.0) == flag.0 }
}

/// `ClutterUnitType` — which unit system is allowed for layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum UnitType {
    #[default]
    Pixel = 0,
    Em = 1,
    Mm = 2,
    Point = 3,
    Cm = 4,
}

/// `ClutterScalingFilter` — texture min/mag filter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum ScalingFilter {
    #[default]
    Nearest = 0,
    Linear = 1,
    Trilinear = 2,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoglContext { pub handle: u32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StageWindow { pub handle: u32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StageManager { pub handle: u32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputMethod { pub handle: u32 }

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StageId(pub u32);

/// Port of `ClutterBackendClass` vtable — the backend virtuals.
pub trait BackendImpl {
    fn get_stage_manager(&self) -> Option<StageManager> { None }
    fn get_default_seat(&self) -> Option<Seat> { None }
    fn get_keymap(&self) -> Option<Box<dyn Keymap>> { None }
    fn create_context(&mut self) -> Option<CoglContext> { None }
    fn ensure_context(&mut self, _stage: StageId) {}
    fn add_stage(&mut self, _stage: StageId) {}
    fn remove_stage(&mut self, _stage: StageId) {}
    fn get_features(&self) -> FeatureFlags { FeatureFlags::NONE }
    fn get_max_texture_size(&self) -> i32 { 0 }
    fn get_buffer_age(&self, _stage: StageId) -> i32 { 0 }
    fn get_sync_delay(&self) -> i32 { 0 }
    fn set_sync_delay(&mut self, _delay: i32) {}
    fn get_stage_window(&self) -> Option<StageWindow> { None }
    fn set_stage_window(&mut self, _window: StageWindow) {}
    fn get_input_method(&self) -> Option<InputMethod> { None }
    fn set_input_method(&mut self, _method: InputMethod) {}
    fn get_caps_lock_state(&self) -> bool { false }
    fn set_caps_lock_state(&mut self, _state: bool) {}
    fn get_num_lock_state(&self) -> bool { false }
    fn set_num_lock_state(&mut self, _state: bool) {}
}

/// A no-op backend for tests and hosts.
#[derive(Debug, Default)]
pub struct NullBackend;
impl BackendImpl for NullBackend {}

/// Port of `ClutterBackend` — the platform/graphics backend base.
#[derive(Debug)]
pub struct Backend {
    context: Option<u32>,
    settings: Option<Settings>,
    stage_window: Option<StageWindow>,
    cogl_context: Option<CoglContext>,
    stage_manager: Option<StageManager>,
    resolution_x: f32,
    resolution_y: f32,
    text_direction: TextDirection,
    has_changed_dpi: bool,
    sync_delay: i32,
    input_method: Option<InputMethod>,
    caps_lock_state: bool,
    num_lock_state: bool,
    allowed_units_mask: UnitType,
    stages: Vec<StageId>,
    impl_: Box<dyn BackendImpl>,
}

impl Default for Backend {
    fn default() -> Self { Self::new(Box::new(NullBackend)) }
}

impl Backend {
    pub fn new(impl_: Box<dyn BackendImpl>) -> Self {
        Backend {
            context: None, settings: None, stage_window: None, cogl_context: None,
            stage_manager: None, resolution_x: 96.0, resolution_y: 96.0,
            text_direction: TextDirection::Ltr, has_changed_dpi: false, sync_delay: 0,
            input_method: None, caps_lock_state: false, num_lock_state: false,
            allowed_units_mask: UnitType::Pixel, stages: Vec::new(), impl_,
        }
    }

    pub fn get_context(&self) -> Option<u32> { self.context }
    pub fn set_context(&mut self, context: u32) { self.context = Some(context); }
    pub fn get_settings(&self) -> Option<&Settings> { self.settings.as_ref() }
    pub fn set_settings(&mut self, settings: Settings) { self.settings = Some(settings); }

    pub fn get_stage_manager(&mut self) -> Option<StageManager> {
        if let Some(sm) = self.impl_.get_stage_manager() { return Some(sm); }
        self.stage_manager
    }

    pub fn set_stage_manager(&mut self, manager: StageManager) { self.stage_manager = Some(manager); }
    pub fn get_cogl_context(&self) -> Option<&CoglContext> { self.cogl_context.as_ref() }
    pub fn set_cogl_context(&mut self, ctx: CoglContext) { self.cogl_context = Some(ctx); }
    pub fn get_resolution_x(&self) -> f32 { self.resolution_x }
    pub fn get_resolution_y(&self) -> f32 { self.resolution_y }

    pub fn set_font_dpi(&mut self, dpi: f32) -> bool {
        let changed = (self.resolution_x - dpi).abs() > f32::EPSILON
            || (self.resolution_y - dpi).abs() > f32::EPSILON;
        self.resolution_x = dpi;
        self.resolution_y = dpi;
        if changed { self.has_changed_dpi = true; }
        changed
    }

    pub fn has_changed_dpi(&self) -> bool { self.has_changed_dpi }
    pub fn clear_dpi_changed(&mut self) { self.has_changed_dpi = false; }
    pub fn get_text_direction(&self) -> TextDirection { self.text_direction }
    pub fn set_text_direction(&mut self, dir: TextDirection) { self.text_direction = dir; }

    pub fn get_stage_window(&mut self) -> Option<StageWindow> {
        if let Some(w) = self.impl_.get_stage_window() { return Some(w); }
        self.stage_window
    }

    pub fn set_stage_window(&mut self, window: StageWindow) {
        self.impl_.set_stage_window(window);
        self.stage_window = Some(window);
    }

    pub fn get_default_seat(&mut self) -> Option<Seat> { self.impl_.get_default_seat() }
    pub fn get_keymap(&mut self) -> Option<Box<dyn Keymap>> { self.impl_.get_keymap() }

    pub fn create_context(&mut self) -> Option<CoglContext> { self.impl_.create_context() }

    pub fn ensure_context(&mut self, stage: StageId) {
        if self.cogl_context.is_none() {
            if let Some(ctx) = self.create_context() { self.cogl_context = Some(ctx); }
        }
        self.impl_.ensure_context(stage);
    }

    pub fn add_stage(&mut self, stage: StageId) {
        if !self.stages.contains(&stage) { self.stages.push(stage); }
        self.impl_.add_stage(stage);
    }

    pub fn remove_stage(&mut self, stage: StageId) {
        self.stages.retain(|s| *s != stage);
        self.impl_.remove_stage(stage);
    }

    pub fn stages(&self) -> &[StageId] { &self.stages }
    pub fn n_stages(&self) -> usize { self.stages.len() }
    pub fn get_features(&self) -> FeatureFlags { self.impl_.get_features() }
    pub fn get_max_texture_size(&self) -> i32 { self.impl_.get_max_texture_size() }
    pub fn get_buffer_age(&self, stage: StageId) -> i32 { self.impl_.get_buffer_age(stage) }

    pub fn get_sync_delay(&self) -> i32 {
        let d = self.impl_.get_sync_delay();
        if d != 0 { d } else { self.sync_delay }
    }

    pub fn set_sync_delay(&mut self, delay: i32) {
        self.impl_.set_sync_delay(delay);
        self.sync_delay = delay;
    }

    pub fn get_input_method(&mut self) -> Option<InputMethod> {
        if let Some(im) = self.impl_.get_input_method() { return Some(im); }
        self.input_method
    }

    pub fn set_input_method(&mut self, method: InputMethod) {
        self.impl_.set_input_method(method);
        self.input_method = Some(method);
    }

    pub fn get_caps_lock_state(&self) -> bool {
        let s = self.impl_.get_caps_lock_state();
        if s { s } else { self.caps_lock_state }
    }

    pub fn set_caps_lock_state(&mut self, state: bool) {
        self.impl_.set_caps_lock_state(state);
        self.caps_lock_state = state;
    }

    pub fn get_num_lock_state(&self) -> bool {
        let s = self.impl_.get_num_lock_state();
        if s { s } else { self.num_lock_state }
    }

    pub fn set_num_lock_state(&mut self, state: bool) {
        self.impl_.set_num_lock_state(state);
        self.num_lock_state = state;
    }

    pub fn get_allowed_units(&self) -> UnitType { self.allowed_units_mask }
    pub fn set_allowed_units(&mut self, units: UnitType) { self.allowed_units_mask = units; }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_backend_defaults() {
        let backend = Backend::default();
        assert!(backend.get_context().is_none());
        assert!(backend.get_settings().is_none());
        assert!(backend.get_cogl_context().is_none());
        assert_eq!(backend.get_resolution_x(), 96.0);
        assert_eq!(backend.get_text_direction(), TextDirection::Ltr);
        assert!(!backend.has_changed_dpi());
        assert_eq!(backend.get_sync_delay(), 0);
        assert!(!backend.get_caps_lock_state());
        assert_eq!(backend.get_allowed_units(), UnitType::Pixel);
        assert_eq!(backend.n_stages(), 0);
    }

    #[test]
    fn set_font_dpi_marks_changed() {
        let mut backend = Backend::default();
        assert!(!backend.set_font_dpi(96.0));
        assert!(backend.set_font_dpi(120.0));
        assert!(backend.has_changed_dpi());
        assert_eq!(backend.get_resolution_x(), 120.0);
        backend.clear_dpi_changed();
        assert!(!backend.has_changed_dpi());
    }

    #[test]
    fn add_remove_stages() {
        let mut backend = Backend::default();
        let s1 = StageId(0);
        let s2 = StageId(1);
        backend.add_stage(s1);
        backend.add_stage(s2);
        assert_eq!(backend.n_stages(), 2);
        backend.add_stage(s1);
        assert_eq!(backend.n_stages(), 2);
        backend.remove_stage(s1);
        assert_eq!(backend.n_stages(), 1);
        assert_eq!(backend.stages(), &[s2]);
    }

    #[test]
    fn ensure_context_creates_cogl_context() {
        struct TestBackend;
        impl BackendImpl for TestBackend {
            fn create_context(&mut self) -> Option<CoglContext> { Some(CoglContext { handle: 99 }) }
        }
        let mut backend = Backend::new(Box::new(TestBackend));
        assert!(backend.get_cogl_context().is_none());
        backend.ensure_context(StageId(0));
        assert_eq!(backend.get_cogl_context().unwrap().handle, 99);
    }

    #[test]
    fn feature_flags_bitfield() {
        let flags = FeatureFlags::STAGE_CURSOR.union(FeatureFlags::SHADERS_GLSL);
        assert!(flags.contains(FeatureFlags::STAGE_CURSOR));
        assert!(flags.contains(FeatureFlags::SHADERS_GLSL));
        assert!(!flags.contains(FeatureFlags::TEXTURE_YUV));
    }
}
