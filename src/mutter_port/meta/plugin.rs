//! Mutter plugin subsystem
//! Ported from meta/meta-plugin.h
use alloc::{format, string::String, vec::Vec};

use crate::mutter_port::meta::display::MetaDisplay;
use crate::mutter_port::meta::window::MetaWindow;
use core::cell::Cell;

/// Plugin version compatibility check
pub const META_PLUGIN_API_VERSION: i32 = 13;

/// Animation states tracked by the plugin system.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginAnimation {
    None,
    Minimize,
    Unminimize,
    Map,
    Destroy,
    SwitchWorkspace,
}

/// Base plugin class for window manager extensions
pub struct MetaPlugin {
    pub name: String,
    manager: *mut MetaDisplay,
    /// Currently active animation.
    current_animation: Cell<PluginAnimation>,
    /// Whether an animation is in progress.
    animating: Cell<bool>,
}

impl MetaPlugin {
    pub fn new(name: String) -> Self {
        Self {
            name,
            manager: core::ptr::null_mut(),
            current_animation: Cell::new(PluginAnimation::None),
            animating: Cell::new(false),
        }
    }

    /// Get plugin name
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Set the manager (display) pointer.
    pub fn set_manager(&mut self, manager: *mut MetaDisplay) {
        self.manager = manager;
    }

    /// Get the manager this plugin belongs to.
    /// Resolves the stored typed pointer to the display.
    pub fn get_manager(&self) -> Option<&MetaDisplay> {
        if self.manager.is_null() {
            None
        } else {
            // SAFETY: The pointer was set by `set_manager` with a valid
            // `*mut MetaDisplay`. The caller guarantees the referent
            // outlives this borrow.
            unsafe { Some(&*self.manager) }
        }
    }

    /// Begin an animation. Sets the current animation and marks as animating.
    fn begin_animation(&self, anim: PluginAnimation) {
        self.current_animation.set(anim);
        self.animating.set(true);
    }

    /// End the current animation. Clears the animation state.
    pub fn end_animation(&self) {
        self.current_animation.set(PluginAnimation::None);
        self.animating.set(false);
    }

    /// Whether an animation is currently in progress.
    pub fn is_animating(&self) -> bool {
        self.animating.get()
    }

    /// Get the current animation type.
    pub fn current_animation(&self) -> PluginAnimation {
        self.current_animation.get()
    }

    /// Minimize window animation. Marks the minimize animation as
    /// in progress. A full implementation would drive the Clutter
    /// timeline/effect to animate the window shrinking.
    pub fn minimize(&self, _window: &MetaWindow) {
        self.begin_animation(PluginAnimation::Minimize);
    }

    /// Unminimize window animation. Marks the unminimize animation
    /// as in progress.
    pub fn unminimize(&self, _window: &MetaWindow) {
        self.begin_animation(PluginAnimation::Unminimize);
    }

    /// Map window animation. Marks the map animation as in progress.
    pub fn map(&self, _window: &MetaWindow) {
        self.begin_animation(PluginAnimation::Map);
    }

    /// Destroy window animation. Marks the destroy animation as
    /// in progress.
    pub fn destroy(&self, _window: &MetaWindow) {
        self.begin_animation(PluginAnimation::Destroy);
    }

    /// Switch workspace animation. Marks the workspace switch
    /// animation as in progress.
    pub fn switch_workspace(&self) {
        self.begin_animation(PluginAnimation::SwitchWorkspace);
    }

    /// Check plugin compatibility
    pub fn check_version(version: i32) -> bool {
        version == META_PLUGIN_API_VERSION
    }
}

impl Default for MetaPlugin {
    fn default() -> Self {
        Self::new(String::new())
    }
}
