//! Port of GNOME mutter's `clutter/clutter-context.{c,h}`.
//!
//! `ClutterContext` is the global per-application singleton holding the
//! backend, settings, stage manager, color manager, pipeline cache, text
//! direction, and frame dispatch/presentation state. In C it's a GObject;
//! here it's a plain struct with opaque-handle placeholders for backend
//! resources.
//!
//! # What's ported
//!
//! - `ClutterContext` struct fields: backend, settings, stage_manager,
//!   text_direction, color_manager, pipeline_cache, show_fps,
//!   last_repaint_id, events_queue (queue placeholder).
//! - Getters: `get_backend`, `get_settings`, `get_stage_manager`,
//!   `get_text_direction`, `get_pipeline_cache`, `get_color_manager`,
//!   `get_show_fps`.
//! - `get_accessibility_enabled` (global state).
//!
//! # What's skipped, with rationale
//!
//! - `clutter_context_new` / `clutter_context_destroy`: GObject
//!   lifecycle with backend constructor callback. Backends are opaque;
//!   direct construction deferred to backend integration.
//! - Backend wiring (`clutter_backend_create_context`, accessibility
//!   init, paint node type init): opaque backend functions.
//! - Font map / font renderer: HAVE_FONTS conditional; skipped with
//!   opaque placeholder type.
//! - Environment variable parsing (`init_clutter_debug`): deferred to
//!   caller/integration.
//! - Stage manager, color manager, pipeline cache: opaque handle types;
//!   storage only, no construction or lifetime.
//! - Events queue: placeholder type for async event dispatch.
//! - `clutter_get_text_direction` (C static function parsing env and
//!   Pango language): skipped; hardcoded `TextDirection::Ltr` as default.
//!
//! As with the rest of `mutter_port::clutter`, this module uses no
//! `unsafe`, no external crates, and only `core`/`alloc`.

use core::sync::atomic::{AtomicBool, Ordering};

/// Text direction: LTR (default) or RTL.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum TextDirection {
    /// Left-to-right (default).
    #[default]
    Ltr = 0,
    /// Right-to-left.
    Rtl = 1,
}

/// Opaque handle to a ClutterBackend (platform/graphics integration).
#[derive(Debug, Clone, Copy)]
pub struct Backend;

/// Opaque handle to ClutterSettings (UI configuration singleton).
#[derive(Debug, Clone, Copy)]
pub struct Settings;

/// Opaque handle to ClutterStageManager (window/stage collection).
#[derive(Debug, Clone, Copy)]
pub struct StageManager;

/// Opaque handle to ClutterColorManager (color space management).
#[derive(Debug, Clone, Copy)]
pub struct ColorManager;

/// Opaque handle to ClutterPipelineCache (GPU pipeline optimization).
#[derive(Debug, Clone, Copy)]
pub struct PipelineCache;

/// Opaque placeholder for async event queue.
#[derive(Debug, Clone, Copy)]
pub struct EventsQueue;

/// Global accessibility enabled flag (mutable, read by `get_accessibility_enabled`).
static ACCESSIBILITY_ENABLED: AtomicBool = AtomicBool::new(true);

/// Port of `struct _ClutterContext`: global per-application state.
#[derive(Debug, Clone)]
pub struct Context {
    /// Opaque backend handle (platform/graphics).
    pub backend: Option<Backend>,
    /// Per-application UI settings.
    pub settings: Option<Settings>,
    /// Opaque stage/window manager.
    pub stage_manager: Option<StageManager>,
    /// Current text direction (LTR/RTL).
    pub text_direction: TextDirection,
    /// Color space manager.
    pub color_manager: Option<ColorManager>,
    /// GPU pipeline cache.
    pub pipeline_cache: Option<PipelineCache>,
    /// Frame-per-second debugging enabled.
    pub show_fps: bool,
    /// Last repaint frame counter.
    pub last_repaint_id: u64,
    /// Placeholder for async event queue.
    pub events_queue: Option<EventsQueue>,
}

impl Default for Context {
    fn default() -> Self {
        Self::new()
    }
}

impl Context {
    /// Create a new ClutterContext with defaults (no backend attached).
    pub fn new() -> Self {
        Context {
            backend: None,
            settings: None,
            stage_manager: None,
            text_direction: TextDirection::Ltr,
            color_manager: None,
            pipeline_cache: None,
            show_fps: false,
            last_repaint_id: 1,
            events_queue: None,
        }
    }

    /// Get the backend (opaque handle).
    pub fn get_backend(&self) -> Option<Backend> {
        self.backend
    }

    /// Set the backend (opaque handle).
    pub fn set_backend(&mut self, backend: Backend) {
        self.backend = Some(backend);
    }

    /// Get settings (opaque handle).
    pub fn get_settings(&self) -> Option<Settings> {
        self.settings
    }

    /// Set settings (opaque handle).
    pub fn set_settings(&mut self, settings: Settings) {
        self.settings = Some(settings);
    }

    /// Get stage manager (opaque handle).
    pub fn get_stage_manager(&self) -> Option<StageManager> {
        self.stage_manager
    }

    /// Set stage manager (opaque handle).
    pub fn set_stage_manager(&mut self, manager: StageManager) {
        self.stage_manager = Some(manager);
    }

    /// Get text direction (LTR/RTL).
    pub fn get_text_direction(&self) -> TextDirection {
        self.text_direction
    }

    /// Set text direction (LTR/RTL).
    pub fn set_text_direction(&mut self, dir: TextDirection) {
        self.text_direction = dir;
    }

    /// Get color manager (opaque handle).
    pub fn get_color_manager(&self) -> Option<ColorManager> {
        self.color_manager
    }

    /// Set color manager (opaque handle).
    pub fn set_color_manager(&mut self, manager: ColorManager) {
        self.color_manager = Some(manager);
    }

    /// Get pipeline cache (opaque handle).
    pub fn get_pipeline_cache(&self) -> Option<PipelineCache> {
        self.pipeline_cache
    }

    /// Set pipeline cache (opaque handle).
    pub fn set_pipeline_cache(&mut self, cache: PipelineCache) {
        self.pipeline_cache = Some(cache);
    }

    /// Get show_fps flag.
    pub fn get_show_fps(&self) -> bool {
        self.show_fps
    }

    /// Set show_fps flag.
    pub fn set_show_fps(&mut self, enabled: bool) {
        self.show_fps = enabled;
    }

    /// Get last repaint frame counter.
    pub fn get_last_repaint_id(&self) -> u64 {
        self.last_repaint_id
    }

    /// Set last repaint frame counter.
    pub fn set_last_repaint_id(&mut self, id: u64) {
        self.last_repaint_id = id;
    }

    /// Get events queue (opaque placeholder).
    pub fn get_events_queue(&self) -> Option<EventsQueue> {
        self.events_queue
    }

    /// Set events queue (opaque placeholder).
    pub fn set_events_queue(&mut self, queue: EventsQueue) {
        self.events_queue = Some(queue);
    }
}

/// Get whether Clutter accessibility support is enabled (global state).
/// Note: in C this reads a static variable initialized by env parsing.
pub fn get_accessibility_enabled() -> bool {
    ACCESSIBILITY_ENABLED.load(Ordering::Relaxed)
}

/// Set whether Clutter accessibility support is enabled (global state).
pub fn set_accessibility_enabled(enabled: bool) {
    ACCESSIBILITY_ENABLED.store(enabled, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_context_defaults() {
        let ctx = Context::new();
        assert_eq!(ctx.text_direction, TextDirection::Ltr);
        assert!(!ctx.show_fps);
        assert_eq!(ctx.last_repaint_id, 1);
        assert!(ctx.backend.is_none());
        assert!(ctx.settings.is_none());
        assert!(ctx.stage_manager.is_none());
        assert!(ctx.color_manager.is_none());
        assert!(ctx.pipeline_cache.is_none());
        assert!(ctx.events_queue.is_none());
    }

    #[test]
    fn set_get_backend() {
        let mut ctx = Context::new();
        let backend = Backend;
        ctx.set_backend(backend);
        assert!(ctx.get_backend().is_some());
    }

    #[test]
    fn set_get_text_direction() {
        let mut ctx = Context::new();
        ctx.set_text_direction(TextDirection::Rtl);
        assert_eq!(ctx.get_text_direction(), TextDirection::Rtl);
    }

    #[test]
    fn set_get_show_fps() {
        let mut ctx = Context::new();
        ctx.set_show_fps(true);
        assert!(ctx.get_show_fps());
    }

    #[test]
    fn set_get_last_repaint_id() {
        let mut ctx = Context::new();
        ctx.set_last_repaint_id(42);
        assert_eq!(ctx.get_last_repaint_id(), 42);
    }

    #[test]
    fn accessibility_enabled_default() {
        assert!(get_accessibility_enabled());
    }

    #[test]
    fn set_accessibility_enabled() {
        set_accessibility_enabled(false);
        assert!(!get_accessibility_enabled());
        set_accessibility_enabled(true);
        assert!(get_accessibility_enabled());
    }
}
