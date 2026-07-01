//! Port of GNOME mutter's `clutter/clutter-main.{c,h}`.
//!
//! Top-level initialization and event-dispatch module.
//!
//! # What's ported
//!
//! - `MainContext` struct: `is_initialized`, `main_loop_level`, `has_quit`,
//!   `show_fps`, `font_name`, `text_direction`, `backend_type`, grabs.
//! - `clutter_init` / `clutter_init_with_args`.
//! - `clutter_main` / `clutter_main_quit` / `clutter_main_level`.
//! - `clutter_get_show_fps` / `clutter_set_show_fps`.
//! - `clutter_get_font_name` / `set_font_name`.
//! - `clutter_get_default_text_direction` / `set_default_text_direction`.
//! - `clutter_check_windowing_backend`.
//! - Deprecated grab API: `grab_pointer` / `grab_keyboard` etc.
//! - `clutter_do_event`: event dispatch with grab routing.
//!
//! # What's skipped
//!
//! - GObject main loop (`GMainLoop`): nesting tracked only.
//! - `clutter_get_current_event` / event filters / global event queue.
//! - `clutter_get_font_map` / Pango.
//! - `GOptionContext` command-line parsing.

#![allow(dead_code)]

use alloc::string::String;
use alloc::vec::Vec;

use super::super::actor::ActorId;
use super::super::context::TextDirection;
use super::super::event::Event;

/// Result of `clutter_init`. Mirrors `ClutterInitError`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InitResult {
    Success,
    ErrorAlreadyInitialized,
    ErrorBackendUnavailable,
}

/// Backend type identifier, used by `clutter_check_windowing_backend`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[repr(u32)]
pub enum BackendType {
    #[default]
    None = 0,
    Native = 1,
    X11 = 2,
    Wayland = 3,
    Headless = 4,
}

/// Port of the global state in `clutter-main.c`.
#[derive(Debug, Clone)]
pub struct MainContext {
    is_initialized: bool,
    main_loop_level: i32,
    has_quit: bool,
    show_fps: bool,
    font_name: String,
    text_direction: TextDirection,
    custom_backend_func: Option<u32>,
    backend_type: BackendType,
    pointer_grab: Option<ActorId>,
    keyboard_grab: Option<ActorId>,
    accessibility_enabled: bool,
    glyph_cache_dirty: bool,
    pending_events: Vec<Event>,
}

impl Default for MainContext {
    fn default() -> Self { Self::new() }
}

impl MainContext {
    pub fn new() -> Self {
        MainContext {
            is_initialized: false, main_loop_level: 0, has_quit: false,
            show_fps: false, font_name: String::from("Sans 12"),
            text_direction: TextDirection::Ltr, custom_backend_func: None,
            backend_type: BackendType::None, pointer_grab: None, keyboard_grab: None,
            accessibility_enabled: true, glyph_cache_dirty: false, pending_events: Vec::new(),
        }
    }

    pub fn init(&mut self) -> InitResult {
        if self.is_initialized { return InitResult::ErrorAlreadyInitialized; }
        self.is_initialized = true;
        InitResult::Success
    }

    pub fn init_with_args(&mut self, backend_type: BackendType) -> InitResult {
        if self.is_initialized { return InitResult::ErrorAlreadyInitialized; }
        if backend_type == BackendType::None { return InitResult::ErrorBackendUnavailable; }
        self.backend_type = backend_type;
        self.is_initialized = true;
        InitResult::Success
    }

    pub fn is_initialized(&self) -> bool { self.is_initialized }

    pub fn main(&mut self) {
        self.main_loop_level = self.main_loop_level.saturating_add(1);
        self.has_quit = false;
    }

    pub fn main_quit(&mut self) -> i32 {
        self.has_quit = true;
        if self.main_loop_level > 0 { self.main_loop_level -= 1; }
        self.main_loop_level
    }

    pub fn main_level(&self) -> i32 { self.main_loop_level }
    pub fn has_quit(&self) -> bool { self.has_quit }
    pub fn get_show_fps(&self) -> bool { self.show_fps }
    pub fn set_show_fps(&mut self, show: bool) { self.show_fps = show; }
    pub fn get_font_name(&self) -> &str { &self.font_name }
    pub fn set_font_name(&mut self, name: impl Into<String>) { self.font_name = name.into(); }
    pub fn get_default_text_direction(&self) -> TextDirection { self.text_direction }
    pub fn set_default_text_direction(&mut self, dir: TextDirection) { self.text_direction = dir; }
    pub fn set_custom_backend_func(&mut self, func_token: u32) { self.custom_backend_func = Some(func_token); }
    pub fn get_custom_backend_func(&self) -> Option<u32> { self.custom_backend_func }
    pub fn check_windowing_backend(&self, backend_type: BackendType) -> bool { self.backend_type == backend_type }
    pub fn set_backend_type(&mut self, backend_type: BackendType) { self.backend_type = backend_type; }
    pub fn get_backend_type(&self) -> BackendType { self.backend_type }
    pub fn get_accessibility_enabled(&self) -> bool { self.accessibility_enabled }
    pub fn disable_accessibility(&mut self) { self.accessibility_enabled = false; }
    pub fn set_accessibility_enabled(&mut self, enabled: bool) { self.accessibility_enabled = enabled; }

    pub fn grab_pointer(&mut self, actor: ActorId) -> Option<ActorId> {
        let prev = self.pointer_grab;
        self.pointer_grab = Some(actor);
        prev
    }
    pub fn ungrab_pointer(&mut self) { self.pointer_grab = None; }
    pub fn get_pointer_grab(&self) -> Option<ActorId> { self.pointer_grab }

    pub fn grab_keyboard(&mut self, actor: ActorId) -> Option<ActorId> {
        let prev = self.keyboard_grab;
        self.keyboard_grab = Some(actor);
        prev
    }
    pub fn ungrab_keyboard(&mut self) { self.keyboard_grab = None; }
    pub fn get_keyboard_grab(&self) -> Option<ActorId> { self.keyboard_grab }

    pub fn do_event(&mut self, event: &Event) -> bool {
        if self.pointer_grab.is_some() {
            match event {
                Event::Button(_) | Event::Motion(_) | Event::Scroll(_) | Event::Touch(_) | Event::Crossing(_) => return true,
                _ => {}
            }
        }
        if self.keyboard_grab.is_some() {
            match event { Event::Key(_) => return true, _ => {} }
        }
        self.pending_events.push(event.clone());
        false
    }

    pub fn drain_pending_events(&mut self) -> Vec<Event> { core::mem::take(&mut self.pending_events) }
    pub fn n_pending_events(&self) -> usize { self.pending_events.len() }

    pub fn clear_glyph_cache(&mut self) { self.glyph_cache_dirty = true; }
    pub fn is_glyph_cache_dirty(&self) -> bool { self.glyph_cache_dirty }
    pub fn clear_glyph_cache_dirty(&mut self) { self.glyph_cache_dirty = false; }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::event::{AnyEvent, EventFlags};

    fn dummy_event() -> Event {
        Event::Any(AnyEvent { time_us: 0, flags: EventFlags::NONE, source_device: None })
    }

    #[test]
    fn init_defaults() {
        let ctx = MainContext::new();
        assert!(!ctx.is_initialized());
        assert_eq!(ctx.main_level(), 0);
        assert!(!ctx.has_quit());
        assert_eq!(ctx.get_font_name(), "Sans 12");
        assert!(ctx.get_accessibility_enabled());
    }

    #[test]
    fn init_success_then_already_initialized() {
        let mut ctx = MainContext::new();
        assert_eq!(ctx.init(), InitResult::Success);
        assert_eq!(ctx.init(), InitResult::ErrorAlreadyInitialized);
    }

    #[test]
    fn init_with_args_sets_backend() {
        let mut ctx = MainContext::new();
        assert_eq!(ctx.init_with_args(BackendType::Native), InitResult::Success);
        assert!(ctx.check_windowing_backend(BackendType::Native));
    }

    #[test]
    fn init_with_args_none_backend_fails() {
        let mut ctx = MainContext::new();
        assert_eq!(ctx.init_with_args(BackendType::None), InitResult::ErrorBackendUnavailable);
    }

    #[test]
    fn main_loop_nesting() {
        let mut ctx = MainContext::new();
        ctx.main();
        assert_eq!(ctx.main_level(), 1);
        ctx.main();
        assert_eq!(ctx.main_level(), 2);
        ctx.main_quit();
        assert_eq!(ctx.main_level(), 1);
        ctx.main_quit();
        assert_eq!(ctx.main_level(), 0);
        ctx.main_quit(); // no-op below zero
        assert_eq!(ctx.main_level(), 0);
    }

    #[test]
    fn do_event_queues_unhandled() {
        let mut ctx = MainContext::new();
        let event = dummy_event();
        assert!(!ctx.do_event(&event));
        assert_eq!(ctx.n_pending_events(), 1);
        let drained = ctx.drain_pending_events();
        assert_eq!(drained.len(), 1);
        assert_eq!(ctx.n_pending_events(), 0);
    }
}
