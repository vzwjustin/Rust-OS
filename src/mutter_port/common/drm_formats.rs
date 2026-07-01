//! DRM format helpers and conversions between DRM, COGL, and multi-texture formats.
//! Ported from src/common/meta-cogl-drm-formats.h/c and meta-drm-format-helpers.h/c

#[repr(u32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoglPixelFormat {
    // TODO: port full enum from COGL library
    Any = 0,
    Invalid = 0xffffffff,
}

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

/// Metadata about a DRM pixel format and its equivalents in other systems.
#[derive(Debug, Clone, Copy)]
pub struct MetaFormatInfo {
    pub drm_format: u32,
    pub opaque_substitute: u32,
    pub cogl_format: CoglPixelFormat,
    pub multi_texture_format: MultiTextureFormat,
}

/// Look up format information by DRM format code.
///
/// # TODO
/// Port the format lookup table and matching logic from meta-cogl-drm-formats.c
pub fn format_info_from_drm_format(drm_format: u32) -> Option<MetaFormatInfo> {
    // TODO: port meta_format_info_from_drm_format from meta-cogl-drm-formats.c
    let _ = drm_format;
    None
}

/// Look up format information by COGL pixel format.
///
/// # TODO
/// Port the format lookup logic from meta-cogl-drm-formats.c
pub fn format_info_from_cogl_format(cogl_format: CoglPixelFormat) -> Option<MetaFormatInfo> {
    // TODO: port meta_format_info_from_cogl_format from meta-cogl-drm-formats.c
    let _ = cogl_format;
    None
}

/// Convert DRM format to COGL pixel format.
///
/// # TODO
/// Port conversion logic from meta-drm-format-helpers.c
pub fn drm_format_to_cogl(drm_format: u32) -> Option<CoglPixelFormat> {
    // TODO: port meta_drm_format_to_cogl from meta-drm-format-helpers.c
    let _ = drm_format;
    None
}

/// Convert COGL format to DRM format.
///
/// # TODO
/// Port conversion logic from meta-drm-format-helpers.c
pub fn cogl_format_to_drm(cogl_format: CoglPixelFormat) -> Option<u32> {
    // TODO: port meta_cogl_format_to_drm from meta-drm-format-helpers.c
    let _ = cogl_format;
    None
}
