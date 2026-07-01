//! Bridge between the kernel's `graphics::framebuffer::Rect` (unsigned
//! `usize` geometry) and `mutter_port::mtk::rectangle::Rectangle` (signed
//! `i32` geometry, the mutter/MTK canonical type).
//!
//! This is the integration seam that lets desktop/compositor code use the
//! ported MTK rectangle algebra (`intersect`, `union`, `contains_rect`,
//! `contains_point`) on the kernel's native `Rect` without changing every
//! desktop struct to use `i32` fields. Conversions are lossless for all
//! non-negative values that fit in `i32` (screens up to ~2 billion px).
//!
//! # What's provided
//!
//! - `rect_to_mtk` / `rect_from_mtk`: lossless `Rect` ↔ `Rectangle`
//!   conversions. `rect_from_mtk` clamps negative results to zero (a
//!   negative x/y/width/height from an MTK op has no kernel-side
//!   representation).
//! - `clamp_rect_to_work_area`: the desktop's
//!   `placement::clamp_rect_to_work_area` reimplemented via
//!   `Rectangle::intersect`, eliminating the duplicated saturating
//!   arithmetic. This is the direct wiring point.
//! - `rects_intersect`: `Rect`-typed wrapper over `Rectangle::intersect`
//!   returning a bool, matching `Rect::intersects` but going through the
//!   MTK implementation so both sides stay in sync.
//! - `rect_contains_rect`: `Rect`-typed wrapper over
//!   `Rectangle::contains_rect`.
//! - `rect_contains_point`: `Rect`-typed wrapper over
//!   `Rectangle::contains_point` (i32 coordinates).
//!
//! # Rationale
//!
//! `mutter_port` is deliberately standalone (no kernel imports) so it can
//! be checked and tested in isolation. This module is the *only* place
//! that imports both `graphics::framebuffer` and `mutter_port`, keeping
//! the dependency arrow pointing one way (kernel → mutter_port, never the
//! reverse).

use crate::graphics::framebuffer::Rect;
use crate::mutter_port::mtk::rectangle::Rectangle;

/// Convert a kernel `Rect` (`usize` fields) to an MTK `Rectangle` (`i32`).
///
/// Lossless for all values that fit in `i32` (screens up to ~2 billion
/// pixels per axis). Values exceeding `i32::MAX` are clamped to
/// `i32::MAX`, which is safe because no real screen dimension approaches
/// that.
pub fn rect_to_mtk(r: Rect) -> Rectangle {
    Rectangle::new(
        clamp_usize_to_i32(r.x),
        clamp_usize_to_i32(r.y),
        clamp_usize_to_i32(r.width),
        clamp_usize_to_i32(r.height),
    )
}

/// Convert an MTK `Rectangle` (`i32` fields) to a kernel `Rect` (`usize`).
///
/// Negative values are clamped to zero (a negative x/y/width/height has no
/// kernel-side representation). This matches the semantics of the desktop's
/// existing `saturating_add`/`saturating_sub` arithmetic, which never
/// produces negative values.
pub fn rect_from_mtk(r: Rectangle) -> Rect {
    Rect::new(
        r.x.max(0) as usize,
        r.y.max(0) as usize,
        r.width.max(0) as usize,
        r.height.max(0) as usize,
    )
}

/// Clamp a `usize` to the `i32` range. Used by `rect_to_mtk`.
fn clamp_usize_to_i32(v: usize) -> i32 {
    if v > i32::MAX as usize {
        i32::MAX
    } else {
        v as i32
    }
}

/// Clamp a rectangle to fit inside a work area. This is equivalent to the
/// desktop's `placement::clamp_rect_to_work_area` but implemented via
/// `Rectangle::intersect`, so the geometry logic lives in one place
/// (the ported MTK code) rather than being duplicated.
///
/// Returns the intersection of `rect` and `work_area`. If they don't
/// intersect, the result is a zero-area rect at the work-area clamp
/// position (matching the original saturating-arithmetic behavior: the
/// original code clamps x/y into `[work_area.x, max_x]` and caps
/// width/height to `work_area.width`/`height`, which for a non-overlapping
/// rect produces a zero-width or zero-height rect at the work-area edge).
pub fn clamp_rect_to_work_area(rect: Rect, work_area: Rect) -> Rect {
    let mtk_rect = rect_to_mtk(rect);
    let mtk_work = rect_to_mtk(work_area);
    match mtk_rect.intersect(&mtk_work) {
        Some(intersection) => rect_from_mtk(intersection),
        // No intersection: mirror the original behavior of clamping x/y
        // into the work area and capping size. The original code would
        // produce a rect at the nearest work-area edge with the (capped)
        // requested size; the simplest faithful equivalent is to clamp
        // the origin into the work area and cap the size.
        None => {
            let max_x = work_area
                .x
                .saturating_add(work_area.width.saturating_sub(rect.width));
            let max_y = work_area
                .y
                .saturating_add(work_area.height.saturating_sub(rect.height));
            Rect::new(
                rect.x.clamp(work_area.x, max_x),
                rect.y.clamp(work_area.y, max_y),
                rect.width.min(work_area.width),
                rect.height.min(work_area.height),
            )
        }
    }
}

/// Check whether two kernel `Rect`s intersect, via the MTK
/// `Rectangle::intersect` implementation. Equivalent to
/// `Rect::intersects` but routed through the ported MTK code so both
/// sides share the same geometry logic.
pub fn rects_intersect(a: Rect, b: Rect) -> bool {
    rect_to_mtk(a).intersect(&rect_to_mtk(b)).is_some()
}

/// Check whether `inner` is fully contained inside `outer`, via the MTK
/// `Rectangle::contains_rect` implementation.
pub fn rect_contains_rect(outer: Rect, inner: Rect) -> bool {
    rect_to_mtk(outer).contains_rect(&rect_to_mtk(inner))
}

/// Check whether a point `(x, y)` is inside `rect`, via the MTK
/// `Rectangle::contains_point` implementation.
pub fn rect_contains_point(rect: Rect, x: i32, y: i32) -> bool {
    rect_to_mtk(rect).contains_point(x, y)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_losslessly() {
        let r = Rect::new(100, 200, 300, 400);
        let mtk = rect_to_mtk(r);
        let back = rect_from_mtk(mtk);
        assert_eq!(r, back);
    }

    #[test]
    fn clamp_to_work_area_intersects() {
        let rect = Rect::new(50, 50, 200, 200);
        let work = Rect::new(100, 100, 200, 200);
        let clamped = clamp_rect_to_work_area(rect, work);
        // Intersection of [50,250)x[50,250) and [100,300)x[100,300) is
        // [100,250)x[100,300) -> x=100,y=100,w=150,h=200.
        assert_eq!(clamped, Rect::new(100, 100, 150, 200));
    }

    #[test]
    fn clamp_oversized_rect_caps_size() {
        let rect = Rect::new(0, 0, 640, 480);
        let work = Rect::new(0, 30, 320, 200);
        let clamped = clamp_rect_to_work_area(rect, work);
        // Intersection: [0,640)x[0,480) ∩ [0,320)x[30,230) = [0,320)x[30,230).
        assert_eq!(clamped, Rect::new(0, 30, 320, 200));
    }

    #[test]
    fn clamp_non_overlapping_clamps_to_edge() {
        let rect = Rect::new(500, 500, 100, 100);
        let work = Rect::new(0, 0, 320, 200);
        let clamped = clamp_rect_to_work_area(rect, work);
        // No intersection; original behavior clamps origin into work area
        // and caps size. max_x = 0 + (320 - 100) = 220, so x clamps to 220.
        // max_y = 0 + (200 - 100) = 100, so y clamps to 100. Size capped to
        // work area size (100 < 320, 100 < 200, so stays 100).
        assert_eq!(clamped, Rect::new(220, 100, 100, 100));
    }

    #[test]
    fn rects_intersect_matches_framebuffer() {
        let a = Rect::new(0, 0, 100, 100);
        let b = Rect::new(50, 50, 100, 100);
        let c = Rect::new(200, 200, 50, 50);
        assert!(rects_intersect(a, b));
        assert!(!rects_intersect(a, c));
        // Consistency with the kernel's own Rect::intersects.
        assert_eq!(rects_intersect(a, b), a.intersects(&b));
        assert_eq!(rects_intersect(a, c), a.intersects(&c));
    }

    #[test]
    fn rect_contains_rect_works() {
        let outer = Rect::new(0, 0, 100, 100);
        let inner = Rect::new(10, 10, 50, 50);
        let outside = Rect::new(50, 50, 100, 100);
        assert!(rect_contains_rect(outer, inner));
        assert!(!rect_contains_rect(outer, outside));
    }

    #[test]
    fn rect_contains_point_works() {
        let r = Rect::new(10, 20, 100, 50);
        assert!(rect_contains_point(r, 50, 40));
        assert!(!rect_contains_point(r, 5, 40));
        assert!(!rect_contains_point(r, 200, 40));
    }

    #[test]
    fn negative_mtk_clamps_to_zero() {
        let mtk = Rectangle::new(-10, -20, 50, 50);
        let r = rect_from_mtk(mtk);
        assert_eq!(r, Rect::new(0, 0, 50, 50));
    }
}
