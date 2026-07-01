//! Output ported from GNOME Mutter's src/backends/meta-output.c
//!
//! An output is a physical connector's worth of display state: its name,
//! vendor/product/serial (derived from EDID), supported modes, tiling and
//! HDR metadata, and the CRTC currently driving it. This port keeps the
//! data model, the EDID-derived detail logic, and the geometry/equality
//! math; DRM/KMS object plumbing and backlight/privacy-screen vfuncs are
//! stubbed for backend implementations.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-output.c

use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use super::connector::MetaConnectorType;
use super::crtc_mode::{MetaCrtcMode, MetaCrtcRefreshRateMode};
use super::edid_parse::EdidInfo;

/// Color mode of an output (mirrors `MetaColorMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaColorMode {
    Default,
    SdrNative,
    Bt2100,
}

/// RGB range of an output (mirrors `MetaOutputRGBRange`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaOutputRGBRange {
    Auto,
    Full,
    Limited,
}

/// Output colorspace (mirrors `MetaOutputColorspace`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaOutputColorspace {
    Default,
    Bt2020,
}

/// HDR EOTF (mirrors `MetaOutputHdrMetadataEOTF`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetaOutputHdrMetadataEOTF {
    TraditionalGammaSdr,
    TraditionalGammaHdr,
    Pq,
    Hlg,
}

/// Privacy screen state bitflags (mirrors `MetaPrivacyScreenState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetaPrivacyScreenState(pub u32);

impl MetaPrivacyScreenState {
    pub const UNAVAILABLE: u32 = 0;
    pub const ENABLED: u32 = 1 << 0;
    pub const LOCKED: u32 = 1 << 1;
}

/// A CIE 1931 chromaticity coordinate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Chromaticity {
    pub x: f64,
    pub y: f64,
}

/// HDR static metadata for an output (mirrors `MetaOutputHdrMetadata`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MetaOutputHdrMetadata {
    pub active: bool,
    pub eotf: MetaOutputHdrMetadataEOTF,
    pub mastering_display_primaries: [Chromaticity; 3],
    pub mastering_display_white_point: Chromaticity,
    pub mastering_display_max_luminance: f64,
    pub mastering_display_min_luminance: f64,
    pub max_cll: f64,
    pub max_fall: f64,
}

fn hdr_primaries_equal(x1: f64, x2: f64) -> bool {
    (x1 - x2).abs() < (0.00002 - f64::EPSILON)
}

fn hdr_nits_equal(x1: f64, x2: f64) -> bool {
    (x1 - x2).abs() < (1.0 - f64::EPSILON)
}

fn hdr_min_luminance_equal(x1: f64, x2: f64) -> bool {
    (x1 - x2).abs() < (0.0001 - f64::EPSILON)
}

impl MetaOutputHdrMetadata {
    /// Tolerance-based equality (mirrors `meta_output_hdr_metadata_equal`).
    pub fn metadata_equal(&self, other: &MetaOutputHdrMetadata) -> bool {
        if !self.active && !other.active {
            return true;
        }
        if self.active != other.active {
            return false;
        }
        if self.eotf != other.eotf {
            return false;
        }

        for i in 0..3 {
            if !hdr_primaries_equal(
                self.mastering_display_primaries[i].x,
                other.mastering_display_primaries[i].x,
            ) || !hdr_primaries_equal(
                self.mastering_display_primaries[i].y,
                other.mastering_display_primaries[i].y,
            ) {
                return false;
            }
        }
        if !hdr_primaries_equal(
            self.mastering_display_white_point.x,
            other.mastering_display_white_point.x,
        ) || !hdr_primaries_equal(
            self.mastering_display_white_point.y,
            other.mastering_display_white_point.y,
        ) {
            return false;
        }

        if !hdr_nits_equal(
            self.mastering_display_max_luminance,
            other.mastering_display_max_luminance,
        ) {
            return false;
        }
        if !hdr_min_luminance_equal(
            self.mastering_display_min_luminance,
            other.mastering_display_min_luminance,
        ) {
            return false;
        }
        if !hdr_nits_equal(self.max_cll, other.max_cll)
            || !hdr_nits_equal(self.max_fall, other.max_fall)
        {
            return false;
        }

        true
    }
}

/// Monitor tiling info (mirrors `MetaTileInfo`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetaTileInfo {
    pub group_id: u32,
    pub flags: u32,
    pub max_h_tiles: u32,
    pub max_v_tiles: u32,
    pub loc_h_tile: u32,
    pub loc_v_tile: u32,
    pub tile_w: u32,
    pub tile_h: u32,
}

/// Immutable-ish description of an output (mirrors `MetaOutputInfo`).
///
/// In C this is a boxed, ref-counted type; here it is a plain struct.
#[derive(Debug, Clone)]
pub struct MetaOutputInfo {
    pub name: String,
    pub vendor: Option<String>,
    pub product: Option<String>,
    pub serial: Option<String>,
    pub edid_checksum_md5: Option<String>,
    pub edid_info: Option<EdidInfo>,

    pub connector_type: MetaConnectorType,
    pub panel_orientation_transform: u32,

    pub modes: Vec<MetaCrtcMode>,
    pub preferred_mode: Option<usize>,

    pub possible_crtcs: Vec<u64>,
    pub possible_clones: Vec<u64>,

    pub tile_info: MetaTileInfo,
}

impl MetaOutputInfo {
    pub fn new(name: String, connector_type: MetaConnectorType) -> Self {
        MetaOutputInfo {
            name,
            vendor: None,
            product: None,
            serial: None,
            edid_checksum_md5: None,
            edid_info: None,
            connector_type,
            panel_orientation_transform: 0,
            modes: Vec::new(),
            preferred_mode: None,
            possible_crtcs: Vec::new(),
            possible_clones: Vec::new(),
            tile_info: MetaTileInfo {
                group_id: 0,
                flags: 0,
                max_h_tiles: 0,
                max_v_tiles: 0,
                loc_h_tile: 0,
                loc_v_tile: 0,
                tile_w: 0,
                tile_h: 0,
            },
        }
    }

    /// Fill vendor/product/serial from parsed EDID, with fallbacks matching
    /// `set_output_details_from_edid`.
    fn set_details_from_edid(&mut self, edid_info: &EdidInfo) {
        self.vendor = validated_nonempty(edid_info.manufacturer_code.clone());

        self.product = match validated_nonempty_option(edid_info.dsc_product_name.clone()) {
            Some(p) => Some(p),
            None => Some(format!("0x{:04x}", edid_info.product_code as u16)),
        };

        self.serial = match validated_nonempty_option(edid_info.dsc_serial_number.clone()) {
            Some(s) => Some(s),
            None => Some(format!("0x{:08x}", edid_info.serial_number)),
        };
    }

    /// Parse an EDID blob and fill in details (mirrors `meta_output_info_parse_edid`).
    ///
    /// The MD5 checksum computation from C is stubbed (no MD5 in kernel core).
    pub fn parse_edid(&mut self, edid: &[u8]) {
        if self.edid_info.is_some() {
            return;
        }

        // Would compute G_CHECKSUM_MD5 over `edid` here; stubbed for no_std kernel.
        if let Some(edid_info) = EdidInfo::parse(edid) {
            self.set_details_from_edid(&edid_info);
            self.edid_info = Some(edid_info);
        }
    }

    /// Minimum refresh rate from EDID range limits, if available and positive.
    pub fn get_min_refresh_rate(&self) -> Option<i32> {
        let edid_info = self.edid_info.as_ref()?;
        let min_vert_rate_hz = edid_info.min_vert_rate_hz;
        if min_vert_rate_hz <= 0 {
            None
        } else {
            Some(min_vert_rate_hz)
        }
    }

    /// Whether this output is a built-in panel.
    pub fn is_builtin(&self) -> bool {
        matches!(
            self.connector_type,
            MetaConnectorType::Edp
                | MetaConnectorType::Lvds
                | MetaConnectorType::Dsi
                | MetaConnectorType::Dpi
        )
    }
}

/// Validate a possibly-empty string, returning `None` if empty (mirrors the
/// C `g_utf8_validate`/empty checks; Rust `String` is already valid UTF-8).
fn validated_nonempty(value: String) -> Option<String> {
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn validated_nonempty_option(value: Option<String>) -> Option<String> {
    value.and_then(validated_nonempty)
}

/// A physical output driven by a CRTC.
///
/// GPU object pointers, monitor back-references, and backend vfuncs
/// (backlight, privacy-screen state) are stubbed for backend implementations.
#[derive(Debug)]
pub struct MetaOutput {
    id: u64,
    info: MetaOutputInfo,

    /// Id of the CRTC driving this output, or `None` if disabled.
    crtc: Option<u64>,

    is_primary: bool,
    is_presentation: bool,
    is_underscanning: bool,

    has_max_bpc: bool,
    max_bpc: u32,

    is_privacy_screen_enabled: bool,

    color_mode: MetaColorMode,
    rgb_range: MetaOutputRGBRange,
}

impl MetaOutput {
    pub fn new(id: u64, info: MetaOutputInfo) -> Self {
        MetaOutput {
            id,
            info,
            crtc: None,
            is_primary: false,
            is_presentation: false,
            is_underscanning: false,
            has_max_bpc: false,
            max_bpc: 0,
            is_privacy_screen_enabled: false,
            color_mode: MetaColorMode::Default,
            rgb_range: MetaOutputRGBRange::Auto,
        }
    }

    pub fn get_id(&self) -> u64 {
        self.id
    }

    pub fn get_name(&self) -> &str {
        &self.info.name
    }

    pub fn get_info(&self) -> &MetaOutputInfo {
        &self.info
    }

    pub fn is_primary(&self) -> bool {
        self.is_primary
    }

    pub fn is_presentation(&self) -> bool {
        self.is_presentation
    }

    pub fn is_underscanning(&self) -> bool {
        self.is_underscanning
    }

    /// Returns the max bpc if set (mirrors `meta_output_get_max_bpc`).
    pub fn get_max_bpc(&self) -> Option<u32> {
        if self.has_max_bpc {
            Some(self.max_bpc)
        } else {
            None
        }
    }

    /// Id of the assigned CRTC, if any.
    pub fn get_assigned_crtc(&self) -> Option<u64> {
        self.crtc
    }

    /// Assign a CRTC id and the associated assignment state.
    #[allow(clippy::too_many_arguments)]
    pub fn assign_crtc(
        &mut self,
        crtc_id: u64,
        is_primary: bool,
        is_presentation: bool,
        is_underscanning: bool,
        rgb_range: Option<MetaOutputRGBRange>,
        max_bpc: Option<u32>,
        color_mode: MetaColorMode,
    ) {
        self.unassign_crtc();
        self.crtc = Some(crtc_id);
        self.is_primary = is_primary;
        self.is_presentation = is_presentation;
        self.is_underscanning = is_underscanning;

        if let Some(range) = rgb_range {
            self.rgb_range = range;
        }

        self.has_max_bpc = max_bpc.is_some();
        if let Some(bpc) = max_bpc {
            self.max_bpc = bpc;
        }

        self.color_mode = color_mode;
    }

    /// Unassign the CRTC and reset primary/presentation flags.
    pub fn unassign_crtc(&mut self) {
        self.crtc = None;
        self.is_primary = false;
        self.is_presentation = false;
    }

    pub fn get_color_mode(&self) -> MetaColorMode {
        self.color_mode
    }

    pub fn peek_rgb_range(&self) -> MetaOutputRGBRange {
        self.rgb_range
    }

    pub fn is_privacy_screen_enabled(&self) -> bool {
        self.is_privacy_screen_enabled
    }

    /// Color/HDR metadata implied by the current color mode
    /// (mirrors `meta_output_get_color_metadata`).
    pub fn get_color_metadata(&self) -> (MetaOutputHdrMetadata, MetaOutputColorspace) {
        let inactive = MetaOutputHdrMetadata {
            active: false,
            eotf: MetaOutputHdrMetadataEOTF::TraditionalGammaSdr,
            mastering_display_primaries: [Chromaticity { x: 0.0, y: 0.0 }; 3],
            mastering_display_white_point: Chromaticity { x: 0.0, y: 0.0 },
            mastering_display_max_luminance: 0.0,
            mastering_display_min_luminance: 0.0,
            max_cll: 0.0,
            max_fall: 0.0,
        };

        match self.color_mode {
            MetaColorMode::Default | MetaColorMode::SdrNative => {
                (inactive, MetaOutputColorspace::Default)
            }
            MetaColorMode::Bt2100 => {
                let mut hdr = inactive;
                hdr.active = true;
                hdr.eotf = MetaOutputHdrMetadataEOTF::Pq;
                (hdr, MetaOutputColorspace::Bt2020)
            }
        }
    }

    /// Update the output's mode list.
    pub fn update_modes(&mut self, preferred_mode: Option<usize>, modes: Vec<MetaCrtcMode>) {
        self.info.modes = modes;
        self.info.preferred_mode = preferred_mode;
    }

    /// Whether two outputs describe the same physical device
    /// (mirrors `meta_output_matches`; GPU identity is left to the caller).
    pub fn matches(&self, other: &MetaOutput) -> bool {
        self.info.name == other.info.name
            && self.info.vendor == other.info.vendor
            && self.info.product == other.info.product
            && self.info.serial == other.info.serial
    }

    /// Whether VRR is enabled: requires an assigned CRTC whose active mode is
    /// variable-refresh. `crtc_mode` supplies the active mode of the assigned
    /// CRTC (backend-provided, since CRTC config lives in the backend).
    pub fn is_vrr_enabled(&self, crtc_mode: Option<&MetaCrtcMode>) -> bool {
        if self.crtc.is_none() {
            return false;
        }

        match crtc_mode {
            Some(mode) => mode.get_info().refresh_rate_mode == MetaCrtcRefreshRateMode::Variable,
            None => false,
        }
    }
}
