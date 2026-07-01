//! Port of GNOME mutter's `clutter/clutter-paint-volume.{c,h}`.
//!
//! `ClutterPaintVolume` is an axis-aligned or oriented bounding box
//! in actor-local coordinates, used for efficient culling and damage
//! tracking during paint traversal. Vertices are lazily computed: only
//! vertices 0, 1, 3, and 4 are updated by setters; the remaining four
//! are derived on-demand via `complete()`.
//!
//! Skipped:
//! - GObject boxed-type machinery (`G_DEFINE_BOXED_TYPE`, refcounting).
//! - Actor pointer: this port has no object system. Pass actor context
//!   separately if needed.
//! - `graphene_point3d_t`: use `Point3D` struct locally.
//! - Transformation functions (`_clutter_paint_volume_project`,
//!   `_clutter_paint_volume_transform`, `_clutter_paint_volume_cull`):
//!   require matrix/frustum types not ported here.
//! - Stage paint-box functions: require `ClutterStage` context.

use super::actor_box::ActorBox;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Point3D {
    pub fn new(x: f32, y: f32, z: f32) -> Self {
        Point3D { x, y, z }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct PaintVolume {
    vertices: [Point3D; 8],
    is_empty: bool,
    is_axis_aligned: bool,
    is_complete: bool,
    is_2d: bool,
}

impl PaintVolume {
    pub fn new() -> Self {
        PaintVolume {
            vertices: [Point3D::new(0.0, 0.0, 0.0); 8],
            is_empty: true,
            is_axis_aligned: true,
            is_complete: true,
            is_2d: true,
        }
    }

    pub fn from_paint_volume(src: &PaintVolume) -> Self {
        *src
    }

    pub fn set_origin(&mut self, origin: Point3D) {
        let dx = origin.x - self.vertices[0].x;
        let dy = origin.y - self.vertices[0].y;
        let dz = origin.z - self.vertices[0].z;

        let key_vertices = [0, 1, 3, 4];
        for &i in &key_vertices {
            self.vertices[i].x += dx;
            self.vertices[i].y += dy;
            self.vertices[i].z += dz;
        }

        self.is_complete = false;
    }

    pub fn get_origin(&self) -> Point3D {
        self.vertices[0]
    }

    pub fn set_width(&mut self, width: f32) {
        if self.is_empty {
            self.vertices[1] = self.vertices[0];
            self.vertices[3] = self.vertices[0];
            self.vertices[4] = self.vertices[0];
        }

        if !self.is_axis_aligned {
            self.axis_align();
        }

        let right_xpos = self.vertices[0].x + width;
        self.vertices[1].x = right_xpos;

        self.is_complete = false;
        self.update_is_empty();
    }

    pub fn get_width(&self) -> f32 {
        if self.is_empty {
            return 0.0;
        }

        if !self.is_axis_aligned {
            let mut tmp = *self;
            tmp.axis_align();
            return tmp.vertices[1].x - tmp.vertices[0].x;
        }

        self.vertices[1].x - self.vertices[0].x
    }

    pub fn set_height(&mut self, height: f32) {
        if self.is_empty {
            self.vertices[1] = self.vertices[0];
            self.vertices[3] = self.vertices[0];
            self.vertices[4] = self.vertices[0];
        }

        if !self.is_axis_aligned {
            self.axis_align();
        }

        let height_ypos = self.vertices[0].y + height;
        self.vertices[3].y = height_ypos;

        self.is_complete = false;
        self.update_is_empty();
    }

    pub fn get_height(&self) -> f32 {
        if self.is_empty {
            return 0.0;
        }

        if !self.is_axis_aligned {
            let mut tmp = *self;
            tmp.axis_align();
            return tmp.vertices[3].y - tmp.vertices[0].y;
        }

        self.vertices[3].y - self.vertices[0].y
    }

    pub fn set_depth(&mut self, depth: f32) {
        if self.is_empty {
            self.vertices[1] = self.vertices[0];
            self.vertices[3] = self.vertices[0];
            self.vertices[4] = self.vertices[0];
        }

        if !self.is_axis_aligned {
            self.axis_align();
        }

        let depth_zpos = self.vertices[0].z + depth;
        self.vertices[4].z = depth_zpos;

        self.is_complete = false;
        self.is_2d = depth == 0.0;
        self.update_is_empty();
    }

    pub fn get_depth(&self) -> f32 {
        if self.is_empty {
            return 0.0;
        }

        if !self.is_axis_aligned {
            let mut tmp = *self;
            tmp.axis_align();
            return tmp.vertices[4].z - tmp.vertices[0].z;
        }

        self.vertices[4].z - self.vertices[0].z
    }

    pub fn scale(&mut self, x_scale: f32, y_scale: f32, z_scale: f32) {
        let mut origin = self.get_origin();
        origin.x *= x_scale;
        origin.y *= y_scale;
        origin.z *= z_scale;
        self.set_origin(origin);

        let width = self.get_width();
        self.set_width(x_scale * width);

        let height = self.get_height();
        self.set_height(y_scale * height);

        let depth = self.get_depth();
        self.set_depth(z_scale * depth);
    }

    pub fn union(&mut self, other: &PaintVolume) {
        if other.is_empty {
            return;
        }

        if self.is_empty {
            *self = *other;
            return;
        }

        if !self.is_axis_aligned {
            self.axis_align();
        }

        self.complete();

        let mut other_aligned = *other;
        if !other.is_axis_aligned || !other.is_complete {
            other_aligned.axis_align();
            other_aligned.complete();
        }

        let self_count = if self.is_2d { 4 } else { 8 };
        let other_count = if other_aligned.is_2d { 4 } else { 8 };

        let mut min = Point3D::new(self.vertices[0].x, self.vertices[0].y, self.vertices[0].z);
        let mut max = Point3D::new(self.vertices[0].x, self.vertices[0].y, self.vertices[0].z);

        for i in 1..self_count {
            min.x = f32_min(min.x, self.vertices[i].x);
            max.x = f32_max(max.x, self.vertices[i].x);
            min.y = f32_min(min.y, self.vertices[i].y);
            max.y = f32_max(max.y, self.vertices[i].y);
            min.z = f32_min(min.z, self.vertices[i].z);
            max.z = f32_max(max.z, self.vertices[i].z);
        }

        for i in 0..other_count {
            min.x = f32_min(min.x, other_aligned.vertices[i].x);
            max.x = f32_max(max.x, other_aligned.vertices[i].x);
            min.y = f32_min(min.y, other_aligned.vertices[i].y);
            max.y = f32_max(max.y, other_aligned.vertices[i].y);
            min.z = f32_min(min.z, other_aligned.vertices[i].z);
            max.z = f32_max(max.z, other_aligned.vertices[i].z);
        }

        self.vertices[0] = min;
        self.vertices[1] = Point3D::new(max.x, min.y, min.z);
        self.vertices[3] = Point3D::new(min.x, max.y, min.z);
        self.vertices[4] = Point3D::new(min.x, min.y, max.z);

        self.is_2d = self.vertices[4].z == self.vertices[0].z;
        self.is_empty = false;
        self.is_complete = false;
    }

    pub fn union_box(&mut self, box_2d: &ActorBox) {
        let mut volume = PaintVolume::new();
        volume.set_origin(Point3D::new(box_2d.x1, box_2d.y1, 0.0));
        volume.set_width(box_2d.x2 - box_2d.x1);
        volume.set_height(box_2d.y2 - box_2d.y1);

        self.union(&volume);
    }

    pub fn complete(&mut self) {
        if self.is_empty || self.is_complete {
            return;
        }

        let dx_l2r = self.vertices[1].x - self.vertices[0].x;
        let dy_l2r = self.vertices[1].y - self.vertices[0].y;
        let dz_l2r = self.vertices[1].z - self.vertices[0].z;

        let dx_t2b = self.vertices[3].x - self.vertices[0].x;
        let dy_t2b = self.vertices[3].y - self.vertices[0].y;
        let dz_t2b = self.vertices[3].z - self.vertices[0].z;

        self.vertices[2].x = self.vertices[3].x + dx_l2r;
        self.vertices[2].y = self.vertices[3].y + dy_l2r;
        self.vertices[2].z = self.vertices[3].z + dz_l2r;

        if !self.is_2d {
            self.vertices[5].x = self.vertices[4].x + dx_l2r;
            self.vertices[5].y = self.vertices[4].y + dy_l2r;
            self.vertices[5].z = self.vertices[4].z + dz_l2r;

            self.vertices[6].x = self.vertices[5].x + dx_t2b;
            self.vertices[6].y = self.vertices[5].y + dy_t2b;
            self.vertices[6].z = self.vertices[5].z + dz_t2b;

            self.vertices[7].x = self.vertices[4].x + dx_t2b;
            self.vertices[7].y = self.vertices[4].y + dy_t2b;
            self.vertices[7].z = self.vertices[4].z + dz_t2b;
        }

        self.is_complete = true;
    }

    pub fn get_bounding_box(&mut self) -> ActorBox {
        if self.is_empty {
            return ActorBox::new(
                self.vertices[0].x,
                self.vertices[0].y,
                self.vertices[0].x,
                self.vertices[0].y,
            );
        }

        self.complete();

        let count = if self.is_2d { 4 } else { 8 };

        let mut x_min = self.vertices[0].x;
        let mut x_max = self.vertices[0].x;
        let mut y_min = self.vertices[0].y;
        let mut y_max = self.vertices[0].y;

        for i in 1..count {
            if self.vertices[i].x < x_min {
                x_min = self.vertices[i].x;
            } else if self.vertices[i].x > x_max {
                x_max = self.vertices[i].x;
            }

            if self.vertices[i].y < y_min {
                y_min = self.vertices[i].y;
            } else if self.vertices[i].y > y_max {
                y_max = self.vertices[i].y;
            }
        }

        ActorBox::new(x_min, y_min, x_max, y_max)
    }

    pub fn is_empty(&self) -> bool {
        self.is_empty
    }

    pub fn is_axis_aligned(&self) -> bool {
        self.is_axis_aligned
    }

    pub fn is_2d(&self) -> bool {
        self.is_2d
    }

    fn update_is_empty(&mut self) {
        self.is_empty = self.vertices[0].x == self.vertices[1].x
            && self.vertices[0].y == self.vertices[3].y
            && self.vertices[0].z == self.vertices[4].z;
    }

    fn axis_align(&mut self) {
        if self.is_empty || self.is_axis_aligned {
            return;
        }

        if self.vertices[0].x == self.vertices[1].x
            && self.vertices[0].y == self.vertices[3].y
            && self.vertices[0].z == self.vertices[4].z
        {
            self.is_axis_aligned = true;
            return;
        }

        if !self.is_complete {
            self.complete();
        }

        let mut origin = self.vertices[0];
        let mut max_x = self.vertices[0].x;
        let mut max_y = self.vertices[0].y;
        let mut max_z = self.vertices[0].z;

        let count = if self.is_2d { 4 } else { 8 };

        for i in 1..count {
            if self.vertices[i].x < origin.x {
                origin.x = self.vertices[i].x;
            } else if self.vertices[i].x > max_x {
                max_x = self.vertices[i].x;
            }

            if self.vertices[i].y < origin.y {
                origin.y = self.vertices[i].y;
            } else if self.vertices[i].y > max_y {
                max_y = self.vertices[i].y;
            }

            if self.vertices[i].z < origin.z {
                origin.z = self.vertices[i].z;
            } else if self.vertices[i].z > max_z {
                max_z = self.vertices[i].z;
            }
        }

        self.vertices[0] = origin;
        self.vertices[1] = Point3D::new(max_x, origin.y, origin.z);
        self.vertices[3] = Point3D::new(origin.x, max_y, origin.z);
        self.vertices[4] = Point3D::new(origin.x, origin.y, max_z);

        self.is_complete = false;
        self.is_axis_aligned = true;
        self.is_2d = self.vertices[4].z == self.vertices[0].z;
    }
}

impl Default for PaintVolume {
    fn default() -> Self {
        Self::new()
    }
}

fn f32_min(a: f32, b: f32) -> f32 {
    if a < b {
        a
    } else {
        b
    }
}

fn f32_max(a: f32, b: f32) -> f32 {
    if a > b {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_paint_volume_is_empty() {
        let pv = PaintVolume::new();
        assert!(pv.is_empty());
        assert!(pv.is_axis_aligned());
        assert!(pv.is_2d());
    }

    #[test]
    fn set_width_marks_not_empty() {
        let mut pv = PaintVolume::new();
        pv.set_width(10.0);
        assert!(!pv.is_empty());
        assert_eq!(pv.get_width(), 10.0);
    }

    #[test]
    fn set_width_height_depth() {
        let mut pv = PaintVolume::new();
        pv.set_width(10.0);
        pv.set_height(20.0);
        pv.set_depth(5.0);

        assert_eq!(pv.get_width(), 10.0);
        assert_eq!(pv.get_height(), 20.0);
        assert_eq!(pv.get_depth(), 5.0);
        assert!(!pv.is_2d());
    }

    #[test]
    fn set_and_get_origin() {
        let mut pv = PaintVolume::new();
        let origin = Point3D::new(5.0, 10.0, 2.0);
        pv.set_origin(origin);
        assert_eq!(pv.get_origin(), origin);
    }

    #[test]
    fn scale_applies_to_origin_and_dimensions() {
        let mut pv = PaintVolume::new();
        pv.set_origin(Point3D::new(2.0, 4.0, 1.0));
        pv.set_width(10.0);
        pv.set_height(20.0);
        pv.set_depth(5.0);

        pv.scale(2.0, 2.0, 2.0);

        let origin = pv.get_origin();
        assert_eq!(origin.x, 4.0);
        assert_eq!(origin.y, 8.0);
        assert_eq!(origin.z, 2.0);
        assert_eq!(pv.get_width(), 20.0);
        assert_eq!(pv.get_height(), 40.0);
        assert_eq!(pv.get_depth(), 10.0);
    }

    #[test]
    fn union_with_empty() {
        let mut pv1 = PaintVolume::new();
        pv1.set_width(10.0);
        pv1.set_height(10.0);

        let pv2 = PaintVolume::new();

        pv1.union(&pv2);
        assert_eq!(pv1.get_width(), 10.0);
        assert_eq!(pv1.get_height(), 10.0);
    }

    #[test]
    fn union_two_volumes() {
        let mut pv1 = PaintVolume::new();
        pv1.set_width(10.0);
        pv1.set_height(10.0);

        let mut pv2 = PaintVolume::new();
        pv2.set_origin(Point3D::new(5.0, 5.0, 0.0));
        pv2.set_width(10.0);
        pv2.set_height(10.0);

        pv1.union(&pv2);
        assert_eq!(pv1.get_width(), 15.0);
        assert_eq!(pv1.get_height(), 15.0);
    }

    #[test]
    fn union_box_2d() {
        let mut pv = PaintVolume::new();
        pv.set_width(10.0);
        pv.set_height(10.0);

        let box_2d = ActorBox::new(5.0, 5.0, 15.0, 15.0);
        pv.union_box(&box_2d);

        assert_eq!(pv.get_width(), 15.0);
        assert_eq!(pv.get_height(), 15.0);
    }

    #[test]
    fn get_bounding_box_returns_2d_box() {
        let mut pv = PaintVolume::new();
        pv.set_origin(Point3D::new(1.0, 2.0, 0.0));
        pv.set_width(10.0);
        pv.set_height(20.0);

        let bbox = pv.get_bounding_box();
        assert_eq!(bbox.x1, 1.0);
        assert_eq!(bbox.y1, 2.0);
        assert_eq!(bbox.x2, 11.0);
        assert_eq!(bbox.y2, 22.0);
    }

    #[test]
    fn copy_paint_volume() {
        let mut pv1 = PaintVolume::new();
        pv1.set_origin(Point3D::new(1.0, 2.0, 3.0));
        pv1.set_width(5.0);
        pv1.set_height(6.0);

        let pv2 = PaintVolume::from_paint_volume(&pv1);
        assert_eq!(pv2.get_origin(), pv1.get_origin());
        assert_eq!(pv2.get_width(), pv1.get_width());
        assert_eq!(pv2.get_height(), pv1.get_height());
    }
}
