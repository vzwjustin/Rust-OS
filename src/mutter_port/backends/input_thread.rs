//! Input Thread
//!
//! Native Rust implementation (no direct Mutter C counterpart).
//! Manages input event processing and dispatching in a dedicated thread context.

use alloc::string::String;

/// Input Thread — manages input event processing and device polling.
/// Placeholder for native Rust implementation.
#[derive(Debug, Clone)]
pub struct InputThread {
    pub enabled: bool,
}

impl InputThread {
    /// Create a new input thread.
    pub fn new() -> Self {
        InputThread { enabled: false }
    }
}

impl Default for InputThread {
    fn default() -> Self {
        Self::new()
    }
}
