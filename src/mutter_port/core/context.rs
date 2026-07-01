//! Application context ported from GNOME Mutter's src/core/meta-context.c / meta-context-private.h
//!
//! Implements MetaContext which represents the global application context for a Mutter
//! compositor session. It manages the display, backend, main event loop, and overall
//! compositor state.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-context.c

use super::display::{DisplayId, MetaDisplay};
use alloc::string::String;

/// The state of the context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextState {
    /// Initial state, not yet configured.
    Init,
    /// Context has been configured.
    Configured,
    /// Backend setup is in progress.
    Setup,
    /// Started, but not yet running main loop.
    Started,
    /// Running main loop.
    Running,
    /// Terminated.
    Terminated,
}

/// Properties/hints for context configuration.
#[derive(Debug, Clone)]
pub struct ContextOptions {
    /// Human-readable context name.
    pub name: String,
    /// Backend plugin name (e.g., "native", "x11").
    pub backend: String,
    /// Whether to run in "unsafe" mode (skip some safety checks).
    pub unsafe_mode: bool,
}

impl Default for ContextOptions {
    fn default() -> Self {
        ContextOptions {
            name: "mutter".into(),
            backend: "native".into(),
            unsafe_mode: false,
        }
    }
}

/// Global compositor context managing a Mutter session.
#[derive(Debug)]
pub struct MetaContext {
    /// Display ID for this context.
    display_id: DisplayId,

    /// The central display/compositor object.
    display: Option<MetaDisplay>,

    /// Context state.
    state: ContextState,

    /// Context configuration options.
    options: ContextOptions,

    /// Whether we're currently processing events.
    processing_events: bool,

    /// Whether we've detected that we're running under Wayland.
    is_wayland_compositor: bool,

    /// Whether we've detected that we're running under X11.
    is_x11_compositor: bool,

    /// Server type.
    server_type: ServerType,
}

/// Type of X11/Wayland server being managed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerType {
    /// Native KMS/DRM backend (no X11/Wayland).
    Native,
    /// X11 server backend.
    X11,
    /// Wayland compositor.
    Wayland,
    /// Nested in another compositor.
    Nested,
}

impl Default for ServerType {
    fn default() -> Self {
        ServerType::Native
    }
}

impl MetaContext {
    /// Create a new compositor context with default options.
    pub fn new() -> Self {
        Self::with_options(ContextOptions::default())
    }

    /// Create a new compositor context with custom options.
    pub fn with_options(options: ContextOptions) -> Self {
        static DISPLAY_ID_COUNTER: core::sync::atomic::AtomicU32 =
            core::sync::atomic::AtomicU32::new(0);

        let display_id =
            DisplayId(DISPLAY_ID_COUNTER.fetch_add(1, core::sync::atomic::Ordering::Relaxed));

        MetaContext {
            display_id,
            display: None,
            state: ContextState::Init,
            options,
            processing_events: false,
            is_wayland_compositor: false,
            is_x11_compositor: false,
            server_type: ServerType::default(),
        }
    }

    /// Get the context display ID.
    pub fn display_id(&self) -> DisplayId {
        self.display_id
    }

    /// Get display name.
    pub fn name(&self) -> &str {
        &self.options.name
    }

    /// Get backend name.
    pub fn backend(&self) -> &str {
        &self.options.backend
    }

    /// Check if running in unsafe mode.
    pub fn unsafe_mode(&self) -> bool {
        self.options.unsafe_mode
    }

    /// Set whether we're a Wayland compositor.
    pub fn set_wayland_compositor(&mut self, is_wayland: bool) {
        self.is_wayland_compositor = is_wayland;
        if is_wayland {
            self.server_type = ServerType::Wayland;
        }
    }

    /// Check if we're a Wayland compositor.
    pub fn is_wayland_compositor(&self) -> bool {
        self.is_wayland_compositor
    }

    /// Set whether we're managing X11 windows.
    pub fn set_x11_compositor(&mut self, is_x11: bool) {
        self.is_x11_compositor = is_x11;
        if is_x11 && self.server_type == ServerType::Native {
            self.server_type = ServerType::X11;
        }
    }

    /// Check if we're managing X11 windows.
    pub fn is_x11_compositor(&self) -> bool {
        self.is_x11_compositor
    }

    /// Get the server type we're managing.
    pub fn server_type(&self) -> ServerType {
        self.server_type
    }

    /// Set context state.
    pub fn set_state(&mut self, state: ContextState) {
        self.state = state;
    }

    /// Get current context state.
    pub fn state(&self) -> ContextState {
        self.state
    }

    /// Initialize the display for this context.
    pub fn initialize_display(&mut self) {
        if self.display.is_none() {
            self.display = Some(MetaDisplay::new(self.display_id));
        }
    }

    /// Get reference to the display (if initialized).
    pub fn display(&self) -> Option<&MetaDisplay> {
        self.display.as_ref()
    }

    /// Get mutable reference to the display (if initialized).
    pub fn display_mut(&mut self) -> Option<&mut MetaDisplay> {
        self.display.as_mut()
    }

    /// Check if display is initialized.
    pub fn has_display(&self) -> bool {
        self.display.is_some()
    }

    /// Prepare to shutdown the context.
    pub fn prepare_shutdown(&mut self) {
        if let Some(display) = &mut self.display {
            display.begin_shutdown();
        }
    }

    /// Check if we're in the process of shutting down.
    pub fn is_shutting_down(&self) -> bool {
        self.state == ContextState::Terminated
            || self
                .display
                .as_ref()
                .is_some_and(|display| display.is_closing())
    }

    /// Mark that we're processing events.
    pub fn set_processing_events(&mut self, processing: bool) {
        self.processing_events = processing;
    }

    /// Check if we're currently processing events.
    pub fn is_processing_events(&self) -> bool {
        self.processing_events
    }

    /// Complete the shutdown process.
    pub fn finish_shutdown(&mut self) {
        self.state = ContextState::Terminated;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_creation() {
        let context = MetaContext::new();
        assert_eq!(context.state(), ContextState::Init);
        assert_eq!(context.server_type(), ServerType::Native);
        assert!(!context.is_wayland_compositor());
        assert!(!context.is_x11_compositor());
    }

    #[test]
    fn test_context_with_options() {
        let options = ContextOptions {
            name: "custom".into(),
            backend: "x11".into(),
            unsafe_mode: true,
        };

        let context = MetaContext::with_options(options);
        assert_eq!(context.name(), "custom");
        assert_eq!(context.backend(), "x11");
        assert!(context.unsafe_mode());
    }

    #[test]
    fn test_display_initialization() {
        let mut context = MetaContext::new();
        assert!(!context.has_display());

        context.initialize_display();
        assert!(context.has_display());
        assert!(context.display().is_some());
    }

    #[test]
    fn test_wayland_mode() {
        let mut context = MetaContext::new();
        assert_eq!(context.server_type(), ServerType::Native);

        context.set_wayland_compositor(true);
        assert!(context.is_wayland_compositor());
        assert_eq!(context.server_type(), ServerType::Wayland);
    }

    #[test]
    fn test_x11_mode() {
        let mut context = MetaContext::new();
        assert_eq!(context.server_type(), ServerType::Native);

        context.set_x11_compositor(true);
        assert!(context.is_x11_compositor());
        assert_eq!(context.server_type(), ServerType::X11);
    }

    #[test]
    fn test_state_transitions() {
        let mut context = MetaContext::new();
        assert_eq!(context.state(), ContextState::Init);

        context.set_state(ContextState::Configured);
        assert_eq!(context.state(), ContextState::Configured);

        context.set_state(ContextState::Running);
        assert_eq!(context.state(), ContextState::Running);

        context.prepare_shutdown();
        assert!(!context.is_shutting_down()); // Still running, just preparing

        context.finish_shutdown();
        assert!(context.is_shutting_down());
    }

    #[test]
    fn test_event_processing() {
        let mut context = MetaContext::new();
        assert!(!context.is_processing_events());

        context.set_processing_events(true);
        assert!(context.is_processing_events());

        context.set_processing_events(false);
        assert!(!context.is_processing_events());
    }
}
