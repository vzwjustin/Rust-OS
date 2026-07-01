//! MetaDrmFormat ported from GNOME Mutter's src/core/meta-drm-format.c
//!
//! DRM format helpers: format definitions, modifier handling, and utility
//! functions for determining format compatibility and capabilities.
//!
//! In Mutter this is a utility module that wraps drm_fourcc.h constants
//! and provides helpers for format negotiation between the compositor,
//! Cogl, and the KMS backend.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-drm-format.c

/// A DRM format identifier (fourcc code). Mirrors `uint32_t` in drm_fourcc.h.
///
/// Format codes are stored as little-endian four-character codes, e.g.
/// `XRGB8888` = `b'X' | (b'R' << 8) | (b'G' << 16) | (b'B' << 24)`.
pub type DrmFormat = u32;

/// Common DRM format codes, matching drm_fourcc.h.
pub mod formats {
    use super::DrmFormat;

    /// Build a fourcc code from 4 ASCII characters.
    pub const fn fourcc(a: u8, b: u8, c: u8, d: u8) -> DrmFormat {
        (a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)
    }

    // 32-bit formats
    pub const XRGB8888: DrmFormat = fourcc(b'X', b'R', b'2', b'4');
    pub const ARGB8888: DrmFormat = fourcc(b'A', b'R', b'2', b'4');
    pub const XBGR8888: DrmFormat = fourcc(b'X', b'B', b'2', b'4');
    pub const ABGR8888: DrmFormat = fourcc(b'A', b'B', b'2', b'4');
    pub const RGBX8888: DrmFormat = fourcc(b'R', b'X', b'2', b'4');
    pub const RGBA8888: DrmFormat = fourcc(b'R', b'A', b'2', b'4');
    pub const BGRX8888: DrmFormat = fourcc(b'B', b'X', b'2', b'4');
    pub const BGRA8888: DrmFormat = fourcc(b'B', b'A', b'2', b'4');

    // 16-bit formats
    pub const RGB565: DrmFormat = fourcc(b'R', b'G', b'1', b'6');
    pub const BGR565: DrmFormat = fourcc(b'B', b'G', b'1', b'6');

    // 10-bit formats
    pub const XRGB2101010: DrmFormat = fourcc(b'X', b'R', b'3', b'0');
    pub const ARGB2101010: DrmFormat = fourcc(b'A', b'R', b'3', b'0');
    pub const XBGR2101010: DrmFormat = fourcc(b'X', b'B', b'3', b'0');
    pub const ABGR2101010: DrmFormat = fourcc(b'A', b'B', b'3', b'0');

    // 16-bit floating point
    pub const ARGB16161616F: DrmFormat = fourcc(b'A', b'R', b'4', b'H');
    pub const ABGR16161616F: DrmFormat = fourcc(b'A', b'B', b'4', b'H');

    // C8 (indexed 8-bit)
    pub const C8: DrmFormat = fourcc(b'C', b'8', b' ', b' ');
}

/// DRM format modifier. Mirrors `uint64_t` in drm_fourcc.h.
///
/// Modifiers describe tiling, compression, or vendor-specific layout
/// of the pixel data in a buffer.
pub type DrmModifier = u64;

/// Common DRM format modifiers, matching drm_fourcc.h.
pub mod modifiers {
    use super::DrmModifier;

    /// Linear (no modifier).
    pub const LINEAR: DrmModifier = 0;

    /// Invalid modifier (not supported).
    pub const INVALID: DrmModifier = u64::MAX;

    /// Intel X-tiling.
    pub const I915_X_TILED: DrmModifier = 1 << 1;

    /// Intel Y-tiling.
    pub const I915_Y_TILED: DrmModifier = 1 << 2;

    /// Intel Yf-tiling.
    pub const I915_YF_TILED: DrmModifier = 1 << 3;
}

/// Bits per pixel for a format. Mirrors meta_drm_format_get_bpp().
pub fn get_bpp(format: DrmFormat) -> u32 {
    match format {
        formats::C8 => 8,
        formats::RGB565 | formats::BGR565 => 16,
        formats::XRGB8888
        | formats::ARGB8888
        | formats::XBGR8888
        | formats::ABGR8888
        | formats::RGBX8888
        | formats::RGBA8888
        | formats::BGRX8888
        | formats::BGRA8888 => 32,
        formats::XRGB2101010
        | formats::ARGB2101010
        | formats::XBGR2101010
        | formats::ABGR2101010 => 32,
        formats::ARGB16161616F | formats::ABGR16161616F => 64,
        _ => 0,
    }
}

/// Whether a format has an alpha channel. Mirrors
/// meta_drm_format_has_alpha().
pub fn has_alpha(format: DrmFormat) -> bool {
    matches!(
        format,
        formats::ARGB8888
            | formats::ABGR8888
            | formats::RGBA8888
            | formats::BGRA8888
            | formats::ARGB2101010
            | formats::ABGR2101010
            | formats::ARGB16161616F
            | formats::ABGR16161616F
    )
}

/// Whether a format is 10-bit (or higher) per channel.
pub fn is_high_bpp(format: DrmFormat) -> bool {
    matches!(
        format,
        formats::XRGB2101010
            | formats::ARGB2101010
            | formats::XBGR2101010
            | formats::ABGR2101010
            | formats::ARGB16161616F
            | formats::ABGR16161616F
    )
}

/// Get the human-readable name of a format.
pub fn format_name(format: DrmFormat) -> &'static str {
    match format {
        formats::XRGB8888 => "XRGB8888",
        formats::ARGB8888 => "ARGB8888",
        formats::XBGR8888 => "XBGR8888",
        formats::ABGR8888 => "ABGR8888",
        formats::RGBX8888 => "RGBX8888",
        formats::RGBA8888 => "RGBA8888",
        formats::BGRX8888 => "BGRX8888",
        formats::BGRA8888 => "BGRA8888",
        formats::RGB565 => "RGB565",
        formats::BGR565 => "BGR565",
        formats::XRGB2101010 => "XRGB2101010",
        formats::ARGB2101010 => "ARGB2101010",
        formats::XBGR2101010 => "XBGR2101010",
        formats::ABGR2101010 => "ABGR2101010",
        formats::ARGB16161616F => "ARGB16161616F",
        formats::ABGR16161616F => "ABGR16161616F",
        formats::C8 => "C8",
        _ => "unknown",
    }
}

/// Convert a format code to a 4-character string.
pub fn format_to_string(format: DrmFormat) -> alloc::string::String {
    let bytes = [
        (format & 0xFF) as u8,
        ((format >> 8) & 0xFF) as u8,
        ((format >> 16) & 0xFF) as u8,
        ((format >> 24) & 0xFF) as u8,
    ];
    alloc::string::String::from_utf8_lossy(&bytes).into_owned()
}

/// A format + modifier pair, as used in KMS plane format negotiation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatModifierPair {
    pub format: DrmFormat,
    pub modifier: DrmModifier,
}

impl FormatModifierPair {
    pub fn new(format: DrmFormat, modifier: DrmModifier) -> Self {
        FormatModifierPair { format, modifier }
    }

    pub fn is_linear(&self) -> bool {
        self.modifier == modifiers::LINEAR
    }
}

/// Find the best format from a list of supported formats, preferring
/// higher bit depth and alpha. Mirrors the format selection logic in
/// meta_renderer_native_check_format().
pub fn pick_best_format(available: &[DrmFormat]) -> Option<DrmFormat> {
    // Preference order: 16F > 10-bit > 8-bit with alpha > 8-bit without alpha.
    let preferences = [
        formats::ARGB16161616F,
        formats::ABGR16161616F,
        formats::ARGB2101010,
        formats::ABGR2101010,
        formats::XRGB2101010,
        formats::XBGR2101010,
        formats::ARGB8888,
        formats::ABGR8888,
        formats::RGBA8888,
        formats::BGRA8888,
        formats::XRGB8888,
        formats::XBGR8888,
        formats::RGBX8888,
        formats::BGRX8888,
    ];

    for &pref in &preferences {
        if available.contains(&pref) {
            return Some(pref);
        }
    }
    available.first().copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fourcc() {
        // XRGB8888 = 'X' 'R' '2' '4' in little-endian.
        assert_eq!(
            formats::XRGB8888,
            b'X' as u32 | (b'R' as u32 << 8) | (b'2' as u32 << 16) | (b'4' as u32 << 24)
        );
    }

    #[test]
    fn test_format_to_string() {
        assert_eq!(format_to_string(formats::XRGB8888), "XRGB8888");
        assert_eq!(format_to_string(formats::ARGB8888), "ARGB8888");
        assert_eq!(format_to_string(formats::RGB565), "RGB565");
    }

    #[test]
    fn test_bpp() {
        assert_eq!(get_bpp(formats::C8), 8);
        assert_eq!(get_bpp(formats::RGB565), 16);
        assert_eq!(get_bpp(formats::XRGB8888), 32);
        assert_eq!(get_bpp(formats::ARGB16161616F), 64);
        assert_eq!(get_bpp(0x12345678), 0); // unknown
    }

    #[test]
    fn test_has_alpha() {
        assert!(has_alpha(formats::ARGB8888));
        assert!(has_alpha(formats::RGBA8888));
        assert!(has_alpha(formats::ABGR8888));
        assert!(!has_alpha(formats::XRGB8888));
        assert!(!has_alpha(formats::RGBX8888));
        assert!(!has_alpha(formats::RGB565));
    }

    #[test]
    fn test_is_high_bpp() {
        assert!(is_high_bpp(formats::XRGB2101010));
        assert!(is_high_bpp(formats::ARGB16161616F));
        assert!(!is_high_bpp(formats::XRGB8888));
        assert!(!is_high_bpp(formats::RGB565));
    }

    #[test]
    fn test_format_name() {
        assert_eq!(format_name(formats::XRGB8888), "XRGB8888");
        assert_eq!(format_name(formats::ARGB8888), "ARGB8888");
        assert_eq!(format_name(0), "unknown");
    }

    #[test]
    fn test_format_modifier_pair() {
        let pair = FormatModifierPair::new(formats::XRGB8888, modifiers::LINEAR);
        assert!(pair.is_linear());

        let pair2 = FormatModifierPair::new(formats::XRGB8888, modifiers::I915_X_TILED);
        assert!(!pair2.is_linear());
    }

    #[test]
    fn test_pick_best_format_empty() {
        assert!(pick_best_format(&[]).is_none());
    }

    #[test]
    fn test_pick_best_format_prefers_alpha() {
        let available = [formats::XRGB8888, formats::ARGB8888];
        assert_eq!(pick_best_format(&available), Some(formats::ARGB8888));
    }

    #[test]
    fn test_pick_best_format_prefers_high_bpp() {
        let available = [formats::XRGB8888, formats::ARGB8888, formats::ARGB2101010];
        assert_eq!(pick_best_format(&available), Some(formats::ARGB2101010));
    }

    #[test]
    fn test_pick_best_format_fallback() {
        let available = [formats::RGB565, formats::XRGB8888];
        assert_eq!(pick_best_format(&available), Some(formats::XRGB8888));
    }

    #[test]
    fn test_pick_best_format_only_unknown() {
        let available = [0x12345678u32];
        assert_eq!(pick_best_format(&available), Some(0x12345678u32));
    }
}
