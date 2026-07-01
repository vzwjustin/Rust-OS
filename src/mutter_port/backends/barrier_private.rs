//! Barrier Private ported from GNOME Mutter's src/backends/
//!
//! Internal interface for barrier implementations with lifecycle and event dispatch.
//! Defines the abstract base class for platform-specific barrier backends.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-barrier-private.h

use alloc::vec::Vec;

/// Opaque barrier event type.
pub struct MetaBarrierEvent {
    // TODO: port event fields
}

/// Base class for barrier implementations.
/// Subclasses implement platform-specific barrier behavior.
pub struct MetaBarrierImplClass {
    /// Check if this barrier is currently active/enabled.
    pub is_active: Option<fn(&MetaBarrierImpl) -> bool>,
    /// Release pointer through barrier.
    pub release: Option<fn(&mut MetaBarrierImpl, &MetaBarrierEvent)>,
    /// Destroy barrier resources.
    pub destroy: Option<fn(&mut MetaBarrierImpl)>,
}

/// Opaque barrier implementation.
pub struct MetaBarrierImpl {
    class: Option<&'static MetaBarrierImplClass>,
}

/// Opaque barrier type.
pub struct MetaBarrier;

/// Opaque backend type.
pub struct MetaBackend;

/// Opaque border/geometry type.
pub struct MetaBorder;

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaBarrierFlags {
    META_BARRIER_FLAG_NONE = 0,
}

impl MetaBarrierImplClass {
    /// Create a new barrier implementation class.
    pub fn new() -> Self {
        MetaBarrierImplClass {
            is_active: None,
            release: None,
            destroy: None,
        }
    }
}

impl Default for MetaBarrierImplClass {
    fn default() -> Self {
        Self::new()
    }
}

impl MetaBarrierImpl {
    /// Create new barrier implementation.
    pub fn new(class: Option<&'static MetaBarrierImplClass>) -> Self {
        MetaBarrierImpl { class }
    }

    /// Check if barrier is active.
    pub fn is_active(&self) -> bool {
        if let Some(cls) = self.class {
            if let Some(func) = cls.is_active {
                return func(self);
            }
        }
        false
    }

    /// Release through barrier.
    pub fn release(&mut self, event: &MetaBarrierEvent) {
        if let Some(cls) = self.class {
            if let Some(func) = cls.release {
                func(self, event);
            }
        }
    }

    /// Destroy barrier.
    pub fn destroy(&mut self) {
        if let Some(cls) = self.class {
            if let Some(func) = cls.destroy {
                func(self);
            }
        }
    }
}

impl Default for MetaBarrierImpl {
    fn default() -> Self {
        Self::new(None)
    }
}

// TODO: Signal emission and barrier query helpers
// These would be wired up with actual signal dispatch and barrier storage
