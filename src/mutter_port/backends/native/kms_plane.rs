//! KMS plane representation for hardware display pipelines.
//!
//! Planes are the fundamental display pipeline components that handle rendering
//! to framebuffers (primary, overlay, cursor). Ported from `meta-kms-plane.c`.

use alloc::collections::BTreeMap;
use alloc::vec;
use alloc::vec::Vec;

/// Plane type in the display pipeline
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaneType {
    /// Primary scanout plane
    Primary,
    /// Overlay plane (composited on top of primary)
    Overlay,
    /// Hardware cursor plane
    Cursor,
}

/// Plane rotation/transform capabilities
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PlaneRotation {
    pub rotate_0: bool,
    pub rotate_90: bool,
    pub rotate_180: bool,
    pub rotate_270: bool,
    pub reflect_x: bool,
    pub reflect_y: bool,
}

impl Default for PlaneRotation {
    fn default() -> Self {
        PlaneRotation {
            rotate_0: true,
            rotate_90: false,
            rotate_180: false,
            rotate_270: false,
            reflect_x: false,
            reflect_y: false,
        }
    }
}

/// Cursor size hints for hardware cursor
#[derive(Debug, Clone, Copy)]
pub struct CursorSizeHint {
    pub width: u16,
    pub height: u16,
}

/// Supported pixel formats and their modifiers
#[derive(Debug, Clone)]
pub struct FormatModifier {
    pub format: u32,
    pub modifiers: Vec<u64>,
}

/// KMS plane object
#[derive(Debug)]
pub struct KmsPlane {
    /// Plane type (primary, overlay, cursor)
    pub plane_type: PlaneType,
    /// Hardware plane ID
    pub id: u32,
    /// CRTC IDs this plane can be used with
    pub possible_crtcs: u32,
    /// Rotation/transform capabilities
    pub rotations: PlaneRotation,
    /// Supported pixel formats and modifiers
    pub format_modifiers: Vec<FormatModifier>,
    /// Cursor size hints (for cursor planes)
    pub cursor_size_hints: Vec<CursorSizeHint>,
    /// Whether this is a fake plane (software implementation)
    pub is_fake: bool,
}

impl KmsPlane {
    /// Create a new KMS plane
    pub fn new(id: u32, plane_type: PlaneType) -> Self {
        KmsPlane {
            plane_type,
            id,
            possible_crtcs: 0,
            rotations: PlaneRotation::default(),
            format_modifiers: Vec::new(),
            cursor_size_hints: Vec::new(),
            is_fake: false,
        }
    }

    /// Set which CRTCs this plane can be used with
    pub fn set_possible_crtcs(&mut self, crtcs: u32) {
        self.possible_crtcs = crtcs;
    }

    /// Check if this plane can be used with a specific CRTC
    pub fn can_use_with_crtc(&self, crtc_id: u32) -> bool {
        (self.possible_crtcs & (1 << crtc_id)) != 0
    }

    /// Set rotation capabilities
    pub fn set_rotations(&mut self, rotations: PlaneRotation) {
        self.rotations = rotations;
    }

    /// Add a supported format
    pub fn add_format(&mut self, format: u32) {
        if !self.format_modifiers.iter().any(|fm| fm.format == format) {
            self.format_modifiers.push(FormatModifier {
                format,
                modifiers: Vec::new(),
            });
        }
    }

    /// Add a format modifier for a specific format
    pub fn add_format_modifier(&mut self, format: u32, modifier: u64) {
        let found = self
            .format_modifiers
            .iter_mut()
            .find(|fm| fm.format == format);
        if let Some(fm) = found {
            if !fm.modifiers.contains(&modifier) {
                fm.modifiers.push(modifier);
            }
        } else {
            self.format_modifiers.push(FormatModifier {
                format,
                modifiers: vec![modifier],
            });
        }
    }

    /// Get supported formats
    pub fn get_formats(&self) -> Vec<u32> {
        self.format_modifiers.iter().map(|fm| fm.format).collect()
    }

    /// Get modifiers for a specific format
    pub fn get_modifiers_for_format(&self, format: u32) -> Option<&Vec<u64>> {
        self.format_modifiers
            .iter()
            .find(|fm| fm.format == format)
            .map(|fm| &fm.modifiers)
    }

    /// Add a cursor size hint
    pub fn add_cursor_size_hint(&mut self, width: u16, height: u16) {
        let hint = CursorSizeHint { width, height };
        if !self
            .cursor_size_hints
            .iter()
            .any(|h| h.width == width && h.height == height)
        {
            self.cursor_size_hints.push(hint);
        }
    }

    /// Mark this as a fake (software) plane
    pub fn set_fake(&mut self, is_fake: bool) {
        self.is_fake = is_fake;
    }

    /// Check if this is a hardware plane
    pub fn is_hardware(&self) -> bool {
        !self.is_fake
    }

    /// Check if this plane can be used with another plane on the same CRTC
    pub fn is_usable_with(&self, other: &KmsPlane, crtc_id: u32) -> bool {
        self.can_use_with_crtc(crtc_id) && other.can_use_with_crtc(crtc_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plane_creation() {
        let plane = KmsPlane::new(42, PlaneType::Primary);
        assert_eq!(plane.id, 42);
        assert_eq!(plane.plane_type, PlaneType::Primary);
        assert!(!plane.is_fake);
    }

    #[test]
    fn test_possible_crtcs() {
        let mut plane = KmsPlane::new(42, PlaneType::Primary);
        plane.set_possible_crtcs(0b1111); // Can use with CRTCs 0-3
        assert!(plane.can_use_with_crtc(0));
        assert!(plane.can_use_with_crtc(1));
        assert!(!plane.can_use_with_crtc(4));
    }

    #[test]
    fn test_format_support() {
        let mut plane = KmsPlane::new(42, PlaneType::Primary);
        plane.add_format(0x34325241); // ARGB format
        let formats = plane.get_formats();
        assert!(formats.contains(&0x34325241));
    }

    #[test]
    fn test_cursor_hints() {
        let mut plane = KmsPlane::new(42, PlaneType::Cursor);
        plane.add_cursor_size_hint(64, 64);
        plane.add_cursor_size_hint(128, 128);
        assert_eq!(plane.cursor_size_hints.len(), 2);
    }
}
