//! Port of mtk/mtk/mtk-monitor-transform.{c,h} from GNOME mutter.
//!
//! `MonitorTransform` mirrors the Wayland `wl_output_transform` enum
//! semantics: a rotation (0/90/180/270 degrees, counter-clockwise) optionally
//! combined with a horizontal flip applied *before* the rotation.
//!
//! The numeric values intentionally match `wl_output_transform` so that
//! `as_wl_output_transform` / `from_wl_output_transform` are trivial.
//!
//! Not ported: `mtk_monitor_transform_transform_matrix` and the
//! `mtk_compute_viewport_matrix` helper from mtk-utils.c, since both operate
//! on `graphene_matrix_t` / `graphene_euler_t` (a 3D math library tied to
//! GPU pipelines) which has no equivalent in this kernel and is out of scope
//! for pure transform-composition logic. See `mtk/utils.rs` for what was
//! ported from mtk-utils.c / mtk-time-utils.c.

#![allow(dead_code)]

use super::rectangle::Rectangle;

/// Mirrors `MtkMonitorTransform`.
///
/// Values match the Wayland `wl_output_transform` numbering:
/// NORMAL=0, 90=1, 180=2, 270=3, FLIPPED=4, FLIPPED_90=5, FLIPPED_180=6,
/// FLIPPED_270=7.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u32)]
pub enum MonitorTransform {
    Normal = 0,
    Rotate90 = 1,
    Rotate180 = 2,
    Rotate270 = 3,
    Flipped = 4,
    Flipped90 = 5,
    Flipped180 = 6,
    Flipped270 = 7,
}

/// `MTK_MONITOR_N_TRANSFORMS`
pub const N_TRANSFORMS: u32 = MonitorTransform::Flipped270 as u32 + 1;

/// `MTK_MONITOR_ALL_TRANSFORMS`
pub const ALL_TRANSFORMS: u32 = (1u32 << N_TRANSFORMS) - 1;

/// Minimal local size type, used by `transform_size`.
///
/// Note: `src/mutter_port/mtk/rectangle.rs` does not exist yet in this tree,
/// so a small local `Size`/`Rectangle` pair is defined here rather than
/// importing it. If/when `rectangle.rs` is added, these can be swapped for
/// its types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Size {
    pub width: i32,
    pub height: i32,
}

impl Size {
    pub const fn new(width: i32, height: i32) -> Self {
        Size { width, height }
    }
}

impl MonitorTransform {
    /// All transform variants in enum order, for iteration.
    pub const ALL: [MonitorTransform; 8] = [
        MonitorTransform::Normal,
        MonitorTransform::Rotate90,
        MonitorTransform::Rotate180,
        MonitorTransform::Rotate270,
        MonitorTransform::Flipped,
        MonitorTransform::Flipped90,
        MonitorTransform::Flipped180,
        MonitorTransform::Flipped270,
    ];

    /// Returns true if applying this transform swaps width and height.
    /// True for the odd-numbered transforms in the enum
    /// (`mtk_monitor_transform_is_rotated`).
    pub const fn is_rotated(self) -> bool {
        (self as u32) % 2 == 1
    }

    /// Returns true if this transform involves a horizontal flip
    /// (`mtk_monitor_transform_is_flipped`).
    pub const fn is_flipped(self) -> bool {
        (self as u32) >= (MonitorTransform::Flipped as u32)
    }

    /// Returns the transform that undoes `self`
    /// (`mtk_monitor_transform_invert`).
    pub const fn invert(self) -> MonitorTransform {
        use MonitorTransform::*;
        match self {
            Rotate90 => Rotate270,
            Rotate270 => Rotate90,
            Normal | Rotate180 | Flipped | Flipped90 | Flipped180 | Flipped270 => self,
        }
    }

    /// Decomposes into (flipped, quarter-turns), matching the enum's
    /// flip-then-rotate semantics: `Normal`=(false,0) .. `Flipped270`=(true,3).
    const fn to_flip_rot(self) -> (bool, u32) {
        use MonitorTransform::*;
        match self {
            Normal => (false, 0),
            Rotate90 => (false, 1),
            Rotate180 => (false, 2),
            Rotate270 => (false, 3),
            Flipped => (true, 0),
            Flipped90 => (true, 1),
            Flipped180 => (true, 2),
            Flipped270 => (true, 3),
        }
    }

    const fn from_flip_rot(flipped: bool, rot: u32) -> MonitorTransform {
        use MonitorTransform::*;
        match (flipped, rot % 4) {
            (false, 0) => Normal,
            (false, 1) => Rotate90,
            (false, 2) => Rotate180,
            (false, 3) => Rotate270,
            (true, 0) => Flipped,
            (true, 1) => Flipped90,
            (true, 2) => Flipped180,
            (true, 3) => Flipped270,
            _ => unreachable!(),
        }
    }

    /// Composes `self` followed by `other`, i.e. the transform you get by
    /// first applying `self` then applying `other`
    /// (`mtk_monitor_transform_transform`).
    ///
    /// Each transform is a flip (optional, applied first) followed by a
    /// rotation. Composing two such transforms is standard dihedral-group
    /// composition: when the second transform also flips, it reverses the
    /// sense of the first transform's rotation (mirroring conjugates
    /// rotation direction), and the flips cancel via XOR.
    pub const fn compose(self, other: MonitorTransform) -> MonitorTransform {
        let (f1, r1) = self.to_flip_rot();
        let (f2, r2) = other.to_flip_rot();
        let r1_signed = if f2 { (4 - r1) % 4 } else { r1 };
        let new_r = (r2 + r1_signed) % 4;
        let new_f = f1 ^ f2;
        MonitorTransform::from_flip_rot(new_f, new_r)
    }

    /// Applies this transform to a logical/physical size, swapping width and
    /// height for rotated transforms.
    pub const fn transform_size(self, size: Size) -> Size {
        if self.is_rotated() {
            Size::new(size.height, size.width)
        } else {
            size
        }
    }

    /// Applies this transform to a rectangle that lives within an area of
    /// `area_width` x `area_height`, mirroring the point-mapping behavior of
    /// `mtk_monitor_transform_transform_point` (which mtk applies per-corner
    /// to map rectangles). The rectangle's origin is mapped via the same
    /// rule as a point, and its size is mapped via `transform_size`.
    pub fn transform_rectangle(
        self,
        area_width: i32,
        area_height: i32,
        rect: Rectangle,
    ) -> Rectangle {
        let (x1, y1) = self.transform_point(area_width, area_height, rect.x, rect.y);
        let (x2, y2) = self.transform_point(
            area_width,
            area_height,
            rect.x + rect.width,
            rect.y + rect.height,
        );

        let x = x1.min(x2);
        let y = y1.min(y2);
        let width = (x1 - x2).abs();
        let height = (y1 - y2).abs();

        Rectangle::new(x, y, width, height)
    }

    /// Maps a single point `(point_x, point_y)` within an `area_width` x
    /// `area_height` area through this transform, returning the new point
    /// coordinates in the (possibly width/height-swapped) transformed area.
    ///
    /// Mirrors `mtk_monitor_transform_transform_point`.
    ///
    /// Decomposed (consistently with `compose`/`invert`) as: optionally
    /// mirror `x` within the area first, then rotate 90 degrees
    /// counter-clockwise, `rot` times, each step swapping the area's width
    /// and height.
    pub fn transform_point(
        self,
        area_width: i32,
        area_height: i32,
        point_x: i32,
        point_y: i32,
    ) -> (i32, i32) {
        let (flipped, rot) = self.to_flip_rot();

        let mut x = if flipped {
            area_width - point_x
        } else {
            point_x
        };
        let mut y = point_y;
        let mut w = area_width;
        let mut h = area_height;

        for _ in 0..rot {
            let (nx, ny) = (h - y, x);
            x = nx;
            y = ny;
            core::mem::swap(&mut w, &mut h);
        }

        (x, y)
    }

    /// Converts to a raw `wl_output_transform` value. The enum's numeric
    /// values already match the Wayland protocol numbering, so this is a
    /// straight cast (`mtk_monitor_transform` has no dedicated
    /// to/from-wl_output_transform pair in upstream mtk; the two enums are
    /// defined with identical numbering by convention).
    pub const fn as_wl_output_transform(self) -> u32 {
        self as u32
    }

    /// Converts from a raw `wl_output_transform` value (0..=7). Returns
    /// `None` for out-of-range values.
    pub const fn from_wl_output_transform(value: u32) -> Option<MonitorTransform> {
        use MonitorTransform::*;
        Some(match value {
            0 => Normal,
            1 => Rotate90,
            2 => Rotate180,
            3 => Rotate270,
            4 => Flipped,
            5 => Flipped90,
            6 => Flipped180,
            7 => Flipped270,
            _ => return None,
        })
    }

    /// `mtk_monitor_transform_to_string`.
    pub const fn as_str(self) -> &'static str {
        use MonitorTransform::*;
        match self {
            Normal => "normal",
            Rotate90 => "90",
            Rotate180 => "180",
            Rotate270 => "270",
            Flipped => "flipped",
            Flipped90 => "flipped-90",
            Flipped180 => "flipped-180",
            Flipped270 => "flipped-270",
        }
    }

    /// `mtk_monitor_transform_from_string`.
    pub fn from_str(name: &str) -> Option<MonitorTransform> {
        use MonitorTransform::*;
        Some(match name {
            "normal" => Normal,
            "90" => Rotate90,
            "180" => Rotate180,
            "270" => Rotate270,
            "flipped" => Flipped,
            "flipped-90" => Flipped90,
            "flipped-180" => Flipped180,
            "flipped-270" => Flipped270,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invert_is_involution_except_90_270() {
        for t in MonitorTransform::ALL {
            assert_eq!(t.invert().invert(), t);
        }
        assert_eq!(
            MonitorTransform::Rotate90.invert(),
            MonitorTransform::Rotate270
        );
        assert_eq!(
            MonitorTransform::Rotate270.invert(),
            MonitorTransform::Rotate90
        );
    }

    #[test]
    fn compose_with_normal_is_identity() {
        for t in MonitorTransform::ALL {
            assert_eq!(t.compose(MonitorTransform::Normal), t);
            assert_eq!(MonitorTransform::Normal.compose(t), t);
        }
    }

    #[test]
    fn compose_with_inverse_yields_normal() {
        // self.compose(self.invert()) undoes self.
        for t in MonitorTransform::ALL {
            assert_eq!(t.compose(t.invert()), MonitorTransform::Normal);
        }
    }

    #[test]
    fn rotate_90_four_times_is_normal() {
        let mut t = MonitorTransform::Rotate90;
        for _ in 0..3 {
            t = t.compose(MonitorTransform::Rotate90);
        }
        assert_eq!(t, MonitorTransform::Normal);
    }

    #[test]
    fn is_rotated_matches_parity() {
        assert!(!MonitorTransform::Normal.is_rotated());
        assert!(MonitorTransform::Rotate90.is_rotated());
        assert!(!MonitorTransform::Rotate180.is_rotated());
        assert!(MonitorTransform::Rotate270.is_rotated());
        assert!(!MonitorTransform::Flipped.is_rotated());
        assert!(MonitorTransform::Flipped90.is_rotated());
    }

    #[test]
    fn is_flipped_matches_enum_range() {
        assert!(!MonitorTransform::Normal.is_flipped());
        assert!(!MonitorTransform::Rotate270.is_flipped());
        assert!(MonitorTransform::Flipped.is_flipped());
        assert!(MonitorTransform::Flipped270.is_flipped());
    }

    #[test]
    fn transform_size_swaps_on_rotation() {
        let size = Size::new(1920, 1080);
        assert_eq!(MonitorTransform::Normal.transform_size(size), size);
        assert_eq!(
            MonitorTransform::Rotate90.transform_size(size),
            Size::new(1080, 1920)
        );
        assert_eq!(MonitorTransform::Rotate180.transform_size(size), size);
    }

    #[test]
    fn wl_output_transform_roundtrip() {
        for t in MonitorTransform::ALL {
            let raw = t.as_wl_output_transform();
            assert_eq!(MonitorTransform::from_wl_output_transform(raw), Some(t));
        }
        assert_eq!(MonitorTransform::from_wl_output_transform(99), None);
    }

    #[test]
    fn string_roundtrip() {
        for t in MonitorTransform::ALL {
            let s = t.as_str();
            assert_eq!(MonitorTransform::from_str(s), Some(t));
        }
        assert_eq!(MonitorTransform::from_str("bogus"), None);
    }

    #[test]
    fn transform_rectangle_normal_is_identity() {
        let rect = Rectangle::new(10, 20, 100, 50);
        let out = MonitorTransform::Normal.transform_rectangle(1920, 1080, rect);
        assert_eq!(out, rect);
    }

    #[test]
    fn transform_rectangle_180_flips_origin() {
        let rect = Rectangle::new(0, 0, 100, 50);
        let out = MonitorTransform::Rotate180.transform_rectangle(1920, 1080, rect);
        // width/height area is unchanged (not rotated), origin moves to the
        // opposite corner.
        assert_eq!(out.width, 100);
        assert_eq!(out.height, 50);
        assert_eq!(out.x, 1920 - 100);
        assert_eq!(out.y, 1080 - 50);
    }
}
