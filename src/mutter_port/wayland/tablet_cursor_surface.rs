//! Wayland tablet-tool cursor surface: the client-provided surface used as
//! the visual cursor image for a graphics-tablet tool.

/// Binds a Wayland surface to a tablet tool as its cursor image, along with
/// the hotspot offset supplied via `set_cursor`.
#[derive(Debug, Clone, Copy, Default)]
pub struct TabletCursorSurface {
    pub surface_id: Option<u32>,
    pub hotspot_x: i32,
    pub hotspot_y: i32,
}

impl TabletCursorSurface {
    pub fn new() -> Self {
        Self::default()
    }

    /// Mirrors the tablet tool's `set_cursor` request: bind a new surface and
    /// hotspot, replacing whatever was previously set.
    pub fn set_cursor(&mut self, surface_id: u32, hotspot_x: i32, hotspot_y: i32) {
        self.surface_id = Some(surface_id);
        self.hotspot_x = hotspot_x;
        self.hotspot_y = hotspot_y;
    }

    pub fn clear(&mut self) {
        self.surface_id = None;
        self.hotspot_x = 0;
        self.hotspot_y = 0;
    }

    pub fn is_set(&self) -> bool {
        self.surface_id.is_some()
    }
}
