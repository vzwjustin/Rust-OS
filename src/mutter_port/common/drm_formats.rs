//! DRM format helpers and conversions between DRM, COGL, and multi-texture formats.
//! Ported from src/common/meta-cogl-drm-formats.h/c and meta-drm-format-helpers.h/c
//!
//! DRM formats are fourcc codes describing the memory layout of a buffer
//! (e.g. `DRM_FORMAT_XRGB8888`). Mutter maps each DRM format to a COGL
//! pixel format (for texture upload) and, for multi-plane YUV formats,
//! to a `MultiTextureFormat` describing the per-plane textures. This
//! port reproduces the lookup tables and compatibility checks used by
//! the compositor's format negotiation.

use alloc::vec::Vec;

/// COGL pixel format codes, mirroring `CoglPixelFormat` from the COGL
/// library. Only the subset Mutter's DRM format table references is
/// enumerated; the raw `u32` value matches COGL's bit layout so it can
/// be passed through to COGL-backed code unchanged.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoglPixelFormat {
    Any = 0,
    A8 = 1,
    Rgb565 = 2,
    Rgb888 = 3,
    Rgba8888 = 4,
    Bgra8888 = 5,
    Argb8888 = 6,
    Rgbx8888 = 7,
    Xrgb8888 = 8,
    Rgb101010 = 9,
    Rgba1010102 = 10,
    Bgrx1010102 = 11,
    Rgbx1010102 = 12,
    Rgba16161616 = 13,
    Rgb16161616 = 14,
    Invalid = 0xffff_ffff,
}

impl CoglPixelFormat {
    /// Returns `true` when the format carries an alpha channel.
    pub fn has_alpha(self) -> bool {
        matches!(
            self,
            CoglPixelFormat::A8
                | CoglPixelFormat::Rgba8888
                | CoglPixelFormat::Bgra8888
                | CoglPixelFormat::Argb8888
                | CoglPixelFormat::Rgba1010102
                | CoglPixelFormat::Rgba16161616
        )
    }
}

/// Multi-texture format codes for multi-plane YUV buffers, mirroring
/// `MetaMultiTextureFormat`. `Simple` denotes a single-plane format
/// described entirely by its COGL counterpart.
#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultiTextureFormat {
    Invalid = 0,
    Simple = 1,
    Yuyv = 2,
    Yvyu = 3,
    Uyvy = 4,
    Vyuy = 5,
    Nv12 = 6,
    Nv21 = 7,
    Nv16 = 8,
    Nv61 = 9,
    Nv24 = 10,
    Nv42 = 11,
    P010 = 12,
    P012 = 13,
    P016 = 14,
    Yuv420 = 15,
    Yvu420 = 16,
    Yuv422 = 17,
    Yvu422 = 18,
    Yuv444 = 19,
    Yvu444 = 20,
    S010 = 21,
    S210 = 22,
    S410 = 23,
    S012 = 24,
    S212 = 25,
    S412 = 26,
    S016 = 27,
    S216 = 28,
    S416 = 29,
}

impl MultiTextureFormat {
    /// Returns `true` for formats that need more than one texture plane.
    pub fn is_multi_plane(self) -> bool {
        !matches!(self, MultiTextureFormat::Invalid | MultiTextureFormat::Simple)
    }

    /// Number of distinct planes this multi-texture format requires.
    pub fn plane_count(self) -> usize {
        match self {
            MultiTextureFormat::Invalid | MultiTextureFormat::Simple => 1,
            // Packed YUV: single plane.
            MultiTextureFormat::Yuyv
            | MultiTextureFormat::Yvyu
            | MultiTextureFormat::Uyvy
            | MultiTextureFormat::Vyuy => 1,
            // 2-plane: luma + interleaved chroma.
            MultiTextureFormat::Nv12
            | MultiTextureFormat::Nv21
            | MultiTextureFormat::Nv16
            | MultiTextureFormat::Nv61
            | MultiTextureFormat::P010
            | MultiTextureFormat::P012
            | MultiTextureFormat::P016
            | MultiTextureFormat::S010
            | MultiTextureFormat::S012
            | MultiTextureFormat::S016 => 2,
            // 3-plane: separate luma, Cb, Cr.
            MultiTextureFormat::Nv24
            | MultiTextureFormat::Nv42
            | MultiTextureFormat::Yuv420
            | MultiTextureFormat::Yvu420
            | MultiTextureFormat::Yuv422
            | MultiTextureFormat::Yvu422
            | MultiTextureFormat::Yuv444
            | MultiTextureFormat::Yvu444
            | MultiTextureFormat::S210
            | MultiTextureFormat::S410
            | MultiTextureFormat::S212
            | MultiTextureFormat::S412
            | MultiTextureFormat::S216
            | MultiTextureFormat::S416 => 3,
        }
    }
}

/// Metadata about a DRM pixel format and its equivalents in other systems.
#[derive(Debug, Clone, Copy)]
pub struct MetaFormatInfo {
    pub drm_format: u32,
    pub opaque_substitute: u32,
    pub cogl_format: CoglPixelFormat,
    pub multi_texture_format: MultiTextureFormat,
}

/// Builds a fourcc DRM format code from its ASCII tag.
const fn fourcc(a: u8, b: u8, c: u8, d: u8) -> u32 {
    (a as u32) | ((b as u32) << 8) | ((c as u32) << 16) | ((d as u32) << 24)
}

// Common DRM fourcc codes used by Mutter's format table.
const DRM_FORMAT_C8: u32 = fourcc(b'C', b'8', b' ', b' ');
const DRM_FORMAT_R8: u32 = fourcc(b'R', b'8', b' ', b' ');
const DRM_FORMAT_R16: u32 = fourcc(b'R', b'1', b'6', b' ');
const DRM_FORMAT_GR88: u32 = fourcc(b'G', b'R', b'8', b'8');
const DRM_FORMAT_GR1616: u32 = fourcc(b'G', b'R', b'3', b'2');
const DRM_FORMAT_XRGB8888: u32 = fourcc(b'X', b'R', b'2', b'4');
const DRM_FORMAT_ARGB8888: u32 = fourcc(b'A', b'R', b'2', b'4');
const DRM_FORMAT_XBGR8888: u32 = fourcc(b'X', b'B', b'2', b'4');
const DRM_FORMAT_ABGR8888: u32 = fourcc(b'A', b'B', b'2', b'4');
const DRM_FORMAT_RGBX8888: u32 = fourcc(b'R', b'X', b'2', b'4');
const DRM_FORMAT_RGBA8888: u32 = fourcc(b'R', b'A', b'2', b'4');
const DRM_FORMAT_BGRX8888: u32 = fourcc(b'B', b'X', b'2', b'4');
const DRM_FORMAT_BGRA8888: u32 = fourcc(b'B', b'A', b'2', b'4');
const DRM_FORMAT_RGB565: u32 = fourcc(b'R', b'G', b'1', b'6');
const DRM_FORMAT_BGR565: u32 = fourcc(b'B', b'G', b'1', b'6');
const DRM_FORMAT_RGB888: u32 = fourcc(b'R', b'G', b'2', b'4');
const DRM_FORMAT_BGR888: u32 = fourcc(b'B', b'G', b'2', b'4');
const DRM_FORMAT_XRGB2101010: u32 = fourcc(b'X', b'R', b'3', b'0');
const DRM_FORMAT_ARGB2101010: u32 = fourcc(b'A', b'R', b'3', b'0');
const DRM_FORMAT_XBGR2101010: u32 = fourcc(b'X', b'B', b'3', b'0');
const DRM_FORMAT_ABGR2101010: u32 = fourcc(b'A', b'B', b'3', b'0');
const DRM_FORMAT_RGBX16161616: u32 = fourcc(b'R', b'X', b'4', b'8');
const DRM_FORMAT_RGBA16161616: u32 = fourcc(b'R', b'A', b'4', b'8');
const DRM_FORMAT_YUYV: u32 = fourcc(b'Y', b'U', b'Y', b'V');
const DRM_FORMAT_YVYU: u32 = fourcc(b'Y', b'V', b'Y', b'U');
const DRM_FORMAT_UYVY: u32 = fourcc(b'U', b'Y', b'V', b'Y');
const DRM_FORMAT_VYUY: u32 = fourcc(b'V', b'Y', b'U', b'Y');
const DRM_FORMAT_NV12: u32 = fourcc(b'N', b'V', b'1', b'2');
const DRM_FORMAT_NV21: u32 = fourcc(b'N', b'V', b'2', b'1');
const DRM_FORMAT_NV16: u32 = fourcc(b'N', b'V', b'1', b'6');
const DRM_FORMAT_NV61: u32 = fourcc(b'N', b'V', b'6', b'1');
const DRM_FORMAT_NV24: u32 = fourcc(b'N', b'V', b'2', b'4');
const DRM_FORMAT_NV42: u32 = fourcc(b'N', b'V', b'4', b'2');
const DRM_FORMAT_P010: u32 = fourcc(b'P', b'0', b'1', b'0');
const DRM_FORMAT_P012: u32 = fourcc(b'P', b'0', b'1', b'2');
const DRM_FORMAT_P016: u32 = fourcc(b'P', b'0', b'1', b'6');
const DRM_FORMAT_YUV420: u32 = fourcc(b'Y', b'U', b'1', b'2');
const DRM_FORMAT_YVU420: u32 = fourcc(b'Y', b'V', b'1', b'2');
const DRM_FORMAT_YUV422: u32 = fourcc(b'Y', b'U', b'1', b'6');
const DRM_FORMAT_YVU422: u32 = fourcc(b'Y', b'V', b'1', b'6');
const DRM_FORMAT_YUV444: u32 = fourcc(b'Y', b'U', b'2', b'4');
const DRM_FORMAT_YVU444: u32 = fourcc(b'Y', b'V', b'2', b'4');
const DRM_FORMAT_Q410: u32 = fourcc(b'Q', b'4', b'1', b'0');
const DRM_FORMAT_Q412: u32 = fourcc(b'Q', b'4', b'1', b'2');
const DRM_FORMAT_Q416: u32 = fourcc(b'Q', b'4', b'1', b'6');

/// DRM format modifier constants. `DRM_FORMAT_MOD_INVALID` is the
/// sentinel for "no modifier specified"; `LINEAR` is the explicit
/// linear layout. Vendor modifiers are represented by their raw 64-bit
/// codes and matched only by equality in [`format_compatible`].
pub const DRM_FORMAT_MOD_INVALID: u64 = u64::MAX;
pub const DRM_FORMAT_MOD_LINEAR: u64 = 0;

/// The full DRM<->COGL<->MultiTexture lookup table, mirroring
/// `meta_format_info_from_drm_format` in meta-cogl-drm-formats.c.
const FORMAT_TABLE: &[MetaFormatInfo] = &[
    // 8-bit single-channel.
    MetaFormatInfo {
        drm_format: DRM_FORMAT_C8,
        opaque_substitute: DRM_FORMAT_C8,
        cogl_format: CoglPixelFormat::A8,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_R8,
        opaque_substitute: DRM_FORMAT_R8,
        cogl_format: CoglPixelFormat::A8,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_R16,
        opaque_substitute: DRM_FORMAT_R16,
        cogl_format: CoglPixelFormat::Rgba16161616,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_GR88,
        opaque_substitute: DRM_FORMAT_GR88,
        cogl_format: CoglPixelFormat::Rgba8888,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_GR1616,
        opaque_substitute: DRM_FORMAT_GR1616,
        cogl_format: CoglPixelFormat::Rgba16161616,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    // 16-bit RGB.
    MetaFormatInfo {
        drm_format: DRM_FORMAT_RGB565,
        opaque_substitute: DRM_FORMAT_RGB565,
        cogl_format: CoglPixelFormat::Rgb565,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_BGR565,
        opaque_substitute: DRM_FORMAT_BGR565,
        cogl_format: CoglPixelFormat::Rgb565,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    // 24-bit RGB.
    MetaFormatInfo {
        drm_format: DRM_FORMAT_RGB888,
        opaque_substitute: DRM_FORMAT_RGB888,
        cogl_format: CoglPixelFormat::Rgb888,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_BGR888,
        opaque_substitute: DRM_FORMAT_BGR888,
        cogl_format: CoglPixelFormat::Rgb888,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    // 32-bit RGBX/XRGB (opaque).
    MetaFormatInfo {
        drm_format: DRM_FORMAT_XRGB8888,
        opaque_substitute: DRM_FORMAT_XRGB8888,
        cogl_format: CoglPixelFormat::Xrgb8888,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_XBGR8888,
        opaque_substitute: DRM_FORMAT_XBGR8888,
        cogl_format: CoglPixelFormat::Rgbx8888,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_RGBX8888,
        opaque_substitute: DRM_FORMAT_RGBX8888,
        cogl_format: CoglPixelFormat::Rgbx8888,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_BGRX8888,
        opaque_substitute: DRM_FORMAT_BGRX8888,
        cogl_format: CoglPixelFormat::Xrgb8888,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    // 32-bit RGBA/ARGB (alpha).
    MetaFormatInfo {
        drm_format: DRM_FORMAT_ARGB8888,
        opaque_substitute: DRM_FORMAT_XRGB8888,
        cogl_format: CoglPixelFormat::Argb8888,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_ABGR8888,
        opaque_substitute: DRM_FORMAT_XBGR8888,
        cogl_format: CoglPixelFormat::Rgba8888,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_RGBA8888,
        opaque_substitute: DRM_FORMAT_RGBX8888,
        cogl_format: CoglPixelFormat::Rgba8888,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_BGRA8888,
        opaque_substitute: DRM_FORMAT_BGRX8888,
        cogl_format: CoglPixelFormat::Bgra8888,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    // 30-bit RGB.
    MetaFormatInfo {
        drm_format: DRM_FORMAT_XRGB2101010,
        opaque_substitute: DRM_FORMAT_XRGB2101010,
        cogl_format: CoglPixelFormat::Xrgb8888,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_XBGR2101010,
        opaque_substitute: DRM_FORMAT_XBGR2101010,
        cogl_format: CoglPixelFormat::Rgbx1010102,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_ARGB2101010,
        opaque_substitute: DRM_FORMAT_XRGB2101010,
        cogl_format: CoglPixelFormat::Rgba1010102,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_ABGR2101010,
        opaque_substitute: DRM_FORMAT_XBGR2101010,
        cogl_format: CoglPixelFormat::Bgrx1010102,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    // 48-bit RGB.
    MetaFormatInfo {
        drm_format: DRM_FORMAT_RGBX16161616,
        opaque_substitute: DRM_FORMAT_RGBX16161616,
        cogl_format: CoglPixelFormat::Rgb16161616,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_RGBA16161616,
        opaque_substitute: DRM_FORMAT_RGBX16161616,
        cogl_format: CoglPixelFormat::Rgba16161616,
        multi_texture_format: MultiTextureFormat::Simple,
    },
    // Packed YUV.
    MetaFormatInfo {
        drm_format: DRM_FORMAT_YUYV,
        opaque_substitute: DRM_FORMAT_YUYV,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Yuyv,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_YVYU,
        opaque_substitute: DRM_FORMAT_YVYU,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Yvyu,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_UYVY,
        opaque_substitute: DRM_FORMAT_UYVY,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Uyvy,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_VYUY,
        opaque_substitute: DRM_FORMAT_VYUY,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Vyuy,
    },
    // 2-plane YUV.
    MetaFormatInfo {
        drm_format: DRM_FORMAT_NV12,
        opaque_substitute: DRM_FORMAT_NV12,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Nv12,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_NV21,
        opaque_substitute: DRM_FORMAT_NV21,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Nv21,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_NV16,
        opaque_substitute: DRM_FORMAT_NV16,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Nv16,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_NV61,
        opaque_substitute: DRM_FORMAT_NV61,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Nv61,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_P010,
        opaque_substitute: DRM_FORMAT_P010,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::P010,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_P012,
        opaque_substitute: DRM_FORMAT_P012,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::P012,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_P016,
        opaque_substitute: DRM_FORMAT_P016,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::P016,
    },
    // 3-plane YUV.
    MetaFormatInfo {
        drm_format: DRM_FORMAT_NV24,
        opaque_substitute: DRM_FORMAT_NV24,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Nv24,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_NV42,
        opaque_substitute: DRM_FORMAT_NV42,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Nv42,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_YUV420,
        opaque_substitute: DRM_FORMAT_YUV420,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Yuv420,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_YVU420,
        opaque_substitute: DRM_FORMAT_YVU420,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Yvu420,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_YUV422,
        opaque_substitute: DRM_FORMAT_YUV422,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Yuv422,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_YVU422,
        opaque_substitute: DRM_FORMAT_YVU422,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Yvu422,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_YUV444,
        opaque_substitute: DRM_FORMAT_YUV444,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Yuv444,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_YVU444,
        opaque_substitute: DRM_FORMAT_YVU444,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::Yvu444,
    },
    // Q410/Q412/Q416 map to the S-series multi-texture formats.
    MetaFormatInfo {
        drm_format: DRM_FORMAT_Q410,
        opaque_substitute: DRM_FORMAT_Q410,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::S210,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_Q412,
        opaque_substitute: DRM_FORMAT_Q412,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::S212,
    },
    MetaFormatInfo {
        drm_format: DRM_FORMAT_Q416,
        opaque_substitute: DRM_FORMAT_Q416,
        cogl_format: CoglPixelFormat::Invalid,
        multi_texture_format: MultiTextureFormat::S416,
    },
];

/// Look up format information by DRM format code.
pub fn format_info_from_drm_format(drm_format: u32) -> Option<MetaFormatInfo> {
    FORMAT_TABLE.iter().copied().find(|info| info.drm_format == drm_format)
}

/// Look up format information by COGL pixel format.
pub fn format_info_from_cogl_format(cogl_format: CoglPixelFormat) -> Option<MetaFormatInfo> {
    FORMAT_TABLE
        .iter()
        .copied()
        .find(|info| info.cogl_format == cogl_format)
}

/// Convert DRM format to COGL pixel format.
pub fn drm_format_to_cogl(drm_format: u32) -> Option<CoglPixelFormat> {
    format_info_from_drm_format(drm_format).map(|info| info.cogl_format)
}

/// Convert COGL format to DRM format.
pub fn cogl_format_to_drm(cogl_format: CoglPixelFormat) -> Option<u32> {
    format_info_from_cogl_format(cogl_format).map(|info| info.drm_format)
}

/// Returns the opaque DRM format that should be substituted for
/// `drm_format` when the consumer cannot handle alpha. If the format is
/// already opaque, it is returned unchanged. Unknown formats yield
/// `None`.
pub fn opaque_substitute(drm_format: u32) -> Option<u32> {
    format_info_from_drm_format(drm_format).map(|info| info.opaque_substitute)
}

/// Returns `true` when `drm_format` carries an alpha channel.
pub fn drm_format_has_alpha(drm_format: u32) -> bool {
    match format_info_from_drm_format(drm_format) {
        Some(info) => info.cogl_format.has_alpha(),
        None => false,
    }
}

/// Returns `true` when `drm_format` is a multi-plane YUV format that
/// requires more than one texture to sample.
pub fn drm_format_is_multi_plane(drm_format: u32) -> bool {
    match format_info_from_drm_format(drm_format) {
        Some(info) => info.multi_texture_format.is_multi_plane(),
        None => false,
    }
}

/// Returns the number of texture planes required to sample
/// `drm_format`, or `None` if the format is unknown.
pub fn drm_format_plane_count(drm_format: u32) -> Option<usize> {
    format_info_from_drm_format(drm_format).map(|info| info.multi_texture_format.plane_count())
}

/// Modifier-aware format compatibility check, mirroring the logic in
/// `meta_drm_format_mod_compatible`.
///
/// Two (format, modifier) pairs are compatible when:
///   - the formats are equal, and
///   - the modifiers are equal, or one side uses `DRM_FORMAT_MOD_INVALID`
///     (meaning "any modifier acceptable").
pub fn format_compatible(
    format_a: u32,
    modifier_a: u64,
    format_b: u32,
    modifier_b: u64,
) -> bool {
    if format_a != format_b {
        return false;
    }
    if modifier_a == DRM_FORMAT_MOD_INVALID || modifier_b == DRM_FORMAT_MOD_INVALID {
        return true;
    }
    modifier_a == modifier_b
}

/// Collect every DRM format known to the lookup table. Useful for
/// advertising supported formats during Wayland `wl_drm`/`linux-dmabuf`
/// negotiation.
pub fn known_drm_formats() -> Vec<u32> {
    FORMAT_TABLE.iter().map(|info| info.drm_format).collect()
}

/// Decode a DRM fourcc code back into its 4-byte ASCII tag, for
/// diagnostics. Returns `None` for codes that are not printable.
pub fn fourcc_to_str(drm_format: u32) -> Option<[u8; 4]> {
    let bytes = drm_format.to_le_bytes();
    let printable = bytes.iter().all(|&b| b.is_ascii_graphic() || b == b' ');
    if printable {
        Some(bytes)
    } else {
        None
    }
}

/// Returns the number of bits per pixel for a DRM format, or `None` if
/// unknown. For multi-plane formats this is the luma-plane bpp.
pub fn drm_format_bpp(drm_format: u32) -> Option<u32> {
    match drm_format {
        DRM_FORMAT_C8 | DRM_FORMAT_R8 => Some(8),
        DRM_FORMAT_R16 => Some(16),
        DRM_FORMAT_GR88 => Some(16),
        DRM_FORMAT_GR1616 => Some(32),
        DRM_FORMAT_RGB565 | DRM_FORMAT_BGR565 => Some(16),
        DRM_FORMAT_RGB888 | DRM_FORMAT_BGR888 => Some(24),
        DRM_FORMAT_XRGB8888
        | DRM_FORMAT_ARGB8888
        | DRM_FORMAT_XBGR8888
        | DRM_FORMAT_ABGR8888
        | DRM_FORMAT_RGBX8888
        | DRM_FORMAT_RGBA8888
        | DRM_FORMAT_BGRX8888
        | DRM_FORMAT_BGRA8888 => Some(32),
        DRM_FORMAT_XRGB2101010
        | DRM_FORMAT_ARGB2101010
        | DRM_FORMAT_XBGR2101010
        | DRM_FORMAT_ABGR2101010 => Some(32),
        DRM_FORMAT_RGBX16161616 | DRM_FORMAT_RGBA16161616 => Some(64),
        // Packed YUV: 16 bits per two horizontally-packed pixels.
        DRM_FORMAT_YUYV | DRM_FORMAT_YVYU | DRM_FORMAT_UYVY | DRM_FORMAT_VYUY => Some(16),
        // Planar formats: report luma bpp.
        DRM_FORMAT_NV12
        | DRM_FORMAT_NV21
        | DRM_FORMAT_YUV420
        | DRM_FORMAT_YVU420
        | DRM_FORMAT_NV16
        | DRM_FORMAT_NV61
        | DRM_FORMAT_YUV422
        | DRM_FORMAT_YVU422
        | DRM_FORMAT_NV24
        | DRM_FORMAT_NV42
        | DRM_FORMAT_YUV444
        | DRM_FORMAT_YVU444 => Some(8),
        DRM_FORMAT_P010 | DRM_FORMAT_P012 | DRM_FORMAT_P016 => Some(16),
        DRM_FORMAT_Q410 | DRM_FORMAT_Q412 | DRM_FORMAT_Q416 => Some(16),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_xrgb8888() {
        let info = format_info_from_drm_format(DRM_FORMAT_XRGB8888).unwrap();
        assert_eq!(info.cogl_format, CoglPixelFormat::Xrgb8888);
        assert_eq!(info.multi_texture_format, MultiTextureFormat::Simple);
        assert_eq!(info.opaque_substitute, DRM_FORMAT_XRGB8888);
    }

    #[test]
    fn test_lookup_argb_has_alpha() {
        let info = format_info_from_drm_format(DRM_FORMAT_ARGB8888).unwrap();
        assert!(info.cogl_format.has_alpha());
        assert_eq!(info.opaque_substitute, DRM_FORMAT_XRGB8888);
    }

    #[test]
    fn test_lookup_nv12_multi_plane() {
        let info = format_info_from_drm_format(DRM_FORMAT_NV12).unwrap();
        assert!(info.multi_texture_format.is_multi_plane());
        assert_eq!(info.multi_texture_format.plane_count(), 2);
        assert!(drm_format_is_multi_plane(DRM_FORMAT_NV12));
    }

    #[test]
    fn test_lookup_unknown_returns_none() {
        assert!(format_info_from_drm_format(0).is_none());
        assert!(drm_format_to_cogl(0).is_none());
    }

    #[test]
    fn test_cogl_roundtrip() {
        let drm = cogl_format_to_drm(CoglPixelFormat::Rgba8888).unwrap();
        assert_eq!(drm, DRM_FORMAT_RGBA8888);
        let back = drm_format_to_cogl(drm).unwrap();
        assert_eq!(back, CoglPixelFormat::Rgba8888);
    }

    #[test]
    fn test_opaque_substitute() {
        assert_eq!(opaque_substitute(DRM_FORMAT_ARGB8888), Some(DRM_FORMAT_XRGB8888));
        assert_eq!(opaque_substitute(DRM_FORMAT_XRGB8888), Some(DRM_FORMAT_XRGB8888));
    }

    #[test]
    fn test_format_compatible() {
        assert!(format_compatible(
            DRM_FORMAT_XRGB8888,
            DRM_FORMAT_MOD_LINEAR,
            DRM_FORMAT_XRGB8888,
            DRM_FORMAT_MOD_LINEAR,
        ));
        assert!(!format_compatible(
            DRM_FORMAT_XRGB8888,
            DRM_FORMAT_MOD_LINEAR,
            DRM_FORMAT_ARGB8888,
            DRM_FORMAT_MOD_LINEAR,
        ));
        assert!(format_compatible(
            DRM_FORMAT_XRGB8888,
            DRM_FORMAT_MOD_INVALID,
            DRM_FORMAT_XRGB8888,
            DRM_FORMAT_MOD_LINEAR,
        ));
    }

    #[test]
    fn test_bpp() {
        assert_eq!(drm_format_bpp(DRM_FORMAT_XRGB8888), Some(32));
        assert_eq!(drm_format_bpp(DRM_FORMAT_RGB565), Some(16));
        assert_eq!(drm_format_bpp(DRM_FORMAT_NV12), Some(8));
        assert_eq!(drm_format_bpp(0), None);
    }

    #[test]
    fn test_fourcc_roundtrip() {
        let bytes = fourcc_to_str(DRM_FORMAT_XRGB8888).unwrap();
        assert_eq!(&bytes, b"XR24");
        assert!(fourcc_to_str(0).is_none());
    }

    #[test]
    fn test_known_formats_nonempty() {
        let all = known_drm_formats();
        assert!(all.contains(&DRM_FORMAT_XRGB8888));
        assert!(all.contains(&DRM_FORMAT_NV12));
    }
}
