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
    /// TODO: port logic from meta_stream_source_close
    pub fn _close(&self) {
        todo!()
    }

    /// TODO: port logic from meta_stream_source_is_enabled
    pub fn _is_enabled(&self) {
        todo!()
    }

}

/// MetaStreamFormat
#[derive(Debug, Clone)]
pub struct MetaStreamFormat {
    pub format: CoglPixelFormat,
}

impl MetaStreamFormat {
    /// TODO: port logic from meta_stream_source_close
    pub fn _close(&self) {
        todo!()
    }

    /// TODO: port logic from meta_stream_source_is_enabled
    pub fn _is_enabled(&self) {
        todo!()
    }

}
