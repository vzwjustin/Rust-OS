//! Mutter compositor initialization ported from GNOME Mutter's src/core/mutter.c
//!
//! Main compositor setup and initialization for RustOS. This module provides the
//! top-level API for initializing and managing a Mutter-based compositor session.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/mutter.c

use super::context::{ContextOptions, ContextState, MetaContext};
use super::context_main::ContextMain;
use super::display::{DisplayId, MetaDisplay};
use super::keybindings::KeyBindingManager;
use super::workspace_manager::MetaWorkspaceManager;
use alloc::string::String;

fn context_options_from_main(main: &ContextMain) -> ContextOptions {
    let options = main.options();
    let backend = if options.headless || options.devkit {
        "headless"
    } else if options.wayland {
        "wayland"
    } else {
        "native"
    };

    ContextOptions {
        name: String::from(main.name()),
        backend: String::from(backend),
        unsafe_mode: options.unsafe_mode,
    }
}

impl MutterCompositor {
    /// Create a compositor from parsed mutter-main state.
    pub fn from_context_main(main: &ContextMain) -> Self {
        Self::with_options(context_options_from_main(main))
    }

    /// Run the configured mutter-main lifecycle through setup and start.
    pub fn run_context_main(main: &mut ContextMain) -> Result<Self, &'static str> {
        main.setup()?;

        let mut compositor = Self::from_context_main(main);
        compositor.initialize()?;

        if main.options().wayland {
            compositor.context_mut().set_wayland_compositor(true);
        }

        compositor.start()?;
        main.notify_ready();

        Ok(compositor)
    }
}

/// Global compositor instance (singleton-like).
pub struct MutterCompositor {
    /// Application context.
    context: MetaContext,
    /// Keybindings manager.
    keybindings: KeyBindingManager,
    /// Workspace manager.
    workspace_manager: MetaWorkspaceManager,
    /// Mutter Development Kit (MDK) service.
    mdk: Option<super::mdk::Mdk>,
    /// Initialized flag.
    initialized: bool,
}

impl MutterCompositor {
    /// Create a new Mutter compositor instance.
    pub fn new() -> Self {
        MutterCompositor {
            context: MetaContext::new(),
            keybindings: KeyBindingManager::new(),
            workspace_manager: MetaWorkspaceManager::new(4),
            mdk: None,
            initialized: false,
        }
    }

    /// Create with custom options.
    pub fn with_options(options: ContextOptions) -> Self {
        MutterCompositor {
            context: MetaContext::with_options(options),
            keybindings: KeyBindingManager::new(),
            workspace_manager: MetaWorkspaceManager::new(4),
            mdk: None,
            initialized: false,
        }
    }

    /// Initialize the compositor (call once on startup).
    pub fn initialize(&mut self) -> Result<(), &'static str> {
        if self.initialized {
            return Err("Compositor already initialized");
        }

        // Initialize the display
        self.context.initialize_display();

        // Update state
        self.context.set_state(ContextState::Configured);
        self.context.set_state(ContextState::Setup);

        // Initialize MDK
        let mdk_flag = super::mdk::MdkFlag::LaunchViewer;
        match super::mdk::Mdk::new(mdk_flag, None, None, None) {
            Ok(mdk) => {
                self.mdk = Some(mdk);
            }
            Err(e) => {
                crate::serial_println!("Mutter: Failed to initialize MDK: {}", e);
            }
        }

        self.initialized = true;
        Ok(())
    }

    /// Start the compositor (begins main loop).
    pub fn start(&mut self) -> Result<(), &'static str> {
        if !self.initialized {
            return Err("Compositor not initialized");
        }

        self.context.set_state(ContextState::Started);
        self.context.set_state(ContextState::Running);

        Ok(())
    }

    /// Get immutable reference to MDK.
    pub fn mdk(&self) -> Option<&super::mdk::Mdk> {
        self.mdk.as_ref()
    }

    /// Get mutable reference to MDK.
    pub fn mdk_mut(&mut self) -> Option<&mut super::mdk::Mdk> {
        self.mdk.as_mut()
    }

    /// Shutdown the compositor.
    pub fn shutdown(&mut self) {
        self.context.prepare_shutdown();
        self.context.set_state(ContextState::Terminated);
    }

    /// Check if compositor is running.
    pub fn is_running(&self) -> bool {
        self.context.state() == ContextState::Running
    }

    /// Get mutable reference to context.
    pub fn context_mut(&mut self) -> &mut MetaContext {
        &mut self.context
    }

    /// Get immutable reference to context.
    pub fn context(&self) -> &MetaContext {
        &self.context
    }

    /// Get mutable reference to keybindings manager.
    pub fn keybindings_mut(&mut self) -> &mut KeyBindingManager {
        &mut self.keybindings
    }

    /// Get immutable reference to keybindings manager.
    pub fn keybindings(&self) -> &KeyBindingManager {
        &self.keybindings
    }

    /// Get mutable reference to workspace manager.
    pub fn workspace_manager_mut(&mut self) -> &mut MetaWorkspaceManager {
        &mut self.workspace_manager
    }

    /// Get immutable reference to workspace manager.
    pub fn workspace_manager(&self) -> &MetaWorkspaceManager {
        &self.workspace_manager
    }

    /// Get mutable reference to display (if initialized).
    pub fn display_mut(&mut self) -> Option<&mut MetaDisplay> {
        self.context.display_mut()
    }

    /// Get immutable reference to display (if initialized).
    pub fn display(&self) -> Option<&MetaDisplay> {
        self.context.display()
    }

    /// Set number of workspaces.
    pub fn set_workspace_count(&mut self, count: usize) -> bool {
        if count == 0 || count > 16 {
            return false; // Validate reasonable range
        }

        // Create new workspace manager with updated count
        self.workspace_manager = MetaWorkspaceManager::new(count);
        true
    }

    /// Get number of workspaces.
    pub fn workspace_count(&self) -> usize {
        self.workspace_manager.count()
    }

    /// Activate a workspace by index.
    pub fn activate_workspace(&mut self, index: usize) -> bool {
        self.workspace_manager.activate_workspace(index)
    }

    /// Get active workspace index.
    pub fn active_workspace_index(&self) -> usize {
        self.workspace_manager.active_index()
    }

    /// Process a key event and execute bound action.
    pub fn handle_key_event(
        &mut self,
        key: super::keybindings::KeyCode,
        modifiers: &[super::keybindings::KeyModifier],
    ) -> Option<super::keybindings::KeyAction> {
        let action = self.keybindings.handle_key_press(key, modifiers)?;

        // Execute the action
        match action {
            super::keybindings::KeyAction::SwitchWorkspaceNext => {
                self.workspace_manager.switch_next();
            }
            super::keybindings::KeyAction::SwitchWorkspacePrevious => {
                self.workspace_manager.switch_previous();
            }
            _ => {
                // Other actions handled by caller or display
            }
        }

        Some(action)
    }
}

impl Default for MutterCompositor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compositor_creation() {
        let compositor = MutterCompositor::new();
        assert!(!compositor.initialized);
        assert!(!compositor.is_running());
    }

    #[test]
    fn test_compositor_initialization() {
        let mut compositor = MutterCompositor::new();
        let result = compositor.initialize();

        assert!(result.is_ok());
        assert!(compositor.initialized);
        assert!(compositor.context().has_display());
    }

    #[test]
    fn test_compositor_startup() {
        let mut compositor = MutterCompositor::new();
        compositor.initialize().unwrap();

        let result = compositor.start();
        assert!(result.is_ok());
        assert!(compositor.is_running());
    }

    #[test]
    fn test_double_initialize_fails() {
        let mut compositor = MutterCompositor::new();
        compositor.initialize().unwrap();

        let result = compositor.initialize();
        assert!(result.is_err());
    }

    #[test]
    fn test_start_without_init_fails() {
        let mut compositor = MutterCompositor::new();
        let result = compositor.start();
        assert!(result.is_err());
    }

    #[test]
    fn test_workspace_management() {
        let mut compositor = MutterCompositor::new();

        assert_eq!(compositor.workspace_count(), 4);
        assert_eq!(compositor.active_workspace_index(), 0);

        compositor.activate_workspace(2);
        assert_eq!(compositor.active_workspace_index(), 2);

        assert!(compositor.set_workspace_count(6));
        assert_eq!(compositor.workspace_count(), 6);
    }

    #[test]
    fn test_workspace_switching() {
        let mut compositor = MutterCompositor::new();

        assert_eq!(compositor.active_workspace_index(), 0);

        // Use keybinding to switch
        use super::keybindings::{KeyAction, KeyBinding, KeyCode, KeyModifier};

        let binding = KeyBinding::new(
            KeyCode(1),
            vec![KeyModifier::Alt],
            KeyAction::SwitchWorkspaceNext,
        );
        compositor.keybindings_mut().register_binding(binding);

        let action = compositor.handle_key_event(KeyCode(1), &[KeyModifier::Alt]);
        assert_eq!(action, Some(KeyAction::SwitchWorkspaceNext));
        assert_eq!(compositor.active_workspace_index(), 1);
    }

    #[test]
    fn test_shutdown() {
        let mut compositor = MutterCompositor::new();
        compositor.initialize().unwrap();
        compositor.start().unwrap();

        assert!(compositor.is_running());

        compositor.shutdown();
        assert!(!compositor.is_running());
    }
}
