//! Compositor module management ported from `meta-module.c`.
//!
//! Manages loading and initialization of compositor modules/plugins.

use alloc::vec::Vec;

/// Module type identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleType {
    /// Rendering backend module
    Backend,
    /// Input handling module
    Input,
    /// Effects/animation module
    Effects,
    /// Custom plugin module
    Plugin,
}

/// Loaded module information
#[derive(Debug)]
pub struct Module {
    pub id: u32,
    pub name: usize, // String reference
    pub module_type: ModuleType,
    pub loaded: bool,
    pub enabled: bool,
}

impl Module {
    /// Create new module
    pub fn new(id: u32, name_ref: usize, module_type: ModuleType) -> Self {
        Module {
            id,
            name: name_ref,
            module_type,
            loaded: false,
            enabled: false,
        }
    }

    /// Load module from storage
    pub fn load(&mut self) -> bool {
        self.loaded = true;
        true
    }

    /// Enable module
    pub fn enable(&mut self) -> bool {
        if self.loaded {
            self.enabled = true;
            true
        } else {
            false
        }
    }

    /// Disable module
    pub fn disable(&mut self) {
        self.enabled = false;
    }

    /// Unload module
    pub fn unload(&mut self) {
        self.enabled = false;
        self.loaded = false;
    }
}

/// Module manager
pub struct ModuleManager {
    modules: Vec<Module>,
}

impl ModuleManager {
    /// Create new module manager
    pub fn new() -> Self {
        ModuleManager {
            modules: Vec::new(),
        }
    }

    /// Register module
    pub fn register_module(&mut self, module: Module) {
        self.modules.push(module);
    }

    /// Load all modules
    pub fn load_all(&mut self) -> bool {
        for module in &mut self.modules {
            if !module.load() {
                return false;
            }
        }
        true
    }

    /// Enable module by ID
    pub fn enable_module(&mut self, id: u32) -> bool {
        if let Some(module) = self.modules.iter_mut().find(|m| m.id == id) {
            module.enable()
        } else {
            false
        }
    }

    /// Disable module by ID
    pub fn disable_module(&mut self, id: u32) {
        if let Some(module) = self.modules.iter_mut().find(|m| m.id == id) {
            module.disable();
        }
    }

    /// Get module count
    pub fn count(&self) -> usize {
        self.modules.len()
    }
}

impl Default for ModuleManager {
    fn default() -> Self {
        Self::new()
    }
}
