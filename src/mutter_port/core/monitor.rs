//! MetaMonitor ported from GNOME Mutter's src/core/meta-monitor.c
//!
//! MetaMonitor represents a physical monitor (display panel). It owns one or
//! more MetaOutput objects (connectors) and provides the current mode, scale,
//! transform, and geometry. Two concrete subclasses exist in Mutter:
//! MetaMonitorNormal (a single output) and MetaMonitorTiled (multiple outputs
//! tiled together to form one logical display surface).
//!
//! In the kernel, the DRM/KMS output objects are modeled by the backend's
//! `Output` type. Here we keep the monitor data model and mode/geometry logic
//! faithful, with hardware probing stubbed.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/core/meta-monitor.c

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// Display mode (resolution + refresh rate). Mirrors MetaCrtcMode values
/// that a monitor can use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitorMode {
    /// Mode identifier (maps to a DRM mode ID).
    pub id: u32,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// Refresh rate in millihertz (e.g. 60000 = 60 Hz).
    pub refresh_rate_mhz: u32,
    /// Whether this is the preferred mode (from EDID).
    pub is_preferred: bool,
    /// Whether this is the current mode.
    pub is_current: bool,
}

impl MonitorMode {
    pub fn new(id: u32, width: u32, height: u32, refresh_rate_mhz: u32) -> Self {
        MonitorMode {
            id,
            width,
            height,
            refresh_rate_mhz,
            is_preferred: false,
            is_current: false,
        }
    }

    /// Refresh rate in Hz (mhz / 1000).
    pub fn refresh_rate_hz(&self) -> f32 {
        self.refresh_rate_mhz as f32 / 1000.0
    }
}

/// A physical output (connector) belonging to a monitor. Mirrors a subset
/// of MetaOutput relevant to monitor-level logic.
#[derive(Debug, Clone)]
pub struct MonitorOutput {
    /// Connector name (e.g. "eDP-1", "DP-1", "HDMI-A-1").
    pub connector: String,
    /// EDID vendor 3-letter code.
    pub vendor: String,
    /// EDID product name.
    pub product: String,
    /// EDID serial number.
    pub serial: String,
    /// Whether this output is presentation-only (not for normal desktop).
    pub is_presentation: bool,
    /// Whether the output can change mode list on hotplug.
    pub hotplug_mode_update: bool,
    /// Physical width in millimeters (from EDID).
    pub width_mm: u32,
    /// Physical height in millimeters (from EDID).
    pub height_mm: u32,
    /// Tile group identifier (for tiled monitors). None for non-tiled.
    pub tile_group: Option<TileGroup>,
    /// CRTC ID currently driving this output, if any.
    pub crtc_id: Option<u32>,
}

impl MonitorOutput {
    pub fn new(connector: &str) -> Self {
        MonitorOutput {
            connector: String::from(connector),
            vendor: String::new(),
            product: String::new(),
            serial: String::new(),
            is_presentation: false,
            hotplug_mode_update: false,
            width_mm: 0,
            height_mm: 0,
            tile_group: None,
            crtc_id: None,
        }
    }
}

/// Tile group info for tiled monitors. Mirrors MetaTileInfo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileGroup {
    /// Group identifier (from the TILE property blob).
    pub group_id: u32,
    /// Number of horizontal tiles.
    pub num_h_tiles: u32,
    /// Number of vertical tiles.
    pub num_v_tiles: u32,
    /// This output's tile location (column, row).
    pub loc_h: u32,
    pub loc_v: u32,
    /// Tile dimensions in pixels.
    pub tile_width: u32,
    pub tile_height: u32,
}

/// Monitor transform (rotation/reflection). Re-exports the logical monitor
/// transform for convenience.
pub use crate::mutter_port::backends::logical_monitor::MonitorTransform;

/// Monitor geometry in physical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MonitorGeometry {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl MonitorGeometry {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        MonitorGeometry {
            x,
            y,
            width,
            height,
        }
    }
}

/// Which monitor type this is. Mirrors the GObject subclass hierarchy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorKind {
    /// Single-output monitor (MetaMonitorNormal).
    Normal,
    /// Multi-output tiled monitor (MetaMonitorTiled).
    Tiled,
}

/// A physical monitor. Mirrors MetaMonitor.
///
/// In Mutter this is an abstract GObject with Normal and Tiled subclasses.
/// Here we fold both into one struct with a `kind` discriminator; the tiled
/// fields are only meaningful when `kind == Tiled`.
#[derive(Debug, Clone)]
pub struct MetaMonitor {
    /// Whether this is a normal or tiled monitor.
    kind: MonitorKind,
    /// Outputs belonging to this monitor (1 for normal, >1 for tiled).
    outputs: Vec<MonitorOutput>,
    /// Available display modes.
    modes: Vec<MonitorMode>,
    /// Index of the current mode in `modes`, if any.
    current_mode_index: Option<usize>,
    /// Index of the preferred mode.
    preferred_mode_index: Option<usize>,
    /// Current transform.
    transform: MonitorTransform,
    /// Current scale factor.
    scale: f32,
    /// Whether this monitor is the primary monitor.
    is_primary: bool,
    /// Whether this monitor is for presentation only.
    is_presentation: bool,
    /// Whether this is a builtin panel (laptop screen).
    is_builtin: bool,
    /// Current geometry in the global stage coordinate space.
    geometry: MonitorGeometry,
    /// Tile group info (only for tiled monitors).
    tile_group: Option<TileGroup>,
}

impl MetaMonitor {
    /// Create a normal (single-output) monitor. Mirrors meta_monitor_normal_new().
    pub fn new_normal(output: MonitorOutput) -> Self {
        let is_builtin =
            output.connector.starts_with("eDP") || output.connector.starts_with("LVDS");
        let is_presentation = output.is_presentation;
        MetaMonitor {
            kind: MonitorKind::Normal,
            outputs: vec![output],
            modes: Vec::new(),
            current_mode_index: None,
            preferred_mode_index: None,
            transform: MonitorTransform::Normal,
            scale: 1.0,
            is_primary: false,
            is_presentation,
            is_builtin,
            geometry: MonitorGeometry::default(),
            tile_group: None,
        }
    }

    /// Create a tiled monitor from multiple outputs. Mirrors
    /// meta_monitor_tiled_new().
    pub fn new_tiled(outputs: Vec<MonitorOutput>, tile_group: TileGroup) -> Self {
        let is_presentation = outputs.iter().all(|o| o.is_presentation);
        let is_builtin = outputs
            .iter()
            .any(|o| o.connector.starts_with("eDP") || o.connector.starts_with("LVDS"));
        MetaMonitor {
            kind: MonitorKind::Tiled,
            outputs,
            modes: Vec::new(),
            current_mode_index: None,
            preferred_mode_index: None,
            transform: MonitorTransform::Normal,
            scale: 1.0,
            is_primary: false,
            is_presentation,
            is_builtin,
            geometry: MonitorGeometry::default(),
            tile_group: Some(tile_group),
        }
    }

    // ── Accessors ─────────────────────────────────────────────────────

    pub fn kind(&self) -> MonitorKind {
        self.kind
    }

    pub fn is_normal(&self) -> bool {
        self.kind == MonitorKind::Normal
    }

    pub fn is_tiled(&self) -> bool {
        self.kind == MonitorKind::Tiled
    }

    pub fn outputs(&self) -> &[MonitorOutput] {
        &self.outputs
    }

    pub fn modes(&self) -> &[MonitorMode] {
        &self.modes
    }

    pub fn current_mode(&self) -> Option<&MonitorMode> {
        self.current_mode_index.and_then(|i| self.modes.get(i))
    }

    pub fn preferred_mode(&self) -> Option<&MonitorMode> {
        self.preferred_mode_index.and_then(|i| self.modes.get(i))
    }

    pub fn transform(&self) -> MonitorTransform {
        self.transform
    }

    pub fn set_transform(&mut self, transform: MonitorTransform) {
        self.transform = transform;
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale;
    }

    pub fn is_primary(&self) -> bool {
        self.is_primary
    }

    pub fn set_primary(&mut self, primary: bool) {
        self.is_primary = primary;
    }

    pub fn is_presentation(&self) -> bool {
        self.is_presentation
    }

    pub fn is_builtin(&self) -> bool {
        self.is_builtin
    }

    pub fn geometry(&self) -> MonitorGeometry {
        self.geometry
    }

    pub fn set_geometry(&mut self, geometry: MonitorGeometry) {
        self.geometry = geometry;
    }

    pub fn tile_group(&self) -> Option<&TileGroup> {
        self.tile_group.as_ref()
    }

    // ── Mode management ───────────────────────────────────────────────

    /// Set the list of available modes. Mirrors meta_monitor_take_modes().
    pub fn set_modes(&mut self, modes: Vec<MonitorMode>) {
        // Find preferred and current modes.
        self.preferred_mode_index = modes.iter().position(|m| m.is_preferred);
        self.current_mode_index = modes.iter().position(|m| m.is_current);
        self.modes = modes;
    }

    /// Set the current mode by index. Mirrors meta_monitor_set_current_mode().
    pub fn set_current_mode(&mut self, index: usize) -> bool {
        if index >= self.modes.len() {
            return false;
        }
        self.current_mode_index = Some(index);
        true
    }

    /// Get the dimensions of the current mode (or preferred, or first).
    /// Mirrors meta_monitor_get_dimensions().
    pub fn dimensions(&self) -> (u32, u32) {
        if let Some(mode) = self.current_mode() {
            (mode.width, mode.height)
        } else if let Some(mode) = self.preferred_mode() {
            (mode.width, mode.height)
        } else if let Some(mode) = self.modes.first() {
            (mode.width, mode.height)
        } else {
            (0, 0)
        }
    }

    // ── Identity ──────────────────────────────────────────────────────

    /// Get the connector name of the first output. Mirrors
    /// meta_monitor_get_main_connector().
    pub fn main_connector(&self) -> &str {
        if let Some(first) = self.outputs.first() {
            &first.connector
        } else {
            ""
        }
    }

    /// Get the EDID vendor code.
    pub fn vendor(&self) -> &str {
        if let Some(first) = self.outputs.first() {
            &first.vendor
        } else {
            ""
        }
    }

    /// Get the EDID product name.
    pub fn product(&self) -> &str {
        if let Some(first) = self.outputs.first() {
            &first.product
        } else {
            ""
        }
    }

    /// Get the EDID serial.
    pub fn serial(&self) -> &str {
        if let Some(first) = self.outputs.first() {
            &first.serial
        } else {
            ""
        }
    }

    // ── Geometry helpers ──────────────────────────────────────────────

    /// Compute the geometry for this monitor given a position and the
    /// current mode dimensions. Mirrors meta_monitor_derive_layout().
    pub fn derive_layout(&mut self, x: i32, y: i32) {
        let (w, h) = self.dimensions();
        // Apply transform: swap width/height for 90°/270° rotations.
        let (w, h) = match self.transform {
            MonitorTransform::Rotate90
            | MonitorTransform::Rotate270
            | MonitorTransform::FlippedRotate90
            | MonitorTransform::FlippedRotate270 => (h, w),
            _ => (w, h),
        };
        self.geometry = MonitorGeometry::new(x, y, w as i32, h as i32);
    }

    /// Physical aspect ratio (width / height) from current mode.
    pub fn aspect_ratio(&self) -> f32 {
        let (w, h) = self.dimensions();
        if h == 0 {
            0.0
        } else {
            w as f32 / h as f32
        }
    }

    /// Physical size in millimeters (sum of output dimensions for tiled).
    pub fn physical_size_mm(&self) -> (u32, u32) {
        if self.kind == MonitorKind::Normal {
            if let Some(o) = self.outputs.first() {
                return (o.width_mm, o.height_mm);
            }
        }
        // For tiled monitors, sum tile dimensions.
        let tile = match &self.tile_group {
            Some(t) => t,
            None => return (0, 0),
        };
        let total_w = tile.tile_width * tile.num_h_tiles;
        let total_h = tile.tile_height * tile.num_v_tiles;
        (total_w, total_h)
    }

    /// DPI (dots per inch) based on current mode and physical size.
    /// 25.4 mm per inch.
    pub fn dpi(&self) -> (f32, f32) {
        let (pw, ph) = self.dimensions();
        let (mw, mh) = self.physical_size_mm();
        if mw == 0 || mh == 0 {
            return (0.0, 0.0);
        }
        (pw as f32 * 25.4 / mw as f32, ph as f32 * 25.4 / mh as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_output(connector: &str) -> MonitorOutput {
        MonitorOutput::new(connector)
    }

    fn make_mode(id: u32, w: u32, h: u32, rate: u32) -> MonitorMode {
        MonitorMode::new(id, w, h, rate)
    }

    #[test]
    fn test_normal_monitor_creation() {
        let monitor = MetaMonitor::new_normal(make_output("eDP-1"));
        assert!(monitor.is_normal());
        assert!(!monitor.is_tiled());
        assert_eq!(monitor.outputs().len(), 1);
        assert!(monitor.is_builtin());
        assert_eq!(monitor.main_connector(), "eDP-1");
    }

    #[test]
    fn test_tiled_monitor_creation() {
        let tile = TileGroup {
            group_id: 1,
            num_h_tiles: 2,
            num_v_tiles: 1,
            loc_h: 0,
            loc_v: 0,
            tile_width: 1920,
            tile_height: 1080,
        };
        let monitor = MetaMonitor::new_tiled(vec![make_output("DP-1"), make_output("DP-2")], tile);
        assert!(monitor.is_tiled());
        assert!(!monitor.is_normal());
        assert_eq!(monitor.outputs().len(), 2);
        assert!(monitor.tile_group().is_some());
    }

    #[test]
    fn test_mode_management() {
        let mut monitor = MetaMonitor::new_normal(make_output("DP-1"));
        let mut m1 = make_mode(1, 1920, 1080, 60000);
        m1.is_preferred = true;
        let mut m2 = make_mode(2, 1280, 720, 60000);
        m2.is_current = true;

        monitor.set_modes(vec![m1, m2]);

        assert!(monitor.preferred_mode().is_some());
        assert_eq!(monitor.preferred_mode().unwrap().width, 1920);
        assert!(monitor.current_mode().is_some());
        assert_eq!(monitor.current_mode().unwrap().width, 1280);
    }

    #[test]
    fn test_dimensions() {
        let mut monitor = MetaMonitor::new_normal(make_output("DP-1"));
        let mut m1 = make_mode(1, 1920, 1080, 60000);
        m1.is_current = true;
        monitor.set_modes(vec![m1]);

        assert_eq!(monitor.dimensions(), (1920, 1080));
    }

    #[test]
    fn test_dimensions_fallback() {
        let monitor = MetaMonitor::new_normal(make_output("DP-1"));
        assert_eq!(monitor.dimensions(), (0, 0));
    }

    #[test]
    fn test_derive_layout() {
        let mut monitor = MetaMonitor::new_normal(make_output("DP-1"));
        let mut m1 = make_mode(1, 1920, 1080, 60000);
        m1.is_current = true;
        monitor.set_modes(vec![m1]);

        monitor.derive_layout(100, 200);
        assert_eq!(
            monitor.geometry(),
            MonitorGeometry::new(100, 200, 1920, 1080)
        );
    }

    #[test]
    fn test_derive_layout_rotated() {
        let mut monitor = MetaMonitor::new_normal(make_output("DP-1"));
        let mut m1 = make_mode(1, 1920, 1080, 60000);
        m1.is_current = true;
        monitor.set_modes(vec![m1]);
        monitor.set_transform(MonitorTransform::Rotate90);

        monitor.derive_layout(0, 0);
        // 90° rotation swaps width and height.
        assert_eq!(monitor.geometry(), MonitorGeometry::new(0, 0, 1080, 1920));
    }

    #[test]
    fn test_aspect_ratio() {
        let mut monitor = MetaMonitor::new_normal(make_output("DP-1"));
        let mut m1 = make_mode(1, 1920, 1080, 60000);
        m1.is_current = true;
        monitor.set_modes(vec![m1]);

        let ar = monitor.aspect_ratio();
        assert!((ar - 16.0 / 9.0).abs() < 0.01);
    }

    #[test]
    fn test_refresh_rate_hz() {
        let mode = make_mode(1, 1920, 1080, 59994);
        assert!((mode.refresh_rate_hz() - 59.994).abs() < 0.01);
    }

    #[test]
    fn test_builtin_detection() {
        let m1 = MetaMonitor::new_normal(make_output("eDP-1"));
        assert!(m1.is_builtin());

        let m2 = MetaMonitor::new_normal(make_output("DP-1"));
        assert!(!m2.is_builtin());

        let m3 = MetaMonitor::new_normal(make_output("LVDS-1"));
        assert!(m3.is_builtin());
    }

    #[test]
    fn test_set_current_mode() {
        let mut monitor = MetaMonitor::new_normal(make_output("DP-1"));
        monitor.set_modes(vec![
            make_mode(1, 1920, 1080, 60000),
            make_mode(2, 1280, 720, 60000),
        ]);

        assert!(monitor.set_current_mode(1));
        assert_eq!(monitor.current_mode().unwrap().width, 1280);

        assert!(!monitor.set_current_mode(99));
    }

    #[test]
    fn test_physical_size_normal() {
        let mut output = make_output("DP-1");
        output.width_mm = 527;
        output.height_mm = 296;
        let monitor = MetaMonitor::new_normal(output);

        assert_eq!(monitor.physical_size_mm(), (527, 296));
    }

    #[test]
    fn test_physical_size_tiled() {
        let tile = TileGroup {
            group_id: 1,
            num_h_tiles: 2,
            num_v_tiles: 1,
            loc_h: 0,
            loc_v: 0,
            tile_width: 1920,
            tile_height: 1080,
        };
        let monitor = MetaMonitor::new_tiled(vec![make_output("DP-1"), make_output("DP-2")], tile);

        // Tiled physical size uses pixel dimensions, not mm.
        assert_eq!(monitor.physical_size_mm(), (3840, 1080));
    }

    #[test]
    fn test_dpi() {
        let mut output = make_output("DP-1");
        output.width_mm = 527;
        output.height_mm = 296;
        let mut monitor = MetaMonitor::new_normal(output);
        let mut m = make_mode(1, 1920, 1080, 60000);
        m.is_current = true;
        monitor.set_modes(vec![m]);

        let (dpi_w, dpi_h) = monitor.dpi();
        assert!(dpi_w > 90.0 && dpi_w < 100.0); // ~92.5 DPI
    }
}
