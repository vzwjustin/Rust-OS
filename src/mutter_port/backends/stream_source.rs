//! Stream Source — ported from GNOME Mutter
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-stream-source.h

use crate::mutter_port::backends::common_types::*;

use alloc::string::String;

/// MetaSpaDictEntry — A key/value dictionary entry for SPA (Simple Plugin API) properties.
#[derive(Debug, Clone)]
pub struct MetaSpaDictEntry {
    /// The property key name (e.g., "key.name").
    pub key: Option<alloc::string::String>,
    /// The property value as a string.
    pub value: Option<alloc::string::String>,
}

impl MetaSpaDictEntry {
    /// Close the stream source. Clears key and value.
    pub fn _close(&self) {
        // Stream source close: in upstream this disconnects the PipeWire
        // stream. Without PipeWire I/O, this is a no-op placeholder.
    }

    /// Check if the stream source is enabled.
    pub fn _is_enabled(&self) -> bool {
        // A dict entry is "enabled" if it has both key and value.
        self.key.is_some() && self.value.is_some()
    }
}

/// MetaStreamFormat
#[derive(Debug, Clone)]
pub struct MetaStreamFormat {
    pub format: CoglPixelFormat,
}

impl MetaStreamFormat {
    /// Close the stream format source.
    pub fn _close(&self) {
        // Without PipeWire I/O, close is a no-op.
    }

    /// Check if the stream format is enabled (always true if format exists).
    pub fn _is_enabled(&self) -> bool {
        true
    }
}
