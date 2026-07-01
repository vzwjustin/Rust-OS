//! Private type definitions for KMS subsystem.
//!
//! Internal enums and data structures used within the KMS layer,
//! including device capabilities, resource change tracking,
//! and implementation-specific markers.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-kms-types-private.h
//! Note: Upstream header not found; minimal stub.

/// Private KMS implementation marker
pub struct MetaKmsPrivateType;

/// Resource change notification types
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u32)]
pub enum MetaKmsResourceChangeType {
    /// No changes
    None = 0,
    /// Device added/removed
    Device = 1,
    /// Connector state changed
    Connector = 2,
    /// CRTC/mode changed
    Crtc = 4,
    /// Plane changed
    Plane = 8,
}