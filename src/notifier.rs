//! Event Notifier Chains for RustOS
//!
//! Implements a mechanism for kernel subsystems to register callbacks for
//! specific kernel-wide events (e.g., panic, reboot, CPU hotplug).

use alloc::vec::Vec;
use spin::RwLock;

/// Notifier callback function type.
///
/// * `event` - The event ID.
///   `val` - Context-specific pointer/argument.
/// Returns a status code (e.g., 0 for success, negative for error, or custom codes).
pub type NotifierFn = fn(event: u32, val: *mut core::ffi::c_void) -> i32;

/// A block representing a registered notifier callback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotifierBlock {
    /// Callback function.
    pub call: NotifierFn,
    /// Priority (higher values are executed first).
    pub priority: i32,
}

impl NotifierBlock {
    /// Create a new notifier block.
    pub const fn new(call: NotifierFn, priority: i32) -> Self {
        Self { call, priority }
    }
}

/// A chain of notifier blocks.
#[derive(Debug)]
pub struct NotifierChain {
    blocks: RwLock<Vec<NotifierBlock>>,
}

impl NotifierChain {
    /// Create a new empty notifier chain.
    pub const fn new() -> Self {
        Self {
            blocks: RwLock::new(Vec::new()),
        }
    }

    /// Register a notifier block in the chain.
    ///
    /// Blocks are inserted in descending order of priority.
    pub fn register(&self, block: NotifierBlock) {
        let mut blocks = self.blocks.write();

        // Find insertion index based on priority
        let index = match blocks.binary_search_by(|probe| block.priority.cmp(&probe.priority)) {
            Ok(idx) => idx, // If equal priority, insert there
            Err(idx) => idx,
        };

        blocks.insert(index, block);
    }

    /// Unregister a notifier block by its callback function.
    pub fn unregister(&self, call: NotifierFn) -> Result<(), &'static str> {
        let mut blocks = self.blocks.write();
        if let Some(pos) = blocks.iter().position(|b| b.call == call) {
            blocks.remove(pos);
            Ok(())
        } else {
            Err("Notifier block not found")
        }
    }

    /// Notify all registered callbacks in the chain.
    ///
    /// Calls each callback in descending order of priority.
    ///
    /// * `event` - Event identifier.
    /// * `val` - Context-specific argument.
    /// Returns the status of the last callback, or 0 if empty.
    pub fn notify(&self, event: u32, val: *mut core::ffi::c_void) -> i32 {
        let blocks = self.blocks.read();
        let mut status = 0;
        for block in blocks.iter() {
            status = (block.call)(event, val);
        }
        status
    }
}

// Predefined global notifier chains for common kernel events.
lazy_static::lazy_static! {
    /// Chain triggered when the kernel panics.
    pub static ref PANIC_CHAIN: NotifierChain = NotifierChain::new();
    /// Chain triggered when the system is rebooting or shutting down.
    pub static ref REBOOT_CHAIN: NotifierChain = NotifierChain::new();
    /// Chain triggered when a CPU's online state changes.
    pub static ref CPU_CHAIN: NotifierChain = NotifierChain::new();
}

/// Initialize global notifier chains during kernel boot.
pub fn init() {
    // PM suspend/hibernate chains are initialized by `power::init()`.
    lazy_static::initialize(&PANIC_CHAIN);
    lazy_static::initialize(&REBOOT_CHAIN);
    lazy_static::initialize(&CPU_CHAIN);
}
