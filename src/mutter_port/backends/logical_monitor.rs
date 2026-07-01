//! Logical monitor ported from GNOME Mutter's src/backends/meta-logical-monitor.c
//!
//! A logical monitor is a group of physical monitors that are treated as one
//! (e.g. two mirrored monitors form a single logical monitor). It provides the
//! viewport (layout rectangle), scaling, and transform for a set of monitors,
//! and is the abstraction rendered onto by a renderer view.
//!
//! Reference: https://gitlab.gnome.org/GNOME/mutter/-/blob/main/src/backends/meta-logical-monitor.c

use alloc::string::String;
use alloc::vec::Vec;

/// Rectangle in the global stage coordinate space (mirrors MtkRectangle).
///
/// Uses signed coordinates to faithfully preserve Mutter's edge arithmetic,
/// where a monitor can be laid out to the left of / above the origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MtkRectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl MtkRectangle {
    pub fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        MtkRectangle {
            x,
            y,
            width,
            height,
        }
    }
}

/// Monitor transform (rotation / reflection). Mirrors MtkMonitorTransform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorTransform {
    Normal,
    Rotate90,
    Rotate180,
    Rotate270,
    Flipped,
    FlippedRotate90,
    FlippedRotate180,
    FlippedRotate270,
}

impl Default for MonitorTransform {
    fn default() -> Self {
        MonitorTransform::Normal
    }
}

/// Direction to a neighboring logical monitor. Mirrors MetaDisplayDirection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisplayDirection {
    Up,
    Down,
    Left,
    Right,
}

/// Stable identifier for a logical monitor.
///
/// In Mutter this is derived from the connector name or EDID of the first
/// monitor in the group ("CONNECTOR:%s" or "EDID:%s:%s:%s").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogicalMonitorId {
    Connector(String),
    Edid {
        vendor: String,
        product: String,
        serial: String,
    },
}

/// A physical monitor participating in a logical monitor.
///
/// Faithful data subset of MetaMonitor: enough to reproduce id generation and
/// presentation detection. Real Mutter tracks CRTCs/outputs/modes as well;
/// those hardware bits are stubbed.
#[derive(Debug, Clone)]
pub struct Monitor {
    pub connector: String,
    pub edid_vendor: String,
    pub edid_product: String,
    pub edid_serial: String,
    /// Whether all outputs of this monitor are presentation displays.
    pub is_presentation: bool,
}

impl Monitor {
    pub fn new(connector: String) -> Self {
        Monitor {
            connector,
            edid_vendor: String::new(),
            edid_product: String::new(),
            edid_serial: String::new(),
            is_presentation: false,
        }
    }
}

/// A logical monitor: a viewport composed of one or more physical monitors.
#[derive(Debug, Clone)]
pub struct LogicalMonitor {
    /// Enumeration index, assigned by the monitor manager.
    pub number: i32,
    pub is_primary: bool,
    pub is_presentation: bool,
    /// -1 when not in fullscreen. (XXX in Mutter this is a per-logical flag.)
    pub in_fullscreen: i32,
    pub scale: f32,
    pub transform: MonitorTransform,
    /// Viewport rectangle in the global stage coordinate space.
    pub rect: MtkRectangle,
    pub monitors: Vec<Monitor>,
    id: Option<LogicalMonitorId>,
}

impl LogicalMonitor {
    /// Create a new logical monitor from a config-like description.
    ///
    /// Mirrors meta_logical_monitor_new(): copies scale/transform/layout and
    /// then appends each monitor, which recomputes presentation state and the id.
    pub fn new(
        number: i32,
        scale: f32,
        transform: MonitorTransform,
        layout: MtkRectangle,
        monitors: Vec<Monitor>,
    ) -> Self {
        let mut logical_monitor = LogicalMonitor {
            number,
            is_primary: false,
            is_presentation: true,
            in_fullscreen: -1,
            scale,
            transform,
            rect: layout,
            monitors: Vec::new(),
            id: None,
        };

        for monitor in monitors {
            logical_monitor.add_monitor(monitor);
        }

        logical_monitor
    }

    /// Generate the stable id from the first monitor in the group.
    fn generate_id(&self) -> Option<LogicalMonitorId> {
        let monitor = self.monitors.first()?;

        if monitor.edid_vendor.is_empty()
            && monitor.edid_product.is_empty()
            && monitor.edid_serial.is_empty()
        {
            Some(LogicalMonitorId::Connector(monitor.connector.clone()))
        } else {
            Some(LogicalMonitorId::Edid {
                vendor: monitor.edid_vendor.clone(),
                product: monitor.edid_product.clone(),
                serial: monitor.edid_serial.clone(),
            })
        }
    }

    /// Add a physical monitor to this logical monitor.
    ///
    /// A logical monitor is a presentation display only if *every* monitor in
    /// the group is a presentation display (faithful to meta_logical_monitor_add_monitor).
    pub fn add_monitor(&mut self, monitor: Monitor) {
        let mut is_presentation = self.is_presentation;
        self.monitors.push(monitor);

        for other in &self.monitors {
            is_presentation = is_presentation && other.is_presentation;
        }

        self.is_presentation = is_presentation;

        if self.id.is_none() {
            self.id = self.generate_id();
        }
    }

    pub fn is_primary(&self) -> bool {
        self.is_primary
    }

    pub fn make_primary(&mut self) {
        self.is_primary = true;
    }

    pub fn get_scale(&self) -> f32 {
        self.scale
    }

    pub fn get_transform(&self) -> MonitorTransform {
        self.transform
    }

    pub fn get_layout(&self) -> MtkRectangle {
        self.rect
    }

    /// Get the enumeration index. Not stable across monitors-changed events.
    pub fn get_number(&self) -> i32 {
        self.number
    }

    pub fn get_monitors(&self) -> &[Monitor] {
        &self.monitors
    }

    pub fn get_id(&self) -> Option<&LogicalMonitorId> {
        self.id.as_ref()
    }

    /// Return whether `neighbor` is directly adjacent to this monitor in the
    /// given direction, sharing an edge with overlapping extent.
    ///
    /// Faithful port of meta_logical_monitor_has_neighbor(): edges must touch
    /// exactly and the perpendicular extents must overlap.
    pub fn has_neighbor(
        &self,
        neighbor: &LogicalMonitor,
        neighbor_direction: DisplayDirection,
    ) -> bool {
        match neighbor_direction {
            DisplayDirection::Right => {
                neighbor.rect.x == (self.rect.x + self.rect.width)
                    && vert_overlap(&neighbor.rect, &self.rect)
            }
            DisplayDirection::Left => {
                self.rect.x == (neighbor.rect.x + neighbor.rect.width)
                    && vert_overlap(&neighbor.rect, &self.rect)
            }
            DisplayDirection::Up => {
                self.rect.y == (neighbor.rect.y + neighbor.rect.height)
                    && horiz_overlap(&neighbor.rect, &self.rect)
            }
            DisplayDirection::Down => {
                neighbor.rect.y == (self.rect.y + self.rect.height)
                    && horiz_overlap(&neighbor.rect, &self.rect)
            }
        }
    }
}

/// Whether two rectangles overlap on the vertical (y) axis.
/// Mirrors mtk_rectangle_vert_overlap.
fn vert_overlap(a: &MtkRectangle, b: &MtkRectangle) -> bool {
    a.y < (b.y + b.height) && b.y < (a.y + a.height)
}

/// Whether two rectangles overlap on the horizontal (x) axis.
/// Mirrors mtk_rectangle_horiz_overlap.
fn horiz_overlap(a: &MtkRectangle, b: &MtkRectangle) -> bool {
    a.x < (b.x + b.width) && b.x < (a.x + a.width)
}
