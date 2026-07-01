//! Renderer Native Private — ported from GNOME Mutter
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-renderer-native-private.h

use alloc::string::String;

/// How a secondary GPU's shared framebuffer copy is produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaSharedFramebufferCopyMode {
    /// No copy is made; the secondary GPU imports the primary GPU's buffer as a KMS framebuffer directly.
    Zero,
    /// The buffer is copied, either via a CPU readback or a GPU blit, into a buffer owned by the primary GPU.
    Primary,
}

// TODO: Extract struct definitions from C header
// TODO: Add type definitions and implementations