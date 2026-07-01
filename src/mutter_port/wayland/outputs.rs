//! GNOME src/wayland/meta-wayland-outputs.c
//!
//! MetaWaylandOutput advertises a monitor to Wayland clients via the
//! `wl_output` global (and the xdg-output extension). This is a compact model
//! of the protocol state pushed to clients: geometry/layout, physical size,
//! subpixel order, transform, scale, and the current + preferred modes.
//! Monitors and bound resources are referenced by id (`u32`).
//!
//! https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/wayland/meta-wayland-outputs.c

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU32, Ordering};

/// wl_output.transform values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputTransform {
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    Flipped90,
    Flipped180,
    Flipped270,
}

/// wl_output.subpixel values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subpixel {
    Unknown,
    None,
    HorizontalRgb,
    HorizontalBgr,
    VerticalRgb,
    VerticalBgr,
}

/// A single monitor mode (resolution + refresh) sent via `wl_output.mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputMode {
    pub width: i32,
    pub height: i32,
    /// Refresh rate in mHz (millihertz), matching mutter's `refresh_rate_khz`.
    pub refresh_mhz: i32,
    pub preferred: bool,
}

/// Rectangle position + size in the global compositor layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

/// MetaWaylandOutput — the protocol-facing view of one monitor.
pub struct MetaWaylandOutput {
    /// Protocol object id (stands in for the `wl_global`).
    pub id: u32,
    /// Backing MetaMonitor id.
    pub monitor_id: u32,
    /// Human-readable identifiers sent in `wl_output.geometry`.
    pub make: String,
    pub model: String,
    /// Position + logical size in the global layout.
    pub layout: Rectangle,
    /// Physical size in millimetres.
    pub physical_width_mm: i32,
    pub physical_height_mm: i32,
    pub subpixel: Subpixel,
    pub transform: OutputTransform,
    /// Fractional scale factor applied to the output.
    pub scale: f32,
    /// Integer scale advertised to wl_output (ceil of `scale`).
    pub scale_int: i32,
    /// Xwayland effective scale (X clients are unaware of fractional scaling).
    pub xwayland_scale: i32,
    /// Available modes; the current + preferred are tracked by index.
    pub modes: Vec<OutputMode>,
    pub current_mode: Option<usize>,
    pub preferred_mode: Option<usize>,
    /// Bound `wl_output` resource ids.
    pub resources: Vec<u32>,
    /// Bound `zxdg_output_v1` resource ids.
    pub xdg_output_resources: Vec<u32>,
}

impl MetaWaylandOutput {
    pub fn new(id: u32, monitor_id: u32) -> Self {
        MetaWaylandOutput {
            id,
            monitor_id,
            make: String::new(),
            model: String::new(),
            layout: Rectangle::default(),
            physical_width_mm: 0,
            physical_height_mm: 0,
            subpixel: Subpixel::Unknown,
            transform: OutputTransform::Normal,
            scale: 1.0,
            scale_int: 1,
            xwayland_scale: 1,
            modes: Vec::new(),
            current_mode: None,
            preferred_mode: None,
            resources: Vec::new(),
            xdg_output_resources: Vec::new(),
        }
    }

    /// Add a mode; the first `preferred` mode becomes `preferred_mode`.
    pub fn add_mode(&mut self, mode: OutputMode) -> usize {
        let idx = self.modes.len();
        if mode.preferred && self.preferred_mode.is_none() {
            self.preferred_mode = Some(idx);
        }
        self.modes.push(mode);
        idx
    }

    /// Select the current mode by index.
    pub fn set_current_mode(&mut self, idx: usize) -> bool {
        if idx < self.modes.len() {
            self.current_mode = Some(idx);
            true
        } else {
            false
        }
    }

    pub fn current_mode(&self) -> Option<OutputMode> {
        self.current_mode.and_then(|i| self.modes.get(i).copied())
    }

    /// Update scale, keeping the integer scale in sync (ceil of fractional).
    pub fn set_scale(&mut self, scale: f32) {
        self.scale = scale.max(1.0);
        self.scale_int = libm_ceil(self.scale) as i32;
    }

    /// Effective scale seen by Xwayland clients.
    pub fn effective_xwayland_scale(&self) -> i32 {
        self.xwayland_scale.max(1)
    }

    // STUB: `wl_output.geometry`/`mode`/`scale`/`done` and the xdg_output
    // events must be marshalled to each bound resource over the Wayland wire
    // whenever this state changes. Returns whether anything changed.
    pub fn send_output_events(&self) {}
}

/// ceil for `no_std` without pulling in libm; small helper.
fn libm_ceil(v: f32) -> f32 {
    let t = v as i64 as f32;
    if v > t {
        t + 1.0
    } else {
        t
    }
}

/// Manages the set of advertised outputs, keyed by output id.
pub struct MetaWaylandOutputManager {
    outputs: BTreeMap<u32, MetaWaylandOutput>,
    next_id: AtomicU32,
}

impl MetaWaylandOutputManager {
    pub fn new() -> Self {
        MetaWaylandOutputManager {
            outputs: BTreeMap::new(),
            next_id: AtomicU32::new(1),
        }
    }

    pub fn create_output(&mut self, monitor_id: u32) -> u32 {
        let id = self.next_id.fetch_add(1, Ordering::Release);
        self.outputs
            .insert(id, MetaWaylandOutput::new(id, monitor_id));
        id
    }

    pub fn get(&self, id: u32) -> Option<&MetaWaylandOutput> {
        self.outputs.get(&id)
    }

    pub fn get_mut(&mut self, id: u32) -> Option<&mut MetaWaylandOutput> {
        self.outputs.get_mut(&id)
    }

    pub fn remove_output(&mut self, id: u32) -> bool {
        self.outputs.remove(&id).is_some()
    }

    pub fn output_for_monitor(&self, monitor_id: u32) -> Option<u32> {
        self.outputs
            .values()
            .find(|o| o.monitor_id == monitor_id)
            .map(|o| o.id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modes_and_preferred() {
        let mut o = MetaWaylandOutput::new(1, 10);
        o.add_mode(OutputMode {
            width: 1920,
            height: 1080,
            refresh_mhz: 60000,
            preferred: false,
        });
        let pref = o.add_mode(OutputMode {
            width: 2560,
            height: 1440,
            refresh_mhz: 144000,
            preferred: true,
        });
        assert_eq!(o.preferred_mode, Some(pref));
        assert!(o.set_current_mode(0));
        assert_eq!(o.current_mode().unwrap().width, 1920);
        assert!(!o.set_current_mode(5));
    }

    #[test]
    fn test_scale_ceils_integer() {
        let mut o = MetaWaylandOutput::new(1, 10);
        o.set_scale(1.5);
        assert_eq!(o.scale_int, 2);
        o.set_scale(2.0);
        assert_eq!(o.scale_int, 2);
        o.set_scale(0.5);
        assert_eq!(o.scale, 1.0);
        assert_eq!(o.scale_int, 1);
    }

    #[test]
    fn test_manager_lookup_by_monitor() {
        let mut mgr = MetaWaylandOutputManager::new();
        let a = mgr.create_output(100);
        let _b = mgr.create_output(200);
        assert_eq!(mgr.output_for_monitor(100), Some(a));
        assert!(mgr.remove_output(a));
        assert_eq!(mgr.output_for_monitor(100), None);
    }
}
