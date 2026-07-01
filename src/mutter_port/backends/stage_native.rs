//! Stage Native — ported from GNOME Mutter
//!
//! Native stage implementation that extends MetaStageImpl.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/native/meta-stage-native.h

/// Native stage implementation.
pub struct MetaStageNative;

impl MetaStageNative {
    pub fn new() -> Self {
        MetaStageNative
    }
}

impl Default for MetaStageNative {
    fn default() -> Self {
        Self::new()
    }
}
