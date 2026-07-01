//! Compositor plugin interface for window effects and animations.
//!
//! Ported from mutter-main/src/compositor/meta-plugin.c and
//! mutter-main/src/compositor/meta-plugin-manager.c (GNU GPL 2+).
//! Provides hook points for minimize, map, destroy, workspace switch,
//! and other window events that a plugin can animate or handle.

use crate::desktop::window_manager::WindowId;
use alloc::boxed::Box;

/// Plugin hook for handling window and workspace events.
pub trait CompositorPlugin {
    /// Called when a window is about to be minimized.
    /// Return true if the plugin handled/animated the effect, false to use default.
    fn on_minimize(&mut self, window_id: WindowId) -> bool {
        false
    }

    /// Called when a window is about to be unminimized.
    /// Return true if the plugin handled/animated the effect, false to use default.
    fn on_unminimize(&mut self, window_id: WindowId) -> bool {
        false
    }

    /// Called when a window is about to be mapped (shown).
    /// Return true if the plugin handled/animated the effect, false to use default.
    fn on_map(&mut self, window_id: WindowId) -> bool {
        false
    }

    /// Called when a window is about to be destroyed.
    /// Return true if the plugin handled/animated the effect, false to use default.
    fn on_destroy(&mut self, window_id: WindowId) -> bool {
        false
    }

    /// Called when a window size is about to change.
    /// Return true if the plugin handled/animated the effect, false to use default.
    fn on_size_change(&mut self, window_id: WindowId) -> bool {
        false
    }

    /// Called when switching to a different workspace.
    /// Return true if the plugin handled/animated the transition, false to use default.
    fn on_switch_workspace(&mut self) -> bool {
        false
    }

    /// Called to kill any active minimize animation.
    fn kill_minimize_effects(&mut self, window_id: WindowId) {}

    /// Called to kill any active unminimize animation.
    fn kill_unminimize_effects(&mut self, window_id: WindowId) {}

    /// Called to kill any active map animation.
    fn kill_map_effects(&mut self, window_id: WindowId) {}

    /// Called to kill any active destroy animation.
    fn kill_destroy_effects(&mut self, window_id: WindowId) {}

    /// Called to kill any active size change animation.
    fn kill_size_change_effects(&mut self, window_id: WindowId) {}

    /// Called to kill any active workspace switch animation.
    fn kill_workspace_switch(&mut self) {}

    /// Called when the plugin is started.
    fn start(&mut self) {}
}

/// Manager that dispatches compositor events to the active plugin.
pub struct PluginManager {
    plugin: Option<Box<dyn CompositorPlugin>>,
}

impl PluginManager {
    /// Create a new plugin manager without an active plugin.
    pub fn new() -> Self {
        Self { plugin: None }
    }

    /// Set the active compositor plugin.
    pub fn set_plugin(&mut self, plugin: Box<dyn CompositorPlugin>) {
        self.plugin = Some(plugin);
    }

    /// Notify the plugin of a minimize event.
    pub fn on_minimize(&mut self, window_id: WindowId) -> bool {
        self.plugin
            .as_mut()
            .map(|p| p.on_minimize(window_id))
            .unwrap_or(false)
    }

    /// Notify the plugin of an unminimize event.
    pub fn on_unminimize(&mut self, window_id: WindowId) -> bool {
        self.plugin
            .as_mut()
            .map(|p| p.on_unminimize(window_id))
            .unwrap_or(false)
    }

    /// Notify the plugin of a map event.
    pub fn on_map(&mut self, window_id: WindowId) -> bool {
        self.plugin
            .as_mut()
            .map(|p| p.on_map(window_id))
            .unwrap_or(false)
    }

    /// Notify the plugin of a destroy event.
    pub fn on_destroy(&mut self, window_id: WindowId) -> bool {
        self.plugin
            .as_mut()
            .map(|p| p.on_destroy(window_id))
            .unwrap_or(false)
    }

    /// Notify the plugin of a size change event.
    pub fn on_size_change(&mut self, window_id: WindowId) -> bool {
        self.plugin
            .as_mut()
            .map(|p| p.on_size_change(window_id))
            .unwrap_or(false)
    }

    /// Notify the plugin of a workspace switch event.
    pub fn on_switch_workspace(&mut self) -> bool {
        self.plugin
            .as_mut()
            .map(|p| p.on_switch_workspace())
            .unwrap_or(false)
    }

    /// Kill all active window effect animations.
    pub fn kill_window_effects(&mut self, window_id: WindowId) {
        if let Some(p) = &mut self.plugin {
            p.kill_minimize_effects(window_id);
            p.kill_unminimize_effects(window_id);
            p.kill_map_effects(window_id);
            p.kill_destroy_effects(window_id);
            p.kill_size_change_effects(window_id);
        }
    }

    /// Kill any active workspace switch animation.
    pub fn kill_workspace_switch(&mut self) {
        if let Some(p) = &mut self.plugin {
            p.kill_workspace_switch();
        }
    }

    /// Start the plugin.
    pub fn start(&mut self) {
        if let Some(p) = &mut self.plugin {
            p.start();
        }
    }

    /// Check if a plugin is active.
    pub fn has_plugin(&self) -> bool {
        self.plugin.is_some()
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}
