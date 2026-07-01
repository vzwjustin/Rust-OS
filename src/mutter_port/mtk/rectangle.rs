// Port of mutter's mtk/mtk/mtk-rectangle.{c,h} to idiomatic Rust.
//
// Original: Mtk, "A low-level base library", Copyright (C) 2023 Red Hat,
// licensed under LGPL (see upstream mutter source for full license text).
//
// This module only depends on `core`/`alloc` and ports the pure integer
// geometry logic from MtkRectangle. Functionality that depends on GLib
// boxed-type registration (g_boxed_copy/g_boxed_free, GType machinery) is
// not portable and not relevant in a no_std/no-GLib context, so it has been
// omitted entirely (Rust's `Copy`/`Clone` already give us the equivalent of
// `mtk_rectangle_copy`/`mtk_rectangle_free`, and there's no GObject type
// system here to register a boxed type with).
//
// `graphene_rect_t` interop (`mtk_rectangle_to_graphene_rect`,
// `mtk_rectangle_crop_and_scale`) requires the `graphene` floating point
// rectangle/vector library, which is not available here. Instead this
// module defines a minimal local `FloatRect` type sufficient to port
// `mtk_rectangle_from_graphene_rect` and `mtk_rectangle_scale_double`
// faithfully; `mtk_rectangle_crop_and_scale` is stubbed out with a TODO
// since it depends on `graphene_rect_scale`/`graphene_rect_offset`, which
// would need to be ported separately (or graphene linked) before this can
// be filled in.
//
// `mtk_rectangle_transform` depends on `MtkMonitorTransform`, which is a
// separate enum/module (mtk-monitor-transform.h) not ported here; it has
// been skipped. `mtk_rectangle_could_be_merged` does not exist in the
// current upstream mtk-rectangle.c (only `could_fit_rect` does) so there is
// nothing to port for it.

#![allow(dead_code)]

/// Strategy used when rounding a floating point rectangle to integer
/// coordinates. Mirrors `MtkRoundingStrategy`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RoundingStrategy {
    /// Round the rectangle inwards (origin away from zero via ceil, size via floor).
    #[default]
    Shrink,
    /// Round the rectangle outwards, growing it to fully contain the input.
    Grow,
    /// Round each component to the nearest integer.
    Round,
}

/// Minimal floating point rectangle, used only as the interop type for
/// `Rectangle::from_float_rect` / `Rectangle::scale_double`. This is a
/// local stand-in for `graphene_rect_t` (origin + size), since the real
/// graphene library is not available in this environment.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FloatRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl FloatRect {
    pub const fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        FloatRect {
            x,
            y,
            width,
            height,
        }
    }
}

/// Port of `MtkRectangle`: an integer rectangle with top-left origin
/// (x, y) and a (width, height) extent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rectangle {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rectangle {
    /// Port of `MTK_RECTANGLE_INIT` / `mtk_rectangle_new`.
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Rectangle {
            x,
            y,
            width,
            height,
        }
    }

    /// Port of `mtk_rectangle_new_empty`.
    pub const fn empty() -> Self {
        Rectangle {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }
    }

    /// Port of `mtk_rectangle_area`.
    pub fn area(&self) -> i32 {
        self.width * self.height
    }

    /// Port of `mtk_rectangle_equal`.
    pub fn equal(&self, other: &Rectangle) -> bool {
        self.x == other.x
            && self.y == other.y
            && self.width == other.width
            && self.height == other.height
    }

    /// Port of `mtk_rectangle_union`. Computes the bounding box of
    /// `self` and `other`.
    pub fn union(&self, other: &Rectangle) -> Rectangle {
        let mut dest_x = self.x;
        let mut dest_y = self.y;
        let mut dest_w = self.width;
        let mut dest_h = self.height;

        if other.x < dest_x {
            dest_w += dest_x - other.x;
            dest_x = other.x;
        }
        if other.y < dest_y {
            dest_h += dest_y - other.y;
            dest_y = other.y;
        }
        if other.x + other.width > dest_x + dest_w {
            dest_w = other.x + other.width - dest_x;
        }
        if other.y + other.height > dest_y + dest_h {
            dest_h = other.y + other.height - dest_y;
        }

        Rectangle {
            x: dest_x,
            y: dest_y,
            width: dest_w,
            height: dest_h,
        }
    }

    /// Port of `mtk_rectangle_intersect`. Returns `Some(intersection)` if
    /// the rectangles overlap with a non-degenerate (positive area)
    /// intersection, `None` otherwise (matching the C function's `dest`
    /// being zeroed-out width/height on failure, represented here by the
    /// absence of a value).
    pub fn intersect(&self, other: &Rectangle) -> Option<Rectangle> {
        let dest_x = self.x.max(other.x);
        let dest_y = self.y.max(other.y);
        let dest_w = (self.x + self.width).min(other.x + other.width) - dest_x;
        let dest_h = (self.y + self.height).min(other.y + other.height) - dest_y;

        if dest_w > 0 && dest_h > 0 {
            Some(Rectangle {
                x: dest_x,
                y: dest_y,
                width: dest_w,
                height: dest_h,
            })
        } else {
            None
        }
    }

    /// Port of `mtk_rectangle_overlap`. Note that a shared edge (e.g.
    /// `self` ending exactly where `other` begins) does NOT count as
    /// overlap, matching the C semantics (`<=` comparisons).
    pub fn overlap(&self, other: &Rectangle) -> bool {
        !(self.x + self.width <= other.x
            || other.x + other.width <= self.x
            || self.y + self.height <= other.y
            || other.y + other.height <= self.y)
    }

    /// Port of `mtk_rectangle_vert_overlap`.
    pub fn vert_overlap(&self, other: &Rectangle) -> bool {
        self.y < other.y + other.height && other.y < self.y + self.height
    }

    /// Port of `mtk_rectangle_horiz_overlap`.
    pub fn horiz_overlap(&self, other: &Rectangle) -> bool {
        self.x < other.x + other.width && other.x < self.x + self.width
    }

    /// Port of `mtk_rectangle_could_fit_rect`. Returns whether `inner`
    /// could fit inside `self` (only compares extents, not position).
    pub fn could_fit_rect(&self, inner: &Rectangle) -> bool {
        self.width >= inner.width && self.height >= inner.height
    }

    /// Port of `mtk_rectangle_contains_rect`. `self` is the outer
    /// rectangle, `inner` the candidate contained rectangle.
    pub fn contains_rect(&self, inner: &Rectangle) -> bool {
        inner.x >= self.x
            && inner.y >= self.y
            && inner.x + inner.width <= self.x + self.width
            && inner.y + inner.height <= self.y + self.height
    }

    /// Port of `mtk_rectangle_contains_point` (integer point overload).
    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        self.contains_pointf(x as f32, y as f32)
    }

    /// Port of `mtk_rectangle_contains_pointf`.
    pub fn contains_pointf(&self, x: f32, y: f32) -> bool {
        x >= self.x as f32
            && x < (self.x + self.width) as f32
            && y >= self.y as f32
            && y < (self.y + self.height) as f32
    }

    /// Port of `mtk_rectangle_to_graphene_rect`, using the local
    /// `FloatRect` stand-in for `graphene_rect_t`.
    pub fn to_float_rect(&self) -> FloatRect {
        FloatRect {
            x: self.x as f32,
            y: self.y as f32,
            width: self.width as f32,
            height: self.height as f32,
        }
    }

    /// Port of `mtk_rectangle_from_graphene_rect`, using the local
    /// `FloatRect` stand-in for `graphene_rect_t`.
    pub fn from_float_rect(rect: &FloatRect, rounding_strategy: RoundingStrategy) -> Rectangle {
        match rounding_strategy {
            RoundingStrategy::Shrink => Rectangle {
                x: ceilf(rect.x) as i32,
                y: ceilf(rect.y) as i32,
                width: floorf(rect.width) as i32,
                height: floorf(rect.height) as i32,
            },
            RoundingStrategy::Grow => {
                // graphene_rect_round_extents rounds the rectangle's
                // extents (x1/y1 down, x2/y2 up) and recomputes
                // width/height from them so the result fully contains
                // the original rect.
                let x1 = floorf(rect.x);
                let y1 = floorf(rect.y);
                let x2 = ceilf(rect.x + rect.width);
                let y2 = ceilf(rect.y + rect.height);

                Rectangle {
                    x: x1 as i32,
                    y: y1 as i32,
                    width: (x2 - x1) as i32,
                    height: (y2 - y1) as i32,
                }
            }
            RoundingStrategy::Round => Rectangle {
                x: roundf(rect.x) as i32,
                y: roundf(rect.y) as i32,
                width: roundf(rect.width) as i32,
                height: roundf(rect.height) as i32,
            },
        }
    }

    // TODO: Port of `mtk_rectangle_crop_and_scale` is intentionally
    // unimplemented. The original scales `rect` against `src_rect`
    // (a graphene_rect_t describing a source crop region) to fit a
    // `dst_width` x `dst_height` target, via `graphene_rect_scale` and
    // `graphene_rect_offset`. Porting it faithfully needs those graphene
    // primitives (or local equivalents) implemented and unit-tested
    // first; left as a stub since it is not pure self-contained geometry.

    /// Port of `mtk_rectangle_scale_double`. Scales `self` by `scale`
    /// (uniformly in both axes) and rounds the result per
    /// `rounding_strategy`.
    pub fn scale_double(&self, scale: f64, rounding_strategy: RoundingStrategy) -> Rectangle {
        let scale = scale as f32;
        let scaled = FloatRect {
            x: self.x as f32 * scale,
            y: self.y as f32 * scale,
            width: self.width as f32 * scale,
            height: self.height as f32 * scale,
        };
        Rectangle::from_float_rect(&scaled, rounding_strategy)
    }

    /// Port of `mtk_rectangle_is_adjacent_to`.
    pub fn is_adjacent_to(&self, other: &Rectangle) -> bool {
        let rect_x1 = self.x;
        let rect_y1 = self.y;
        let rect_x2 = self.x + self.width;
        let rect_y2 = self.y + self.height;
        let other_x1 = other.x;
        let other_y1 = other.y;
        let other_x2 = other.x + other.width;
        let other_y2 = other.y + other.height;

        if (rect_x1 == other_x2 || rect_x2 == other_x1)
            && !(rect_y2 <= other_y1 || rect_y1 >= other_y2)
        {
            true
        } else {
            (rect_y1 == other_y2 || rect_y2 == other_y1)
                && !(rect_x2 <= other_x1 || rect_x1 >= other_x2)
        }
    }

    /// Port of `mtk_rectangle_is_empty`.
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    // `mtk_rectangle_transform` is skipped: it depends on
    // `MtkMonitorTransform`, a separate enum/module
    // (mtk/mtk-monitor-transform.h) not in scope for this port.
}

// Minimal no_std-friendly float helpers (core::f32 doesn't expose
// ceil/floor/round without `std` or `libm`; implement them directly so
// this module has zero external dependencies beyond core/alloc).
fn floorf(v: f32) -> f32 {
    let truncated = v as i64 as f32;
    if v < truncated {
        truncated - 1.0
    } else {
        truncated
    }
}

fn ceilf(v: f32) -> f32 {
    let truncated = v as i64 as f32;
    if v > truncated {
        truncated + 1.0
    } else {
        truncated
    }
}

fn roundf(v: f32) -> f32 {
    if v >= 0.0 {
        floorf(v + 0.5)
    } else {
        ceilf(v - 0.5)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_init() {
        let rect = Rectangle::new(1, 2, 3, 4);
        assert_eq!(rect.x, 1);
        assert_eq!(rect.y, 2);
        assert_eq!(rect.width, 3);
        assert_eq!(rect.height, 4);
    }

    #[test]
    fn test_area() {
        let rect = Rectangle::new(0, 0, 5, 7);
        assert_eq!(rect.area(), 35);
    }

    #[test]
    fn test_equal() {
        let a = Rectangle::new(100, 200, 50, 40);
        let b = Rectangle::new(100, 200, 50, 40);
        let c = Rectangle::new(100, 200, 10, 50);
        assert!(a.equal(&b));
        assert!(!a.equal(&c));
    }

    #[test]
    fn test_intersect_disjoint() {
        let a = Rectangle::new(0, 0, 10, 10);
        let b = Rectangle::new(20, 0, 10, 5);
        assert_eq!(a.intersect(&b), None);
        assert!(!a.overlap(&b));
    }

    #[test]
    fn test_intersect_overlapping() {
        let a = Rectangle::new(0, 0, 10, 10);
        let b = Rectangle::new(5, 5, 10, 10);
        let result = a.intersect(&b).unwrap();
        assert_eq!(result, Rectangle::new(5, 5, 5, 5));
        assert!(a.overlap(&b));
    }

    #[test]
    fn test_overlap_touching_edges_is_false() {
        // Sharing only an edge does not count as overlapping.
        let a = Rectangle::new(0, 0, 10, 10);
        let b = Rectangle::new(10, 0, 10, 10);
        assert!(!a.overlap(&b));
        assert!(a.intersect(&b).is_none());
    }

    #[test]
    fn test_union() {
        let a = Rectangle::new(0, 0, 10, 10);
        let b = Rectangle::new(5, 5, 10, 10);
        assert_eq!(a.union(&b), Rectangle::new(0, 0, 15, 15));
    }

    #[test]
    fn test_could_fit_rect() {
        let outer = Rectangle::new(100, 200, 10, 20);
        let inner = Rectangle::new(0, 0, 10, 20);
        let too_big = Rectangle::new(0, 0, 11, 20);
        assert!(outer.could_fit_rect(&inner));
        assert!(!outer.could_fit_rect(&too_big));
    }

    #[test]
    fn test_contains_rect() {
        let outer = Rectangle::new(0, 0, 10, 10);
        let inner = Rectangle::new(2, 2, 4, 4);
        let outside = Rectangle::new(8, 8, 4, 4);
        assert!(outer.contains_rect(&inner));
        assert!(!outer.contains_rect(&outside));
    }

    #[test]
    fn test_contains_point() {
        let rect = Rectangle::new(10, 12, 4, 18);
        assert!(rect.contains_point(10, 12));
        assert!(rect.contains_point(13, 29));
        // Right/bottom edges are exclusive.
        assert!(!rect.contains_point(14, 12));
        assert!(!rect.contains_point(10, 30));
    }

    #[test]
    fn test_is_adjacent_to() {
        let rect = Rectangle::new(0, 0, 10, 10);

        // Adjacent on the right edge, overlapping in y.
        assert!(rect.is_adjacent_to(&Rectangle::new(10, 5, 10, 10)));
        // Adjacent on the right edge, but not overlapping in y at all.
        assert!(!rect.is_adjacent_to(&Rectangle::new(10, 20, 10, 10)));
        // Not touching at all.
        assert!(!rect.is_adjacent_to(&Rectangle::new(20, 20, 10, 10)));
        // Adjacent below.
        assert!(rect.is_adjacent_to(&Rectangle::new(5, 10, 10, 10)));
    }

    #[test]
    fn test_is_empty() {
        assert!(Rectangle::new(0, 0, 0, 10).is_empty());
        assert!(Rectangle::new(0, 0, 10, 0).is_empty());
        assert!(!Rectangle::new(0, 0, 10, 10).is_empty());
    }

    #[test]
    fn test_vert_horiz_overlap() {
        let a = Rectangle::new(0, 0, 10, 10);
        let b = Rectangle::new(5, 5, 10, 10);
        let c = Rectangle::new(20, 20, 10, 10);
        assert!(a.vert_overlap(&b));
        assert!(a.horiz_overlap(&b));
        assert!(!a.vert_overlap(&c));
        assert!(!a.horiz_overlap(&c));
    }

    #[test]
    fn test_from_float_rect_shrink() {
        let f = FloatRect::new(1.2, 1.8, 5.9, 5.1);
        let r = Rectangle::from_float_rect(&f, RoundingStrategy::Shrink);
        // ceil(1.2)=2, ceil(1.8)=2, floor(5.9)=5, floor(5.1)=5
        assert_eq!(r, Rectangle::new(2, 2, 5, 5));
    }

    #[test]
    fn test_from_float_rect_round() {
        let f = FloatRect::new(1.4, 1.6, 5.5, 5.4);
        let r = Rectangle::from_float_rect(&f, RoundingStrategy::Round);
        assert_eq!(r, Rectangle::new(1, 2, 6, 5));
    }

    #[test]
    fn test_from_float_rect_grow() {
        // Rect spans [1.2, 6.1) in x and [1.8, 5.9) in y; grow should
        // fully contain it: x in [1, 7), y in [1, 6).
        let f = FloatRect::new(1.2, 1.8, 4.9, 4.1);
        let r = Rectangle::from_float_rect(&f, RoundingStrategy::Grow);
        assert_eq!(r, Rectangle::new(1, 1, 6, 5));
    }

    #[test]
    fn test_scale_double() {
        let rect = Rectangle::new(0, 0, 10, 10);
        let scaled = rect.scale_double(2.0, RoundingStrategy::Round);
        assert_eq!(scaled, Rectangle::new(0, 0, 20, 20));
    }
}
