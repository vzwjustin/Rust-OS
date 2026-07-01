//! GNOME src/wayland/meta-wayland-outputs.c
//!
//! Represents physical displays/outputs exposed via wl_output protocol.
//! Manages display modes, scales, and per-monitor surface assignments.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-outputs.c

use alloc::{string::String, vec::Vec};

/// Display mode (resolution, refresh rate)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DisplayMode {
    pub width: u32,
    pub height: u32,
    pub refresh: u32, // mHz (millihertz)
    pub preferred: bool,
}

impl DisplayMode {
    pub fn new(width: u32, height: u32, refresh: u32, preferred: bool) -> Self {
        DisplayMode {
            width,
            height,
            refresh,
            preferred,
        }
    }
}

/// Output transformation (rotation/flip)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transform {
    Normal = 0,
    Rotated90 = 1,
    Rotated180 = 2,
    Rotated270 = 3,
    Flipped = 4,
    Flipped90 = 5,
    Flipped180 = 6,
    Flipped270 = 7,
}

/// Represents a wl_output resource
pub struct WaylandOutput {
    pub id: u32,
    pub name: String,
    pub make: String,
    pub model: String,
    pub x: i32,
    pub y: i32,
    pub width_mm: i32,
    pub height_mm: i32,
    pub scale: i32,
    pub transform: Transform,
    pub modes: Vec<DisplayMode>,
    pub current_mode_index: usize,
}

impl WaylandOutput {
    pub fn new(id: u32, name: impl Into<String>) -> Self {
        WaylandOutput {
            id,
            name: name.into(),
            make: String::new(),
            model: String::new(),
            x: 0,
            y: 0,
            width_mm: 0,
            height_mm: 0,
            scale: 1,
            transform: Transform::Normal,
            modes: Vec::new(),
            current_mode_index: 0,
        }
    }

    pub fn set_position(&mut self, x: i32, y: i32) {
        self.x = x;
        self.y = y;
    }

    pub fn get_position(&self) -> (i32, i32) {
        (self.x, self.y)
    }

    pub fn set_physical_size(&mut self, width_mm: i32, height_mm: i32) {
        self.width_mm = width_mm;
        self.height_mm = height_mm;
    }

    pub fn get_physical_size(&self) -> (i32, i32) {
        (self.width_mm, self.height_mm)
    }

    pub fn set_scale(&mut self, scale: i32) {
        self.scale = scale.max(1);
    }

    pub fn get_scale(&self) -> i32 {
        self.scale
    }

    pub fn set_transform(&mut self, transform: Transform) {
        self.transform = transform;
    }

    pub fn get_transform(&self) -> Transform {
        self.transform
    }

    pub fn add_mode(&mut self, mode: DisplayMode) {
        if mode.preferred && !self.modes.is_empty() {
            self.current_mode_index = self.modes.len();
        }
        self.modes.push(mode);
    }

    pub fn get_modes(&self) -> &[DisplayMode] {
        &self.modes
    }

    pub fn get_current_mode(&self) -> Option<DisplayMode> {
        if self.current_mode_index < self.modes.len() {
            Some(self.modes[self.current_mode_index])
        } else {
            None
        }
    }

    pub fn get_current_resolution(&self) -> Option<(u32, u32)> {
        self.get_current_mode().map(|m| (m.width, m.height))
    }

    /// STUB: Set output mode. Requires CRTC reprogramming and
    /// client notification via wl_output.mode events.
    pub fn set_mode(&mut self, mode_index: usize) {
        if mode_index < self.modes.len() {
            self.current_mode_index = mode_index;
        }
    }

    /// STUB: Get DPI calculations. Requires physical size and
    /// resolution matching.
    pub fn get_dpi(&self) -> Option<(f32, f32)> {
        if self.width_mm == 0 || self.height_mm == 0 {
            return None;
        }

        if let Some(mode) = self.get_current_mode() {
            let dpi_x = (mode.width as f32 * 25.4) / self.width_mm as f32;
            let dpi_y = (mode.height as f32 * 25.4) / self.height_mm as f32;
            Some((dpi_x, dpi_y))
        } else {
            None
        }
    }
}

/// Manages Wayland outputs
pub struct OutputManager {
    outputs: alloc::collections::BTreeMap<u32, WaylandOutput>,
    next_id: u32,
}

impl OutputManager {
    pub fn new() -> Self {
        OutputManager {
            outputs: alloc::collections::BTreeMap::new(),
            next_id: 1,
        }
    }

    pub fn create_output(&mut self, name: impl Into<String>) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let output = WaylandOutput::new(id, name);
        self.outputs.insert(id, output);
        id
    }

    pub fn get_output(&self, id: u32) -> Option<&WaylandOutput> {
        self.outputs.get(&id)
    }

    pub fn get_output_mut(&mut self, id: u32) -> Option<&mut WaylandOutput> {
        self.outputs.get_mut(&id)
    }

    pub fn destroy_output(&mut self, id: u32) -> bool {
        self.outputs.remove(&id).is_some()
    }

    pub fn get_all_outputs(&self) -> Vec<&WaylandOutput> {
        self.outputs.values().collect()
    }

    pub fn get_outputs_for_region(&self, x: i32, y: i32, width: u32, height: u32) -> Vec<u32> {
        self.outputs
            .values()
            .filter(|out| {
                // Check if output overlaps with region
                out.x < (x + width as i32)
                    && (out.x + out.width_mm / 254) > x
                    && out.y < (y + height as i32)
                    && (out.y + out.height_mm / 254) > y
            })
            .map(|out| out.id)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_mode() {
        let mode = DisplayMode::new(1920, 1080, 60000, true);
        assert_eq!(mode.width, 1920);
        assert_eq!(mode.height, 1080);
        assert!(mode.preferred);
    }

    #[test]
    fn test_output_creation() {
        let output = WaylandOutput::new(1, "HDMI-1");
        assert_eq!(output.id, 1);
        assert_eq!(output.name.as_str(), "HDMI-1");
        assert_eq!(output.get_scale(), 1);
    }

    #[test]
    fn test_output_modes() {
        let mut output = WaylandOutput::new(1, "HDMI-1");
        output.add_mode(DisplayMode::new(1920, 1080, 60000, false));
        output.add_mode(DisplayMode::new(2560, 1440, 60000, true));

        assert_eq!(output.get_modes().len(), 2);
        assert_eq!(output.get_current_mode().unwrap().width, 2560);
    }

    #[test]
    fn test_output_manager() {
        let mut mgr = OutputManager::new();
        let id1 = mgr.create_output("HDMI-1");
        let id2 = mgr.create_output("DP-1");

        assert!(mgr.get_output(id1).is_some());
        assert!(mgr.destroy_output(id1));
        assert!(mgr.get_output(id1).is_none());
        assert!(mgr.get_output(id2).is_some());
    }
}
