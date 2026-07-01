//! Mutter backend subsystem
//! Ported from meta/meta-backend.h and meta/meta-context.h
//!
//! MetaBackend abstracts display server protocol (X11, Wayland).
//! MetaContext is the top-level Mutter session container.

use alloc::boxed::Box;
use crate::mutter_port::meta::types::*;

/// Backend abstraction (X11, Wayland, etc)
pub struct MetaBackend {
    display: Option<Box<MetaDisplay>>,
    context: Option<Box<MetaContext>>,
    compositor: Option<Box<MetaCompositor>>,
    is_running: bool,
}

impl MetaBackend {
    /// Create a new MetaBackend
    pub fn new() -> Self {
        Self {
            display: None,
            context: None,
            compositor: None,
            is_running: false,
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

    /// Start the backend
    pub fn start(&mut self) {
        self.is_running = true;
        // TODO: implement
    }

    /// Stop the backend
    pub fn stop(&mut self) {
        self.is_running = false;
        // TODO: implement
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
}

impl MetaContext {
    /// Create a new Mutter context
    pub fn new() -> Self {
        Self {
            backend: None,
            display: None,
            is_running: false,
        }
    }

    /// Initialize the context
    pub fn setup(&mut self) {
        // TODO: implement
    }

    /// Start the context main loop
    pub fn run(&mut self) {
        self.is_running = true;
        // TODO: implement
    }

    /// Stop the context main loop
    pub fn stop(&mut self) {
        self.is_running = false;
        // TODO: implement
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

