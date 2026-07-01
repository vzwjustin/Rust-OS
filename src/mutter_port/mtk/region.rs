// Port of mutter's mtk/mtk/mtk-region.c (MtkRegion) to Rust.
//
// Upstream `MtkRegion` is a thin wrapper around pixman's `pixman_region32_t`,
// which represents an arbitrary 2D area as a sorted, banded list of
// non-overlapping rectangles and implements region algebra (union, subtract,
// intersect, ...) with a scanline algorithm.
//
// We have no pixman in this no_std-ish kernel, so this module reimplements
// the region as a plain `Vec<Rectangle>` of non-overlapping rectangles and
// performs the algebra by direct rectangle splitting. This is asymptotically
// worse than pixman's scanline approach but is simple, allocation-only (no
// unsafe), and produces an equivalent (though not necessarily identically
// ordered/merged) set of non-overlapping rectangles for every operation.
//
// NOTE: `Rectangle` here is a local minimal stand-in. The task description
// pointed at `src/mutter_port/mtk/rectangle.rs` as the canonical definition
// ported by a parallel agent, but that file does not exist in this checkout
// at the time of writing (the whole `src/mutter_port/` tree was created by
// this port). Once the real `Rectangle` type lands, this local definition
// should be deleted and replaced with `use super::rectangle::Rectangle;`
// (field names/types should match: x, y as i32, width/height as i32).

use super::rectangle::Rectangle;
use alloc::vec::Vec;

impl Rectangle {
    #[inline]
    fn x2(&self) -> i32 {
        self.x + self.width
    }

    #[inline]
    fn y2(&self) -> i32 {
        self.y + self.height
    }

    /// Returns the intersection of `self` and `other`, or `None` if they
    /// don't overlap (or the result would be empty).
    fn intersection(&self, other: &Rectangle) -> Option<Rectangle> {
        let x1 = self.x.max(other.x);
        let y1 = self.y.max(other.y);
        let x2 = self.x2().min(other.x2());
        let y2 = self.y2().min(other.y2());
        if x2 > x1 && y2 > y1 {
            Some(Rectangle::new(x1, y1, x2 - x1, y2 - y1))
        } else {
            None
        }
    }

    fn overlaps(&self, other: &Rectangle) -> bool {
        self.x < other.x2() && other.x < self.x2() && self.y < other.y2() && other.y < self.y2()
    }

    /// Splits `self` into the (up to 4) pieces of `self` that do not overlap
    /// `cut`, pushing them into `out`. Used as the core primitive of
    /// subtraction.
    fn subtract_one(&self, cut: &Rectangle, out: &mut Vec<Rectangle>) {
        if !self.overlaps(cut) {
            out.push(*self);
            return;
        }

        let (sx1, sy1, sx2, sy2) = (self.x, self.y, self.x2(), self.y2());
        let (cx1, cy1, cx2, cy2) = (cut.x, cut.y, cut.x2(), cut.y2());

        // Top strip: full width, above the cut.
        if sy1 < cy1 {
            out.push(Rectangle::new(sx1, sy1, sx2 - sx1, cy1.min(sy2) - sy1));
        }
        // Bottom strip: full width, below the cut.
        if sy2 > cy2 {
            out.push(Rectangle::new(
                sx1,
                cy2.max(sy1),
                sx2 - sx1,
                sy2 - cy2.max(sy1),
            ));
        }

        // Middle band: same y-range as the overlap, left/right slivers only.
        let mid_y1 = sy1.max(cy1);
        let mid_y2 = sy2.min(cy2);
        if mid_y2 > mid_y1 {
            if sx1 < cx1 {
                out.push(Rectangle::new(
                    sx1,
                    mid_y1,
                    cx1.min(sx2) - sx1,
                    mid_y2 - mid_y1,
                ));
            }
            if sx2 > cx2 {
                out.push(Rectangle::new(
                    cx2.max(sx1),
                    mid_y1,
                    sx2 - cx2.max(sx1),
                    mid_y2 - mid_y1,
                ));
            }
        }
    }
}

/// Overlap classification result for [`Region::contains_rectangle`],
/// mirroring `MtkRegionOverlap` / pixman's `pixman_region_overlap_t`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionOverlap {
    Out,
    In,
    Part,
}

/// Port of `MtkRegion`: an area described by a set of non-overlapping
/// rectangles.
///
/// Invariant maintained by all public mutators: `rects` contains no empty
/// rectangles and no two rectangles overlap. Rectangles are *not* required
/// to be merged into maximal spans (e.g. two horizontally-adjacent
/// same-height rectangles may remain separate instead of being coalesced
/// into one) -- this matches "an equivalent set of non-overlapping
/// rectangles" rather than pixman's exact banded/merged output.
#[derive(Debug, Clone, Default)]
pub struct Region {
    rects: Vec<Rectangle>,
}

impl Region {
    /// `mtk_region_create`
    pub fn create() -> Self {
        Region { rects: Vec::new() }
    }

    /// `mtk_region_create_rectangle`
    pub fn create_rectangle(rect: &Rectangle) -> Self {
        let mut region = Region::create();
        if !rect.is_empty() {
            region.rects.push(*rect);
        }
        region
    }

    /// `mtk_region_create_rectangles`
    pub fn create_rectangles(rects: &[Rectangle]) -> Self {
        let mut region = Region::create();
        for r in rects {
            region.union_rectangle(r);
        }
        region
    }

    /// `mtk_region_copy`
    pub fn copy(&self) -> Self {
        self.clone()
    }

    /// `mtk_region_equal`
    ///
    /// Since rectangle order/merging is not canonicalized, we compare by
    /// normalizing both sides (same total area covered, same cell
    /// membership) rather than requiring identical `rects` vectors. We do
    /// this cheaply by checking each rectangle of `self` is fully covered by
    /// `other` and vice versa, and that areas match.
    pub fn equal(&self, other: &Region) -> bool {
        if core::ptr::eq(self, other) {
            return true;
        }
        if self.is_empty() && other.is_empty() {
            return true;
        }
        if self.total_area() != other.total_area() {
            return false;
        }
        self.rects.iter().all(|r| other.fully_contains(r))
            && other.rects.iter().all(|r| self.fully_contains(r))
    }

    fn total_area(&self) -> i64 {
        self.rects
            .iter()
            .map(|r| (r.width as i64) * (r.height as i64))
            .sum()
    }

    /// Whether `rect` is entirely covered by this region (used internally by
    /// `equal` and `contains_rectangle`).
    fn fully_contains(&self, rect: &Rectangle) -> bool {
        if rect.is_empty() {
            return true;
        }
        let covered = Region::create_rectangle(rect).subtracted_by(self);
        covered.is_empty()
    }

    fn subtracted_by(&self, other: &Region) -> Region {
        let mut result = self.clone();
        result.subtract(other);
        result
    }

    /// `mtk_region_is_empty`
    pub fn is_empty(&self) -> bool {
        self.rects.is_empty()
    }

    /// `mtk_region_get_extents`: bounding box of all rectangles, or an empty
    /// `Rectangle` (0,0,0,0) if the region is empty.
    pub fn get_extents(&self) -> Rectangle {
        let mut iter = self.rects.iter();
        let first = match iter.next() {
            Some(r) => *r,
            None => return Rectangle::default(),
        };
        let mut x1 = first.x;
        let mut y1 = first.y;
        let mut x2 = first.x2();
        let mut y2 = first.y2();
        for r in iter {
            x1 = x1.min(r.x);
            y1 = y1.min(r.y);
            x2 = x2.max(r.x2());
            y2 = y2.max(r.y2());
        }
        Rectangle::new(x1, y1, x2 - x1, y2 - y1)
    }

    /// `mtk_region_num_rectangles`
    pub fn num_rectangles(&self) -> usize {
        self.rects.len()
    }

    /// `mtk_region_get_rectangle`
    pub fn get_rectangle(&self, nth: usize) -> Rectangle {
        self.rects[nth]
    }

    /// `mtk_region_translate`
    pub fn translate(&mut self, dx: i32, dy: i32) {
        for r in &mut self.rects {
            r.x += dx;
            r.y += dy;
        }
    }

    /// `mtk_region_scale` (upstream `mtk_region_scale`, simple multiply).
    pub fn scale(&mut self, scale: i32) {
        if scale == 1 {
            return;
        }
        for r in &mut self.rects {
            r.x *= scale;
            r.y *= scale;
            r.width *= scale;
            r.height *= scale;
        }
    }

    /// `mtk_region_contains_point`
    pub fn contains_point(&self, x: i32, y: i32) -> bool {
        self.rects
            .iter()
            .any(|r| x >= r.x && x < r.x2() && y >= r.y && y < r.y2())
    }

    /// `mtk_region_union_rectangle`
    pub fn union_rectangle(&mut self, rect: &Rectangle) {
        if rect.is_empty() {
            return;
        }
        // Remove any existing overlap with `rect`, then add `rect` itself.
        // This guarantees the non-overlap invariant without needing to
        // merge adjacent spans.
        let mut new_rects = Vec::with_capacity(self.rects.len() + 1);
        for r in &self.rects {
            r.subtract_one(rect, &mut new_rects);
        }
        new_rects.push(*rect);
        self.rects = new_rects;
    }

    /// `mtk_region_union`
    pub fn union_region(&mut self, other: &Region) {
        for r in &other.rects {
            self.union_rectangle(r);
        }
    }

    /// `mtk_region_subtract_rectangle`
    pub fn subtract_rectangle(&mut self, rect: &Rectangle) {
        if rect.is_empty() || self.rects.is_empty() {
            return;
        }
        let mut new_rects = Vec::with_capacity(self.rects.len());
        for r in &self.rects {
            r.subtract_one(rect, &mut new_rects);
        }
        self.rects = new_rects;
    }

    /// `mtk_region_subtract`
    pub fn subtract(&mut self, other: &Region) {
        for r in &other.rects {
            self.subtract_rectangle(r);
        }
    }

    /// `mtk_region_intersect_rectangle`
    pub fn intersect_rectangle(&mut self, rect: &Rectangle) {
        let mut new_rects = Vec::with_capacity(self.rects.len());
        for r in &self.rects {
            if let Some(i) = r.intersection(rect) {
                new_rects.push(i);
            }
        }
        self.rects = new_rects;
    }

    /// `mtk_region_intersect`
    pub fn intersect_region(&mut self, other: &Region) {
        if other.rects.is_empty() {
            self.rects.clear();
            return;
        }
        let mut new_rects = Vec::new();
        for r in &self.rects {
            for o in &other.rects {
                if let Some(i) = r.intersection(o) {
                    new_rects.push(i);
                }
            }
        }
        self.rects = new_rects;
    }

    /// `mtk_region_contains_rectangle`
    pub fn contains_rectangle(&self, rect: &Rectangle) -> RegionOverlap {
        if rect.is_empty() {
            return RegionOverlap::Out;
        }
        if self.fully_contains(rect) {
            return RegionOverlap::In;
        }
        let any_overlap = self.rects.iter().any(|r| r.overlaps(rect));
        if any_overlap {
            RegionOverlap::Part
        } else {
            RegionOverlap::Out
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: i32, y: i32, w: i32, h: i32) -> Rectangle {
        Rectangle::new(x, y, w, h)
    }

    #[test]
    fn create_and_extents() {
        let region = Region::create_rectangle(&r(10, 10, 20, 30));
        assert_eq!(region.num_rectangles(), 1);
        assert_eq!(region.get_extents(), r(10, 10, 20, 30));
        assert!(!region.is_empty());
    }

    #[test]
    fn empty_region() {
        let region = Region::create();
        assert!(region.is_empty());
        assert_eq!(region.get_extents(), Rectangle::default());
        assert_eq!(region.num_rectangles(), 0);
    }

    #[test]
    fn union_disjoint() {
        let mut region = Region::create_rectangle(&r(0, 0, 10, 10));
        region.union_rectangle(&r(20, 0, 10, 10));
        assert_eq!(region.num_rectangles(), 2);
        assert_eq!(region.get_extents(), r(0, 0, 30, 10));
        assert!(region.contains_point(5, 5));
        assert!(region.contains_point(25, 5));
        assert!(!region.contains_point(15, 5));
    }

    #[test]
    fn union_overlapping_covers_full_area() {
        let mut a = Region::create_rectangle(&r(0, 0, 10, 10));
        let b = Region::create_rectangle(&r(5, 5, 10, 10));
        a.union_region(&b);
        // Total area should equal the area of the union (150), not the sum
        // of the two rectangles (200), since they overlap by 25.
        assert_eq!(a.total_area(), 175);
        assert_eq!(a.get_extents(), r(0, 0, 15, 15));
        assert!(a.contains_point(0, 0));
        assert!(a.contains_point(14, 14));
        assert!(!a.contains_point(15, 15));
    }

    #[test]
    fn subtract_rectangle_punches_hole() {
        let mut region = Region::create_rectangle(&r(0, 0, 10, 10));
        region.subtract_rectangle(&r(2, 2, 4, 4));
        assert!(region.contains_point(0, 0));
        assert!(!region.contains_point(3, 3));
        assert!(region.contains_point(9, 9));
        // Area should be 100 - 16 = 84.
        assert_eq!(region.total_area(), 84);
    }

    #[test]
    fn subtract_no_overlap_is_noop() {
        let mut region = Region::create_rectangle(&r(0, 0, 10, 10));
        region.subtract_rectangle(&r(20, 20, 5, 5));
        assert_eq!(region.total_area(), 100);
    }

    #[test]
    fn subtract_region() {
        let mut a = Region::create();
        a.union_rectangle(&r(0, 0, 10, 10));
        a.union_rectangle(&r(20, 0, 10, 10));
        let mut b = Region::create();
        b.union_rectangle(&r(5, 5, 10, 10)); // overlaps only first rect
        a.subtract(&b);
        assert!(!a.contains_point(7, 7));
        assert!(a.contains_point(25, 5));
        assert!(a.contains_point(0, 0));
    }

    #[test]
    fn intersect_rectangle() {
        let mut region = Region::create_rectangle(&r(0, 0, 10, 10));
        region.intersect_rectangle(&r(5, 5, 10, 10));
        assert_eq!(region.get_extents(), r(5, 5, 5, 5));
        assert_eq!(region.total_area(), 25);
    }

    #[test]
    fn intersect_rectangle_disjoint_is_empty() {
        let mut region = Region::create_rectangle(&r(0, 0, 10, 10));
        region.intersect_rectangle(&r(20, 20, 5, 5));
        assert!(region.is_empty());
    }

    #[test]
    fn intersect_region() {
        let mut a = Region::create();
        a.union_rectangle(&r(0, 0, 10, 10));
        a.union_rectangle(&r(20, 0, 10, 10));
        let b = Region::create_rectangle(&r(5, 0, 20, 10));
        a.intersect_region(&b);
        // Overlaps: [5,0,5,10] from first rect, [20,0,5,10] from second.
        assert_eq!(a.total_area(), 100);
        assert!(a.contains_point(7, 5));
        assert!(a.contains_point(22, 5));
        assert!(!a.contains_point(2, 5));
    }

    #[test]
    fn contains_rectangle_classification() {
        let region = Region::create_rectangle(&r(0, 0, 10, 10));
        assert_eq!(region.contains_rectangle(&r(2, 2, 2, 2)), RegionOverlap::In);
        assert_eq!(
            region.contains_rectangle(&r(5, 5, 10, 10)),
            RegionOverlap::Part
        );
        assert_eq!(
            region.contains_rectangle(&r(20, 20, 5, 5)),
            RegionOverlap::Out
        );
    }

    #[test]
    fn translate_moves_all_rects() {
        let mut region = Region::create();
        region.union_rectangle(&r(0, 0, 10, 10));
        region.union_rectangle(&r(20, 0, 10, 10));
        region.translate(5, 7);
        assert_eq!(region.get_extents(), r(5, 7, 30, 10));
    }

    #[test]
    fn scale_multiplies_all_dims() {
        let mut region = Region::create_rectangle(&r(1, 2, 3, 4));
        region.scale(2);
        assert_eq!(region.get_extents(), r(2, 4, 6, 8));
    }

    #[test]
    fn equal_ignores_rect_ordering_and_splitting() {
        let mut a = Region::create();
        a.union_rectangle(&r(0, 0, 10, 10));

        // Build the same area as two adjacent halves so the rect list
        // differs from `a` but the covered area is identical.
        let mut b = Region::create();
        b.union_rectangle(&r(0, 0, 5, 10));
        b.union_rectangle(&r(5, 0, 5, 10));

        assert!(a.equal(&b));

        let c = Region::create_rectangle(&r(0, 0, 10, 9));
        assert!(!a.equal(&c));
        let _ = c.equal(&a); // exercise both directions without panics
    }

    #[test]
    fn create_rectangles_dedupes_overlap() {
        let region = Region::create_rectangles(&[r(0, 0, 10, 10), r(5, 5, 10, 10)]);
        assert_eq!(region.total_area(), 175);
    }
}
