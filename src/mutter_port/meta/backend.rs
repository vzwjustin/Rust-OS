//! Mutter backend subsystem
//! Ported from meta/meta-backend.h and meta/meta-context.h
//!
//! MetaBackend abstracts display server protocol (X11, Wayland).
//! MetaContext is the top-level Mutter session container.

use crate::mutter_port::meta::compositor::MetaCompositor;
use crate::mutter_port::meta::display::MetaDisplay;
use alloc::boxed::Box;

/// Backend abstraction (X11, Wayland, etc)
pub struct MetaBackend {
    display: Option<Box<MetaDisplay>>,
    context: Option<Box<MetaContext>>,
    compositor: Option<Box<MetaCompositor>>,
    is_running: bool,
    is_setup: bool,
}

impl MetaBackend {
    /// Create a new MetaBackend
    pub fn new() -> Self {
        Self {
            display: None,
            context: None,
            compositor: None,
            is_running: false,
            is_setup: false,
        }
    }

    /// Get the display for this backend
    pub fn get_display(&self) -> Option<&MetaDisplay> {
        self.display.as_ref().map(|b| &**b)
    }

    /// Get the context this backend belongs to
    pub fn get_context(&self) -> Option<&MetaContext> {
        self.context.as_ref().map(|b| &**b)
    }

    /// Get the compositor
    pub fn get_compositor(&self) -> Option<&MetaCompositor> {
        self.compositor.as_ref().map(|b| &**b)
    }

    /// Check if backend is running
    pub fn is_running(&self) -> bool {
        self.is_running
    }

    /// Mark the backend as set up (ready to start).
    pub fn set_setup(&mut self) {
        self.is_setup = true;
    }

    /// Start the backend. Requires setup to have been called.
    /// Initializes the display and compositor if not already present.
    pub fn start(&mut self) {
        if !self.is_setup {
            return;
        }
        if self.display.is_none() {
            self.display = Some(Box::new(MetaDisplay::new()));
        }
        if self.compositor.is_none() {
            self.compositor = Some(Box::new(MetaCompositor::new()));
        }
        self.is_running = true;
    }

    /// Stop the backend. Marks as not running and disables the compositor.
    pub fn stop(&mut self) {
        self.is_running = false;
        if let Some(ref mut comp) = self.compositor {
            comp.set_enabled(false);
        }
    }
}

impl Default for MetaBackend {
    fn default() -> Self {
        Self::new()
    }
}

/// Main Mutter context
pub struct MetaContext {
    backend: Option<Box<MetaBackend>>,
    display: Option<Box<MetaDisplay>>,
    is_running: bool,
    is_setup: bool,
}

impl MetaContext {
    /// Create a new Mutter context
    pub fn new() -> Self {
        Self {
            backend: None,
            display: None,
            is_running: false,
            is_setup: false,
        }
    }

    /// Initialize the context. Creates the backend and display if not
    /// already present, and marks the context as set up.
    pub fn setup(&mut self) {
        if self.backend.is_none() {
            self.backend = Some(Box::new(MetaBackend::new()));
        }
        if self.display.is_none() {
            self.display = Some(Box::new(MetaDisplay::new()));
        }
        if let Some(ref mut backend) = self.backend {
            backend.set_setup();
        }
        self.is_setup = true;
    }

    /// Start the context main loop. Requires setup to have been called.
    /// Starts the backend and marks the context as running.
    pub fn run(&mut self) {
        if !self.is_setup {
            return;
        }
        if let Some(ref mut backend) = self.backend {
            backend.start();
        }
        self.is_running = true;
    }

    /// Stop the context main loop. Stops the backend and marks as not running.
    pub fn stop(&mut self) {
        self.is_running = false;
        if let Some(ref mut backend) = self.backend {
            backend.stop();
        }
    }

    /// Get the backend
    pub fn get_backend(&self) -> Option<&MetaBackend> {
        self.backend.as_ref().map(|b| &**b)
    }

    /// Get the display
    pub fn get_display(&self) -> Option<&MetaDisplay> {
        self.display.as_ref().map(|b| &**b)
    }

    /// Check if running
    pub fn is_running(&self) -> bool {
        self.is_running
    }
}

impl Default for MetaContext {
    fn default() -> Self {
        Self::new()
    }
}
