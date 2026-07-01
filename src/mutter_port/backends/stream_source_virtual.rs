//! Stream Source Virtual — ported from GNOME Mutter
//!
//! MetaStreamSourceVirtual provides the actual pixel data capture for a virtual
//! monitor stream, handling rendering and frame recording for software-defined
//! displays.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-source-virtual.h

pub struct MetaStreamSource {
    // TODO: port from meta-stream-source.h
}

pub struct MetaStreamVirtual {
    // TODO: port from meta-stream-virtual.h
}

pub struct ClutterStageView {
    // Opaque Clutter type
}

pub struct MetaLogicalMonitor {
    // TODO: port from logical monitor defs
}

/// MetaStreamSourceVirtual: Pixel source for virtual monitor captures.
pub struct MetaStreamSourceVirtual {
    // TODO: base: MetaStreamSource,
    // TODO: stream_virtual: *mut MetaStreamVirtual,
    // TODO: view: *mut ClutterStageView,
}

impl MetaStreamSourceVirtual {
    pub fn new() -> Self {
        MetaStreamSourceVirtual {}
    }
}

impl Default for MetaStreamSourceVirtual {
    fn default() -> Self {
        Self::new()
    }
}
