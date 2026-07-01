//! Eis Viewport — Abstract viewport interface for EIS from GNOME Mutter
//!
//! Defines the interface contract for EIS viewports (e.g., monitors, virtual regions).
//! Viewports report position, size, scale, and coordinate transformation.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-eis-viewport.h

/// Abstract interface for an EIS viewport.
/// Implementations (e.g., MetaEisMonitorViewport) provide geometry and coordinates.
pub trait MetaEisViewport {
    /// Whether this viewport is standalone (e.g., a full monitor vs. a sub-region).
    fn is_standalone(&self) -> bool;

    /// Mapping ID (stable identifier for remoting protocol).
    fn get_mapping_id(&self) -> Option<&str>;

    /// Top-left position in EIS coordinate space.
    fn get_position(&self) -> Option<(i32, i32)>;

    /// Width and height in EIS coordinates.
    fn get_size(&self) -> (i32, i32);

    /// Physical scale (dots per inch / base DPI ratio).
    fn get_physical_scale(&self) -> f64;

    /// Transform a coordinate from EIS space to device/screen space.
    fn transform_coordinate(&self, x: f64, y: f64) -> Option<(f64, f64)>;
}
