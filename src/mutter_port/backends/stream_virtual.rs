//! Stream Virtual — ported from GNOME Mutter
//!
//! MetaStreamVirtual represents a screen capture stream for a virtual (software-defined)
//! monitor. Used for capturing output from headless or remote display scenarios with configurable modes.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-virtual.h

use super::stream::MetaStream;
use alloc::vec::Vec;
use core::ffi::c_void;

/// Display mode information for a virtual monitor.
#[derive(Debug, Clone, Copy)]
pub struct DisplayModeInfo {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Refresh rate in Hz (as integer, e.g. 60000 for 60 Hz).
    pub refresh_rate: u32,
}

/// Virtual monitor display object.
pub struct MetaVirtualMonitor {
    /// List of available display modes.
    pub modes: Vec<DisplayModeInfo>,
}

impl MetaVirtualMonitor {
    pub fn new() -> Self {
        MetaVirtualMonitor {
            modes: Vec::new(),
        }
    }
}

impl Default for MetaVirtualMonitor {
    fn default() -> Self {
        Self::new()
    }
}

/// MetaStreamVirtual: Captures a virtual monitor's output.
pub struct MetaStreamVirtual {
    /// Base stream configuration and state.
    pub base: MetaStream,
    /// List of display mode information (GList equivalent).
    pub mode_infos: Vec<DisplayModeInfo>,
}

impl MetaStreamVirtual {
    pub fn new() -> Self {
        MetaStreamVirtual {
            base: MetaStream::new(),
            mode_infos: Vec::new(),
        }
    }
}

impl Default for MetaStreamVirtual {
    fn default() -> Self {
        Self::new()
    }
}
