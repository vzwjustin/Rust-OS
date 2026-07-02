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

/// Resource change notification types (bitmask). A type alias + consts (rather
/// than an `enum`) so the values can be combined with bitwise OR.
pub type MetaKmsResourceChangeType = u32;

/// No changes
pub const META_KMS_RESOURCE_CHANGE_TYPE_NONE: MetaKmsResourceChangeType = 0;
/// Device added/removed
pub const META_KMS_RESOURCE_CHANGE_TYPE_DEVICE: MetaKmsResourceChangeType = 1;
/// Connector state changed
pub const META_KMS_RESOURCE_CHANGE_TYPE_CONNECTOR: MetaKmsResourceChangeType = 2;
/// CRTC/mode changed
pub const META_KMS_RESOURCE_CHANGE_TYPE_CRTC: MetaKmsResourceChangeType = 4;
/// Plane changed
pub const META_KMS_RESOURCE_CHANGE_TYPE_PLANE: MetaKmsResourceChangeType = 8;
