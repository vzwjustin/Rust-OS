//! Plugin manager for compositor extensions ported from `meta-plugin-manager.c`.
//!
//! Manages effect plugins and compositor extensions.

use alloc::vec::Vec;

/// Effect plugin callback
pub type PluginCallback = fn(u32) -> bool;

/// Compositor plugin/effect
#[derive(Debug)]
pub struct Plugin {
    pub id: u32,
    pub name: usize, // String reference
    pub loaded: bool,
    pub enabled: bool,
}

impl Plugin {
    /// Create new plugin
    pub fn new(id: u32, name_ref: usize) -> Self {
        Plugin {
            id,
            name: name_ref,
            loaded: false,
            enabled: false,
        }
    }

    /// Load plugin
    pub fn load(&mut self) -> bool {
        self.loaded = true;
        true
    }

    /// Enable plugin
    pub fn enable(&mut self) -> bool {
        if self.loaded {
            self.enabled = true;
            true
        } else {
            false
        }
    }

    /// Disable plugin
    pub fn disable(&mut self) {
        self.enabled = false;
    }
}

/// Plugin manager for effects and extensions
pub struct PluginManager {
    plugins: Vec<Plugin>,
    callbacks: Vec<PluginCallback>,
}

impl PluginManager {
    /// Create new plugin manager
    pub fn new() -> Self {
        PluginManager {
            plugins: Vec::new(),
            callbacks: Vec::new(),
        }
    }

    /// Register plugin
    pub fn register_plugin(&mut self, plugin: Plugin) -> u32 {
        let id = plugin.id;
        self.plugins.push(plugin);
        id
    }

    /// Unregister plugin
    pub fn unregister_plugin(&mut self, id: u32) -> bool {
        if let Some(pos) = self.plugins.iter().position(|p| p.id == id) {
            self.plugins.remove(pos);
            true
        } else {
            false
        }
    }

    /// Enable plugin by ID
    pub fn enable_plugin(&mut self, id: u32) -> bool {
        if let Some(plugin) = self.plugins.iter_mut().find(|p| p.id == id) {
            plugin.enable()
        } else {
            false
        }
    }

    /// Disable plugin by ID
    pub fn disable_plugin(&mut self, id: u32) {
        if let Some(plugin) = self.plugins.iter_mut().find(|p| p.id == id) {
            plugin.disable();
        }
    }

    /// Get plugin count
    pub fn count(&self) -> usize {
        self.plugins.len()
    }

    /// Register callback
    pub fn register_callback(&mut self, callback: PluginCallback) {
        self.callbacks.push(callback);
    }

    /// Invoke all callbacks
    pub fn invoke_callbacks(&self, plugin_id: u32) {
        for callback in &self.callbacks {
            callback(plugin_id);
        }
    }
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}
